//! `game_core/src/pvp.rs` — penalizaciones PK (alineación, exp, drops).
//! Parity `CHARACTER::UpdateAlignment` (char_battle.cpp:3225-3243), la pena
//! del asesinato en `onDead` (char_battle.cpp:1284-1313), la pérdida de exp
//! por muerte en `DeathPenalty` (char_battle.cpp:286-340, `__GetExpLossPerc`
//! :51-56 + `aiExpLossPercents` constants.cpp:768-796) y el drop de items
//! por alineación en `ItemDropPenalty` (char_battle.cpp:917-997).
//!
//! Funciones PURAS: el lane aplicará los flags de estado (muerte por PC —
//! `INSTANT_FLAG_DEATH_PENALTY` se LIMPIA en :1261 —, `AFFECT_NO_DEATH_PENALTY`,
//! revive en town, `UNIQUE_ITEM_SKIP_ITEM_DROP_PENALTY` equipado, level ≥ 50
//! para el drop) antes de llamar estos cálculos.

use crate::guild::GuildWar;

/// Rango Lawful/Chaotic de la alineación (char.h:1360 — −200000..200000).
pub const ALIGNMENT_MAX: i32 = 200_000;

/// `UpdateAlignment` (char_battle.cpp:3225-3243): suma `amount` y clamp a
/// [−200000, 200000] (el `MINMAX` de :3234).
pub fn update_alignment(alignment: i32, amount: i32) -> i32 {
    (alignment + amount).clamp(-ALIGNMENT_MAX, ALIGNMENT_MAX)
}

/// Penalización del asesino (char_battle.cpp:1284-1313): matar a un
/// inocente (víctima sin killer-flag y con alineación ≥ 0 — :1284) cuesta
/// −20000. El roll `number(1,100)` decide la salvación: asesino limpio
/// (≥ 0) → 33 % de no-pena; ya negativo → 20 % (:1288-1291; `roll < pct` →
/// 0, parity estricta). Con party, el total se reparte entre los miembros
/// en rango (`FPartyAlignmentCompute`, :1115-1148 — `party_in_range` = el
/// m_iCount, INCLUYE al asesino a distancia 0); 0 en rango → el asesino
/// paga el total (:1302-1303).
///
/// Devuelve el DELTA de alineación (0 si el roll salvó).
pub fn pk_penalty_delta(
    attacker_alignment: i32,
    party_in_range: u32,
    mut roll: impl FnMut(i32, i32) -> i32,
) -> i32 {
    const PK_ALIGNMENT_PENALTY: i32 = -20_000;
    let no_penalty_pct = if attacker_alignment >= 0 { 33 } else { 20 };
    if roll(1, 100) < no_penalty_pct {
        return 0;
    }
    if party_in_range == 0 {
        PK_ALIGNMENT_PENALTY
    } else {
        PK_ALIGNMENT_PENALTY / party_in_range as i32
    }
}

/// Tabla `aItemDropPenalty_kor` (char_battle.cpp:917-936): cuanto más
/// caótico el jugador, más probable y numeroso el drop al morir en PK.
/// `iInventoryPct` es 1..1000 y `iEquipmentPct` 1..100 (escala distinta
/// por diseño del C++).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DropPenalty {
    pub inventory_pct: u32,
    pub inventory_qty: u32,
    pub equipment_pct: u32,
    pub equipment_qty: u32,
}

/// `__GetExpLossPerc` (char_battle.cpp:51-56) + `aiExpLossPercents`
/// (constants.cpp:768-796): % de exp perdida por nivel. Guard del C++:
/// level 0 o > PLAYER_EXP_TABLE_MAX (120, length.h:52) → 1. Bandas:
/// 1-9 → 5 %, 10-27 → 4 %, 28-44 → 3 %, 45-62 → 2 %, 63+ → 1 %.
pub fn exp_loss_pct(level: u32) -> u32 {
    match level {
        0 | 121.. => 1,
        1..=9 => 5,
        10..=27 => 4,
        28..=44 => 3,
        45..=62 => 2,
        _ => 1, // 63..=120
    }
}

/// `DeathPenalty` (char_battle.cpp:286-340): exp perdida al morir. Gates en
/// orden C++: level < 10 → 0 SIN consumir roll; `number(0,2)` != 0 → 0
/// (suerte: 1/3 paga); flag sin-pena (muerte por PC / affect / town) → 0.
/// Pérdida = `next_exp * pct / 100` con cap 800000 (`MIN` :328) y la
/// lágrima de dios (`UNIQUE_ITEM_TEARDROP_OF_GODNESS`) divide tras el cap.
pub fn death_exp_loss(
    level: u32,
    next_exp: u64,
    mut roll: impl FnMut(i32, i32) -> i32,
    no_death_penalty: bool,
    teardrop_of_godness: bool,
) -> u64 {
    const EXP_LOSS_CAP: u64 = 800_000;
    if level < 10 {
        return 0;
    }
    if roll(0, 2) != 0 {
        return 0;
    }
    if no_death_penalty {
        return 0;
    }
    let loss = (next_exp * exp_loss_pct(level) as u64 / 100).min(EXP_LOSS_CAP);
    if teardrop_of_godness { loss / 2 } else { loss }
}

/// Escalera `GetRealAlignment` (char_battle.cpp:951-970) → fila de
/// `aItemDropPenalty_kor` (:925-936). Tiers 0-4 (alineación ≥ 0) no
/// dropean; 5-8 escalan {25,1,5,1} → {100,8,20,1}. El gate level ≥ 50
/// (:943) es del lane, como `GetMyShop`.
pub fn drop_penalty(alignment: i32) -> DropPenalty {
    const TABLE: [DropPenalty; 9] = [
        DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        },
        DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        },
        DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        },
        DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        },
        DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        },
        DropPenalty {
            inventory_pct: 25,
            inventory_qty: 1,
            equipment_pct: 5,
            equipment_qty: 1,
        },
        DropPenalty {
            inventory_pct: 50,
            inventory_qty: 2,
            equipment_pct: 10,
            equipment_qty: 1,
        },
        DropPenalty {
            inventory_pct: 75,
            inventory_qty: 4,
            equipment_pct: 15,
            equipment_qty: 1,
        },
        DropPenalty {
            inventory_pct: 100,
            inventory_qty: 8,
            equipment_pct: 20,
            equipment_qty: 1,
        },
    ];
    let idx = if alignment >= 120_000 {
        0
    } else if alignment >= 80_000 {
        1
    } else if alignment >= 40_000 {
        2
    } else if alignment >= 10_000 {
        3
    } else if alignment >= 0 {
        4
    } else if alignment > -40_000 {
        5
    } else if alignment > -80_000 {
        6
    } else if alignment > -120_000 {
        7
    } else {
        8
    };
    TABLE[idx]
}

/// Rolls del drop (char_battle.cpp:980-997): `pct >= number(1, …)` —
/// comparación INCLUSIVA, escalas 1000 (inventario) y 100 (equipo). Los
/// DOS rolls se consumen siempre; `skip_item` (SKIP_ITEM_DROP_PENALTY)
/// anula ambos resultados después.
///
/// Devuelve `(drop_inventory, drop_equipment)`.
pub fn drop_allowed(
    penalty: DropPenalty,
    mut roll: impl FnMut(i32, i32) -> i32,
    skip_item: bool,
) -> (bool, bool) {
    let drop_inventory = penalty.inventory_pct >= roll(1, 1000) as u32;
    let drop_equipment = penalty.equipment_pct >= roll(1, 100) as u32;
    if skip_item {
        (false, false)
    } else {
        (drop_inventory, drop_equipment)
    }
}

/// Guerra de guilds: ¿están en guerra? (parity `CGuild::UnderWar`, guild.cpp
/// — `Dead` char_battle.cpp:1198-1200: `g1->UnderWar(g2->GetID())`). `None` =
/// sin guild → nunca en guerra. Orden invariante.
pub fn is_at_war(
    attacker_guild: Option<i64>,
    victim_guild: Option<i64>,
    wars: &[GuildWar],
) -> bool {
    match (attacker_guild, victim_guild) {
        (Some(a), Some(b)) if a != b => wars
            .iter()
            .any(|w| (w.guild_a == a && w.guild_b == b) || (w.guild_a == b && w.guild_b == a)),
        _ => false,
    }
}

/// PK con guerra: si ambos en guerra → 0 (parity `Dead` :1226-1231 +
/// :1284 — `!isUnderGuildWar` gatea `ItemDropPenalty` y `UpdateAlignment`).
pub fn pk_penalty_delta_war(
    attacker_alignment: i32,
    party_in_range: u32,
    attacker_guild: Option<i64>,
    victim_guild: Option<i64>,
    wars: &[GuildWar],
    roll: impl FnMut(i32, i32) -> i32,
) -> i32 {
    if is_at_war(attacker_guild, victim_guild, wars) {
        0
    } else {
        pk_penalty_delta(attacker_alignment, party_in_range, roll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clamp parity (`MINMAX`, char_battle.cpp:3234): nunca sale de
    /// [−200000, 200000], ni con sumas que desbordan el rango.
    #[test]
    fn alignment_clamps_to_bounds() {
        assert_eq!(update_alignment(150_000, 100_000), ALIGNMENT_MAX);
        assert_eq!(update_alignment(-190_000, -50_000), -ALIGNMENT_MAX);
        assert_eq!(update_alignment(199_999, 1), ALIGNMENT_MAX);
        assert_eq!(update_alignment(-199_999, -1), -ALIGNMENT_MAX);
        assert_eq!(update_alignment(0, -20_000), -20_000);
        assert_eq!(update_alignment(-5_000, 5_000), 0);
    }

    /// VERIFIER (regla 20) de la penalidad PK: matar a un inocente cuesta
    /// −20000 (roll ≥ pct — parity `number(1,100) < pct` = salvado); el
    /// reparto de party es entero (`m_iAmount / m_iCount`, :1137). FALLA si
    /// se quita la pena, el roll de salvación o el clamp (mutation).
    #[test]
    fn pk_penalty_applies_and_rolls() {
        // Asesino limpio: 33 % de salvación (roll < 33); 40 → pena completa.
        assert_eq!(
            pk_penalty_delta(0, 0, |_, _| 40),
            -20_000,
            "roll 40 ≥ 33 → pena"
        );
        assert_eq!(
            pk_penalty_delta(0, 0, |_, _| 32),
            0,
            "roll 32 < 33 → salvado"
        );
        // Ya negativo: 20 % (roll < 20); 20 → pena.
        assert_eq!(
            pk_penalty_delta(-5, 0, |_, _| 19),
            0,
            "roll 19 < 20 → salvado"
        );
        assert_eq!(
            pk_penalty_delta(-5, 0, |_, _| 20),
            -20_000,
            "roll 20 ≥ 20 → pena"
        );
        // Party en rango (FPartyAlignmentCompute): reparto entero truncado.
        assert_eq!(pk_penalty_delta(0, 4, |_, _| 100), -5_000, "-20000/4");
        assert_eq!(
            pk_penalty_delta(0, 3, |_, _| 100),
            -6_666,
            "-20000/3 (truncado)"
        );
        // Con party pero 0 en rango → el asesino paga el total (:1302-1303).
        assert_eq!(pk_penalty_delta(0, 0, |_, _| 100), -20_000);
    }

    /// Parity `aiExpLossPercents` (constants.cpp:768-796) en todas las
    /// bandas + guard de `__GetExpLossPerc` (:51-56): level 0 o > 120 → 1.
    #[test]
    fn exp_loss_pct_matches_table() {
        assert_eq!(exp_loss_pct(0), 1, "guard !level → 1");
        assert_eq!(exp_loss_pct(1), 5);
        assert_eq!(exp_loss_pct(9), 5);
        assert_eq!(exp_loss_pct(10), 4);
        assert_eq!(exp_loss_pct(27), 4);
        assert_eq!(exp_loss_pct(28), 3);
        assert_eq!(exp_loss_pct(44), 3);
        assert_eq!(exp_loss_pct(45), 2);
        assert_eq!(exp_loss_pct(62), 2);
        assert_eq!(exp_loss_pct(63), 1);
        assert_eq!(exp_loss_pct(120), 1);
        assert_eq!(exp_loss_pct(121), 1, "guard level > MAX → 1");
        assert_eq!(exp_loss_pct(999), 1);
    }

    /// Gates de `DeathPenalty` (:286-307): level < 10 → 0 SIN consumir el
    /// roll; `number(0,2)` != 0 → 0 (suerte: 2/3 salvado).
    #[test]
    fn death_exp_loss_level_and_luck_gates() {
        let mut rolls = 0;
        assert_eq!(
            death_exp_loss(
                9,
                100_000,
                |_, _| {
                    rolls += 1;
                    0
                },
                false,
                false
            ),
            0
        );
        assert_eq!(rolls, 0, "level < 10 sale antes del roll (:295)");
        assert_eq!(
            death_exp_loss(
                10,
                100_000,
                |_, _| {
                    rolls += 1;
                    1
                },
                false,
                false
            ),
            0,
            "roll 1 → suerte"
        );
        assert_eq!(
            death_exp_loss(
                10,
                100_000,
                |_, _| {
                    rolls += 1;
                    2
                },
                false,
                false
            ),
            0,
            "roll 2 → suerte"
        );
        assert_eq!(rolls, 2);
    }

    /// VERIFIER (regla 20) de la pérdida: fórmula `next*pct/100`, cap
    /// 800000 (`MIN` :328), lágrima tras el cap (:333-334) y flag sin-pena
    /// (:309). FALLA si se quita el pct, el cap, la lágrima o el flag
    /// (mutation).
    #[test]
    fn death_exp_loss_computes_caps_halves() {
        // level 30 → 3 %: 1_000_000 * 3 / 100 = 30_000.
        assert_eq!(
            death_exp_loss(30, 1_000_000, |_, _| 0, false, false),
            30_000
        );
        // Cap: level 63 → 1 %: 200_000_000 / 100 = 2_000_000 → 800_000.
        assert_eq!(
            death_exp_loss(63, 200_000_000, |_, _| 0, false, false),
            800_000
        );
        // Lágrima tras el cap: 800_000 / 2 = 400_000.
        assert_eq!(
            death_exp_loss(63, 200_000_000, |_, _| 0, false, true),
            400_000
        );
        // Lágrima sin cap: 30_000 / 2 = 15_000.
        assert_eq!(death_exp_loss(30, 1_000_000, |_, _| 0, false, true), 15_000);
        // Flag sin-pena (muerte por PC limpia el flag — :1261): 0.
        assert_eq!(death_exp_loss(30, 1_000_000, |_, _| 0, true, false), 0);
    }

    /// Parity del ORDEN: el roll de suerte se consume antes del flag (:302
    /// vs :309) — hasta una muerte por PC rueda el dado y luego sale 0.
    #[test]
    fn death_exp_loss_rolls_before_flag() {
        let mut rolls = 0;
        assert_eq!(
            death_exp_loss(
                30,
                1_000_000,
                |_, _| {
                    rolls += 1;
                    0
                },
                true,
                false
            ),
            0
        );
        assert_eq!(rolls, 1, "roll consumido aunque el flag cancele la pena");
    }

    /// VERIFIER (regla 20) de la escalera `GetRealAlignment` (:951-970) y
    /// la tabla `aItemDropPenalty_kor` (:925-936). FALLA si se mueve un
    /// escalón o un valor de la tabla (mutation).
    #[test]
    fn drop_penalty_tiers_and_table() {
        let none = DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        };
        let t5 = DropPenalty {
            inventory_pct: 25,
            inventory_qty: 1,
            equipment_pct: 5,
            equipment_qty: 1,
        };
        let t6 = DropPenalty {
            inventory_pct: 50,
            inventory_qty: 2,
            equipment_pct: 10,
            equipment_qty: 1,
        };
        let t7 = DropPenalty {
            inventory_pct: 75,
            inventory_qty: 4,
            equipment_pct: 15,
            equipment_qty: 1,
        };
        let t8 = DropPenalty {
            inventory_pct: 100,
            inventory_qty: 8,
            equipment_pct: 20,
            equipment_qty: 1,
        };
        assert_eq!(drop_penalty(ALIGNMENT_MAX), none, "≥ 120000 → tier 0");
        assert_eq!(drop_penalty(119_999), none, "≥ 80000 → tier 1");
        assert_eq!(drop_penalty(79_999), none, "≥ 40000 → tier 2");
        assert_eq!(drop_penalty(39_999), none, "≥ 10000 → tier 3");
        assert_eq!(drop_penalty(9_999), none, "≥ 0 → tier 4");
        assert_eq!(drop_penalty(0), none, "≥ 0 → tier 4");
        assert_eq!(drop_penalty(-1), t5, "> -40000 → tier 5");
        assert_eq!(drop_penalty(-39_999), t5, "> -40000 → tier 5");
        assert_eq!(drop_penalty(-40_000), t6, "> -80000 → tier 6");
        assert_eq!(drop_penalty(-80_000), t7, "> -120000 → tier 7");
        assert_eq!(drop_penalty(-120_000), t8, "else → tier 8");
        assert_eq!(drop_penalty(-ALIGNMENT_MAX), t8, "else → tier 8");
    }

    /// VERIFIER (regla 20) del roll de drop (:980-997): `pct >= number` —
    /// INCLUSIVO (roll == pct → drop); `skip_item` anula ambos PERO los dos
    /// rolls se consumen igual (:984-989). FALLA si se invierte la
    /// comparación o se salta el roll (mutation).
    #[test]
    fn drop_allowed_rolls_and_skip_item() {
        let t5 = DropPenalty {
            inventory_pct: 25,
            inventory_qty: 1,
            equipment_pct: 5,
            equipment_qty: 1,
        };
        let none = DropPenalty {
            inventory_pct: 0,
            inventory_qty: 0,
            equipment_pct: 0,
            equipment_qty: 0,
        };
        let mut n = 0;
        let mut roll = |_, _| {
            n += 1;
            if n == 1 { 25 } else { 5 }
        };
        assert_eq!(
            drop_allowed(t5, &mut roll, false),
            (true, true),
            "roll == pct → drop (>=)"
        );
        assert_eq!(n, 2);
        let mut roll = |_, _| {
            n += 1;
            if n == 3 { 26 } else { 6 }
        };
        assert_eq!(
            drop_allowed(t5, &mut roll, false),
            (false, false),
            "roll > pct → no"
        );
        assert_eq!(n, 4);
        let mut roll = |_, _| {
            n += 1;
            1
        };
        assert_eq!(
            drop_allowed(t5, &mut roll, true),
            (false, false),
            "skip item → sin drops"
        );
        assert_eq!(
            n, 6,
            "skip item consume los 2 rolls igual (parity :980-989)"
        );
        assert_eq!(
            drop_allowed(none, |_, _| 1, false),
            (false, false),
            "tier 0-4 (pct 0) nunca dropea"
        );
    }

    /// VERIFIER war-PK (regla 20): si ambos en guerra → 0 sin roll; fuera de
    /// guerra delega en `pk_penalty_delta`. FALLA si se quita el gate war.
    #[test]
    fn war_pk_no_penalty_verifier() {
        use crate::guild::GuildWar;
        let wars = [GuildWar {
            guild_a: 1,
            guild_b: 2,
            score_a: 0,
            score_b: 0,
        }];
        assert!(is_at_war(Some(1), Some(2), &wars));
        assert!(is_at_war(Some(2), Some(1), &wars), "orden invariante");
        assert!(!is_at_war(Some(1), Some(3), &wars));
        assert!(!is_at_war(None, Some(2), &wars));
        assert!(!is_at_war(Some(1), Some(1), &wars), "misma guild → no war");
        // En guerra: 0 incluso con roll que daría pena fuera de guerra.
        assert_eq!(
            pk_penalty_delta_war(0, 0, Some(1), Some(2), &wars, |_, _| 100),
            0
        );
        // Fuera de guerra: delega (roll 100 → -20000).
        assert_eq!(
            pk_penalty_delta_war(0, 0, Some(1), Some(3), &wars, |_, _| 100),
            -20_000
        );
        // Sin guilds: también pena.
        assert_eq!(
            pk_penalty_delta_war(0, 0, None, None, &wars, |_, _| 100),
            -20_000
        );
    }
}

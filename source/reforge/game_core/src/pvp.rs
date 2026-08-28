//! `game_core/src/pvp.rs` — penalización PK (alineación). Parity
//! `CHARACTER::UpdateAlignment` (char_battle.cpp:3225-3243) y la pena del
//! asesinato en `onDead` (char_battle.cpp:1284-1313).
//!
//! Funciones PURAS: el lane de persistencia (player.player NO expone la
//! alineación hoy — GAP documentado) llamará `pk_penalty_delta` al aplicar
//! un kill PvP y `update_alignment` para mutar el valor guardado.

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
        assert_eq!(pk_penalty_delta(0, 0, |_, _| 40), -20_000, "roll 40 ≥ 33 → pena");
        assert_eq!(pk_penalty_delta(0, 0, |_, _| 32), 0, "roll 32 < 33 → salvado");
        // Ya negativo: 20 % (roll < 20); 20 → pena.
        assert_eq!(pk_penalty_delta(-5, 0, |_, _| 19), 0, "roll 19 < 20 → salvado");
        assert_eq!(pk_penalty_delta(-5, 0, |_, _| 20), -20_000, "roll 20 ≥ 20 → pena");
        // Party en rango (FPartyAlignmentCompute): reparto entero truncado.
        assert_eq!(pk_penalty_delta(0, 4, |_, _| 100), -5_000, "-20000/4");
        assert_eq!(pk_penalty_delta(0, 3, |_, _| 100), -6_666, "-20000/3 (truncado)");
        // Con party pero 0 en rango → el asesino paga el total (:1302-1303).
        assert_eq!(pk_penalty_delta(0, 0, |_, _| 100), -20_000);
    }
}
//! Verificadores sintéticos (rule 20): PASAN con el fix actual, FALLAN si se
//! revierte al valor bug. Determinísticos, sin RNG/I/O, sweeps de dominio (<1 ms).

use database::attr::{roll_attrs, AttrRow, AttrTables};
use game_core::ai::{mob_attack_cooldown_ms, mob_move_speed};

/// synthetic verifier: fails if fix reverted — C29. Bug: el rewrite atacaba
/// cada tick (250 ms); legacy = `CalculateDuration(att_speed, 2000)`
/// (char_state.cpp:1005-1012). Mutation que debe fallar: hardcode 250.
#[test]
fn mob_attack_cooldown_never_250ms_tick() {
    assert_eq!(mob_attack_cooldown_ms(100), 2_000, "default attack_speed");
    // Dominio: cooldown >= 500 ms para TODO factor 0..=300 (mínimo real 660 @
    // 300 — un hardcode 250 falla ya en f=0) y monótono no-creciente.
    for f in 0..=300 {
        assert!(mob_attack_cooldown_ms(f) >= 500, "cooldown({f}) >= 500");
        if f < 300 {
            assert!(
                mob_attack_cooldown_ms(f) >= mob_attack_cooldown_ms(f + 1),
                "cooldown({f}) >= cooldown({})",
                f + 1
            );
        }
    }
}

/// synthetic verifier: fails if fix reverted — attrs lane (2026-08-16,
/// 79ae59e). Bug: los items nacían SIN attrs; el fix = `roll_attrs` en el
/// create — drop de mob (session.rs `roll_drop_attrs`), GM `item`
/// (gm.rs) y rewards de quest (quest.rs, solo sockets) — con las tablas
/// `item_attr` 54 + `item_attr_rare` 20 (fail-open: sin tablas → plano).
/// Mutaciones que deben fallar: quitar alter_to_magic_item/add_rare_attr
/// del roll, hardcodear attrs/sockets a 0 en el create, o un `>=` en el
/// roll de magic_pct (=100 roto).
#[test]
fn roll_attrs_never_leaves_items_plain() {
    let row = |apply: i16, prob: i32, values: [i32; 5], sets: [i16; 8]| AttrRow {
        apply_index: apply,
        prob,
        values,
        max_level_by_set: sets,
    };
    // Mínimo representativo de las 54+20 filas reales: 2 normales (prob
    // ponderada, MAX_HP en todos los sets / ATT_SPEED solo arma) + 1 rare.
    let tables = AttrTables {
        normal: vec![
            row(1, 10, [10, 20, 30, 40, 50], [5; 8]), // MAX_HP — todos los sets
            row(7, 5, [1, 2, 3, 4, 5], [5, 0, 0, 0, 0, 0, 0, 0]), // ATT_SPEED — arma
        ],
        rare: vec![row(53, 0, [1, 2, 3, 4, 5], [3, 3, 0, 0, 0, 0, 0, 0])], // ATT_GRADE_BONUS
    };
    // RNG determinista xorshift32 (sin dependencias — patrón del rand32 del
    // canal; seed != 0 nunca produce 0).
    let mut seed = 0x1234_5678u32;
    let mut rng: Box<dyn FnMut() -> u32> = Box::new(move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    });
    // Sweep: magic_pct=100 → TODOS los rolls crean item mágico (≥ 1 attr
    // normal + el rare del lane) y socket_pct=1 → socket 0 abierto.
    let mut norm_count = 0usize;
    for _ in 0..200 {
        let mut sockets = [0i64; 3];
        let mut attrs = [(0i16, 0i16); 7];
        roll_attrs(&mut rng, 100, 1, &tables, 1, 0, &mut sockets, &mut attrs);
        assert_eq!(sockets[0], 1, "socket 0 abierto (socket_pct=1)");
        assert!(
            attrs[..5].iter().any(|(t, v)| *t != 0 && *v > 0),
            "magic_pct=100 → attr normal asignado: {attrs:?}"
        );
        assert_eq!(attrs[5], (53, 3), "rare ATT_GRADE_BONUS lv3 en slot 5");
        assert!(
            attrs[..5].iter().all(|(t, _)| matches!(*t, 0 | 1 | 7)),
            "apply solo del set ARMA: {attrs:?}"
        );
        norm_count += attrs[..5].iter().filter(|(t, _)| *t != 0).count();
    }
    assert!(norm_count > 200, "media ≥ 1 attr normal por item (High + Low): {norm_count}");
    // Fail-open parity: usable (sin set) → no-op aunque acierte el roll.
    let mut sockets = [0i64; 3];
    let mut attrs = [(0i16, 0i16); 7];
    roll_attrs(&mut rng, 100, 0, &tables, 3, 0, &mut sockets, &mut attrs);
    assert_eq!(attrs, [(0, 0); 7], "usable: sin attrs (GetAttributeSetIndex None)");
}

/// synthetic verifier: fails if fix reverted — C30. Bug: la columna `move_speed`
/// usada como u/s → factor 0/1 = 0 u/s (mobs congelados), 100 = 100 u/s (~3×
/// lento). Legacy = motion(300)×10000/`CalculateDuration(factor, 10000)`
/// (char.cpp:2751-2754).
#[test]
fn mob_move_speed_never_freezes() {
    assert_eq!(mob_move_speed(100), 300, "factor neutro → motion");
    assert_eq!(mob_move_speed(0), 150, "factor 0 no congela (parity)");
    // Dominio: NUNCA < 150 u/s para TODO factor 0..=255 (u8) y monótono no-
    // decreciente. Mutations que fallan: f→f u/s, 0→0, hardcode 0/150.
    for f in 0..=255 {
        assert!(mob_move_speed(f) >= 150, "speed({f}) >= 150");
        assert!(
            mob_move_speed(f + 1) >= mob_move_speed(f),
            "speed({}) >= speed({f})",
            f + 1
        );
    }
}
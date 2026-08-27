//! Verificadores sintéticos (rule 20): PASAN con el fix actual, FALLAN si se
//! revierte al valor bug. Determinísticos, sin RNG/I/O, sweeps de dominio (<1 ms).

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
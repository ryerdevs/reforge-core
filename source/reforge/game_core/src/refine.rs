//! REFINE real (parity `DoRefine`/`DoRefineWithScroll`, char_item.cpp:821-1216):
//! la prob del roll sale del `refine_proto` REAL (`database::item::RefineRecipe
//! .prob`, no del stub 70% fijo) + overrides de los scrolls especiales
//! (`value0`, :1087-1148). FUERA del módulo: los gates de nivel/tipo de los
//! scrolls (MUSIN ≤+3, MEMO nivel exacto, BDRAGON metin +4) — subset del canal;
//! dragon_soul/horse no tocan.

use database::item::RefineRecipe;

/// `value0` de los scrolls de refine (enum_RefineScrolls — char_item.cpp:964-973).
pub mod scroll {
    pub const CHUKBOK: i32 = 0;
    pub const HYUNIRON: i32 = 1;
    pub const YONGSIN: i32 = 2;
    pub const MUSIN: i32 = 3;
    pub const YAGONG: i32 = 4;
    pub const MEMO: i32 = 5;
    pub const BDRAGON: i32 = 6;
}

/// Prob por nivel 0..8 (char_item.cpp:1091-1092; index = MINMAX(0, level, 8)).
const YONGSIN_PROB: [i32; 9] = [100, 75, 65, 55, 45, 40, 35, 25, 20];
const YAGONG_PROB: [i32; 9] = [100, 100, 90, 80, 70, 60, 50, 30, 20];

/// Prob efectiva (%) del roll: base `recipe.prob` + override del scroll
/// (:1087-1148). HYUNIRON conserva la base; MUSIN/MEMO 100; BDRAGON 80;
/// YONGSIN/YAGONG por vector·nivel.
pub fn effective_prob(recipe: &RefineRecipe, scroll_value0: i32, level: u8) -> i32 {
    let lvl = usize::from(level.min(8));
    match scroll_value0 {
        scroll::YONGSIN => YONGSIN_PROB[lvl],
        scroll::YAGONG => YAGONG_PROB[lvl],
        scroll::MUSIN | scroll::MEMO => 100,
        scroll::BDRAGON => 80,
        _ => recipe.prob, // CHUKBOK y HYUNIRON
    }
}

/// `bDestroyWhenFail` (HYUNIRON, :1112-1113): el fallo con scroll NO degrada
/// — observable: el item se queda en su nivel (rama else de :1208).
pub fn destroy_when_fail(scroll_value0: i32) -> bool {
    scroll_value0 == scroll::HYUNIRON
}

/// Éxito si `roll` (1..=100, `number(1,100)` :1081) `<=` prob efectiva — el
/// borde `<=` (:1152) es literal del C++ (prob 70 + roll 70 = éxito).
pub fn is_success(recipe: &RefineRecipe, scroll_value0: i32, level: u8, roll: i32) -> bool {
    roll <= effective_prob(recipe, scroll_value0, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(prob: i32) -> RefineRecipe {
        RefineRecipe {
            cost: 1000,
            prob,
            materials: [(71055, 3), (0, 0), (0, 0), (0, 0), (0, 0)],
        }
    }

    /// VERIFIER: falla si vuelve el stub — prob real de refine_proto (no 70
    /// fijo) y borde `<=` (char_item.cpp:1152) son el contrato del C++.
    #[test]
    fn verifier_real_prob_not_stub() {
        assert_eq!(
            effective_prob(&recipe(30), scroll::CHUKBOK, 0),
            30,
            "prob real, no 70"
        );
        assert!(
            is_success(&recipe(70), scroll::CHUKBOK, 0, 70),
            "70<=70 = éxito"
        );
    }

    #[test]
    fn special_scroll_overrides() {
        let r = recipe(30); // base 30
        assert_eq!(effective_prob(&r, scroll::MUSIN, 4), 100);
        assert_eq!(effective_prob(&r, scroll::MEMO, 4), 100);
        assert_eq!(effective_prob(&r, scroll::BDRAGON, 4), 80);
        assert_eq!(effective_prob(&r, scroll::HYUNIRON, 4), 30, "HYUNIRON=base");
        assert!(destroy_when_fail(scroll::HYUNIRON) && !destroy_when_fail(scroll::YAGONG));
        assert_eq!(effective_prob(&r, scroll::YONGSIN, 8), 20, "YONGSIN[8]=20");
        assert_eq!(effective_prob(&r, scroll::YAGONG, 2), 90, "YAGONG[2]=90");
    }
}

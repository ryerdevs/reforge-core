//! DRAGON SOUL: stage/grade + fee/prob reales de refine_proto.

use database::item::RefineRecipe;

/// Pasos de refine (EDragonSoulStepTypes::DRAGON_SOUL_STEP_MAX).
pub const DS_STEP_MAX: u8 = 5;
/// Grados (EDragonSoulGradeTypes::DRAGON_SOUL_GRADE_MAX, con MYTH).
pub const DS_GRADE_MAX: u8 = 6;

/// Fee real del refine DS (parity DragonSoul.cpp:532/682 — `fee` de la tabla;
/// aquí viene del `refine_proto.cost` — una fuente, no un stub).
pub fn ds_fee(recipe: &RefineRecipe) -> i64 {
    recipe.cost as i64
}
/// Éxito si `roll` 1..100 ≤ `recipe.prob` (parity Gamble vec_probs / DoRefineStrength fProb).
pub fn ds_is_success(recipe: &RefineRecipe, roll: i32) -> bool {
    roll <= recipe.prob
}

/// Un Dragon Soul: `stage` 0 = LOWEST .. 4 = HIGHEST; `grade` 0 = NORMAL ..
/// 5 = MYTH (parity item_length.h:190-211).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragonSoul {
    pub stage: u8,
    pub grade: u8,
}

/// Crea un Dragon Soul nuevo en el paso `stage` (clamped al rango válido) con
/// grado NORMAL — el grado inicial de un alma recién obtenida.
pub fn create_dragon_soul(stage: u8) -> DragonSoul {
    DragonSoul {
        stage: stage.min(DS_STEP_MAX - 1),
        grade: 0,
    }
}

/// Refina el alma: sube un paso; en HIGHEST sube el grado (capado en MYTH);
/// el tope (HIGHEST/MYTH) es identidad.
pub fn upgrade_dragon_soul(ds: DragonSoul) -> DragonSoul {
    if ds.stage < DS_STEP_MAX - 1 {
        DragonSoul {
            stage: ds.stage + 1,
            grade: ds.grade,
        }
    } else {
        DragonSoul {
            stage: ds.stage,
            grade: (ds.grade + 1).min(DS_GRADE_MAX - 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: el refine sube el paso hasta HIGHEST y luego el grado hasta
    /// MYTH; el tope es identidad. Revertir `upgrade_dragon_soul` (devolver el
    /// alma sin mutar) o `create_dragon_soul` (grado ≠ NORMAL) rompe este test.
    #[test]
    fn upgrade_advances_stage_then_grade() {
        let ds = create_dragon_soul(0);
        assert_eq!(
            ds,
            DragonSoul { stage: 0, grade: 0 },
            "creada LOWEST/NORMAL"
        );
        let ds = upgrade_dragon_soul(ds);
        assert_eq!(ds.stage, 1, "primer refine sube el paso a LOW");
        let ds = (0..3).fold(ds, |d, _| upgrade_dragon_soul(d));
        assert_eq!(ds.stage, DS_STEP_MAX - 1, "paso capado en HIGHEST");
        let ds = upgrade_dragon_soul(ds);
        assert_eq!(
            ds,
            DragonSoul {
                stage: DS_STEP_MAX - 1,
                grade: 1
            },
            "en HIGHEST sube a BRILLIANT"
        );
        let ds = (0..DS_GRADE_MAX as usize).fold(ds, |d, _| upgrade_dragon_soul(d));
        assert_eq!(ds.grade, DS_GRADE_MAX - 1, "grado capado en MYTH");
        let top = DragonSoul {
            stage: DS_STEP_MAX - 1,
            grade: DS_GRADE_MAX - 1,
        };
        assert_eq!(upgrade_dragon_soul(top), top, "tope es identidad");
    }

    /// VERIFIER phase 2: fee/prob vienen del refine_proto real, no stub.
    #[test]
    fn verifier_ds_fee_and_prob_from_refine_proto() {
        let r = RefineRecipe {
            cost: 12345,
            prob: 70,
            materials: [(0, 0); 5],
        };
        assert_eq!(ds_fee(&r), 12345, "cost real, no stub");
        assert!(ds_is_success(&r, 70), "70<=70 éxito (borde <=)");
        assert!(!ds_is_success(&r, 71), "71>70 fallo");
        let r2 = RefineRecipe {
            cost: 50000,
            prob: 30,
            materials: [(71055, 3), (0, 0), (0, 0), (0, 0), (0, 0)],
        };
        assert_eq!(ds_fee(&r2), 50000);
        assert!(!ds_is_success(&r2, 31));
    }
}

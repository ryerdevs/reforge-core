//! DRAGON SOUL (slice stub): dominio puro del sistema Dragon Soul (parity
//! `item_length.h` + `dragon_soul_table.cpp`): un alma tiene `stage` (paso de
//! refine, `EDragonSoulStepTypes` LOWEST..HIGHEST = 5 valores) y `grade`
//! (`EDragonSoulGradeTypes` NORMAL..MYTH = 6 valores — `ENABLE_DS_GRADE_MYTH`
//! activo, CommonDefines.h:53). El stub modela el refine determinista: sube el
//! paso (`GetRefineStepValues` solo admite `step_idx < DRAGON_SOUL_STEP_MAX-1`,
//! dragon_soul_table.cpp:723) y, en HIGHEST, sube el grado (`GetRefineGradeValues`,
//! dragon_soul_table.cpp:708-716). Las probabilidades por step/grade (`vec_probs`)
//! y los materiales entran en el slice real.

/// Pasos de refine (EDragonSoulStepTypes::DRAGON_SOUL_STEP_MAX).
pub const DS_STEP_MAX: u8 = 5;
/// Grados (EDragonSoulGradeTypes::DRAGON_SOUL_GRADE_MAX, con MYTH).
pub const DS_GRADE_MAX: u8 = 6;

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
    DragonSoul { stage: stage.min(DS_STEP_MAX - 1), grade: 0 }
}

/// Refina el alma: sube un paso; en HIGHEST sube el grado (capado en MYTH);
/// el tope (HIGHEST/MYTH) es identidad.
pub fn upgrade_dragon_soul(ds: DragonSoul) -> DragonSoul {
    if ds.stage < DS_STEP_MAX - 1 {
        DragonSoul { stage: ds.stage + 1, grade: ds.grade }
    } else {
        DragonSoul { stage: ds.stage, grade: (ds.grade + 1).min(DS_GRADE_MAX - 1) }
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
        assert_eq!(ds, DragonSoul { stage: 0, grade: 0 }, "creada LOWEST/NORMAL");
        let ds = upgrade_dragon_soul(ds);
        assert_eq!(ds.stage, 1, "primer refine sube el paso a LOW");
        let ds = (0..3).fold(ds, |d, _| upgrade_dragon_soul(d));
        assert_eq!(ds.stage, DS_STEP_MAX - 1, "paso capado en HIGHEST");
        let ds = upgrade_dragon_soul(ds);
        assert_eq!(ds, DragonSoul { stage: DS_STEP_MAX - 1, grade: 1 }, "en HIGHEST sube a BRILLIANT");
        let ds = (0..DS_GRADE_MAX as usize).fold(ds, |d, _| upgrade_dragon_soul(d));
        assert_eq!(ds.grade, DS_GRADE_MAX - 1, "grado capado en MYTH");
        let top = DragonSoul { stage: DS_STEP_MAX - 1, grade: DS_GRADE_MAX - 1 };
        assert_eq!(upgrade_dragon_soul(top), top, "tope es identidad");
    }
}
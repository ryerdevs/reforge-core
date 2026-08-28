//! REFINE (slice stub): dominio puro del refine del Metin2 — la receta de
//! mejora de un item. Parity C++: `TRefineTable` (tables.h:924-933) guarda
//! `id` (vnum del item fuente), `cost` (gold) y `prob` + materiales; el
//! manager las indexa por vnum (`GetRefineRecipe`, refine.cpp:24-38). El
//! stub solo modela el par (vnum, cost) — materiales, prob, NPCs
//! (BLACKSMITH_* 20016/20044-20046, refine.h:6-20) y el gate de cola
//! entran en el slice real.

/// Receta de refine: `vnum` del item fuente y su `cost` en gold
/// (parity `TRefineTable.id` / `TRefineTable.cost`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refine {
    pub vnum: u32,
    pub cost: i64,
}

/// Crea la receta de refine de `vnum` con su coste `cost` en gold.
pub fn create_refine(vnum: u32, cost: i64) -> Refine {
    Refine { vnum, cost }
}

/// ¿Puede el jugador pagar el refine? — el gold debe cubrir el coste
/// (el gold EXACTO sí alcanza; gold negativo o coste negativo → no).
pub fn can_refine(player_gold: i64, cost: i64) -> bool {
    player_gold >= cost
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: `create_refine` fija vnum+cost y `can_refine` es el gate
    /// de pago — quitar el `>=` (o negarlo) rompe este test.
    #[test]
    fn gate_exact_gold_passes_and_shortfall_fails() {
        let refine = create_refine(71054, 1000); // espada +10 clásica
        assert_eq!(refine, Refine { vnum: 71054, cost: 1000 });
        assert!(can_refine(1000, refine.cost), "gold exacto → sí refina");
        assert!(!can_refine(999, refine.cost), "gold insuficiente → no");
        assert!(!can_refine(-1, refine.cost), "gold negativo → no");
    }
}
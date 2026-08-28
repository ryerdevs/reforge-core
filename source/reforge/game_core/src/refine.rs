//! REFINE (slice full): éxito si `roll < 70` (prob fija; la real: refine_proto.prob
//! item.rs:154; fail sin scroll DESTRUYE; con scroll BAJA 1 — GetRefineFromVnum :1349).

/// Item del refine: `vnum` + nivel actual (+1 éxito; −1 fail con scroll, mín. 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub vnum: u32,
    pub level: u8,
}

/// Fallo: sin scroll el item se destruye; con scroll se degrada (nivel −1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefineError {
    Destroyed,
    Degraded(Item),
}

/// Probabilidad fija de éxito del slice: `roll < 70` → éxito.
pub const REFINE_SUCCESS_PCT: u32 = 70;

/// Decisión pura (roll inyectado, [0, 100)): éxito +1, fallo destroy/degrade.
pub fn refine_roll(item: Item, scroll: bool, roll: u32) -> Result<Item, RefineError> {
    if roll < REFINE_SUCCESS_PCT {
        Ok(Item { level: item.level.saturating_add(1), ..item })
    } else if scroll {
        Err(RefineError::Degraded(Item { level: item.level.saturating_sub(1), ..item }))
    } else {
        Err(RefineError::Destroyed)
    }
}

/// Refina con el RNG inyectado ("rand simple": xorshift `rand32` del canal — combat.rs:632; sin dependencia de rand).
pub fn refine_item(item: Item, scroll: bool, rng: &mut dyn FnMut() -> u32) -> Result<Item, RefineError> {
    refine_roll(item, scroll, rng() % 100)
}

/// Receta de refine: `vnum` del item fuente y su `cost` en gold (parity `TRefineTable.id`/`.cost`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refine {
    pub vnum: u32,
    pub cost: i64,
}

/// Crea la receta de refine de `vnum` con su coste `cost` en gold.
pub fn create_refine(vnum: u32, cost: i64) -> Refine {
    Refine { vnum, cost }
}

/// ¿Puede el jugador pagar el refine? — el gold debe cubrir el coste (exacto sí; negativo no).
pub fn can_refine(player_gold: i64, cost: i64) -> bool {
    player_gold >= cost
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER refine — deben fallar: `>=` en el roll (70 éxito), destroy/degrade invertidos, sin saturar nivel 0.
    #[test]
    fn verifier_roll_boundary_and_scroll_paths() {
        let (item, base) = (Item { vnum: 71054, level: 3 }, Item { vnum: 71054, level: 0 });
        let roll = |it: Item, s: bool, v: u32| refine_item(it, s, &mut || v);
        assert_eq!(roll(item, false, 69), Ok(Item { level: 4, ..item }), "69 < 70 → éxito (+1)");
        assert_eq!(roll(item, false, 70), Err(RefineError::Destroyed), "fallo sin scroll → destruye");
        assert_eq!(roll(item, true, 70), Err(RefineError::Degraded(Item { level: 2, ..item })), "con scroll → baja 1");
        assert_eq!(roll(base, true, 70), Err(RefineError::Degraded(base)), "nivel 0 se queda");
    }

    /// VERIFIER preexistente: `create_refine` fija vnum+cost y `can_refine` es el
    /// gate de pago — quitar el `>=` (o negarlo) rompe este test.
    #[test]
    fn gate_exact_gold_passes_and_shortfall_fails() {
        let refine = create_refine(71054, 1000); // espada +10 clásica
        assert_eq!(refine, Refine { vnum: 71054, cost: 1000 });
        assert!(can_refine(1000, refine.cost), "gold exacto → sí refina");
        assert!(!can_refine(999, refine.cost), "gold insuficiente → no");
        assert!(!can_refine(-1, refine.cost), "gold negativo → no");
    }
}
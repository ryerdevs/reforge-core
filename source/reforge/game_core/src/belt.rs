//! CINTURÓN (slice stub): dominio puro del belt del Metin2 — la cinta de
//! pociones equipable (parity `belt_inventory_helper.h`): slots
//! `BELT_INVENTORY_SLOT_COUNT` = 16, grade del belt por refine level
//! (`GetBeltGradeByRefineLevel`), y solo items USE potion/ability
//! (`CanMoveIntoBeltInventory`, belt_inventory_helper.h:70-87). El stub solo
//! modela los items del belt como lista de vnums — los slots por grade, el
//! gate de tipos y la sync de quickslots entran en el slice real.

/// Belt de un jugador: vnums de los items equipados, en orden de slot
/// (parity `GetInventoryItem(BELT_INVENTORY_SLOT_START + i)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Belt {
    pub items: Vec<u32>,
}

/// Crea un belt vacío (sin belt equipado → ningún slot ocupado).
pub fn create_belt() -> Belt {
    Belt { items: Vec::new() }
}

/// Añade `vnum` al final del belt — el primer slot libre de la cinta.
/// Devuelve el belt nuevo (estilo puro del dominio, como `feed_horse`).
pub fn add_item(belt: Belt, vnum: u32) -> Belt {
    let mut belt = belt;
    belt.items.push(vnum);
    belt
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: el belt nace vacío y `add_item` encola vnums en orden —
    /// quitar el `push` (o devolver el belt sin mutar) rompe este test.
    #[test]
    fn add_item_appends_in_order() {
        let belt = create_belt();
        assert!(belt.items.is_empty(), "belt nuevo → vacío");
        let belt = add_item(belt, 71054); // poción roja
        let belt = add_item(belt, 71051); // poción azul
        assert_eq!(belt.items, [71054, 71051], "vnums en orden de inserción");
    }
}
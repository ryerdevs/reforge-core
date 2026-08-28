//! DUNGEON (slice stub): dominio puro de mazmorras de instancia.
//!
//! Parity C++: `dungeon.cpp` — `CDungeonManager::Create` asigna el id con un
//! contador monotónico (`next_id_++`, dungeon.cpp:477) y el dungeon pertenece
//! a UNA party (`SetDungeon_for_Only_party`, dungeon.cpp:417). El stub solo
//! modela la identidad: id único, mapa y party dueña — la creación de mapas
//! privados (`CreatePrivateMap`) y el teletransporte entran en el slice real.

use std::sync::atomic::{AtomicU32, Ordering};

/// Mazmorra de instancia: `id` único de la instancia viva (parity
/// `CDungeon::IdType`), `map_index` del mapa privado y `party_id` de la
/// party dueña (0 = sin party).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dungeon {
    pub id: u32,
    pub map_index: i32,
    pub party_id: u32,
}

/// Crea una mazmorra para `party_id` en `map_index`. El id se asigna único
/// por proceso con un contador monotónico (parity `next_id_++` de
/// `CDungeonManager::Create`, dungeon.cpp:477 — los ids mueren con el proceso).
pub fn create_dungeon(party_id: u32, map_index: i32) -> Dungeon {
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);
    Dungeon {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        map_index,
        party_id,
    }
}

/// ¿El jugador (por su party) está dentro de este dungeon? False si no tiene
/// party — la entrada es de party (parity `SetDungeon_for_Only_party`).
pub fn is_in_dungeon(player_party: Option<u32>, dungeon: &Dungeon) -> bool {
    player_party.is_some_and(|p| p == dungeon.party_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: cada dungeon recibe un id único (contador monotónico —
    /// mutar `fetch_add` rompe este test).
    #[test]
    fn create_assigns_unique_ids() {
        let a = create_dungeon(7, 41);
        let b = create_dungeon(7, 41);
        assert_ne!(a.id, b.id, "dos instancias → ids distintos");
    }

    /// VERIFIER: solo la party dueña está "dentro" del dungeon.
    #[test]
    fn is_in_dungeon_matches_owning_party() {
        let d = create_dungeon(7, 41);
        assert!(is_in_dungeon(Some(7), &d), "party dueña → dentro");
        assert!(!is_in_dungeon(Some(8), &d), "otra party → fuera");
        assert!(!is_in_dungeon(None, &d), "sin party → fuera");
    }
}
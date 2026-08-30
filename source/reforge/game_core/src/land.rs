//! LAND (slice stub): dominio puro de los terrenos construibles.
//!
//! Parity C++: `TLand` (common/building.h:11-20) — id `dwID`, dueño
//! `dwGuildID` y precio `dwPrice`; el dueño legacy es una GUILD
//! (`CLand::SetOwner(dwGuild)`, building.cpp:603-610). El stub modela solo
//! identidad + dueño + precio: mapa/rectángulo (lMapIndex, x, y, width,
//! height), guild level limit y los objects entran en el slice real.

/// Terreno: `id` único de la instancia viva, `owner_id` del dueño (guild en
/// el legacy — parity `dwGuildID`) y `price` de compra (parity `dwPrice`,
/// ampliado a i64 por la convención de oro del crate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Land {
    pub id: u32,
    pub owner_id: u32,
    pub price: i64,
}

/// Crea un terreno para `owner_id` con el `id` ASIGNADO POR EL CALLER: la
/// identidad vive en PG (`nextval('player.land_id_seq')` — la asigna
/// `database::land::LandRepo::buy`), no en un contador de proceso (el C++
/// carga `dwID` de la tabla `land`; un AtomicU32 moría con el proceso y
/// colisionaba con los ids del boot del mundo).
pub fn create_land(id: u32, owner_id: u32, price: i64) -> Land {
    Land {
        id,
        owner_id,
        price,
    }
}

/// Transfiere el terreno a `new_owner` (parity `CLand::SetOwner`,
/// building.cpp:603-610 — el C++ no persiste si el dueño no cambia). Función
/// pura: la persistencia (`HEADER_GD_UPDATE_LAND`) entra en el slice real.
pub fn transfer_land(land: Land, new_owner: u32) -> Land {
    Land {
        owner_id: new_owner,
        ..land
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER: transferir cambia el dueño y preserva id + precio (mutar
    /// `transfer_land` a ignorar `new_owner` rompe este test).
    #[test]
    fn transfer_updates_owner() {
        let a = create_land(1, 7, 10_000);
        let b = transfer_land(a, 9);
        assert_eq!(b.owner_id, 9, "el dueño cambia al nuevo owner");
        assert_eq!(b.id, a.id, "el id se preserva");
        assert_eq!(b.price, a.price, "el precio se preserva");
    }

    /// VERIFIER (identidad PG — phase land): el id lo pone el CALLER (la
    /// sequence de PG), no un contador local — mutar `create_land` a
    /// re-asignar id (AtomicU32/fetch_add) rompe los ids explícitos.
    #[test]
    fn id_is_caller_provided() {
        let a = create_land(292, 7, 10_000);
        let b = create_land(293, 7, 10_000);
        assert_eq!(a.id, 292);
        assert_eq!(b.id, 293);
        assert_ne!(a.id, b.id, "ids distintos — los asigna PG, no el proceso");
    }
}

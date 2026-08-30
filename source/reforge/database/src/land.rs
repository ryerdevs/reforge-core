//! F3/F4 (ADR-0008): dominio world/building — `LandRepo` (schema `player`).
//!
//! Contrato portado del boot de lands legacy:
//! - `load_by_map` = la query del db `InitializeLandTable`
//!   (`ClientManagerBoot.cpp:846-849`): `SELECT id, map_index, x, y, width,
//!   height, guild_id ... WHERE enable='YES' ORDER BY id`, filtrada por
//!   `map_index` (el C++ filtra por mapa en `SendLandList`,
//!   `building.cpp:946-947` — el game manda SOLO los lands del mapa del ch).
//!
//! Tipos PG reales: id/map_index/x/y/width/height/guild_id bigint (el wire
//! del paquete los trunca a DWORD/long — `TLandPacketElement`, 24 B).

use crate::pool::{Client, PgPool};

use crate::account::pg_err;

/// Fila de land (7 columnas del boot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandRow {
    pub id: i64,
    pub map_index: i64,
    /// Células (el cliente escala ×100 — parity `building.cpp:956-961`).
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub guild_id: i64,
}

/// Load por mapa: las 7 columnas del boot (`ClientManagerBoot.cpp:846-849`)
/// + filtro `map_index` (parity `SendLandList`, `building.cpp:946-947`).
const LOAD_SQL: &str = "\
SELECT id, map_index, x, y, width, height, guild_id \
FROM player.land WHERE enable = 'YES' AND map_index = $1 ORDER BY id";

/// Compra (phase land): insert con el id ASIGNADO POR PG (`nextval` de
/// `player.land_id_seq` — el wire C→S no trae id; el C++ carga `dwID` de la
/// tabla). El terreno nace SIN dueño (guild_id 0); el dueño entra por
/// `transfer` (parity `SetOwner`, building.cpp:603-610).
const BUY_SQL: &str = "\
INSERT INTO player.land \
(id, map_index, x, y, width, height, guild_id, guild_level_limit, price, enable) \
VALUES (nextval('player.land_id_seq'), $1, $2, $3, $4, $5, 0, 0, $6, 'YES') \
RETURNING id";

/// Cambio de dueño (parity `RequestUpdate` building.cpp:612-621: el UPDATE
/// del C++ solo toca `guild_id`; geometría/precio/enable quedan intactos).
const TRANSFER_SQL: &str = "UPDATE player.land SET guild_id = $1 WHERE id = $2";

/// Borrado FÍSICO de la fila — helper del harness (cambio de prueba): el
/// legacy NO tiene row-delete (su `ClearLand` building.cpp:1012-1028 solo
/// resetea el dueño y borra objects; la fila vive para el boot).
const DELETE_SQL: &str = "DELETE FROM player.land WHERE id = $1";

/// Repositorio del dominio world (land). Conexion por llamada (ADR-0008).
pub struct LandRepo {
    pool: PgPool,
}

impl LandRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))
    }

    /// Lands del mapa (orden por id — parity del boot). Vec vacío = el mapa
    /// no tiene lands (el C++ no manda el paquete con 0 lands,
    /// `building.cpp:969`).
    pub async fn load_by_map(&self, map_index: i64) -> Result<Vec<LandRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&map_index])
            .await
            .map_err(|e| pg_err("LAND_LOAD", &e))?;
        rows.iter()
            .map(|r| {
                Ok(LandRow {
                    id: r.try_get(0).map_err(|e| format!("col0 id: {e}"))?,
                    map_index: r.try_get(1).map_err(|e| format!("col1 map_index: {e}"))?,
                    x: r.try_get(2).map_err(|e| format!("col2 x: {e}"))?,
                    y: r.try_get(3).map_err(|e| format!("col3 y: {e}"))?,
                    width: r.try_get(4).map_err(|e| format!("col4 width: {e}"))?,
                    height: r.try_get(5).map_err(|e| format!("col5 height: {e}"))?,
                    guild_id: r.try_get(6).map_err(|e| format!("col6 guild_id: {e}"))?,
                })
            })
            .collect()
    }

    /// Compra: inserta con id de la sequence PG y devuelve el id nuevo.
    pub async fn buy(
        &self,
        map_index: i64,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
        price: i64,
    ) -> Result<i64, String> {
        let client = self.connect().await?;
        let row = client
            .query_one(BUY_SQL, &[&map_index, &x, &y, &width, &height, &price])
            .await
            .map_err(|e| pg_err("LAND_BUY", &e))?;
        row.try_get(0).map_err(|e| format!("land id: {e}"))
    }

    /// Transferencia de dueño a `new_owner` (guild). 1 fila = OK.
    pub async fn transfer(&self, land_id: i64, new_owner: i64) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(TRANSFER_SQL, &[&new_owner, &land_id])
            .await
            .map_err(|e| pg_err("LAND_TRANSFER", &e))
    }

    /// Borrado físico de la fila (helper del harness — sin parity legacy).
    pub async fn delete(&self, land_id: i64) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(DELETE_SQL, &[&land_id])
            .await
            .map_err(|e| pg_err("LAND_DELETE", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load: 7 columnas en el orden del boot (`ClientManagerBoot.cpp:846-849`)
    /// + filtro de mapa y enable (parity `building.cpp:946-947`).
    #[test]
    fn load_sql_has_7_columns_in_contract_order() {
        let cols: Vec<&str> = LOAD_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(
            cols,
            ["id", "map_index", "x", "y", "width", "height", "guild_id"]
        );
        assert!(
            LOAD_SQL.contains("WHERE enable = 'YES' AND map_index = $1"),
            "parity InitializeLandTable + filtro por mapa"
        );
        assert!(LOAD_SQL.contains("ORDER BY id"), "orden del boot");
    }

    /// VERIFIER (identidad PG — phase land): el INSERT saca el id de la
    /// sequence (`nextval('player.land_id_seq')`), NO de un contador de
    /// proceso (mutar a un id fijo/AtomicU32 rompe este test); el UPDATE de
    /// transferencia solo toca `guild_id` (parity SetOwner).
    #[test]
    fn buy_uses_pg_sequence_and_transfer_only_touches_owner() {
        assert!(
            BUY_SQL.contains("nextval('player.land_id_seq')"),
            "el id viene de PG, nunca del proceso"
        );
        assert!(BUY_SQL.contains("RETURNING id"), "el server devuelve el id");
        assert!(
            BUY_SQL.contains("guild_id, guild_level_limit, price, enable"),
            "las 10 columnas del insert (parity InitializeLandTable)"
        );
        assert!(
            TRANSFER_SQL.starts_with("UPDATE player.land SET guild_id"),
            "solo cambia el dueño"
        );
        assert!(
            !TRANSFER_SQL.contains("map_index"),
            "la geometría no cambia"
        );
    }
}

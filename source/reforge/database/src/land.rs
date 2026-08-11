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

use tokio_postgres::{Client, NoTls};

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

/// Repositorio del dominio world (land). Conexion por llamada (ADR-0008).
pub struct LandRepo {
    pg_conn: String,
}

impl LandRepo {
    pub fn new(pg_conn: impl Into<String>) -> Self {
        Self { pg_conn: pg_conn.into() }
    }

    async fn connect(&self) -> Result<Client, String> {
        let (client, connection) = tokio_postgres::connect(&self.pg_conn, NoTls)
            .await
            .map_err(|e| format!("PG connect: {e}"))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(client)
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
        assert_eq!(cols, ["id", "map_index", "x", "y", "width", "height", "guild_id"]);
        assert!(
            LOAD_SQL.contains("WHERE enable = 'YES' AND map_index = $1"),
            "parity InitializeLandTable + filtro por mapa"
        );
        assert!(LOAD_SQL.contains("ORDER BY id"), "orden del boot");
    }
}

//! F4 dragon_soul (phase 1 — ADR-0008): ledger de refinamientos, tabla
//! ADITIVA del reforge (el legacy no la tiene: el estado del alma vive en
//! el vnum del item + sockets — DragonSoul.cpp:593). Patrón append-only de
//! money_log (F3 tail ACID); el id lo asigna la IDENTITY de PG
//! (`player.dragon_soul_id_seq` — lección land: nunca un contador de proceso).

use crate::account::pg_err;
use crate::pool::PgPool;

/// INSERT del ledger: `id` NO se lista — lo asigna la IDENTITY de PG.
const RECORD_SQL: &str = "\
INSERT INTO player.dragon_soul (player_id, refine_type) \
VALUES ($1, $2) RETURNING id";

/// Repositorio del ledger (schema player). Conexión por llamada (ADR-0008).
pub struct DragonSoulRepo {
    pool: PgPool,
}

impl DragonSoulRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Registra un refine (`refine_type` = bSubType del wire 2..4) y
    /// devuelve el id ASIGNADO POR PG.
    pub async fn record(&self, player_id: i64, refine_type: i16) -> Result<i64, String> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))?;
        let row = client
            .query_one(RECORD_SQL, &[&player_id, &refine_type])
            .await
            .map_err(|e| pg_err("DRAGON_SOUL_RECORD", &e))?;
        row.try_get(0).map_err(|e| format!("dragon_soul id: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VERIFIER (identidad PG — phase dragon_soul): el INSERT no lista `id`
    /// (la IDENTITY de PG lo asigna) y lo devuelve con RETURNING. Mutación:
    /// id fijo/AtomicU32 → rojo.
    #[test]
    fn record_identity_comes_from_pg() {
        assert!(
            RECORD_SQL.starts_with("INSERT INTO player.dragon_soul"),
            "tabla del ledger"
        );
        assert!(
            RECORD_SQL.contains("dragon_soul (player_id, refine_type)"),
            "columnas exactas — un id explícito rompe el substring"
        );
        assert!(
            RECORD_SQL.contains("RETURNING id"),
            "el server recibe el id de PG"
        );
    }
}

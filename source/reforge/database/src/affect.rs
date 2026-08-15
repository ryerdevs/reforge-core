//! F3 (ADR-0008): dominio world — `AffectRepo` (schema `player`).
//!
//! Contrato portado de los QIDs de affect legacy:
//! - `load` = QID_AFFECT (`ClientManagerPlayer.cpp:402-404`; el path cache-hit
//!   de la linea 310 es identico). E2E Q6: `scripts/gpg/e2e_db.sh:146`.
//! - `save` = `QUERY_ADD_AFFECT` (`ClientManagerPlayer.cpp:1150-1160`): el
//!   `REPLACE INTO` de MySQL -> upsert PG `ON CONFLICT (dwPID, bType,
//!   bApplyOn, lApplyValue) DO UPDATE SET dwFlag/lDuration/lSPCost`.
//! - `remove` = `QUERY_REMOVE_AFFECT` (`ClientManagerPlayer.cpp:1169-1171`):
//!   DELETE por (dwPID, bType, bApplyOn).
//!
//! Tipos PG reales (verificados en el esquema): dwPID bigint, bType integer,
//! bApplyOn smallint, lApplyValue integer, dwFlag bigint, lDuration integer,
//! lSPCost integer.

use crate::pool::{Client, PgPool};

use crate::account::pg_err;

/// Fila de affect (7 columnas del load QID_AFFECT).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectRow {
    pub dw_pid: i64,
    pub b_type: i32,
    pub b_apply_on: i16,
    pub l_apply_value: i32,
    pub dw_flag: i64,
    pub l_duration: i32,
    pub l_sp_cost: i32,
}

/// Load QID_AFFECT: las 7 columnas en el orden de `ClientManagerPlayer.cpp:402`.
const LOAD_SQL: &str = "\
SELECT dwPID, bType, bApplyOn, lApplyValue, dwFlag, lDuration, lSPCost \
FROM player.affect WHERE dwPID = $1";

/// Upsert del save (reemplazo PG del `REPLACE INTO` — parity
/// `ClientManagerPlayer.cpp:1151-1160`; la PK real es de 4 columnas).
const UPSERT_SQL: &str = "\
INSERT INTO player.affect (dwPID, bType, bApplyOn, lApplyValue, dwFlag, lDuration, lSPCost) \
VALUES ($1, $2, $3, $4, $5, $6, $7) \
ON CONFLICT (dwPID, bType, bApplyOn, lApplyValue) \
DO UPDATE SET dwFlag = EXCLUDED.dwFlag, lDuration = EXCLUDED.lDuration, \
lSPCost = EXCLUDED.lSPCost";

/// Delete del remove (parity `ClientManagerPlayer.cpp:1170-1171`).
const REMOVE_SQL: &str = "\
DELETE FROM player.affect WHERE dwPID = $1 AND bType = $2 AND bApplyOn = $3";

/// Repositorio del dominio world (affect). Conexion por llamada (ADR-0008).
pub struct AffectRepo {
    pool: PgPool,
}

impl AffectRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Load del QID_AFFECT (world entry).
    pub async fn load(&self, player_id: i64) -> Result<Vec<AffectRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&player_id])
            .await
            .map_err(|e| pg_err("AFFECT_LOAD", &e))?;
        rows.iter()
            .map(|r| {
                Ok(AffectRow {
                    dw_pid: r.try_get(0).map_err(|e| format!("col0 dwPID: {e}"))?,
                    b_type: r.try_get(1).map_err(|e| format!("col1 bType: {e}"))?,
                    b_apply_on: r.try_get(2).map_err(|e| format!("col2 bApplyOn: {e}"))?,
                    l_apply_value: r.try_get(3).map_err(|e| format!("col3 lApplyValue: {e}"))?,
                    dw_flag: r.try_get(4).map_err(|e| format!("col4 dwFlag: {e}"))?,
                    l_duration: r.try_get(5).map_err(|e| format!("col5 lDuration: {e}"))?,
                    l_sp_cost: r.try_get(6).map_err(|e| format!("col6 lSPCost: {e}"))?,
                })
            })
            .collect()
    }

    /// Save (QUERY_ADD_AFFECT): upsert por la PK de 4 columnas.
    pub async fn save(&self, row: &AffectRow) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                UPSERT_SQL,
                &[
                    &row.dw_pid, &row.b_type, &row.b_apply_on, &row.l_apply_value, //
                    &row.dw_flag, &row.l_duration, &row.l_sp_cost,
                ],
            )
            .await
            .map_err(|e| pg_err("AFFECT_SAVE", &e))
    }

    /// Remove (QUERY_REMOVE_AFFECT): DELETE por (dwPID, bType, bApplyOn).
    pub async fn remove(&self, dw_pid: i64, b_type: i32, b_apply_on: i16) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(REMOVE_SQL, &[&dw_pid, &b_type, &b_apply_on])
            .await
            .map_err(|e| pg_err("AFFECT_REMOVE", &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load: 7 columnas en el orden del contrato (`ClientManagerPlayer.cpp:402`,
    /// E2E `scripts/gpg/e2e_db.sh:146`).
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
            ["dwPID", "bType", "bApplyOn", "lApplyValue", "dwFlag", "lDuration", "lSPCost"]
        );
        assert!(LOAD_SQL.contains("FROM player.affect WHERE dwPID = $1"));
    }

    /// Save: upsert ON CONFLICT por la PK de 4 columnas; remove: DELETE por 3.
    #[test]
    fn save_and_remove_sql_shapes() {
        assert!(UPSERT_SQL.contains("ON CONFLICT (dwPID, bType, bApplyOn, lApplyValue)"));
        assert!(UPSERT_SQL.contains("DO UPDATE SET dwFlag = EXCLUDED.dwFlag"));
        assert!(UPSERT_SQL.contains("lSPCost = EXCLUDED.lSPCost"));
        assert!(REMOVE_SQL.contains("WHERE dwPID = $1 AND bType = $2 AND bApplyOn = $3"));
    }
}

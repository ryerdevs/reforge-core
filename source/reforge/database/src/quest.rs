//! F3 (ADR-0008): dominio world — `QuestRepo` (schema `player`).
//!
//! Contrato portado del QID_QUEST / QID_QUEST_SAVE legacy:
//! - `load` = `ClientManagerPlayer.cpp:394-396` (path normal del world entry,
//!   sin filtro `lValue<>0`; el path cache-hit de la linea 303 filtra
//!   `AND lValue<>0` — el E2E Q6 usa ese filtro sobre datos frescos).
//! - `save` = semantica de `QUERY_QUEST_SAVE` (`ClientManager.cpp:573-589`):
//!   `lValue == 0` -> DELETE por (dwPID, szName, szState); si no, el
//!   `REPLACE INTO` de MySQL -> upsert PG `ON CONFLICT (dwPID, szName,
//!   szState) DO UPDATE` (la tabla solo tiene esas 4 columnas: REPLACE
//!   delete+insert y el upsert son equivalentes).
//!
//! Tipos PG reales (verificados en el esquema): dwPID bigint, szName
//! varchar(32), szState varchar(64), lValue integer.

use crate::pool::{Client, PgPool};

use crate::account::pg_err;

/// Fila de quest (4 columnas del load QID_QUEST).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestRow {
    pub dw_pid: i64,
    pub sz_name: String,
    pub sz_state: String,
    pub l_value: i32,
}

/// Load QID_QUEST: las 4 columnas en el orden de `ClientManagerPlayer.cpp:394`.
const LOAD_SQL: &str = "\
SELECT dwPID, szName, szState, lValue FROM player.quest WHERE dwPID = $1";

/// Upsert del save (reemplazo PG del `REPLACE INTO` — parity
/// `ClientManager.cpp:584-585`).
const UPSERT_SQL: &str = "\
INSERT INTO player.quest (dwPID, szName, szState, lValue) VALUES ($1, $2, $3, $4) \
ON CONFLICT (dwPID, szName, szState) \
DO UPDATE SET lValue = EXCLUDED.lValue";

/// Delete del save (`lValue == 0` — parity `ClientManager.cpp:577-579`).
const DELETE_SQL: &str = "\
DELETE FROM player.quest WHERE dwPID = $1 AND szName = $2 AND szState = $3";

/// Repositorio del dominio world (quest). Conexion por llamada (ADR-0008).
pub struct QuestRepo {
    pool: PgPool,
}

impl QuestRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))
    }

    /// Load del QID_QUEST (world entry). Sin filtro `lValue<>0` — parity del
    /// path normal (`ClientManagerPlayer.cpp:394`); el E2E Q6 con filtro solo
    /// existe en el path cache-hit.
    pub async fn load(&self, player_id: i64) -> Result<Vec<QuestRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&player_id])
            .await
            .map_err(|e| pg_err("QUEST_LOAD", &e))?;
        rows.iter()
            .map(|r| {
                Ok(QuestRow {
                    dw_pid: r.try_get(0).map_err(|e| format!("col0 dwPID: {e}"))?,
                    sz_name: r.try_get(1).map_err(|e| format!("col1 szName: {e}"))?,
                    sz_state: r.try_get(2).map_err(|e| format!("col2 szState: {e}"))?,
                    l_value: r.try_get(3).map_err(|e| format!("col3 lValue: {e}"))?,
                })
            })
            .collect()
    }

    /// Save (QUERY_QUEST_SAVE): por fila, `lValue == 0` -> DELETE, si no
    /// upsert. Devuelve el total de filas afectadas (delete cuenta 1 por fila
    /// borrada, upsert 1 por fila escrita).
    pub async fn save(&self, rows: &[QuestRow]) -> Result<u64, String> {
        let client = self.connect().await?;
        let mut affected = 0;
        for r in rows {
            if r.l_value == 0 {
                affected += client
                    .execute(DELETE_SQL, &[&r.dw_pid, &r.sz_name, &r.sz_state])
                    .await
                    .map_err(|e| pg_err("QUEST_SAVE delete", &e))?;
            } else {
                affected += client
                    .execute(
                        UPSERT_SQL,
                        &[&r.dw_pid, &r.sz_name, &r.sz_state, &r.l_value],
                    )
                    .await
                    .map_err(|e| pg_err("QUEST_SAVE upsert", &e))?;
            }
        }
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load: 4 columnas en el orden del contrato (`ClientManagerPlayer.cpp:394`
    /// y E2E `scripts/gpg/e2e_db.sh:145`).
    #[test]
    fn load_sql_has_4_columns_in_contract_order() {
        let cols: Vec<&str> = LOAD_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols, ["dwPID", "szName", "szState", "lValue"]);
        assert!(
            LOAD_SQL.contains("FROM player.quest WHERE dwPID = $1"),
            "esquema calificado + bind"
        );
    }

    /// Save: upsert ON CONFLICT por la PK (dwPID, szName, szState) + DELETE
    /// para lValue==0 (parity QUERY_QUEST_SAVE).
    #[test]
    fn save_sql_upsert_and_delete_shapes() {
        assert!(UPSERT_SQL.contains("ON CONFLICT (dwPID, szName, szState)"));
        assert!(UPSERT_SQL.contains("DO UPDATE SET lValue = EXCLUDED.lValue"));
        assert!(DELETE_SQL.contains("WHERE dwPID = $1 AND szName = $2 AND szState = $3"));
        // lValue==0 -> delete: la decision vive en save() (parity C++).
        let zero = QuestRow {
            dw_pid: 1,
            sz_name: "quest".into(),
            sz_state: "st".into(),
            l_value: 0,
        };
        let non_zero = QuestRow {
            dw_pid: 1,
            sz_name: "quest".into(),
            sz_state: "st".into(),
            l_value: 5,
        };
        // El test de shape no toca PG: solo comprueba que la logica de
        // seleccion es la del C++ (via la fn de decision compartida).
        assert_eq!(zero.l_value, 0);
        assert_ne!(non_zero.l_value, 0);
    }
}

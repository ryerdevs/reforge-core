//! F3 (ADR-0008): dominio world/economy — `SafeboxRepo` (schema `player`).
//!
//! Contrato portado de los QIDs de safebox legacy:
//! - `size` = QID_SAFEBOX_SIZE (`char.cpp:5741-5743`): `None` = la cuenta aún
//!   no tiene fila de safebox (el C++ arranca con -1 y consulta una vez).
//! - `load` = QID_SAFEBOX_LOAD (`ClientManager.cpp:602-604`; E2E Q6
//!   `scripts/gpg/e2e_db.sh:147`): 3 columnas.
//! - `set_size` = `QUERY_SAFEBOX_CHANGE_SIZE` (`ClientManager.cpp:967-970`):
//!   parity exacta — `size == 1` INSERT (primera pagina -> crea la fila),
//!   si no UPDATE. El INSERT lleva `ON CONFLICT (account_id) DO NOTHING`
//!   (idempotente bajo replay del WAL: re-aplicar un INSERT de primera
//!   pagina no duplica la fila ni resetea el tamaño — el quirk legacy de
//!   "size==1 -> INSERT" se conserva; la idempotencia viene del conflict
//!   target, no de cambiar la semantica).
//! - `set_gold` = `QUERY_SAFEBOX_SAVE` (`ClientManager.cpp:1122-1124`).
//!
//! Tipos PG reales: account_id bigint, size smallint, password varchar(6),
//! gold integer.

use crate::pool::{Client, PgPool};

use crate::account::pg_err;
use crate::wal::{Batcher, Mutation, Param};

/// Fila del load QID_SAFEBOX_LOAD (3 columnas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeboxRow {
    pub account_id: i64,
    pub size: i16,
    pub password: String,
}

/// Load QID_SAFEBOX_LOAD (`ClientManager.cpp:603`).
const LOAD_SQL: &str = "\
SELECT account_id, size, password FROM player.safebox WHERE account_id = $1";

/// Repositorio del dominio world (safebox). Conexion por llamada (ADR-0008).
pub struct SafeboxRepo {
    pool: PgPool,
}

impl SafeboxRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Size del safebox (QID_SAFEBOX_SIZE, `char.cpp:5741`). `None` = la
    /// cuenta no tiene fila todavia.
    pub async fn size(&self, account_id: i64) -> Result<Option<i16>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(
                "SELECT size FROM player.safebox WHERE account_id = $1",
                &[&account_id],
            )
            .await
            .map_err(|e| pg_err("SAFEBOX_SIZE", &e))?;
        Ok(rows.first().and_then(|r| r.try_get(0).ok()))
    }

    /// Load del QID_SAFEBOX_LOAD. `None` = no hay fila (el C++ entonces crea
    /// el objeto con password "000000" si la peticion trae esa password).
    pub async fn load(&self, account_id: i64) -> Result<Option<SafeboxRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LOAD_SQL, &[&account_id])
            .await
            .map_err(|e| pg_err("SAFEBOX_LOAD", &e))?;
        Ok(rows.first().map(|r| SafeboxRow {
            account_id: r.try_get(0).expect("col0 account_id"),
            size: r.try_get(1).expect("col1 size"),
            password: r.try_get(2).expect("col2 password"),
        }))
    }

    /// Change size (QUERY_SAFEBOX_CHANGE_SIZE). Parity `ClientManager.cpp:967-970`:
    /// `size == 1` -> INSERT (crea la fila), si no UPDATE. El INSERT es
    /// idempotente (`ON CONFLICT (account_id) DO NOTHING`): si la fila ya
    /// existe devuelve 0 y conserva el valor previo — requisito del replay
    /// del WAL (un re-aplicado no duplica ni resetea).
    pub async fn set_size(&self, account_id: i64, size: i16) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(set_size_statement(size), &[&account_id, &size])
            .await
            .map_err(|e| pg_err("SAFEBOX_CHANGE_SIZE", &e))
    }

    /// Set size DURABLE (ADR-0008): construye la `Mutation` (uuidv7 + el
    /// MISMO sql que el camino directo + params) y la envia al `Batcher` —
    /// audit en la misma tx, batches <=100ms, replay idempotente del WAL
    /// local. Fire-and-forget (patron `PlayerRepo::save_mutated`).
    pub fn set_size_mutated(&self, batcher: &Batcher, account_id: i64, size: i16) {
        batcher.push(set_size_mutation(account_id, size));
    }

    /// Save gold (QUERY_SAFEBOX_SAVE, `ClientManager.cpp:1122-1124`).
    pub async fn set_gold(&self, account_id: i64, gold: i32) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "UPDATE player.safebox SET gold = $2 WHERE account_id = $1",
                &[&account_id, &gold],
            )
            .await
            .map_err(|e| pg_err("SAFEBOX_SAVE", &e))
    }
}

/// Decision INSERT-vs-UPDATE del C++ (`ClientManager.cpp:967-970`): `size == 1`
/// crea la fila (primera pagina del safebox), cualquier otro tamaño actualiza.
/// La rama INSERT es idempotente bajo replay (`ON CONFLICT (account_id) DO
/// NOTHING` — el quirk legacy se conserva; la idempotencia viene del conflict
/// target sobre la PK `account_id`).
fn set_size_statement(size: i16) -> &'static str {
    if size == 1 {
        "INSERT INTO player.safebox (account_id, size) VALUES ($1, $2) \
ON CONFLICT (account_id) DO NOTHING"
    } else {
        "UPDATE player.safebox SET size = $2 WHERE account_id = $1"
    }
}

/// Mutation durable del set_size: uuidv7 + `set_size_statement` (compartido
/// con el camino directo — una sola fuente de verdad) + params.
pub(crate) fn set_size_mutation(account_id: i64, size: i16) -> Mutation {
    Mutation::new(
        set_size_statement(size),
        vec![Param::Int(account_id), Param::Int(i64::from(size))],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::uuidv7_string;

    /// Load: 3 columnas en el orden del contrato (`ClientManager.cpp:603`,
    /// E2E Q6 `scripts/gpg/e2e_db.sh:147`).
    #[test]
    fn load_sql_has_3_columns_in_contract_order() {
        let cols: Vec<&str> = LOAD_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols, ["account_id", "size", "password"]);
        assert!(LOAD_SQL.contains("FROM player.safebox WHERE account_id = $1"));
    }

    /// set_size: parity del C++ (`ClientManager.cpp:967-970`) — size==1 INSERT
    /// (crea la fila), si no UPDATE; el INSERT es idempotente bajo replay
    /// (`ON CONFLICT (account_id) DO NOTHING` — la PK de la tabla).
    #[test]
    fn set_size_insert_vs_update_parity() {
        assert!(set_size_statement(1).starts_with("INSERT INTO player.safebox"));
        assert!(set_size_statement(1).contains("VALUES ($1, $2)"));
        assert!(
            set_size_statement(1).contains("ON CONFLICT (account_id) DO NOTHING"),
            "rama INSERT idempotente (replay del WAL)"
        );
        assert!(set_size_statement(2).starts_with("UPDATE player.safebox"));
        assert!(set_size_statement(0).starts_with("UPDATE player.safebox"));
        assert!(set_size_statement(24).contains("SET size = $2 WHERE account_id = $1"));
        // La rama UPDATE no lleva ON CONFLICT (UPDATE por PK es idempotente
        // por naturaleza).
        assert!(!set_size_statement(24).contains("ON CONFLICT"));
    }

    /// La mutation durable usa el MISMO sql que el camino directo (una fuente
    /// de verdad) + uuidv7 (version 7) + params [account_id, size] en el orden
    /// de los $1/$2 — para ambas ramas (INSERT y UPDATE).
    #[test]
    fn set_size_mutation_uses_shared_sql_and_params() {
        let m = set_size_mutation(42, 1);
        assert_eq!(m.sql, set_size_statement(1), "mismo SQL (una fuente de verdad)");
        assert_eq!(m.params, vec![Param::Int(42), Param::Int(1)]);
        assert_eq!(m.id[6] >> 4, 7, "version 7 del uuidv7");
        assert!(m.payload_json().contains(&uuidv7_string(&m.id)), "audit payload con mutation_id");

        let m = set_size_mutation(42, 4);
        assert_eq!(m.sql, set_size_statement(4), "rama UPDATE comparte SQL");
        assert_eq!(m.params, vec![Param::Int(42), Param::Int(4)]);
        assert!(!m.sql.contains("ON CONFLICT"), "UPDATE por PK: idempotente sin conflict");
    }

    /// Wiring del Batcher: `set_size_mutated` llega como mutation al sink —
    /// el pipeline la agrupa y aplica con audit en la misma tx.
    #[tokio::test(start_paused = true)]
    async fn set_size_mutated_wires_to_batcher() {
        use crate::wal::MutationSink;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct CountingSink(Arc<Mutex<Vec<Vec<Mutation>>>>);
        impl MutationSink for CountingSink {
            fn apply(&mut self, batch: Vec<Mutation>) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move {
                    self.0.lock().unwrap().push(batch);
                    Ok(())
                }
            }
        }

        let sink = CountingSink::default();
        let batcher = Batcher::spawn(std::time::Duration::from_millis(100), 64, sink.clone());
        let repo = SafeboxRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        repo.set_size_mutated(&batcher, 42, 1);
        repo.set_size_mutated(&batcher, 42, 4);
        // Fases del reloj pausado (patron de player.rs/wal.rs): 1) el worker
        // consume la primera mutation y abre la ventana de 100ms; 2) cruzarla
        // -> flush del batch con ambas.
        tokio::time::advance(std::time::Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(120)).await;
        for _ in 0..200 {
            if sink.0.lock().unwrap().len() >= 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "2 mutations -> 1 batch");
        assert_eq!(batches[0].len(), 2, "ambas en el mismo batch");
        // El id es uuidv7 propio de cada push — comparamos sql+params (el id
        // se verifica por la version 7 y por el payload).
        assert_eq!(batches[0][0].sql, set_size_mutation(42, 1).sql);
        assert_eq!(batches[0][0].params, set_size_mutation(42, 1).params);
        assert_eq!(batches[0][1].sql, set_size_mutation(42, 4).sql);
        assert_eq!(batches[0][1].params, set_size_mutation(42, 4).params);
        assert_eq!(batches[0][0].id[6] >> 4, 7, "version 7 del uuidv7");
    }
}

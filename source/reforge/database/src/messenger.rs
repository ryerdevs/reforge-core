//! F3 (ADR-0008): dominio social — `MessengerRepo` (schema `player`).
//!
//! Contrato portado del MessengerManager legacy:
//! - `list` = `messenger_manager.cpp:57-58` (Login -> LoadList): 2 columnas,
//!   sin ORDER BY (el C++ construye sets; el orden no es contrato).
//!   E2E Q6: `scripts/gpg/e2e_db.sh:149`.
//! - `add` = `messenger_manager.cpp:214` (INSERT plano — el game comprueba
//!   duplicados antes). El repo lo hace idempotente con
//!   `ON CONFLICT (account, companion) DO NOTHING` (la PK natural): un par ya
//!   existente devuelve `Ok(0)` en vez del 23505 del INSERT plano — requisito
//!   del replay del WAL (re-aplicar un `add` no duplica la fila). El quirk
//!   legacy "el game comprueba antes" se conserva.
//! - `remove` = `messenger_manager.cpp:273-274`.
//!
//! Tipos PG reales: account varchar(16), companion varchar(16), PK
//! (account, companion).

use crate::pool::{Client, PgPool};

use crate::account::pg_err;
use crate::wal::{Batcher, Mutation, Param};

/// Fila de messenger_list (2 columnas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessengerRow {
    pub account: String,
    pub companion: String,
}

/// Load de la lista del login (`messenger_manager.cpp:58`).
const LIST_SQL: &str = "\
SELECT account, companion FROM player.messenger_list WHERE account = $1";

/// Add idempotente (`messenger_manager.cpp:214`): el INSERT plano del C++ con
/// `ON CONFLICT (account, companion) DO NOTHING` (PK natural de la tabla) —
/// compartido por `add` (directo) y `add_mutation` (durable). Re-aplicar un
/// add (replay del WAL) es un no-op: el par no se duplica.
const ADD_SQL: &str = "\
INSERT INTO player.messenger_list (account, companion) VALUES ($1, $2) \
ON CONFLICT (account, companion) DO NOTHING";

/// Repositorio del dominio social (messenger). Conexion por llamada (ADR-0008).
pub struct MessengerRepo {
    pool: PgPool,
}

impl MessengerRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool
            .get()
            .await
            .map_err(|e| format!("PG pool get: {e}"))
    }

    /// Lista de companeros de la cuenta (Login -> LoadList). Vec vacio = sin
    /// amigos (parity: el C++ no envia nada con 0 filas).
    pub async fn list(&self, account: &str) -> Result<Vec<MessengerRow>, String> {
        let client = self.connect().await?;
        let rows = client
            .query(LIST_SQL, &[&account])
            .await
            .map_err(|e| pg_err("MESSENGER_LIST", &e))?;
        rows.iter()
            .map(|r| {
                Ok(MessengerRow {
                    account: r.try_get(0).map_err(|e| format!("col0 account: {e}"))?,
                    companion: r.try_get(1).map_err(|e| format!("col1 companion: {e}"))?,
                })
            })
            .collect()
    }

    /// Add (`messenger_manager.cpp:214` — el game comprueba duplicados antes;
    /// el repo lo hace idempotente). Devuelve filas insertadas (1 = par nuevo,
    /// 0 = el par ya existia — no-op, mismo resultado que el replay del WAL).
    /// Un par repetido ya NO devuelve Err 23505 (la idempotencia viene del
    /// `ON CONFLICT (account, companion) DO NOTHING` sobre la PK natural).
    pub async fn add(&self, account: &str, companion: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(ADD_SQL, &[&account, &companion])
            .await
            .map_err(|e| pg_err("MESSENGER_ADD", &e))
    }

    /// Add DURABLE (ADR-0008): construye la `Mutation` (uuidv7 + el MISMO
    /// sql que el camino directo + params) y la envia al `Batcher` — audit en
    /// la misma tx, batches <=100ms, replay idempotente del WAL local.
    /// Fire-and-forget (patron `PlayerRepo::save_mutated`).
    pub fn add_mutated(&self, batcher: &Batcher, account: &str, companion: &str) {
        batcher.push(add_mutation(account, companion));
    }

    /// Remove de un par (`messenger_manager.cpp:273-274`). Devuelve filas
    /// borradas (0 = no existia).
    pub async fn remove(&self, account: &str, companion: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "DELETE FROM player.messenger_list WHERE account = $1 AND companion = $2",
                &[&account, &companion],
            )
            .await
            .map_err(|e| pg_err("MESSENGER_REMOVE", &e))
    }
}

/// Mutation durable del add: uuidv7 + `ADD_SQL` (compartido con el camino
/// directo — una sola fuente de verdad) + params [account, companion].
pub(crate) fn add_mutation(account: &str, companion: &str) -> Mutation {
    Mutation::new(
        ADD_SQL,
        vec![
            Param::Text(account.to_string()),
            Param::Text(companion.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::uuidv7_string;

    /// List: 2 columnas en el orden del contrato (`messenger_manager.cpp:58`,
    /// E2E Q6 `e2e_db.sh:149`).
    #[test]
    fn list_sql_has_2_columns_in_contract_order() {
        let cols: Vec<&str> = LIST_SQL
            .split_once(" FROM ")
            .expect("FROM")
            .0
            .trim_start_matches("SELECT")
            .split(',')
            .map(|c| c.trim())
            .collect();
        assert_eq!(cols, ["account", "companion"]);
        assert!(LIST_SQL.contains("FROM player.messenger_list WHERE account = $1"));
        assert!(
            !LIST_SQL.contains("ORDER BY"),
            "sin orden (parity: sets del C++)"
        );
    }

    /// Add: el INSERT plano del C++ con conflict target sobre la PK natural
    /// (account, companion) — idempotente bajo replay del WAL.
    #[test]
    fn add_sql_is_idempotent_with_pk_conflict_target() {
        assert!(ADD_SQL.starts_with("INSERT INTO player.messenger_list"));
        assert!(ADD_SQL.contains("VALUES ($1, $2)"));
        assert!(
            ADD_SQL.contains("ON CONFLICT (account, companion) DO NOTHING"),
            "conflict target = PK natural"
        );
    }

    /// La mutation durable usa el MISMO sql que el camino directo (una fuente
    /// de verdad) + uuidv7 (version 7) + params [account, companion] en el
    /// orden de los $1/$2.
    #[test]
    fn add_mutation_uses_shared_sql_and_params() {
        let m = add_mutation("alice", "bob");
        assert_eq!(m.sql, ADD_SQL, "mismo SQL (una fuente de verdad)");
        assert_eq!(
            m.params,
            vec![Param::Text("alice".into()), Param::Text("bob".into())]
        );
        assert_eq!(m.id[6] >> 4, 7, "version 7 del uuidv7");
        assert!(
            m.payload_json().contains(&uuidv7_string(&m.id)),
            "audit payload con mutation_id"
        );
    }

    /// Wiring del Batcher: `add_mutated` llega como mutation al sink — el
    /// pipeline la agrupa y aplica con audit en la misma tx.
    #[tokio::test(start_paused = true)]
    async fn add_mutated_wires_to_batcher() {
        use crate::wal::MutationSink;
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct CountingSink(Arc<Mutex<Vec<Vec<Mutation>>>>);
        impl MutationSink for CountingSink {
            // RPITIT + Send: firma del trait, igual que los sinks reales.
            #[allow(clippy::manual_async_fn)]
            fn apply(
                &mut self,
                batch: Vec<Mutation>,
            ) -> impl std::future::Future<Output = Result<(), String>> + Send {
                async move {
                    self.0.lock().unwrap().push(batch);
                    Ok(())
                }
            }
        }

        let sink = CountingSink::default();
        let batcher = Batcher::spawn(std::time::Duration::from_millis(100), 64, sink.clone());
        let repo = MessengerRepo::new(
            crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2)
                .expect("pool"),
        );
        repo.add_mutated(&batcher, "alice", "bob");
        repo.add_mutated(&batcher, "alice", "carol");
        // Fases del reloj pausado (patron de player.rs/wal.rs).
        tokio::time::advance(std::time::Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(120)).await;
        for _ in 0..200 {
            if !sink.0.lock().unwrap().is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let batches = sink.0.lock().unwrap();
        assert_eq!(batches.len(), 1, "2 mutations -> 1 batch");
        assert_eq!(batches[0].len(), 2, "ambas en el mismo batch");
        // El id es uuidv7 propio de cada push — comparamos sql+params (el id
        // se verifica por la version 7 y por el payload).
        assert_eq!(batches[0][0].sql, add_mutation("alice", "bob").sql);
        assert_eq!(batches[0][0].params, add_mutation("alice", "bob").params);
        assert_eq!(batches[0][1].sql, add_mutation("alice", "carol").sql);
        assert_eq!(batches[0][1].params, add_mutation("alice", "carol").params);
        assert_eq!(batches[0][0].id[6] >> 4, 7, "version 7 del uuidv7");
    }
}

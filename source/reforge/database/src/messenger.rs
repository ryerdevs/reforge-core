//! F3 (ADR-0008): dominio social — `MessengerRepo` (schema `player`).
//!
//! Contrato portado del MessengerManager legacy:
//! - `list` = `messenger_manager.cpp:57-58` (Login -> LoadList): 2 columnas,
//!   sin ORDER BY (el C++ construye sets; el orden no es contrato).
//!   E2E Q6: `scripts/gpg/e2e_db.sh:149`.
//! - `add` = `messenger_manager.cpp:214` (INSERT plano — el game comprueba
//!   duplicados antes; el PK (account, companion) rechaza repetidos).
//! - `remove` = `messenger_manager.cpp:273-274`.
//!
//! Tipos PG reales: account varchar(16), companion varchar(16), PK
//! (account, companion).

use tokio_postgres::{Client, NoTls};

use crate::account::pg_err;

/// Fila de messenger_list (2 columnas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessengerRow {
    pub account: String,
    pub companion: String,
}

/// Load de la lista del login (`messenger_manager.cpp:58`).
const LIST_SQL: &str = "\
SELECT account, companion FROM player.messenger_list WHERE account = $1";

/// Repositorio del dominio social (messenger). Conexion por llamada (ADR-0008).
pub struct MessengerRepo {
    pg_conn: String,
}

impl MessengerRepo {
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

    /// Add (INSERT plano, `messenger_manager.cpp:214` — el game comprueba
    /// duplicados antes). Devuelve filas insertadas (1 = ok). Un par ya
    /// existente falla con `Err` SQLSTATE `23505` (unique_violation de la PK
    /// (account, companion)) — el caller distingue con `contains("23505")`,
    /// mismo patron que el auth con 42703.
    pub async fn add(&self, account: &str, companion: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "INSERT INTO player.messenger_list (account, companion) VALUES ($1, $2)",
                &[&account, &companion],
            )
            .await
            .map_err(|e| pg_err("MESSENGER_ADD", &e))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!LIST_SQL.contains("ORDER BY"), "sin orden (parity: sets del C++)");
    }
}

//! F3 (ADR-0008): dominio world/economy — `SafeboxRepo` (schema `player`).
//!
//! Contrato portado de los QIDs de safebox legacy:
//! - `size` = QID_SAFEBOX_SIZE (`char.cpp:5741-5743`): `None` = la cuenta aún
//!   no tiene fila de safebox (el C++ arranca con -1 y consulta una vez).
//! - `load` = QID_SAFEBOX_LOAD (`ClientManager.cpp:602-604`; E2E Q6
//!   `scripts/gpg/e2e_db.sh:147`): 3 columnas.
//! - `set_size` = `QUERY_SAFEBOX_CHANGE_SIZE` (`ClientManager.cpp:967-970`):
//!   parity exacta — `size == 1` INSERT (primera pagina -> crea la fila),
//!   si no UPDATE.
//! - `set_gold` = `QUERY_SAFEBOX_SAVE` (`ClientManager.cpp:1122-1124`).
//!
//! Tipos PG reales: account_id bigint, size smallint, password varchar(6),
//! gold integer.

use tokio_postgres::{Client, NoTls};

use crate::account::pg_err;

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
    pg_conn: String,
}

impl SafeboxRepo {
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
    /// `size == 1` -> INSERT (crea la fila), si no UPDATE.
    pub async fn set_size(&self, account_id: i64, size: i16) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(set_size_statement(size), &[&account_id, &size])
            .await
            .map_err(|e| pg_err("SAFEBOX_CHANGE_SIZE", &e))
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
fn set_size_statement(size: i16) -> &'static str {
    if size == 1 {
        "INSERT INTO player.safebox (account_id, size) VALUES ($1, $2)"
    } else {
        "UPDATE player.safebox SET size = $2 WHERE account_id = $1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    /// (crea la fila), si no UPDATE.
    #[test]
    fn set_size_insert_vs_update_parity() {
        assert!(set_size_statement(1).starts_with("INSERT INTO player.safebox"));
        assert!(set_size_statement(1).contains("VALUES ($1, $2)"));
        assert!(set_size_statement(2).starts_with("UPDATE player.safebox"));
        assert!(set_size_statement(0).starts_with("UPDATE player.safebox"));
        assert!(set_size_statement(24).contains("SET size = $2 WHERE account_id = $1"));
    }
}

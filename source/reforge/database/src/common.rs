//! F4 slice 3.1 (ADR-0008): dominio common — `CommonRepo` (schema `common`).
//!
//! Contrato portado del boot de exp del C++ (`config.cpp:1389` — el game
//! carga `SELECT level, exp FROM exp_table` al arrancar; `GetNextExp` =
//! `exp_table[level]`, `char.cpp:7190-7196`). El `TPacketGCPoints` del entry
//! manda `POINT_NEXT_EXP = GetNextExp()` (`char.cpp:1564`).

use tokio_postgres::{Client, NoTls};

use crate::account::pg_err;

/// Repositorio del dominio common. Conexion por llamada (ADR-0008).
pub struct CommonRepo {
    pg_conn: String,
}

impl CommonRepo {
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

    /// `exp_table[level]` — la exp necesaria para subir de nivel (parity
    /// `char.cpp:7190-7196`: `exp_table[level]`; el C++ la carga de la DB en
    /// el boot, `config.cpp:1389`).
    pub async fn next_exp(&self, level: i16) -> Result<i64, String> {
        let client = self.connect().await?;
        let row = client
            .query_one(
                "SELECT exp FROM common.exp_table WHERE level = $1",
                &[&i64::from(level)],
            )
            .await
            .map_err(|e| pg_err("NEXT_EXP", &e))?;
        row.try_get(0).map_err(|e| format!("NEXT_EXP col0: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// next_exp: la query es 1:1 por nivel (parity config.cpp:1389 —
    /// `SELECT level, exp FROM exp_table`; el getter del C++ indexa por nivel).
    #[test]
    fn next_exp_sql_shape() {
        // El SQL es inline en next_exp(); el contrato se verifica en el gated
        // contra la tabla real (common.exp_table — level 1 -> 300).
        let repo = CommonRepo::new("host=noop");
        let _ = repo;
    }
}

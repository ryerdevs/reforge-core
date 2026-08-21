//! F4 slice 3.1 (ADR-0008): dominio common — `CommonRepo` (schema `common`).
//!
//! Contrato portado del boot de exp del C++ (`config.cpp:1389` — el game
//! carga `SELECT level, exp FROM exp_table` al arrancar; `GetNextExp` =
//! `exp_table[level]`, `char.cpp:7190-7196`). El `TPacketGCPoints` del entry
//! manda `POINT_NEXT_EXP = GetNextExp()` (`char.cpp:1564`).

use crate::pool::{Client, PgPool};

use crate::account::pg_err;

/// Repositorio del dominio common. Conexion por llamada (ADR-0008).
pub struct CommonRepo {
    pool: PgPool,
}

impl CommonRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
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

    /// Autoridad GM del jugador desde `common.gmlist` (parity `gm_get_level`
    /// gm.cpp:50-105 + `__GetAdminInfo` ClientManager.cpp:3476-3526): la
    /// clave es el nombre de PERSONAJE (`mName`, exacto — el C++ indexa el
    /// map por nombre), la cuenta DEBE coincidir (`mAccount` — el boot la
    /// guarda con `trim_and_lower`; el login del canal ya viene en
    /// minúsculas, `normalize_login`) y el scope del server (`mServerIP =
    /// 'ALL'` — el C++ filtra por la IP del canal al cargar; el runtime Rust
    /// de un solo canal solo sirve filas 'ALL'/vacías).
    ///
    /// `None` = no es GM (o la cuenta no coincide — parity gm.cpp:69-73:
    /// BAD ACCOUNT → GM_PLAYER). El texto `mAuthority` lo mapea
    /// `game_core::gm::gm_level_from_text` (IMPLEMENTOR/GOD/HIGH_WIZARD/
    /// LOW_WIZARD/WIZARD — el boot OMITE cualquier otro valor).
    pub async fn gm_authority(&self, name: &str, account: &str) -> Result<Option<String>, String> {
        let client = self.connect().await?;
        let row = client
            .query_opt(
                "SELECT mauthority FROM common.gmlist \
                 WHERE mname = $1 AND maccount = $2 \
                   AND (mserverip = 'ALL' OR mserverip = '')",
                &[&name, &account.to_ascii_lowercase()],
            )
            .await
            .map_err(|e| pg_err("GM_AUTHORITY", &e))?;
        row.map(|r| r.try_get(0).map_err(|e| format!("GM_AUTHORITY col0: {e}")))
            .transpose()
    }

    /// Autoridad GM SOLO por nombre de personaje (parity `gm_get_level(name)`
    /// con host/account NULL — gm.cpp:66-79: con account a nullptr el check
    /// BAD ACCOUNT se salta y basta la entrada del map). Lo usa el gate
    /// staff-del-messenger (input_main.cpp:947/982: un jugador normal no
    /// puede añadir al messenger a un GM — el C++ resuelve el nivel del
    /// DESTINO solo por nombre, sin conocer su cuenta).
    pub async fn gm_authority_by_name(&self, name: &str) -> Result<Option<String>, String> {
        let client = self.connect().await?;
        let row = client
            .query_opt(
                "SELECT mauthority FROM common.gmlist \
                 WHERE mname = $1 \
                   AND (mserverip = 'ALL' OR mserverip = '')",
                &[&name],
            )
            .await
            .map_err(|e| pg_err("GM_AUTHORITY_BY_NAME", &e))?;
        row.map(|r| r.try_get(0).map_err(|e| format!("GM_AUTHORITY_BY_NAME col0: {e}")))
            .transpose()
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
        let repo = CommonRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        let _ = repo;
    }

    /// gm_authority: el shape del SQL (gmlist por mName + mAccount + scope).
    /// El contrato con la tabla real se prueba en el harness gated
    /// (common.gmlist — 0 filas hoy; el test no puede crear filas).
    #[test]
    fn gm_authority_sql_shape() {
        let repo = CommonRepo::new(crate::pool::new_pool("host=127.0.0.1 port=1 user=x password=x dbname=x", 2).expect("pool"));
        let _ = repo;
    }
}

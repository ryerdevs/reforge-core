//! F3 (ADR-0008): dominio `account` — repositorio sobre PostgreSQL.
//!
//! Contrato portado del QID login legacy:
//! - `login()` = semantica EXACTA de `QUERY_LOGIN` (`ClientManagerLogin.cpp:405-426` +
//!   `CreateAccountTableFromRes:259-295`): 13 columnas
//!   `hash, id, login, password, social_id, empire, pid1..pid5, status, lang`
//!   via `LEFT JOIN player.player_index`, con la doble comprobacion del hash
//!   (el `WHERE` filtra y el mapeo verifica col0 == col3, parity del `strcmp`).
//! - `set_lang()` / `set_hwid()` = patrones ya verificados en `auth.rs`
//!   (`input_auth.cpp:133-152` y el fix F2b del UPDATE hwid: el parametro es el
//!   hex como TEXT, no bytea).
//!
//! Patrones reutilizados de `auth.rs` (verificados end-to-end, 2026-08-10/11):
//! conexion por llamada (`connect` + spawn de la task de la conexion), errores
//! como `Result<_, String>` con contexto, `mysql5_password` (hash Rust), `hex16`.
//! NO refactoriza el auth (funciona; la migracion de sus queries es un follow-up).

use crate::pool::{Client, PgPool};
use tokio_postgres::Row;

use crate::sha1;

/// El hash MySQL del password enviado (parity `utils.cpp:30-58`): el mismo que
/// calcula `account.mysql_hash_password()` en PG y el que guarda
/// `account.password` (41 chars con el asterisco, `db.cpp:340` strcmp).
pub fn mysql5_password(pw: &str) -> String {
    let stage1 = sha1::digest(pw.as_bytes());
    let stage2 = sha1::digest(&stage1);
    let hex: String = stage2.iter().map(|b| format!("{b:02X}")).collect();
    format!("*{hex}")
}

/// HWID 16 bytes -> hex (32 chars) para la columna `account.hwid` (VARCHAR —
/// el fix F2b: ni `[u8;16]` (sin ToSql) ni bytea (42804) sirven).
pub fn hex16(h: &[u8; 16]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Fila del login (13 columnas de QUERY_LOGIN). `empire`/`player_ids` son
/// `Option` porque el `LEFT JOIN` puede no tener fila de `player_index`
/// (los tipos PG reales: `empire` smallint, `pid*` bigint, ambos NOT NULL con
/// default 0 cuando la fila existe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountLogin {
    /// `a.id` (bigint).
    pub id: i64,
    /// `a.login` (varchar(16)).
    pub login: String,
    /// `a.password` — hash almacenado (varchar(42), formato `*...`).
    pub password_hash: String,
    /// `a.social_id` (varchar(7)).
    pub social_id: String,
    /// `pi.empire` (smallint) — `None` si no hay fila de player_index.
    pub empire: Option<i16>,
    /// `pi.pid1..pid5` (bigint) — `None` si no hay fila de player_index.
    pub player_ids: [Option<i64>; 5],
    /// `a.status` (varchar(8), p.ej. "OK").
    pub status: String,
    /// `a.lang` (varchar(4), p.ej. "es").
    pub lang: String,
}

impl AccountLogin {
    /// Player ID de la ranura (1..5) — 0 si la ranura esta vacia
    /// (parity: el C++ trata pid=0 como "sin personaje").
    pub fn player_id(&self, slot: usize) -> i64 {
        self.player_ids.get(slot).copied().flatten().unwrap_or(0)
    }
}

/// LOGIN_SQL: las 13 columnas en el orden exacto de
/// `CreateAccountTableFromRes` (col0 = hash calculado; el `WHERE` filtra por
/// `a.password = hash($2)` y el mapeo re-verifica col0 == col3, parity del
/// `strcmp` defensivo del C++). Esquemas calificados (`account.account`,
/// `player.player_index`) — sin search_path especial (parity auth.rs).
const LOGIN_SQL: &str = "\
SELECT $2, a.id, a.login, a.password, a.social_id, pi.empire, \
pi.pid1, pi.pid2, pi.pid3, pi.pid4, pi.pid5, a.status, a.lang \
FROM account.account a \
LEFT JOIN player.player_index pi ON pi.id = a.id \
WHERE a.login = $1 AND a.password = $2";

/// Repositorio del dominio account (ADR-0008): una conexion por llamada
/// (patron verificado en `auth.rs` — coste local ~ms; el pool se decide con
/// el pipeline WAL).
pub struct AccountRepo {
    pool: PgPool,
}

impl AccountRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Conexion nueva por llamada (parity `auth.rs:293-298`).
    async fn connect(&self) -> Result<Client, String> {
        self.pool.get().await.map_err(|e| format!("PG pool get: {e}"))
    }

    /// Semantica de QUERY_LOGIN: `Some(AccountLogin)` solo si login + password
    /// son validos (el `WHERE` filtra `a.password = mysql5_password($2)` y el
    /// mapeo re-verifica el hash, parity `CreateAccountTableFromRes:288-292`);
    /// `None` = credenciales malas (0 filas). `Err` = fallo de DB.
    pub async fn login(&self, login: &str, password: &str) -> Result<Option<AccountLogin>, String> {
        let client = self.connect().await?;
        let hash = mysql5_password(password);
        let rows = client
            .query(LOGIN_SQL, &[&login, &hash])
            .await
            .map_err(|e| pg_err("QUERY_LOGIN", &e))?;
        match rows.first() {
            Some(row) => account_from_row(row),
            None => Ok(None),
        }
    }

    /// `UPDATE account.account SET lang` (patron `auth.rs` / `input_auth.cpp`).
    /// Devuelve filas afectadas. El error incluye el SQLSTATE (p.ej. `42703`)
    /// para que el auth distinga "columna aún no existe" de otros fallos.
    pub async fn set_lang(&self, login: &str, lang: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "UPDATE account.account SET lang = $1 WHERE login = $2",
                &[&lang, &login],
            )
            .await
            .map_err(|e| pg_err("UPDATE lang", &e))
    }

    /// `UPDATE account.account SET hwid` con el hex como TEXT (fix F2b — la
    /// columna es VARCHAR(64)). Devuelve filas afectadas. El error incluye el
    /// SQLSTATE: `42703` = la columna aún no existe (el auth lo loguea y sigue).
    pub async fn set_hwid(&self, login: &str, hwid_hex: &str) -> Result<u64, String> {
        let client = self.connect().await?;
        client
            .execute(
                "UPDATE account.account SET hwid = $1 WHERE login = $2",
                &[&hwid_hex, &login],
            )
            .await
            .map_err(|e| pg_err("UPDATE hwid", &e))
    }
}

/// Error con contexto + SQLSTATE (`format!("{ctx}: {e} (sqlstate {code})")`) —
/// el auth usa `contains("42703")` para el caso "columna hwid aún no existe".
pub fn pg_err(ctx: &str, e: &tokio_postgres::Error) -> String {
    let code = e.code().map(|c| c.code().to_string()).unwrap_or_default();
    format!("{ctx}: {e} (sqlstate {code})")
}

/// Mapeo fila -> `AccountLogin` (orden de columnas de `CreateAccountTableFromRes`).
/// Re-verifica col0 (hash calculado) == col3 (hash almacenado): si difieren la
/// fila es invalida (parity del `strcmp` del C++) -> `None`.
fn account_from_row(row: &Row) -> Result<Option<AccountLogin>, String> {
    let computed: String = row.try_get(0).map_err(|e| format!("col0 hash: {e}"))?;
    let id: i64 = row.try_get(1).map_err(|e| format!("col1 id: {e}"))?;
    let login: String = row.try_get(2).map_err(|e| format!("col2 login: {e}"))?;
    let password_hash: String = row.try_get(3).map_err(|e| format!("col3 password: {e}"))?;
    let social_id: String = row.try_get(4).map_err(|e| format!("col4 social_id: {e}"))?;
    let empire: Option<i16> = row.try_get(5).map_err(|e| format!("col5 empire: {e}"))?;
    let mut player_ids = [None; 5];
    for (i, slot) in player_ids.iter_mut().enumerate() {
        *slot = row.try_get(6 + i).map_err(|e| format!("col{} pid: {e}", 6 + i))?;
    }
    let status: String = row.try_get(11).map_err(|e| format!("col11 status: {e}"))?;
    let lang: Option<String> = row.try_get(12).map_err(|e| format!("col12 lang: {e}"))?;

    // Parity `CreateAccountTableFromRes:288-292`: la query ya filtra por el
    // hash, pero el C++ re-comprueba col0 == col3 — misma guarda aqui.
    if password_hash != computed {
        return Ok(None);
    }
    Ok(Some(AccountLogin {
        id,
        login,
        password_hash,
        social_id,
        empire,
        player_ids,
        status,
        lang: lang.unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vector REAL del entorno: account test / 1234 →
    /// `*A4B6157319038724E3560894F7F932C8886EBFCF` (AGENTS.md).
    #[test]
    fn mysql5_password_real_vector() {
        assert_eq!(
            mysql5_password("1234"),
            "*A4B6157319038724E3560894F7F932C8886EBFCF"
        );
        assert_eq!(mysql5_password("").len(), 41, "41 chars con el asterisco");
        assert!(mysql5_password("1234").starts_with('*'));
    }

    #[test]
    fn hex16_roundtrip() {
        let h: [u8; 16] = [
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, //
            0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
        ];
        assert_eq!(hex16(&h), "aabbccddeeff00112233445566778899");
        assert_eq!(hex16(&h).len(), 32, "16 bytes -> 32 chars hex");
    }

    /// Formato del SQL de login: las 13 columnas en el orden del contrato
    /// (`CreateAccountTableFromRes`), esquemas calificados y filtro por hash.
    #[test]
    fn login_sql_has_13_columns_in_contract_order() {
        let select = LOGIN_SQL
            .split("FROM")
            .next()
            .expect("SELECT part")
            .trim_start_matches("SELECT")
            .trim();
        let cols: Vec<&str> = select.split(',').map(|c| c.trim()).collect();
        assert_eq!(cols.len(), 13, "13 columnas (hash+id+login+password+social_id+empire+pid1..5+status+lang)");
        assert_eq!(cols[0], "$2", "col0 = hash calculado (parity input_pwd)");
        assert_eq!(cols[1], "a.id");
        assert_eq!(cols[2], "a.login");
        assert_eq!(cols[3], "a.password");
        assert_eq!(cols[4], "a.social_id");
        assert_eq!(cols[5], "pi.empire");
        assert_eq!(cols[6..11], ["pi.pid1", "pi.pid2", "pi.pid3", "pi.pid4", "pi.pid5"]);
        assert_eq!(cols[11], "a.status");
        assert_eq!(cols[12], "a.lang");
        assert!(LOGIN_SQL.contains("LEFT JOIN player.player_index"), "join calificado");
        assert!(LOGIN_SQL.contains("WHERE a.login = $1 AND a.password = $2"), "filtro por hash");
    }

    /// `player_id(slot)` — 0 para ranuras vacias (parity del C++).
    #[test]
    fn player_id_slot_semantics() {
        let acc = AccountLogin {
            id: 1,
            login: "test".into(),
            password_hash: "*A4B6157319038724E3560894F7F932C8886EBFCF".into(),
            social_id: "1234567".into(),
            empire: Some(3),
            player_ids: [Some(1), Some(3), None, Some(0), Some(2)],
            status: "OK".into(),
            lang: "en".into(),
        };
        assert_eq!(acc.player_id(0), 1);
        assert_eq!(acc.player_id(1), 3);
        assert_eq!(acc.player_id(2), 0, "None -> 0");
        assert_eq!(acc.player_id(3), 0, "Some(0) -> 0");
        assert_eq!(acc.player_id(9), 0, "slot fuera de rango -> 0");
    }
}

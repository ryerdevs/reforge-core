//! Sesión PostgreSQL 1:1 con cada conexión MySQL (spec §8.2.1c;
//! `legacy-sql-compatibility.md` §5: un `CAsyncSQL` = una conexión; el estado de
//! sesión — `@var`, charset, `sql_mode` — depende de esa correspondencia).
//!
//! - `search_path` por slot: `SQL_ACCOUNT`→`account,player` (QUERY_LOGIN cruza
//!   `player.player_index`, `ClientManagerLogin.cpp:413`), `SQL_PLAYER`→`player`,
//!   `SQL_COMMON`→`common`, `SQL_LOG`→`log`; game `player_sql`→`player,account`
//!   (el auth consulta `account` por su slot de player, `input_auth.cpp:144-218`),
//!   `common_sql`→`common`, `log_sql`→`log`. El mapeo lo decide el nombre de db
//!   del handshake (CLIENT_CONNECT_WITH_DB); el config puede overridearlo
//!   (`[slots]`). Para el `player_sql` del game, el conf.txt de runtime usa el
//!   nombre de db `playerauth` (cambio B6 — runtime, no fuente).
//! - Init de sesión: `standard_conforming_strings = off` (escapado backslash
//!   estilo MySQL, spec §4 fila `%s`) y `TimeZone` server-local (OD-7).
//! - Catálogo de tablas (PK/columnas/identity/bytea) consultado a pg_catalog y
//!   cacheado por sesión (OD-3: metadata map del adapter).
//!
//! Ejecución vía simple query protocol: el text protocol de PG ES el formato
//! del wire MySQL (mismo texto para ints, timestamps, numerics); las columnas
//! `bytea` (varbinary/CP949) llegan como `\x…` y se decodifican a bytes crudos
//! (OD-6). La API simple de tokio-postgres solo expone NOMBRES de columna, así
//! que el tipo bytea se resuelve por nombre contra el catálogo de la tabla del
//! FROM (fase 1: SELECTs de una tabla — mob_proto/item_proto/skill_proto/player;
//! el resto de columnas se reporta como VAR_STRING, que es como el C++ las
//! consume: `str_to_number`/`strlcpy` — desviación documentada en el reporte).

use std::collections::HashMap;

use tokio_postgres::{Client, NoTls, SimpleQueryMessage};

use crate::translate::{TableCatalog, TableInfo};
use crate::wire;

/// `search_path` por slot (spec §8.2.1c). `playerauth` es el override de runtime
/// para el `player_sql` del game (mismo nombre de db en el handshake que el
/// `SQL_PLAYER` del db → se distingue por nombre de db en conf.txt).
pub fn default_search_path(db: &str) -> Option<&'static str> {
    match db {
        "account" => Some("account,player"), // db SQL_ACCOUNT (QUERY_LOGIN cruza player.player_index)
        "player" => Some("player"),          // db SQL_PLAYER
        "common" => Some("common"),
        "log" => Some("log"),
        "playerauth" => Some("player,account"), // game player_sql (auth consulta account)
        _ => None,
    }
}

/// Error de ejecución PG (sqlstate para mapear al errno MySQL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgError {
    pub sqlstate: Option<String>,
    pub message: String,
}

impl std::fmt::Display for PgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.sqlstate {
            Some(s) => write!(f, "PG {s}: {}", self.message),
            None => write!(f, "PG: {}", self.message),
        }
    }
}

impl std::error::Error for PgError {}

/// Resultado de un statement: result set (columnas+filas) o bien OK
/// (affected rows). Contrato `SQLMsg::Store` (`AsyncSQL.h:59-80`):
/// uiNumRows / uiAffectedRows / uiInsertID.
#[derive(Debug, Clone)]
pub struct QueryOutcome {
    pub columns: Vec<wire::ColumnDef>,
    pub rows: Vec<Vec<Option<Vec<u8>>>>,
    pub affected: u64,
    pub is_result_set: bool,
}

/// Statements de init de sesión PG (función pura, testeable):
/// 1. `search_path` por slot (spec §8.2.1c);
/// 2. `standard_conforming_strings = off` — escapado backslash estilo MySQL
///    (spec §4 fila `%s`);
/// 3. `TimeZone` server-local (OD-7).
///
/// `server::handle_connection` los aplica ANTES de enviar el OK de auth: el
/// primer COM_QUERY del cliente nunca ve una sesión sin init. Si cualquiera
/// falla, la conexión se aborta con un ERR visible (nunca se sirve una query
/// con un search_path equivocado).
pub fn init_statements(search_path: &str, timezone: &str) -> Vec<String> {
    vec![
        format!("SET search_path TO {search_path}"),
        "SET standard_conforming_strings = off".to_string(),
        format!("SET TimeZone = '{}'", timezone.replace('\'', "''")),
    ]
}

/// Sesión PG con catálogo cacheado.
pub struct PgSession {
    client: Client,
    tables: HashMap<String, Option<TableInfo>>,
}

impl PgSession {
    /// Conecta y aplica el init de sesión (search_path, escaping, timezone).
    pub async fn connect(conn_str: &str, search_path: &str, timezone: &str) -> Result<Self, PgError> {
        let (client, connection) = tokio_postgres::connect(conn_str, NoTls)
            .await
            .map_err(|e| PgError { sqlstate: None, message: format!("PG connect: {e}") })?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let s = Self { client, tables: HashMap::new() };
        s.init(search_path, timezone).await?;
        Ok(s)
    }

    async fn init(&self, search_path: &str, timezone: &str) -> Result<(), PgError> {
        for sql in init_statements(search_path, timezone) {
            self.simple(&sql).await?;
        }
        Ok(())
    }

    async fn simple(&self, sql: &str) -> Result<(), PgError> {
        self.client.simple_query(sql).await.map_err(pg_err)?;
        Ok(())
    }

    /// Ejecuta un statement traducido (simple query protocol). Los errores de PG
    /// llegan como `Err(Error::db)` con sqlstate (mapeado a errno MySQL en server).
    pub async fn execute(&mut self, sql: &str) -> Result<QueryOutcome, PgError> {
        // Contexto bytea: la tabla del FROM (solo SELECT produce result sets).
        // Fallback SEGURO: si la tabla no existe en el catálogo → error claro
        // (42P01, la query fallaría igual en PG) en vez de servir un result set
        // con columnas bytea sin decodificar (texto `\x…` corrupta los blobs
        // binarios del C++ — peor que un error visible).
        let bytea_cols: Vec<String> = match first_from_table(sql) {
            Some(t) => match self.fetch_table_info(&t).await {
                Some(info) => info.bytea,
                None => {
                    return Err(PgError {
                        sqlstate: Some("42P01".into()),
                        message: format!("relation \"{t}\" does not exist (bytea context)"),
                    });
                }
            },
            None => Vec::new(),
        };

        let msgs = self.client.simple_query(sql).await.map_err(pg_err)?;
        let mut columns: Vec<wire::ColumnDef> = Vec::new();
        let mut rows: Vec<Vec<Option<Vec<u8>>>> = Vec::new();
        let mut affected: u64 = 0;
        let mut is_result_set = false;
        for m in msgs {
            match m {
                SimpleQueryMessage::RowDescription(fields) => {
                    is_result_set = true;
                    for f in fields.iter() {
                        let is_bytea = bytea_cols.iter().any(|c| c == f.name());
                        let (type_code, charset, column_length, flags) = if is_bytea {
                            (wire::MYSQL_TYPE_BLOB, wire::CHARSET_BINARY, 65_535u32, wire::BLOB_FLAG | wire::BINARY_FLAG)
                        } else {
                            (wire::MYSQL_TYPE_VAR_STRING, wire::CHARSET_UTF8MB4_GENERAL_CI, 255, 0)
                        };
                        columns.push(wire::ColumnDef {
                            name: f.name().to_string(),
                            schema: String::new(),
                            table: String::new(),
                            charset,
                            column_length,
                            type_code,
                            flags,
                            decimals: 0,
                        });
                    }
                }
                SimpleQueryMessage::Row(row) => {
                    let mut r = Vec::with_capacity(row.len());
                    for i in 0..row.len() {
                        match row.try_get::<usize>(i).ok().flatten() {
                            None => r.push(None),
                            Some(text) => {
                                let is_bytea = columns.get(i).map(|c| c.type_code == wire::MYSQL_TYPE_BLOB).unwrap_or(false);
                                let bytes = if is_bytea {
                                    wire::decode_bytea_text(text.as_bytes())
                                } else {
                                    text.as_bytes().to_vec()
                                };
                                r.push(Some(bytes));
                            }
                        }
                    }
                    rows.push(r);
                }
                SimpleQueryMessage::CommandComplete(n) => affected = n,
                _ => {}
            }
        }
        Ok(QueryOutcome { columns, rows, affected, is_result_set })
    }

    /// `uiInsertID` para `InsertIdHint::Generated`: `SELECT lastval()` (error → 0,
    /// p.ej. el INSERT no consumió secuencia — item con id explícito).
    pub async fn last_insert_id(&mut self) -> u64 {
        match self.client.simple_query("SELECT lastval()").await {
            Ok(msgs) => {
                for m in msgs {
                    if let SimpleQueryMessage::Row(row) = m {
                        if let Some(v) = row.try_get::<usize>(0).ok().flatten() {
                            return v.trim().parse::<u64>().unwrap_or(0);
                        }
                    }
                }
                0
            }
            Err(_) => 0,
        }
    }

    /// Metadatos de tabla vía pg_catalog, cacheados por sesión.
    /// `to_regclass($1)` resuelve el nombre por `search_path` (igual que el
    /// resto de las queries). Nota: nombres mixed-case (`GameTime*`) quedan fuera
    /// del alcance fase 1 (todas las tablas del subset son lowercase).
    async fn fetch_table_info(&mut self, table: &str) -> Option<TableInfo> {
        if let Some(v) = self.tables.get(table) {
            return v.clone();
        }
        let cols = self
            .client
            .query(
                "SELECT a.attname, (a.attidentity <> '') AS is_identity, (a.atttypid = 17) AS is_bytea \
                 FROM pg_attribute a \
                 WHERE a.attrelid = to_regclass($1) AND a.attnum > 0 AND NOT a.attisdropped \
                 ORDER BY a.attnum",
                &[&table],
            )
            .await
            .ok()?;
        if cols.is_empty() {
            self.tables.insert(table.to_string(), None);
            return None;
        }
        let columns = cols
            .iter()
            .filter_map(|r| r.try_get::<_, String>(0).ok())
            .collect::<Vec<_>>();
        let identity = cols
            .iter()
            .filter(|r| r.try_get::<_, bool>(1).ok().unwrap_or(false))
            .filter_map(|r| r.try_get::<_, String>(0).ok())
            .collect::<Vec<_>>();
        let bytea = cols
            .iter()
            .filter(|r| r.try_get::<_, bool>(2).ok().unwrap_or(false))
            .filter_map(|r| r.try_get::<_, String>(0).ok())
            .collect::<Vec<_>>();
        let pk = self
            .client
            .query(
                "SELECT a.attname FROM pg_index i \
                 JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                 WHERE i.indrelid = to_regclass($1) AND i.indisprimary \
                 ORDER BY array_position(i.indkey, a.attnum)",
                &[&table],
            )
            .await
            .ok()?
            .iter()
            .filter_map(|r| r.try_get::<_, String>(0).ok())
            .collect::<Vec<_>>();
        let info = TableInfo { columns, pk, identity, bytea };
        self.tables.insert(table.to_string(), Some(info.clone()));
        Some(info)
    }
}

impl TableCatalog for PgSession {
    async fn table_info(&mut self, table: &str) -> Option<TableInfo> {
        self.fetch_table_info(table).await
    }
}

/// Error de tokio-postgres → `PgError` con sqlstate (los errores de query llegan
/// como `Err(Error::db)`, `client.rs:50` de tokio-postgres).
fn pg_err(e: tokio_postgres::Error) -> PgError {
    PgError {
        sqlstate: e.code().map(|c| c.code().to_string()),
        message: e.as_db_error().map(|d| d.message().to_string()).unwrap_or_else(|| e.to_string()),
    }
}

/// Primera tabla de un SELECT (contexto para resolver columnas bytea).
///
/// El `FROM` de la CLAUSULA está a profundidad de paréntesis 0; los "FROM"
/// dentro de funciones/expresiones se ignoran. Esto es CRÍTICO porque la
/// traducción de `UNIX_TIMESTAMP(x)` produce `EXTRACT(EPOCH FROM x)` — sin el
/// filtro de profundidad, la query de carga de personaje
/// (`ClientManagerPlayer.cpp:361-375`, con
/// `UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play)` en la proyección) resolvía
/// "localtimestamp" como tabla → catálogo vacío → bytea nunca detectado →
/// `skill_level` salía como texto `\x…` (bug crítico 2026-08-10: el cliente se
/// cerraba al entrar al mundo).
fn first_from_table(sql: &str) -> Option<String> {
    if !sql[..sql.len().min(6)].eq_ignore_ascii_case("SELECT") {
        return None;
    }
    let b = sql.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    while i < b.len() {
        match b[i] {
            b'\'' => i = skip_string(sql, i),
            b'`' => i = skip_backtick(sql, i),
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => {
                if depth == 0
                    && sql[i..].len() >= 4
                    && sql[i..i + 4].eq_ignore_ascii_case("from")
                {
                    if let Some(t) = parse_ident(sql[i + 4..].trim_start()) {
                        return Some(t);
                    }
                    // `FROM (subquery)` o forma rara → la primera "tabla" no es
                    // un identificador simple: seguir escaneando (el FROM de una
                    // subquery está a depth > 0 y se ignora).
                }
                i += 1;
            }
        }
    }
    None
}

/// Identificador: `t`, `t `t``, `"t"` → (nombre, bytes consumidos).
fn parse_ident(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let close = match b.first()? {
        b'`' => b'`',
        b'"' => b'"',
        _ => {
            let mut i = 0;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$') {
                i += 1;
            }
            return if i == 0 { None } else { Some(s[..i].to_string()) };
        }
    };
    let mut i = 1;
    while i < b.len() && b[i] != close {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    Some(s[1..i].to_string())
}

fn skip_string(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    i += 1;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'\'' {
            if i + 1 < b.len() && b[i + 1] == b'\'' {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    i
}

fn skip_backtick(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    i += 1;
    while i < b.len() && b[i] != b'`' {
        i += 1;
    }
    i + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_search_path_mapping() {
        assert_eq!(default_search_path("account"), Some("account,player"));
        assert_eq!(default_search_path("player"), Some("player"));
        assert_eq!(default_search_path("common"), Some("common"));
        assert_eq!(default_search_path("log"), Some("log"));
        assert_eq!(default_search_path("playerauth"), Some("player,account"));
        assert_eq!(default_search_path("hotbackup"), None);
        assert_eq!(default_search_path(""), None);
    }

    /// Regresión del gate: el init de sesión debe aplicar el search_path del
    /// slot ANTES del primer COM_QUERY (las 3 SETs en orden; timezone escapado).
    #[test]
    fn init_statements_apply_search_path_first() {
        let stmts = init_statements("account,player", "Europe/Madrid");
        assert_eq!(
            stmts,
            vec![
                "SET search_path TO account,player",
                "SET standard_conforming_strings = off",
                "SET TimeZone = 'Europe/Madrid'",
            ]
        );
        // Timezone con comilla simple escapada (nunca rompe el statement).
        let stmts = init_statements("player", "America/Argentina/Buenos_Aires");
        assert!(stmts[2].starts_with("SET TimeZone = '"));
        let stmts = init_statements("player", "a'b");
        assert_eq!(stmts[2], "SET TimeZone = 'a''b'");
    }

    #[test]
    fn from_table_of_select() {
        assert_eq!(
            first_from_table("SELECT vnum, name, locale_name FROM mob_proto ORDER BY vnum").as_deref(),
            Some("mob_proto")
        );
        assert_eq!(
            first_from_table("SELECT id, name FROM player WHERE id=1").as_deref(),
            Some("player")
        );
        assert_eq!(
            first_from_table("SELECT shop.vnum, shop_item.item_vnum FROM shop LEFT JOIN shop_item ON shop.vnum = shop_item.shop_vnum").as_deref(),
            Some("shop")
        );
        assert_eq!(
            first_from_table("SELECT a.name, NOW() FROM player AS a, player_index AS b WHERE a.id=1").as_deref(),
            Some("player")
        );
        assert_eq!(first_from_table("SELECT 1"), None);
        assert_eq!(first_from_table("INSERT INTO item (id) VALUES(1)"), None);
        // "from" dentro de un literal no engaña.
        assert_eq!(first_from_table("SELECT 'from x' AS v"), None);
    }

    /// Regresión del bug crítico 2026-08-10: la query de carga de personaje
    /// (`ClientManagerPlayer.cpp:361-375`) lleva
    /// `UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play)` en la proyección — su
    /// traducción produce `EXTRACT(EPOCH FROM …)` con un "FROM" DENTRO de la
    /// función. Sin el filtro de profundidad de paréntesis, `first_from_table`
    /// resolvía "localtimestamp" como tabla → catálogo vacío → `skill_level`
    /// salía como texto `\x…` (blobs corruptos → cliente se cerraba al entrar).
    #[test]
    fn from_table_ignores_from_inside_functions() {
        // La query EXACTA del C++ (forma MySQL, sin traducir).
        let player_load = "SELECT id,name,job,voice,dir,x,y,z,map_index,exit_x,exit_y,exit_map_index,hp,mp,stamina,random_hp,random_sp,playtime,gold,level,level_step,st,ht,dx,iq,exp,stat_point,skill_point,sub_skill_point,stat_reset_count,part_base,part_hair,part_acce,skill_level,quickslot,skill_group,alignment,horse_level,horse_riding,horse_hp,horse_hp_droptime,horse_stamina,UNIX_TIMESTAMP(NOW())-UNIX_TIMESTAMP(last_play),horse_skill_point,cheque FROM player WHERE id=1";
        assert_eq!(first_from_table(player_load).as_deref(), Some("player"));
        // La misma query YA TRADUCIDA (lo que ejecuta el proxy — el caso que
        // fallaba: EXTRACT(EPOCH FROM …) delante del FROM de la cláusula).
        let player_load_translated = "SELECT id,name,job,voice,dir,x,y,z,map_index,exit_x,exit_y,exit_map_index,hp,mp,stamina,random_hp,random_sp,playtime,gold,level,level_step,st,ht,dx,iq,exp,stat_point,skill_point,sub_skill_point,stat_reset_count,part_base,part_hair,part_acce,skill_level,quickslot,skill_group,alignment,horse_level,horse_riding,horse_hp,horse_hp_droptime,horse_stamina,EXTRACT(EPOCH FROM LOCALTIMESTAMP)-EXTRACT(EPOCH FROM last_play),horse_skill_point,cheque FROM player WHERE id=1";
        assert_eq!(first_from_table(player_load_translated).as_deref(), Some("player"));
        // FROM de subquery a profundidad 1 → se ignora; sin tabla simple en la
        // cláusula → None (las subqueries en el FROM no existen en fase 1).
        assert_eq!(
            first_from_table("SELECT x FROM (SELECT MAX(id) FROM log) AS t WHERE id=1"),
            None
        );
        // FROM con dos tablas separadas por coma → la primera.
        assert_eq!(
            first_from_table("SELECT a.x FROM a, b WHERE a.id=1").as_deref(),
            Some("a")
        );
    }
}

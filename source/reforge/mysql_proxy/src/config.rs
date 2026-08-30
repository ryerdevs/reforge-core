//! Config TOML mínima del proxy (ADR-0004; sin config-rs/clap hasta F2 — spec
//! §8.2.1c). Subconjunto a mano (~60 líneas): claves `key = "value"` / `key = 123`
//! y una sección `[slots]` (db name → search_path).
//!
//! ```toml
//! listen = "127.0.0.1:3307"
//! mysql_user = "mt2"
//! mysql_password = "mt2"
//! pg_conn = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2"
//! timezone = "Europe/Madrid"          # OD-7: server-local (sesión PG)
//!
//! [slots]
//! account = "account,player"          # db SQL_ACCOUNT (QUERY_LOGIN cruza player.player_index)
//! player = "player"                   # db SQL_PLAYER
//! common = "common"
//! log = "log"
//! playerauth = "player,account"       # game player_sql — el auth del game consulta
//!                                     # `account` por su slot de player (input_auth.cpp:144-218)
//! ```

use std::collections::HashMap;

use crate::session;

/// Configuración del proxy (spec §8.2.1c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Dirección de escucha del wire MySQL (`127.0.0.1:3307`; MariaDB sigue en 3306).
    pub listen: String,
    /// Credencial MySQL esperada en el handshake (misma que conf.txt del C++).
    pub mysql_user: String,
    pub mysql_password: String,
    /// Connection string libpq para tokio-postgres (bases: `metin2`, esquemas account/player/common/log).
    pub pg_conn: String,
    /// TimeZone de la sesión PG (OD-7, server-local).
    pub timezone: String,
    /// Overrides del mapa db name → search_path (defaults en `session::default_search_path`).
    pub slots: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:3307".into(),
            mysql_user: "mt2".into(),
            mysql_password: "mt2".into(),
            pg_conn: "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".into(),
            timezone: "Europe/Madrid".into(),
            slots: HashMap::new(),
        }
    }
}

impl Config {
    /// `search_path` para la db pedida por el cliente en el handshake
    /// (CLIENT_CONNECT_WITH_DB): override del config o default por slot.
    pub fn search_path(&self, db: &str) -> Option<String> {
        if let Some(p) = self.slots.get(db) {
            return Some(p.clone());
        }
        session::default_search_path(db).map(str::to_string)
    }

    /// Carga `path` y parsea (errores con mensaje legible).
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("config {path}: {e}"))?;
        Self::parse(&text)
    }

    /// Parser TOML mínimo: líneas `clave = valor`, sección `[slots]`, comentarios `#`.
    /// Estricto: clave desconocida o valor malformado → error (caza typos).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = Config::default();
        let mut section: Option<String> = None;
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let where_at = |what: &str| format!("config: línea {}: {what}", lineno + 1);
            if line.starts_with('[') {
                let name = line.trim_end_matches(']').trim_start_matches('[').trim();
                section = Some(name.to_string());
                continue;
            }
            let Some(eq) = line.find('=') else {
                return Err(where_at("se esperaba `clave = valor`"));
            };
            let key = line[..eq].trim();
            let value = parse_value(line[eq + 1..].trim(), &where_at)?;
            match section.as_deref() {
                None => match key {
                    "listen" => cfg.listen = value,
                    "mysql_user" => cfg.mysql_user = value,
                    "mysql_password" => cfg.mysql_password = value,
                    "pg_conn" => cfg.pg_conn = value,
                    "timezone" => cfg.timezone = value,
                    other => return Err(where_at(&format!("clave desconocida: {other}"))),
                },
                Some("slots") => {
                    cfg.slots.insert(key.to_string(), value);
                }
                Some(other) => return Err(where_at(&format!("sección desconocida: [{other}]"))),
            }
        }
        Ok(cfg)
    }
}

/// Valor de una asignación: `"string"` (con escapes `\"` y `\\`) o bare/integer.
fn parse_value(raw: &str, err: &dyn Fn(&str) -> String) -> Result<String, String> {
    if let Some(rest) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Ok(out),
                '\\' => match chars.next() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(other),
                    None => return Err(err("string sin cerrar")),
                },
                other => out.push(other),
            }
        }
        return Err(err("string sin cerrar"));
    }
    if raw.is_empty() {
        return Err(err("valor vacío"));
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
# proxy G-PG (spec §8.2.1c)
listen = "127.0.0.1:3307"
mysql_user = "mt2"
mysql_password = "mt2"
pg_conn = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2"
timezone = "Europe/Madrid"

[slots]
account = "account,player"
player = "player"
common = "common"
log = "log"
playerauth = "player,account"
"#;

    #[test]
    fn parses_full_config() {
        let cfg = Config::parse(SAMPLE).unwrap();
        assert_eq!(cfg.listen, "127.0.0.1:3307");
        assert_eq!(cfg.mysql_user, "mt2");
        assert_eq!(
            cfg.pg_conn,
            "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2"
        );
        assert_eq!(cfg.timezone, "Europe/Madrid");
        assert_eq!(cfg.slots.len(), 5);
        assert_eq!(
            cfg.slots.get("account").map(String::as_str),
            Some("account,player")
        );
        assert_eq!(
            cfg.slots.get("playerauth").map(String::as_str),
            Some("player,account")
        );
    }

    #[test]
    fn defaults_when_minimal() {
        let cfg = Config::parse("listen = \"127.0.0.1:3307\"\n").unwrap();
        assert_eq!(cfg.mysql_user, "mt2"); // default
        assert_eq!(cfg.timezone, "Europe/Madrid"); // default
    }

    #[test]
    fn rejects_unknown_key_and_section() {
        assert!(Config::parse("bogus = 1\n").is_err());
        assert!(Config::parse("[nope]\nx = 1\n").is_err());
        assert!(Config::parse("listen = \"sin cerrar\n").is_err());
        assert!(Config::parse("listen\n").is_err());
    }

    #[test]
    fn escapes_in_strings() {
        let cfg = Config::parse(r#"mysql_password = "a\"b\\c""#).unwrap();
        assert_eq!(cfg.mysql_password, "a\"b\\c");
    }

    #[test]
    fn search_path_merge_override_then_default() {
        let mut cfg = Config::default();
        cfg.slots.insert("player".into(), "player,extra".into());
        assert_eq!(cfg.search_path("player").as_deref(), Some("player,extra"));
        assert_eq!(
            cfg.search_path("account").as_deref(),
            Some("account,player")
        );
        assert_eq!(cfg.search_path("log").as_deref(), Some("log"));
        assert_eq!(cfg.search_path("nodb"), None);
    }
}

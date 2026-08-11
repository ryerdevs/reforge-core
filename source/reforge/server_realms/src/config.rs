//! Config TOML mínima de `server_realms` (F2a: rol auth).
//!
//! Parser a mano (patrón verificado del proxy, `mysql_proxy/src/config.rs`):
//! sin config-rs — YAGNI para 4 campos; la deferral config-rs se revisa en F5
//! si el config crece.
//!
//! ```toml
//! # server_realms.toml (rol auth)
//! listen = "127.0.0.1:30001"
//! pg_conn = "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2"
//! timeout_ms = 15000
//! no_more_clients = false
//! legacy_dir = ""          # dir de panama/ + cshybridcrypt* (vacío = sin legacy)
//! ```

use std::time::Duration;

/// Configuración del rol auth (F2a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `addr:port` del listener (el auth C++ usaba 30001).
    pub listen: String,
    /// Connection string libpq para tokio-postgres (bases `metin2`, esquema
    /// `account` — la query es calificada, sin search_path especial).
    pub pg_conn: String,
    /// Timeout global del intento de login por conexión (deuda F1.5: una
    /// conexión silenciosa no puede vivir los ~17.6 s de retries del handshake).
    pub timeout: Duration,
    /// `g_bNoMoreClient` del C++ (input_auth.cpp:96-105): rechaza con
    /// GC_LOGIN_FAILURE "SHUTDOWN".
    pub no_more_clients: bool,
    /// Directorio base de los archivos legacy (panama/ + cshybridcrypt*) —
    /// parity del cwd del auth C++. Vacío = sin legacy (el runtime srv1 actual
    /// no tiene los archivos → el auth C++ tampoco envía 151-153).
    pub legacy_dir: String,
    /// F2b: versión del cliente esperada en el LOGIN3 auth (campo `version`,
    /// 72/88 B). Default = 40999 (`source/client/EterBase/Version.h:6` — la
    /// constante del cliente v40999). `None` no aplica: si el LOGIN3 trae
    /// version y no coincide → cierre limpio con log.
    pub expected_version: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:30001".into(),
            pg_conn: "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".into(),
            timeout: Duration::from_secs(15),
            no_more_clients: false,
            legacy_dir: String::new(),
            expected_version: 40999,
        }
    }
}

impl Config {
    /// Carga `path` y parsea (errores con mensaje legible).
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("config {path}: {e}"))?;
        Self::parse(&text)
    }

    /// Parser TOML mínimo: líneas `clave = valor`, comentarios `#`. Estricto:
    /// clave desconocida o valor malformado → error (caza typos).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = Config::default();
        for (lineno, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let where_at = |what: &str| format!("config: línea {}: {what}", lineno + 1);
            let Some(eq) = line.find('=') else {
                return Err(where_at("se esperaba `clave = valor`"));
            };
            let key = line[..eq].trim();
            let value = line[eq + 1..].trim();
            match key {
                "listen" => cfg.listen = parse_string(value, &where_at)?,
                "pg_conn" => cfg.pg_conn = parse_string(value, &where_at)?,
                "timeout_ms" => {
                    let ms: u64 = value
                        .parse()
                        .map_err(|_| where_at("timeout_ms debe ser un entero (ms)"))?;
                    cfg.timeout = Duration::from_millis(ms);
                }
                "no_more_clients" => {
                    cfg.no_more_clients = match value {
                        "true" => true,
                        "false" => false,
                        _ => return Err(where_at("no_more_clients debe ser true|false")),
                    };
                }
                "legacy_dir" => cfg.legacy_dir = parse_string(value, &where_at)?,
                "expected_version" => {
                    cfg.expected_version = value
                        .parse()
                        .map_err(|_| where_at("expected_version debe ser un entero (u32)"))?;
                }
                other => return Err(where_at(&format!("clave desconocida: {other}"))),
            }
        }
        Ok(cfg)
    }
}

/// Valor string: `"..."` (con escapes `\"` y `\\`) o bare.
fn parse_string(raw: &str, err: &dyn Fn(&str) -> String) -> Result<String, String> {
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

    #[test]
    fn parses_full_config() {
        let cfg = Config::parse(
            "listen = \"127.0.0.1:30001\"\npg_conn = \"host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2\"\ntimeout_ms = 15000\nno_more_clients = false\n",
        )
        .unwrap();
        assert_eq!(cfg.listen, "127.0.0.1:30001");
        assert_eq!(cfg.timeout, Duration::from_secs(15));
        assert!(!cfg.no_more_clients);
    }

    #[test]
    fn defaults_when_minimal() {
        let cfg = Config::parse("listen = \"127.0.0.1:30001\"\n").unwrap();
        assert_eq!(cfg.timeout, Duration::from_secs(15));
        assert_eq!(cfg.pg_conn, Config::default().pg_conn);
    }

    #[test]
    fn rejects_unknown_key_and_bad_values() {
        assert!(Config::parse("bogus = 1\n").is_err());
        assert!(Config::parse("timeout_ms = \"x\"\n").is_err());
        assert!(Config::parse("no_more_clients = maybe\n").is_err());
        assert!(Config::parse("listen\n").is_err());
    }
}

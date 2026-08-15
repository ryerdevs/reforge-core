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
//! # F5 — lista de canales + manifest (rates) que el auth manda al cliente
//! # (GC_CHANNEL_LIST): el cliente conecta al canal con ESTA lista (adiós al
//! # IP bakeado de serverinfo.py). `players` es opcional (default 0).
//! channels = [{name = "CH-1", ip = "172.25.104.175", port = 30003}]
//! exp_rate = 100           # manifest: rate de exp (%)
//! gold_rate = 100          # manifest: rate de oro (%)
//! drop_rate = 100          # manifest: rate de drop (%)
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
    /// Número del canal (parity `g_bChannel` del CONFIG del core — el
    /// `TPacketGCChannel` del entry, `input_login.cpp:653-656`). El rol auth
    /// no lo usa (default 1).
    pub channel: u8,
    /// Heartbeat del servidor: intervalo del `GC_PING` (44, 1 B) en la fase
    /// de juego — el cliente en reposo NO manda nada; responde `CG_PONG`
    /// (0xfe) a cada ping (parity `ping_event`, `desc.cpp:179-214`; el C++
    /// usa 60 s — `ping_event_second_cycle = passes_per_sec * 60`,
    /// config.cpp:30). DEBE ser menor que `timeout_ms` (el pong del cliente
    /// resetea el timeout de inactividad). Default 10 s < 15 s.
    pub ping_interval_ms: u64,
    /// Directorio de los spawns del mapa (F5): el canal carga los NPCs del
    /// mapa del jugador desde aquí (`game_core::npc::load_map_spawns` — el
    /// `index`/`npc.txt`/`regen.txt`/... del runtime). Default: el path real
    /// del runtime srv1.
    pub map_path: String,
    /// Directorio de las quests legacy (F5 - wiring 2026-08-13): el canal
    /// convierte el corpus qc->DSL al arrancar (quest_dsl::convert) y carga
    /// las quests convertibles. Vacio = derivado del map_path (el dir
    /// quest hermano del map).
    pub quest_path: String,
    /// Directorio base de los archivos legacy (panama/ + cshybridcrypt*) —
    /// parity del cwd del auth C++. Vacío = sin legacy (el runtime srv1 actual
    /// no tiene los archivos → el auth C++ tampoco envía 151-153).
    pub legacy_dir: String,
    /// F2b: versión del cliente esperada en el LOGIN3 auth (campo `version`,
    /// 72/88 B). Default = 40999 (`source/client/EterBase/Version.h:6` — la
    /// constante del cliente v40999). `None` no aplica: si el LOGIN3 trae
    /// version y no coincide → cierre limpio con log.
    pub expected_version: u32,
    /// F5: canales servidos — el auth los manda al cliente en el
    /// `GC_CHANNEL_LIST` tras el login OK (el cliente conecta al canal con
    /// esta lista; ya no depende del IP bakeado de serverinfo.py).
    pub channels: Vec<ChannelCfg>,
    /// F5 manifest: rate de exp en % (u16 — el wire del GC_CHANNEL_LIST).
    pub exp_rate: u16,
    /// F5 manifest: rate de oro en %.
    pub gold_rate: u16,
    /// F5 manifest: rate de drop en %.
    pub drop_rate: u16,
    /// Tamaño máximo del pool de conexiones PG del proceso (fix del cuello
    /// del entry 2026-08-13 — una conexión por llamada era el cuello del
    /// login; el pool lo crea el arranque de cada rol). Default 10.
    pub pool_max_size: usize,
}

/// Un canal de la lista F5 (`channels` del toml). Los mismos campos que el
/// wire del `GC_CHANNEL_LIST` (name[16] + ip[16] + port u16 + players u16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelCfg {
    /// Nombre visible del canal (p.ej. "CH-1"; truncado a 15 chars en el wire).
    pub name: String,
    /// IPv4 dotted-quad (p.ej. "172.25.104.175"; truncado a 15 chars).
    pub ip: String,
    /// Puerto TCP del canal (p.ej. 30003).
    pub port: u16,
    /// Jugadores actuales (0 = desconocido; informativo).
    pub players: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:30001".into(),
            pg_conn: "host=127.0.0.1 port=5432 user=mt2 password=mt2 dbname=metin2".into(),
            timeout: Duration::from_secs(15),
            no_more_clients: false,
            channel: 1,
            ping_interval_ms: 10_000,
            map_path: "/home/m2/source/metin2_svfiles/main/srv1/share/locale/spain/map".into(),
            quest_path: String::new(),
            legacy_dir: String::new(),
            expected_version: 40999,
            channels: Vec::new(),
            exp_rate: 100,
            gold_rate: 100,
            drop_rate: 100,
            pool_max_size: 10,
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
                "channel" => {
                    cfg.channel = value
                        .parse()
                        .map_err(|_| where_at("channel debe ser un entero (u8)"))?;
                }
                "ping_interval_ms" => {
                    cfg.ping_interval_ms = value
                        .parse()
                        .map_err(|_| where_at("ping_interval_ms debe ser un entero (ms)"))?;
                }
                "map_path" => cfg.map_path = parse_string(value, &where_at)?,
                "quest_path" => cfg.quest_path = parse_string(value, &where_at)?,
                "legacy_dir" => cfg.legacy_dir = parse_string(value, &where_at)?,
                "expected_version" => {
                    cfg.expected_version = value
                        .parse()
                        .map_err(|_| where_at("expected_version debe ser un entero (u32)"))?;
                }
                "channels" => cfg.channels = parse_channels(value, &where_at)?,
                "exp_rate" => {
                    cfg.exp_rate = parse_u16(value, &where_at, "exp_rate")?;
                }
                "gold_rate" => {
                    cfg.gold_rate = parse_u16(value, &where_at, "gold_rate")?;
                }
                "drop_rate" => {
                    cfg.drop_rate = parse_u16(value, &where_at, "drop_rate")?;
                }
                "pool_max_size" => {
                    cfg.pool_max_size = value
                        .parse()
                        .map_err(|_| where_at("pool_max_size debe ser un entero (usize)"))?;
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

/// Entero u16 (para `exp_rate`/`gold_rate`/`drop_rate`/`port`).
fn parse_u16(raw: &str, err: &dyn Fn(&str) -> String, what: &str) -> Result<u16, String> {
    raw.parse().map_err(|_| err(&format!("{what} debe ser un entero (u16)")))
}

/// F5 — parsea `channels = [{name = "CH-1", ip = "172.25.104.175",
/// port = 30003, players = 0}, ...]` (array de dicts en UNA línea; el parser
/// del config es por líneas — el formato mínimo de la decisión TOML).
fn parse_channels(raw: &str, err: &dyn Fn(&str) -> String) -> Result<Vec<ChannelCfg>, String> {
    let inner = raw.trim();
    let inner = inner
        .strip_prefix('[')
        .ok_or_else(|| err("channels: se esperaba `[{name = ..., ip = ..., port = ...}]`"))?;
    let inner = inner
        .strip_suffix(']')
        .ok_or_else(|| err("channels: falta el `]` final"))?;

    let mut out = Vec::new();
    for entry in split_top_level(inner, ',').iter() {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let entry = entry
            .strip_prefix('{')
            .and_then(|e| e.strip_suffix('}'))
            .ok_or_else(|| err("channels: cada canal debe ser `{name = ..., ip = ..., port = ...}`"))?;

        let mut ch = ChannelCfg { name: String::new(), ip: String::new(), port: 0, players: 0 };
        let mut have_name = false;
        let mut have_ip = false;
        let mut have_port = false;
        for field in split_top_level(entry, ',').iter() {
            let field = field.trim();
            if field.is_empty() {
                continue;
            }
            let Some(eq) = field.find('=') else {
                return Err(err("channels: campo sin `=`"));
            };
            let key = field[..eq].trim();
            let value = field[eq + 1..].trim();
            match key {
                "name" => {
                    ch.name = parse_string(value, err)?;
                    have_name = true;
                }
                "ip" => {
                    ch.ip = parse_string(value, err)?;
                    have_ip = true;
                }
                "port" => {
                    ch.port = parse_u16(value, err, "port")?;
                    have_port = true;
                }
                "players" => ch.players = parse_u16(value, err, "players")?,
                other => return Err(err(&format!("channels: campo desconocido: {other}"))),
            }
        }
        if !have_name || !have_ip || !have_port {
            return Err(err("channels: cada canal necesita name, ip y port"));
        }
        out.push(ch);
    }
    Ok(out)
}

/// Divide `s` por `sep` en profundidad 0 (respeta `{...}` y `"..."`).
fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '{' if !in_str => depth += 1,
            '}' if !in_str => depth -= 1,
            c if !in_str && depth == 0 && c == sep => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
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

    /// F5: `channels` (array de dicts en una línea) + rates con defaults.
    #[test]
    fn parses_channels_and_rates() {
        let cfg = Config::parse(
            "listen = \"127.0.0.1:30001\"\n\
             channels = [{name = \"CH-1\", ip = \"172.25.104.175\", port = 30003}, {name = \"CH-2\", ip = \"172.25.104.175\", port = 30007, players = 42}]\n\
             exp_rate = 150\n\
             gold_rate = 200\n\
             drop_rate = 100\n",
        )
        .unwrap();
        assert_eq!(
            cfg.channels,
            vec![
                ChannelCfg { name: "CH-1".into(), ip: "172.25.104.175".into(), port: 30003, players: 0 },
                ChannelCfg { name: "CH-2".into(), ip: "172.25.104.175".into(), port: 30007, players: 42 },
            ]
        );
        assert_eq!(cfg.exp_rate, 150);
        assert_eq!(cfg.gold_rate, 200);
        assert_eq!(cfg.drop_rate, 100);
    }

    /// F5: sin `channels`/rates en el toml → lista vacía y rates default 100
    /// (compatibilidad: el config actual del runtime no los tiene).
    #[test]
    fn channels_default_to_empty_and_rates_100() {
        let cfg = Config::parse("listen = \"127.0.0.1:30001\"\n").unwrap();
        assert!(cfg.channels.is_empty());
        assert_eq!((cfg.exp_rate, cfg.gold_rate, cfg.drop_rate), (100, 100, 100));
    }

    /// F5: errores del array — canal sin ip, malformado, campo desconocido.
    #[test]
    fn rejects_malformed_channels() {
        assert!(Config::parse("channels = [\"x\"]\n").is_err());
        assert!(Config::parse("channels = [{name = \"CH-1\", port = 30003}]\n").is_err());
        assert!(Config::parse("channels = [{name = \"CH-1\", ip = \"1.2.3.4\", port = 30003, bogus = 1}]\n").is_err());
        assert!(Config::parse("channels = [{name = \"CH-1\", ip = \"1.2.3.4\"}]\n").is_err());
        assert!(Config::parse("channels = {name = \"CH-1\"}\n").is_err());
        assert!(Config::parse("exp_rate = \"x\"\n").is_err());
        assert!(Config::parse("exp_rate = 70000\n").is_err()); // > u16
    }

    /// F5: nombres con comas/llaves no rompen el split (profundidad 0).
    #[test]
    fn channel_name_with_braces_survives_splitting() {
        let cfg = Config::parse(
            "channels = [{name = \"CH,{1}\", ip = \"172.25.104.175\", port = 30003}]\n",
        )
        .unwrap();
        assert_eq!(cfg.channels.len(), 1);
        assert_eq!(cfg.channels[0].name, "CH,{1}");
        assert_eq!(cfg.channels[0].port, 30003);
    }
}

//! Debug logging del proxy (gate G-PG). Activación: flag `--debug` del binario
//! o variable de entorno `MYSQL_PROXY_DEBUG=1`. Sale por stderr con prefijo
//! `[proxy]` (grepable).
//!
//! Regla: NUNCA se loguea contenido de filas — solo metadata y conteos (un log
//! de 50MB de valores ensucia la diagnosis; AGENTS.md regla de logs). Las
//! queries SÍ se loguean (texto) truncadas a 200 bytes.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Activa el debug (flag `--debug` del binario).
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Activa desde `MYSQL_PROXY_DEBUG=1` si está presente (llamado en main).
pub fn init_from_env() {
    if std::env::var_os("MYSQL_PROXY_DEBUG").is_some() {
        enable();
    }
}

/// Log condicional: `debug::log(format_args!("conn {id}: query: {sql}"))`.
pub fn log(args: std::fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("[proxy] {args}");
    }
}

/// Trunca un texto a `max` bytes respetando límites UTF-8 (para queries largas).
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…({}B)", &s[..end], s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_utf8_boundaries() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcdef", 3), "abc…(6B)");
        // 'á' son 2 bytes: cortar en 3 bytes no puede partir el carácter.
        let s = "aáb";
        let t = truncate(s, 3);
        assert!(t.starts_with("aá"), "{t}");
        assert!(t.ends_with("…(4B)"), "{t}");
    }

    #[test]
    fn enable_toggles() {
        assert!(!enabled());
        enable();
        assert!(enabled());
    }
}

//! `mysql_proxy` — binario del adaptador temporal G-PG (ADR-0005, spec §8.2.1c).
//! Se borra en F6.
//!
//! Uso: `mysql_proxy --config proxy.toml [--debug]` (default `proxy.toml`).
//! Sin clap (deferral → F2): parseo de args con std.
//!
//! Debug: `--debug` o variable de entorno `MYSQL_PROXY_DEBUG=1` — log a stderr
//! con prefijo `[proxy]` (ver `debug` module): init de sesión, search_path,
//! cada COM_QUERY, errores PG y metadata de result sets (nunca filas).

use std::process::ExitCode;

use mysql_proxy::config::Config;
use mysql_proxy::debug;
use mysql_proxy::server;

struct Args {
    config_path: String,
    debug: bool,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let mut path: Option<String> = None;
    let mut debug_flag = false;
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--config" => {
                let v = it.next().ok_or("--config requiere un valor")?;
                path = Some(v.clone());
            }
            "--debug" => debug_flag = true,
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(Args { config_path: path.unwrap_or_else(|| "proxy.toml".into()), debug: debug_flag })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("mysql_proxy: {e}");
            return ExitCode::from(2);
        }
    };
    debug::init_from_env();
    if args.debug {
        debug::enable();
    }
    let config = match Config::load(&args.config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mysql_proxy: {e}");
            return ExitCode::from(2);
        }
    };
    if let Err(e) = server::serve(config).await {
        eprintln!("mysql_proxy: {e}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_path_and_debug() {
        assert_eq!(parse_args(&[]).unwrap().config_path, "proxy.toml");
        assert!(!parse_args(&[]).unwrap().debug);
        assert_eq!(
            parse_args(&["--config".into(), "gpg.toml".into()]).unwrap().config_path,
            "gpg.toml"
        );
        assert!(parse_args(&["--debug".into()]).unwrap().debug);
        assert!(parse_args(&["--config".into()]).is_err());
        assert!(parse_args(&["--bogus".into()]).is_err());
    }
}


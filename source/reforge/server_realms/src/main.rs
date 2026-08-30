//! `server_realms` — binario único con roles (ADR-0004): un proceso por región.
//!
//! F2a: rol `auth` REAL — login del cliente legacy contra PostgreSQL
//! (parity con el auth C++: `input_auth.cpp` / `input_db.cpp:1697-1728`).
//! F4 slice 2: rol `channel` — flujo login→select (+ spawn best-effort)
//! contra PostgreSQL directo (`game_core::WorldStore`, ADR-0008).
//!
//! Uso: `server_realms --role auth|channel --config server_realms.toml`
//! (sin clap: parseo de args con std, el config TOML es el parser mínimo).
//! Opcional: `--bench-capture <dir>` (F5 benchmark) — captura cruda del wire
//! por conexión; ver `bench_capture.rs` para el contrato del hook (el lane
//! del canal lo cablea — hoy el flag solo inicializa el módulo).

mod auth;
mod bench_capture;
mod channel;
mod config;

use std::path::Path;
use std::process::ExitCode;

use config::Config;

/// Rol del proceso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Auth,
    Channel,
}

struct Args {
    role: Role,
    config_path: String,
    /// `--bench-capture <dir>`: directorio de la captura golden (None = off).
    bench_capture: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut role = Role::Auth;
    let mut path: Option<String> = None;
    let mut bench_capture = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--role" => {
                let Some(v) = it.next() else {
                    return Err("--role requiere un valor (auth|channel)".into());
                };
                role = match v.as_str() {
                    "auth" => Role::Auth,
                    "channel" => Role::Channel,
                    other => return Err(format!("rol desconocido: {other} (auth|channel)")),
                };
            }
            "--config" => {
                let v = it.next().ok_or("--config requiere un valor")?;
                path = Some(v.clone());
            }
            "--bench-capture" => {
                let v = it.next().ok_or("--bench-capture requiere un valor (dir)")?;
                bench_capture = Some(v.clone());
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(Args {
        role,
        config_path: path.unwrap_or_else(|| "server_realms.toml".into()),
        bench_capture,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("server_realms: {e}");
            return ExitCode::from(2);
        }
    };
    // F5 benchmark: la captura golden (streams crudos por conexión). El hook
    // del canal aún no está cableado — el flag solo prepara el módulo.
    if let Some(dir) = &args.bench_capture {
        match bench_capture::init(Path::new(dir)) {
            Ok(()) => eprintln!(
                "server_realms: bench-capture activo en {dir} — hooks del canal pendientes \
                 (TODO en bench_capture.rs; sin open_conn la captura es no-op)"
            ),
            Err(e) => {
                eprintln!("server_realms: bench-capture: {e}");
                return ExitCode::from(2);
            }
        }
    }
    match args.role {
        Role::Auth => {
            let cfg = match Config::load(&args.config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("server_realms: {e}");
                    return ExitCode::from(2);
                }
            };
            if let Err(e) = auth::run(cfg).await {
                eprintln!("server_realms: {e}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Role::Channel => {
            let cfg = match Config::load(&args.config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("server_realms: {e}");
                    return ExitCode::from(2);
                }
            };
            if let Err(e) = channel::run(cfg).await {
                eprintln!("server_realms: {e}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_role_and_config_path() {
        assert_eq!(parse_args(&[]).unwrap().role, Role::Auth);
        assert_eq!(parse_args(&[]).unwrap().config_path, "server_realms.toml");
        assert_eq!(parse_args(&[]).unwrap().bench_capture, None);
    }

    #[test]
    fn parses_explicit_roles() {
        assert_eq!(
            parse_args(&[
                "--role".into(),
                "auth".into(),
                "--config".into(),
                "a.toml".into()
            ])
            .unwrap()
            .config_path,
            "a.toml"
        );
        assert_eq!(
            parse_args(&["--role".into(), "channel".into()])
                .unwrap()
                .role,
            Role::Channel
        );
    }

    #[test]
    fn rejects_invalid_role_and_args() {
        assert!(parse_args(&["--role".into(), "game".into()]).is_err());
        assert!(parse_args(&["--role".into()]).is_err());
        assert!(parse_args(&["--bogus".into()]).is_err());
        assert!(parse_args(&["--config".into()]).is_err());
    }

    #[test]
    fn parses_bench_capture_flag() {
        let a = parse_args(&[
            "--role".into(),
            "channel".into(),
            "--bench-capture".into(),
            "capture".into(),
        ])
        .unwrap();
        assert_eq!(a.bench_capture.as_deref(), Some("capture"));
        assert!(
            parse_args(&["--bench-capture".into()]).is_err(),
            "falta el valor"
        );
    }
}

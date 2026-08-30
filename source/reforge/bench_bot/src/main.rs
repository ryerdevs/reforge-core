//! `bench_bot` — simulador wire-level de N bots (harness provisional del
//! benchmark F5, ADR-0010 §2). Cada bot es un CLIENTE real del protocolo
//! legacy: auth → canal → select → mundo → CG_MOVE periódico, contra los
//! servidores Rust en vivo (`server_realms --role auth|channel`).
//!
//! Uso (sin clap — parseo std, patrón de `server_realms`):
//!
//! ```text
//! bench_bot --bots 10 --duration 60 --create-accounts 10
//! bench_bot --bots 1 --duration 5 --login test --password 1234   # smoke (cuenta real)
//! bench_bot --cleanup-accounts                                   # borra bench_*
//! ```
//!
//! Flags:
//! - `--bots N` (default 1) — bots concurrentes; el bot i usa `{prefix}{i}`
//!   (pegado, sin separador: el auth solo acepta logins alfanuméricos).
//! - `--duration S` (default 30) — segundos del loop de juego.
//! - `--login L` — login explícito (smoke; fuerza `--bots 1`).
//! - `--login-prefix P` (default "bench") — prefijo de cuentas desechables.
//! - `--password PW` (default "1234").
//! - `--auth ADDR` / `--channel ADDR` (defaults 127.0.0.1:30001/30003).
//! - `--move-interval-ms M` (default 1000) — intervalo del CG_MOVE.
//! - `--walk-speed U` (default 200) — velocidad del paseo en units/s: el
//!   paso por MOVE = `U × intervalo / 1000`; 200 u/s queda dentro del
//!   envelope del server (margen 1.98× a 1000 ms — ver bot.rs); 300 = la
//!   velocidad exacta del server; > 300 ejercita el rechazo (test negativo).
//! - `--timeout-s S` (default 20) — timeout de silencio por fase
//!   (connect/auth/select/entry/game): un bot que no recibe bytes en S
//!   segundos falla como `timeout` en vez de colgar para siempre (clave en
//!   el escenario PG caído).
//! - `--expect-failures CLASES` — modo de aserción: las clases válidas son
//!   `auth_fail,login_fail,no_char,world_fail,desync,disconnected,timeout`.
//!   Si TODOS los fallos del run son de una clase esperada → exit 0; si
//!   aparece una clase NO esperada → exit 3; sin `--expect-failures`,
//!   cualquier fallo → exit 1.
//! - `--summary` — imprime solo la línea de resumen agregada
//!   (ok/total + auth/sel/world min/median/p95/max) en vez de la tabla por
//!   bot — la lectura de la escalera sin abrir archivos.
//! - `--mobs-density D` (default 10) — dimensión de densidad del escenario:
//!   aceptada y registrada en el reporte; el escenario completo (spawn
//!   dinámico + sharding) es el follow-up del benchmark F5.
//! - `--create-accounts N` — provisiona N cuentas `{prefix}{i}` + personaje
//!   (PG; idempotente). `--cleanup-accounts` las borra.
//! - `--pg CONN` (default `host=127.0.0.1 port=5432 user=mt2 password=mt2
//!   dbname=metin2`) — solo para provisionar/limpiar.
//! - `--json PATH` — escribe el reporte JSON a un archivo.
//! - `--help` — esta ayuda.
//!
//! Exit code: 0 si todos los bots OK (o todos los fallos son de clase
//! esperada con `--expect-failures`), 1 si hubo fallos, 2 error de args,
//! 3 fallo de clase NO esperada (assertion).
//!
//! # Modo de fallo (PG caído) — receta MANUAL (el harness NO toca servicios)
//!
//! ```powershell
//! Stop-Service postgresql-metin2
//! bench_bot --bots 1 --duration 8 --timeout-s 8 --expect-failures timeout
//! Start-Service postgresql-metin2
//! ```
//!
//! El run intermedio debe terminar con fallos de clase `timeout` (o
//! `login_fail` si el auth alcanza a responder antes de morir) y exit 0
//! (todos esperados); una clase distinta (p.ej. `desync`) dispararía exit 3
//! — la aserción detecta degradación de protocolo, no solo caída. Tras el
//! arranque, repetir el smoke (`--bots 1 --duration 5 --login test`) → `ok`
//! de nuevo (recuperación).
//!
//! # Escenario sharded-regions — CONTRATO para la lane del canal (no implementado)
//!
//! El caso "2 regiones" necesita el lado server: dos canales/regiones
//! (worldes separados), bots repartidos entre ambos y aserciones cross-
//! región (un jugador de la región A no ve los mobs de la B; warp/chat
//! entre regiones se rechaza). El harness de hoy apunta a UN canal
//! (`--channel`); lo que la lane del canal debe ofrecer para el test:
//! - un flag de config por canal (`--channel2` o lista) y worldes distintos
//!   por región;
//! - la región en los reportes del canal (para los reportes del bot);
//! - un modo `--regions N` en el harness que reparte los bots (round-robin)
//!   entre los N canales y agrega las métricas por región (world_ms/auth_ms
//!   por región + comparación).
//!
//! Hasta que exista, el harness NO emula sharding con varias conexiones por
//! bot (cada bot es un cliente real de UN canal — eso ya lo cubre `--bots N`).

mod accounts;
mod bot;
mod report;
mod splitter;

use std::path::Path;
use std::time::Duration;

use bot::{BotConfig, run_bot};
use report::{render_json, render_summary_line, render_table, summarize};

const DEFAULT_AUTH: &str = "127.0.0.1:30001";
const DEFAULT_CHANNEL: &str = "127.0.0.1:30003";
/// Velocidad del paseo por defecto (units/s) — dentro del envelope (ver bot.rs).
const DEFAULT_WALK_SPEED: u32 = 200;
/// Timeout de fase por defecto (s) — el knob `--timeout-s`.
const DEFAULT_TIMEOUT_S: u64 = 20;

/// Clases de fallo válidas para `--expect-failures` (los labels de `Status`).
const EXPECTABLE_FAILURES: &[&str] = &[
    "auth_fail",
    "login_fail",
    "no_char",
    "world_fail",
    "desync",
    "disconnected",
    "timeout",
];

#[derive(Debug, Clone, PartialEq)]
struct Args {
    bots: usize,
    duration_s: u64,
    login: Option<String>,
    login_prefix: String,
    password: String,
    auth: String,
    channel: String,
    move_interval_ms: u64,
    walk_speed: u32,
    timeout_s: u64,
    expect_failures: Vec<String>,
    summary: bool,
    mobs_density: u32,
    create_accounts: Option<usize>,
    cleanup_accounts: bool,
    pg: Option<String>,
    json: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            bots: 1,
            duration_s: 30,
            login: None,
            login_prefix: "bench".into(),
            password: "1234".into(),
            auth: DEFAULT_AUTH.into(),
            channel: DEFAULT_CHANNEL.into(),
            move_interval_ms: 1_000,
            walk_speed: DEFAULT_WALK_SPEED,
            timeout_s: DEFAULT_TIMEOUT_S,
            expect_failures: Vec::new(),
            summary: false,
            mobs_density: 10,
            create_accounts: None,
            cleanup_accounts: false,
            pg: None,
            json: None,
        }
    }
}

fn usage() -> String {
    "uso: bench_bot [--bots N] [--duration S] [--login L | --login-prefix P] [--password PW] \
     [--auth ADDR] [--channel ADDR] [--move-interval-ms M] [--walk-speed U] [--timeout-s S] \
     [--expect-failures auth_fail,login_fail,timeout] [--summary] [--mobs-density D] \
     [--create-accounts N] [--cleanup-accounts] [--pg CONN] [--json PATH] [--help]\n\
     (--login fuerza --bots 1; las cuentas <prefix><i> son desechables — ver accounts.rs;\
     escenarios PG caído y sharded-regions: ver el doc del módulo)"
        .into()
}

fn parse_u64(arg: &str, v: &str, what: &str) -> Result<u64, String> {
    v.parse::<u64>()
        .map_err(|_| format!("{what}: '{v}' no es un entero"))
        .and_then(|x| {
            if x == 0 {
                Err(format!("{what}: debe ser > 0"))
            } else {
                Ok(x)
            }
        })
        .and_then(|x| {
            if x > 1_000_000 {
                Err(format!("{what}: {v} es irrealmente grande"))
            } else {
                Ok(x)
            }
        })
        .map_err(|e| format!("{arg}: {e}"))
}

/// Valida la lista de clases de `--expect-failures` (los labels de `Status`;
/// `ok` no es un fallo y no se acepta).
fn parse_expect_failures(arg: &str, v: &str) -> Result<Vec<String>, String> {
    let labels: Vec<String> = v
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if labels.is_empty() {
        return Err(format!(
            "{arg}: lista vacía (p.ej. --expect-failures login_fail,timeout)"
        ));
    }
    for l in &labels {
        if !EXPECTABLE_FAILURES.contains(&l.as_str()) {
            return Err(format!(
                "{arg}: '{l}' no es una clase de fallo (válidas: {})",
                EXPECTABLE_FAILURES.join(",")
            ));
        }
    }
    Ok(labels)
}

/// Fallos observados cuya clase NO está en `expected` (assertion del modo
/// `--expect-failures`). Vacío = la aserción pasa.
fn unexpected_failures(
    reports: &[report::BotReport],
    expected: &[String],
) -> Vec<(usize, String, String)> {
    reports
        .iter()
        .filter(|r| r.failed())
        .filter(|r| !expected.iter().any(|e| e == r.status.label()))
        .map(|r| (r.index, r.status.label().to_string(), r.note.clone()))
        .collect()
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut a = Args::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut next_val = |what: &str| -> Result<String, String> {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{arg}: falta el valor de {what}"))
        };
        match arg.as_str() {
            "--help" | "-h" => return Err("HELP".into()),
            "--bots" => a.bots = parse_u64(arg, &next_val("--bots")?, "--bots")? as usize,
            "--duration" => a.duration_s = parse_u64(arg, &next_val("--duration")?, "--duration")?,
            "--login" => a.login = Some(next_val("--login")?),
            "--login-prefix" => a.login_prefix = next_val("--login-prefix")?,
            "--password" => a.password = next_val("--password")?,
            "--auth" => a.auth = next_val("--auth")?,
            "--channel" => a.channel = next_val("--channel")?,
            "--move-interval-ms" => {
                a.move_interval_ms =
                    parse_u64(arg, &next_val("--move-interval-ms")?, "--move-interval-ms")?
            }
            "--walk-speed" => {
                a.walk_speed = parse_u64(arg, &next_val("--walk-speed")?, "--walk-speed")? as u32
            }
            "--timeout-s" => {
                a.timeout_s = parse_u64(arg, &next_val("--timeout-s")?, "--timeout-s")?
            }
            "--expect-failures" => {
                a.expect_failures = parse_expect_failures(arg, &next_val("--expect-failures")?)?
            }
            "--summary" => a.summary = true,
            "--mobs-density" => {
                a.mobs_density =
                    parse_u64(arg, &next_val("--mobs-density")?, "--mobs-density")? as u32
            }
            "--create-accounts" => {
                a.create_accounts = Some(parse_u64(
                    arg,
                    &next_val("--create-accounts")?,
                    "--create-accounts",
                )? as usize)
            }
            "--cleanup-accounts" => a.cleanup_accounts = true,
            "--pg" => a.pg = Some(next_val("--pg")?),
            "--json" => a.json = Some(next_val("--json")?),
            other => return Err(format!("argumento desconocido: {other}\n{}", usage())),
        }
    }
    if a.login.is_some() && a.bots != 1 {
        a.bots = 1; // un login explícito = una sesión (smoke)
    }
    if a.bots > 10_000 {
        return Err(format!("--bots: {0} excede el límite de 10000", a.bots));
    }
    if a.walk_speed > 1_000 {
        return Err(format!(
            "--walk-speed: {} excede el límite de 1000 u/s (300 = la velocidad del server; \
             por encima el envelope del canal rechaza los MOVE — test negativo deliberado)",
            a.walk_speed
        ));
    }
    if a.timeout_s > 600 {
        return Err(format!(
            "--timeout-s: {} excede el límite de 600 s",
            a.timeout_s
        ));
    }
    accounts::validate_prefix(&a.login_prefix)?;
    Ok(a)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Err(e) if e == "HELP" => {
            println!("{}", usage());
            return std::process::ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("bench_bot: {e}");
            return std::process::ExitCode::from(2);
        }
        Ok(a) => a,
    };
    let pg = args
        .pg
        .clone()
        .unwrap_or_else(|| accounts::DEFAULT_PG.into());

    // Provisionar / limpiar cuentas desechables (PG; antes de correr los bots).
    if args.cleanup_accounts {
        match accounts::cleanup_accounts(&pg, &args.login_prefix).await {
            Ok(n) => println!(
                "bench_bot: cleanup: {n} cuentas '{}*' borradas",
                args.login_prefix
            ),
            Err(e) => {
                eprintln!("bench_bot: cleanup falló: {e}");
                return std::process::ExitCode::from(1);
            }
        }
        return std::process::ExitCode::SUCCESS;
    }
    if let Some(n) = args.create_accounts {
        match accounts::create_accounts(&pg, &args.login_prefix, n, &args.password).await {
            Ok(accs) => {
                println!(
                    "bench_bot: {n} cuentas '{}0..{}' listas (password '{}')",
                    args.login_prefix,
                    n - 1,
                    args.password
                );
                for a in &accs {
                    println!("  {} id={} char={}", a.login, a.account_id, a.player_id);
                }
            }
            Err(e) => {
                eprintln!("bench_bot: create-accounts falló: {e}");
                return std::process::ExitCode::from(1);
            }
        }
    }

    // Config de cada bot: login explícito (smoke) o `{prefix}_{i}`.
    let login_of = |i: usize| -> String {
        match &args.login {
            Some(l) => l.clone(),
            None => accounts::bench_login(&args.login_prefix, i),
        }
    };
    let cfg = |i: usize| BotConfig {
        auth_addr: args.auth.clone(),
        channel_addr: args.channel.clone(),
        login: login_of(i),
        password: args.password.clone(),
        duration: Duration::from_secs(args.duration_s),
        move_interval: Duration::from_millis(args.move_interval_ms),
        walk_speed: args.walk_speed,
        timeout: Duration::from_secs(args.timeout_s),
    };

    println!(
        "bench_bot: {} bot(s), duración {}s, auth {}, canal {}, move cada {}ms, paseo {} u/s, \
         timeout {}s, densidad mobs {}",
        args.bots,
        args.duration_s,
        args.auth,
        args.channel,
        args.move_interval_ms,
        args.walk_speed,
        args.timeout_s,
        args.mobs_density
    );

    // Correr todos los bots en paralelo (una tarea por bot).
    let tasks: Vec<_> = (0..args.bots)
        .map(|i| tokio::spawn(run_bot(cfg(i), i)))
        .collect();
    let mut reports = Vec::with_capacity(args.bots);
    let grace = Duration::from_secs(args.duration_s + 120);
    for t in tasks {
        match tokio::time::timeout(grace, t).await {
            Ok(Ok(r)) => reports.push(r),
            Ok(Err(e)) => {
                eprintln!("bench_bot: join de la tarea falló: {e}");
                return std::process::ExitCode::from(1);
            }
            Err(_) => {
                eprintln!("bench_bot: timeout global del run ({grace:?}) — reporte parcial");
                break;
            }
        }
    }

    let summary = summarize(&reports);
    if args.summary {
        println!("{}", render_summary_line(&summary));
    } else {
        print!("{}", render_table(&reports, &summary));
    }

    if let Some(path) = &args.json {
        let meta = vec![
            ("bots", args.bots.to_string()),
            ("duration_s", args.duration_s.to_string()),
            ("login_prefix", args.login_prefix.clone()),
            ("password", args.password.clone()),
            ("auth", args.auth.clone()),
            ("channel", args.channel.clone()),
            ("move_interval_ms", args.move_interval_ms.to_string()),
            ("walk_speed", args.walk_speed.to_string()),
            ("timeout_s", args.timeout_s.to_string()),
            ("expect_failures", args.expect_failures.join(",")),
            ("mobs_density", args.mobs_density.to_string()),
        ];
        let json = render_json(&reports, &summary, &meta);
        if let Err(e) = std::fs::write(Path::new(path), json) {
            eprintln!("bench_bot: no pude escribir {path}: {e}");
            return std::process::ExitCode::from(1);
        }
        println!("bench_bot: reporte JSON en {path}");
    }

    // Aserción `--expect-failures`: los fallos observados deben ser de una
    // clase esperada (exit 0 si todos lo son); una clase inesperada → exit 3.
    let unexpected = unexpected_failures(&reports, &args.expect_failures);
    if !unexpected.is_empty() {
        let expected_list = if args.expect_failures.is_empty() {
            "ninguna (run limpio)".to_string()
        } else {
            args.expect_failures.join(",")
        };
        eprintln!(
            "bench_bot: assertion FALLIDA — {} fallo(s) de clase NO esperada (esperadas: {}):",
            unexpected.len(),
            expected_list
        );
        for (i, st, note) in &unexpected {
            eprintln!("  bot {i}: {st} ({note})");
        }
        return std::process::ExitCode::from(3);
    }
    if summary.failed > 0 {
        if !args.expect_failures.is_empty() {
            println!(
                "bench_bot: assertion OK — {} fallo(s), todos de clase esperada ({})",
                summary.failed,
                args.expect_failures.join(",")
            );
            return std::process::ExitCode::SUCCESS;
        }
        return std::process::ExitCode::from(1);
    }
    if !args.expect_failures.is_empty() {
        println!(
            "bench_bot: assertion OK — 0 fallos (esperados: {})",
            args.expect_failures.join(",")
        );
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<Args, String> {
        parse_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn defaults() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.bots, 1);
        assert_eq!(a.duration_s, 30);
        assert_eq!(a.login, None);
        assert_eq!(a.login_prefix, "bench");
        assert_eq!(a.password, "1234");
        assert_eq!(a.auth, DEFAULT_AUTH);
        assert_eq!(a.channel, DEFAULT_CHANNEL);
        assert_eq!(a.move_interval_ms, 1_000);
        assert_eq!(a.walk_speed, DEFAULT_WALK_SPEED);
        assert_eq!(a.timeout_s, DEFAULT_TIMEOUT_S);
        assert!(a.expect_failures.is_empty());
        assert!(!a.summary);
        assert_eq!(a.mobs_density, 10);
        assert_eq!(a.create_accounts, None);
        assert!(!a.cleanup_accounts);
        assert_eq!(a.json, None);
    }

    #[test]
    fn parses_all_flags() {
        let a = parse(&[
            "--bots",
            "8",
            "--duration",
            "60",
            "--login-prefix",
            "bm",
            "--password",
            "pw9",
            "--auth",
            "127.0.0.1:39999",
            "--channel",
            "127.0.0.1:39998",
            "--move-interval-ms",
            "250",
            "--walk-speed",
            "250",
            "--timeout-s",
            "8",
            "--expect-failures",
            "login_fail,timeout",
            "--summary",
            "--mobs-density",
            "42",
            "--create-accounts",
            "8",
            "--json",
            "r.json",
        ])
        .unwrap();
        assert_eq!(a.bots, 8);
        assert_eq!(a.duration_s, 60);
        assert_eq!(a.login_prefix, "bm");
        assert_eq!(a.password, "pw9");
        assert_eq!(a.auth, "127.0.0.1:39999");
        assert_eq!(a.channel, "127.0.0.1:39998");
        assert_eq!(a.move_interval_ms, 250);
        assert_eq!(a.walk_speed, 250);
        assert_eq!(a.timeout_s, 8);
        assert_eq!(
            a.expect_failures,
            vec!["login_fail".to_string(), "timeout".to_string()]
        );
        assert!(a.summary);
        assert_eq!(a.mobs_density, 42);
        assert_eq!(a.create_accounts, Some(8));
        assert_eq!(a.json.as_deref(), Some("r.json"));
    }

    #[test]
    fn explicit_login_forces_single_bot() {
        let a = parse(&["--bots", "5", "--login", "test"]).unwrap();
        assert_eq!(a.bots, 1, "--login es una sesión (smoke)");
        assert_eq!(a.login.as_deref(), Some("test"));
    }

    #[test]
    fn rejects_bad_args() {
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["--bots"]).is_err(), "falta valor");
        assert!(parse(&["--bots", "0"]).is_err(), "cero");
        assert!(parse(&["--bots", "abc"]).is_err(), "no entero");
        assert!(parse(&["--duration", "0"]).is_err());
        assert!(
            parse(&["--bots", "999999999999"]).is_err(),
            "fuera de rango"
        );
        assert!(parse(&["--cleanup-accounts", "--create-accounts", "3"]).is_ok());
        // Prefijo demasiado largo → error de validación.
        assert!(parse(&["--login-prefix", "abcdefghijklmnopqrst"]).is_err());
    }

    #[test]
    fn rejects_bad_new_knobs() {
        assert!(parse(&["--walk-speed", "0"]).is_err(), "cero");
        assert!(
            parse(&["--walk-speed", "1001"]).is_err(),
            "por encima del límite"
        );
        assert!(parse(&["--walk-speed", "abc"]).is_err(), "no entero");
        assert!(parse(&["--timeout-s", "0"]).is_err(), "cero");
        assert!(
            parse(&["--timeout-s", "601"]).is_err(),
            "por encima del límite"
        );
        assert!(
            parse(&["--expect-failures", "bogus"]).is_err(),
            "clase inválida"
        );
        assert!(
            parse(&["--expect-failures", "ok"]).is_err(),
            "ok no es un fallo"
        );
        assert!(parse(&["--expect-failures", ""]).is_err(), "lista vacía");
        assert!(parse(&["--expect-failures", "timeout,desync"]).is_ok());
        assert!(parse(&["--summary"]).is_ok());
        assert!(
            parse(&["--summary", "--bots", "2"]).is_ok(),
            "--summary no consume valor"
        );
        assert!(
            parse(&["--summary", "x"]).is_err(),
            "'x' es un argumento desconocido"
        );
    }

    #[test]
    fn cleanup_flag_parses() {
        let a = parse(&["--cleanup-accounts", "--pg", "host=x"]).unwrap();
        assert!(a.cleanup_accounts);
        assert_eq!(a.pg.as_deref(), Some("host=x"));
    }

    #[test]
    fn unexpected_failures_classifies_by_expected_set() {
        use report::Status;
        let mk = |i: usize, st: Status| report::BotReport {
            index: i,
            login: format!("bench{i}"),
            status: st,
            note: "x".into(),
            auth_ms: None,
            channel_login_ms: None,
            select_ms: None,
            world_ms: None,
            alive_ms: 0,
            rx_packets: 0,
            rx_bytes: 0,
            tx_packets: 0,
            tx_bytes: 0,
            moves: 0,
            pings: 0,
            spawns: 0,
        };
        let reps = vec![
            mk(0, Status::Ok),
            mk(1, Status::LoginFailed),
            mk(2, Status::Timeout),
            mk(3, Status::Desync),
        ];
        // Solo login_fail/timeout esperados → el desync es inesperado.
        let exp = vec!["login_fail".to_string(), "timeout".to_string()];
        let un = unexpected_failures(&reps, &exp);
        assert_eq!(un.len(), 1);
        assert_eq!(un[0].0, 3, "el bot 3 (desync) no está en las esperadas");
        assert_eq!(un[0].1, "desync");
        // Sin expectativas → todo fallo es inesperado.
        assert_eq!(unexpected_failures(&reps, &[]).len(), 3);
        // Todas las clases cubiertas → aserción OK.
        let all = vec![
            "login_fail".to_string(),
            "timeout".to_string(),
            "desync".to_string(),
        ];
        assert!(unexpected_failures(&reps, &all).is_empty());
    }
}

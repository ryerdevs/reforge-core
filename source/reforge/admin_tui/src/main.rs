//! admin_tui v1 - operator panel for the reforge-core deploy.

#![forbid(unsafe_code)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod app;
mod logs;
mod ops;
mod process;
mod ui;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    let deploy_dir = deploy_dir_from_args(&args);
    eprintln!("admin_tui v0.1.0 - deploy dir: {}", deploy_dir.display());

    let mut skip_next = false;
    let mut subcmd = None;
    for arg in &args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--deploy-dir" || arg == "-d" {
            skip_next = true;
            continue;
        }
        if !arg.starts_with('-') && subcmd.is_none() {
            subcmd = Some(arg.as_str());
        }
    }

    if let Some(cmd) = subcmd {
        return match cmd {
            "start" => run_cli_op("start", ops::do_start(&deploy_dir)),
            "stop" => run_cli_op("stop", ops::do_stop(&deploy_dir)),
            "restart" => run_cli_op("restart", ops::do_restart(&deploy_dir)),
            "status" => probe(&deploy_dir),
            "backup" => run_cli_op("backup", ops::do_backup(&deploy_dir)),
            "postgres" => run_cli_op("postgres", ops::do_postgres(&deploy_dir)),
            "doctor" => run_doctor(&deploy_dir),
            "db-init" => run_cli_op("db-init", ops::do_db_init(&deploy_dir)),
            "db-seed" => run_cli_op("db-seed", ops::do_db_seed(&deploy_dir)),
            "db-reset" => run_cli_op("db-reset", ops::do_db_reset(&deploy_dir)),
            "db-check" => run_cli_op("db-check", ops::do_db_check(&deploy_dir)),
            other => {
                eprintln!("admin_tui: unknown command '{other}'");
                ExitCode::FAILURE
            }
        };
    }

    if args.iter().any(|a| a == "--probe" || a == "--status") {
        return probe(&deploy_dir);
    }
    if args.iter().any(|a| a == "--start") {
        return run_cli_op("start", ops::do_start(&deploy_dir));
    }
    if args.iter().any(|a| a == "--stop") {
        return run_cli_op("stop", ops::do_stop(&deploy_dir));
    }
    if args.iter().any(|a| a == "--restart") {
        return run_cli_op("restart", ops::do_restart(&deploy_dir));
    }
    if args.iter().any(|a| a == "--backup") {
        return run_cli_op("backup", ops::do_backup(&deploy_dir));
    }
    if args.iter().any(|a| a == "--doctor") {
        return run_doctor(&deploy_dir);
    }

    let mut app = app::App::new(deploy_dir);
    if let Err(e) = ui::run(&mut app) {
        eprintln!("admin_tui: TUI error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn run_cli_op(label: &str, res: process::OpResult) -> ExitCode {
    match res {
        process::OpResult::Ok(msg) => {
            println!("{label}: OK - {msg}");
            ExitCode::SUCCESS
        }
        process::OpResult::Failed(msg) => {
            eprintln!("{label}: FAILED - {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run_doctor(deploy_dir: &Path) -> ExitCode {
    println!("=== Reforge Environment Doctor ===");
    println!("Deploy Bundle  : {}", deploy_dir.display());
    let pg = process::is_postgres_running();
    println!(
        "PostgreSQL 5432: {}",
        if pg {
            "[OK] Responding"
        } else {
            "[WARN] Not responding"
        }
    );
    let exe = process::find_server_realms_exe(deploy_dir);
    println!(
        "server_realms  : {}",
        if let Some(p) = exe {
            format!("[OK] {}", p.display())
        } else {
            "[FAIL] Not found in deploy_dir or target/".to_string()
        }
    );
    let auth = process::find_config(deploy_dir, "auth.toml");
    let ch = process::find_config(deploy_dir, "channel.toml");
    println!(
        "Config auth    : {}",
        if let Some(p) = auth {
            format!("[OK] {}", p.display())
        } else {
            "[FAIL] Missing auth.toml".to_string()
        }
    );
    println!(
        "Config channel : {}",
        if let Some(p) = ch {
            format!("[OK] {}", p.display())
        } else {
            "[FAIL] Missing channel.toml".to_string()
        }
    );
    let logs_dir = deploy_dir.join("logs");
    let write_ok = std::fs::create_dir_all(&logs_dir).is_ok();
    println!(
        "Logs Directory : {}",
        if write_ok {
            format!("[OK] Writeable ({})", logs_dir.display())
        } else {
            "[FAIL] Cannot write".to_string()
        }
    );
    println!("==================================");
    ExitCode::SUCCESS
}

fn probe(deploy_dir: &Path) -> ExitCode {
    let pg = process::is_postgres_running();
    let auth = process::status(process::Role::Auth);
    let channel = process::status(process::Role::Channel);
    println!("postgres: {}", if pg { "running" } else { "stopped" });
    println!("auth    : {:?}", auth);
    println!("channel : {:?}", channel);
    if let Ok(lines) = logs::tail(&logs::latest_log(deploy_dir, process::Role::Auth)) {
        println!("auth log lines: {}", lines.len());
    }
    if let Ok(dumps) = logs::list_dumps() {
        println!("dumps available: {}", dumps.len());
    }
    ExitCode::SUCCESS
}

fn deploy_dir_from_args(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--deploy-dir" || a == "-d")
        && let Some(v) = args.get(i + 1)
    {
        return PathBuf::from(v);
    }
    env::var("REFORGE_DEPLOY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| discover_deploy_dir())
}

fn discover_deploy_dir() -> PathBuf {
    if let Ok(exe) = env::current_exe() {
        for ancestor in exe.ancestors() {
            if (ancestor.ends_with("deploy/win") || ancestor.ends_with(r"deploy\win"))
                && ancestor.is_dir()
            {
                return ancestor.to_path_buf();
            }
            let candidate = ancestor.join("source").join("deploy").join("win");
            if candidate.is_dir() {
                return candidate;
            }
        }
    }
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for ancestor in cwd.ancestors() {
        if (ancestor.ends_with("deploy/win") || ancestor.ends_with(r"deploy\win"))
            && ancestor.is_dir()
        {
            return ancestor.to_path_buf();
        }
        let candidate = ancestor.join("source").join("deploy").join("win");
        if candidate.is_dir() {
            return candidate;
        }
    }
    cwd.join("source").join("deploy").join("win")
}

fn print_help() {
    println!("admin_tui - reforge-core operator panel");
    println!();
    println!("USAGE:");
    println!("    admin_tui [SUBCOMMAND] [--deploy-dir <PATH>]");
    println!();
    println!("SUBCOMMANDS:");
    println!("    start        Start PostgreSQL, auth, and channel in background");
    println!("    stop         Stop running server_realms processes");
    println!("    restart      Restart server_realms");
    println!("    status       Show status probe (no TUI)");
    println!("    backup       Create PostgreSQL dump backup in backups/");
    println!("    postgres     Start PostgreSQL service");
    println!("    doctor       Run system health checks");
    println!("    db-init      Create database and apply versioned schema DDL");
    println!("    db-seed      Load minimal lawful synthetic development seed");
    println!("    db-reset     Drop and recreate database from schema + seed");
    println!("    db-check     Verify database connectivity, schemas, and counts");
    println!();
    println!("OPTIONS:");
    println!("    -d, --deploy-dir <PATH>   Deploy dir (default: REFORGE_DEPLOY_DIR).");
    println!("    -h, --help                Print this help.");
    println!("    --probe                   Run status + log probe and exit (no TUI).");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_dir_from_args_explicit_flag() {
        let args = vec![
            "--deploy-dir".to_string(),
            r"C:\custom\deploy\path".to_string(),
        ];
        assert_eq!(
            deploy_dir_from_args(&args),
            PathBuf::from(r"C:\custom\deploy\path")
        );
    }

    #[test]
    fn deploy_dir_from_args_short_flag() {
        let args = vec!["-d".to_string(), r"D:\alt\path".to_string()];
        assert_eq!(deploy_dir_from_args(&args), PathBuf::from(r"D:\alt\path"));
    }
}

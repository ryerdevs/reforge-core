//! admin_tui v1 - operator panel for the reforge-core deploy.
//!
//! Sub-modules (process, logs, ops) provide the controller layer.
//! The TUI event loop lives in `ui` (added in the next commit).
//! This commit only wires the CLI so the modules can be smoke-tested
//! before the TUI exists.

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

mod logs;
mod ops;
mod process;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return ExitCode::SUCCESS;
    }
    let deploy_dir = deploy_dir_from_args(&args);
    eprintln!("admin_tui v0.1.0 - deploy dir: {}", deploy_dir.display());

    // CLI smoke (not the TUI): if --probe is given, run a couple of
    // status checks and print the result. Useful for CI / scripts.
    if let Some(idx) = args.iter().position(|a| a == "--probe") {
        return probe(&deploy_dir);
    }

    eprintln!("the TUI loop is not yet wired (this commit only ships the controller).");
    ExitCode::SUCCESS
}

fn probe(deploy_dir: &PathBuf) -> ExitCode {
    let auth = process::status(process::Role::Auth);
    let channel = process::status(process::Role::Channel);
    println!("auth    : {:?}", auth);
    println!("channel : {:?}", channel);
    if let Ok(lines) = logs::tail(&logs::log_path(deploy_dir, process::Role::Auth)) {
        println!("auth log lines: {}", lines.len());
    }
    if let Ok(dumps) = logs::list_dumps() {
        println!("dumps available: {}", dumps.len());
        for d in dumps.iter().take(5) {
            println!("  - {}", d);
        }
    }
    ExitCode::SUCCESS
}

fn deploy_dir_from_args(args: &[String]) -> PathBuf {
    if let Some(i) = args.iter().position(|a| a == "--deploy-dir" || a == "-d") {
        if let Some(v) = args.get(i + 1) {
            return PathBuf::from(v);
        }
    }
    env::var("REFORGE_DEPLOY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cwd_relative_deploy())
}

fn cwd_relative_deploy() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let last = cwd.file_name().and_then(|n| n.to_str());
    match last {
        Some("reforge") => cwd.join("deploy").join("win"),
        Some("admin_tui") => cwd.join("..").join("..").join("deploy").join("win"),
        _ => cwd.join("source").join("reforge").join("..").join("deploy").join("win"),
    }
}

fn print_help() {
    println!("admin_tui - reforge-core operator panel");
    println!();
    println!("USAGE:");
    println!("    admin_tui [--deploy-dir <PATH>]");
    println!();
    println!("OPTIONS:");
    println!("    -d, --deploy-dir <PATH>   Deploy dir (default: REFORGE_DEPLOY_DIR).");
    println!("    -h, --help                 Print this help.");
    println!("    --probe                    Run status + log probe and exit (no TUI).");
}
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

    if args.iter().any(|a| a == "--probe") {
        return probe(&deploy_dir);
    }

    let mut app = app::App::new(deploy_dir);
    if let Err(e) = ui::run(&mut app) {
        eprintln!("admin_tui: TUI error: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn probe(deploy_dir: &Path) -> ExitCode {
    let auth = process::status(process::Role::Auth);
    let channel = process::status(process::Role::Channel);
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
        .unwrap_or_else(|_| cwd_relative_deploy())
}

fn cwd_relative_deploy() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for ancestor in cwd.ancestors() {
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
    println!("    admin_tui [--deploy-dir <PATH>]");
    println!();
    println!("OPTIONS:");
    println!("    -d, --deploy-dir <PATH>   Deploy dir (default: REFORGE_DEPLOY_DIR).");
    println!("    -h, --help                 Print this help.");
    println!("    --probe                    Run status + log probe and exit (no TUI).");
}

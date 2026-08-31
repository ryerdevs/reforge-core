//! admin_tui v1 - operator panel for the reforge-core deploy.

#![forbid(unsafe_code)]

use std::env;
use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return std::process::ExitCode::SUCCESS;
    }
    let deploy_dir = deploy_dir_from_args(&args);
    eprintln!("admin_tui v0.1.0 - deploy dir: {}", deploy_dir.display());
    std::process::ExitCode::SUCCESS
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
}
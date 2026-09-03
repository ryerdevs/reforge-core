//! Operator actions delegated to the PowerShell scripts.

use std::path::Path;

use super::process::{OpResult, backup, restart, start, stop};

pub fn do_start(deploy_dir: &Path) -> OpResult {
    start(deploy_dir)
}

pub fn do_stop(deploy_dir: &Path) -> OpResult {
    stop(deploy_dir)
}

pub fn do_restart(deploy_dir: &Path) -> OpResult {
    restart(deploy_dir)
}

pub fn do_backup(deploy_dir: &Path) -> OpResult {
    backup(deploy_dir)
}

pub fn do_postgres(_deploy_dir: &Path) -> OpResult {
    if !crate::process::is_postgres_running() {
        match crate::process::ensure_postgres_running() {
            Ok(()) => OpResult::Ok("PostgreSQL started successfully".to_string()),
            Err(e) => OpResult::Failed(format!("Failed to start PostgreSQL: {e}")),
        }
    } else {
        OpResult::Ok("PostgreSQL is already running".to_string())
    }
}

pub fn do_doctor(deploy_dir: &Path) -> OpResult {
    let pg = crate::process::is_postgres_running();
    let exe = crate::process::find_server_realms_exe(deploy_dir).is_some();
    let auth = crate::process::find_config(deploy_dir, "auth.toml").is_some();
    let ch = crate::process::find_config(deploy_dir, "channel.toml").is_some();
    if pg && exe && auth && ch {
        OpResult::Ok("all checks passed (PG, binary, configs OK)".to_string())
    } else {
        let mut missing = Vec::new();
        if !pg {
            missing.push("PG down");
        }
        if !exe {
            missing.push("server_realms missing");
        }
        if !auth {
            missing.push("auth.toml missing");
        }
        if !ch {
            missing.push("channel.toml missing");
        }
        OpResult::Failed(format!("issues found: {}", missing.join(", ")))
    }
}

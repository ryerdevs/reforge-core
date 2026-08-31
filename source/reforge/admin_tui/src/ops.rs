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

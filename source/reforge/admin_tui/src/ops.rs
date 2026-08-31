//! Operator actions: backup + restore (delegates to the PowerShell
//! scripts the operator would run by hand).

#![allow(dead_code)] // do_restore wired in commit 4 (Restore prompt)

use std::path::Path;

use super::process::{backup, restore, start, stop, restart, OpResult};

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

pub fn do_restore(deploy_dir: &Path, dump: &str) -> OpResult {
    restore(deploy_dir, dump)
}
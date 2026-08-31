//! Operator actions: backup + restore (delegates to the PowerShell
//! scripts the operator would run by hand).

use std::path::Path;

use super::process::{backup, restore, OpResult};

pub fn do_backup(deploy_dir: &Path) -> OpResult {
    backup(deploy_dir)
}

pub fn do_restore(deploy_dir: &Path, dump: &str) -> OpResult {
    restore(deploy_dir, dump)
}
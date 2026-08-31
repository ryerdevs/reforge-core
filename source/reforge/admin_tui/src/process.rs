//! Process controller: start/stop/status/restart of auth + channel.

#![allow(dead_code)] // restore() wired in commit 4 (Restore prompt)

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Auth,
    Channel,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Auth => "auth",
            Role::Channel => "channel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Running(u32),
    Stopped,
    Unknown,
}

impl ProcState {
    pub fn is_running(self) -> bool {
        matches!(self, ProcState::Running(_))
    }
}

#[derive(Debug)]
pub enum OpResult {
    Ok(String),
    Failed(String),
}

pub fn status(_role: Role) -> ProcState {
    let out = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq server_realms.exe", "/NH"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                if let Some(pid) = parse_tasklist_pid(line) {
                    return ProcState::Running(pid);
                }
            }
            ProcState::Stopped
        }
        _ => ProcState::Unknown,
    }
}

fn parse_tasklist_pid(line: &str) -> Option<u32> {
    let mut iter = line.split_whitespace();
    let name = iter.next()?;
    if !name.eq_ignore_ascii_case("server_realms.exe") {
        return None;
    }
    for tok in iter {
        if let Ok(n) = tok.parse::<u32>() {
            return Some(n);
        }
    }
    None
}

pub fn start(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "start_win.ps1", &["start"])
}

pub fn stop(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "stop_win.ps1", &[])
}

pub fn restart(deploy_dir: &Path) -> OpResult {
    let _ = stop(deploy_dir);
    std::thread::sleep(std::time::Duration::from_millis(500));
    start(deploy_dir)
}

pub fn backup(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "backup_win.ps1", &[])
}

pub fn restore(deploy_dir: &Path, dump_name: &str) -> OpResult {
    run_script(deploy_dir, "restore_drill.ps1", &[dump_name])
}

fn run_script(deploy_dir: &Path, script: &str, args: &[&str]) -> OpResult {
    let mut script_path = PathBuf::from("..").join("..").join("scripts").join(script);
    if !script_path.exists() {
        script_path = deploy_dir.join("scripts").join(script);
    }
    if !script_path.exists() {
        return OpResult::Failed(format!("script not found: {}", script));
    }
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy").arg("Bypass")
        .arg("-File").arg(&script_path);
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(deploy_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match cmd.status() {
        Ok(s) if s.success() => OpResult::Ok(format!("{} ok", script)),
        Ok(s) => OpResult::Failed(format!("{} exit {:?}", script, s.code())),
        Err(e) => OpResult::Failed(format!("{} spawn: {}", script, e)),
    }
}
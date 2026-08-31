//! Process controller: start/stop/status/restart of auth + channel.
//!
//! The TUI shells out to the existing PowerShell scripts in
//! `scripts/` (start_win.ps1, stop_win.ps1, status.ps1) so the
//! behaviour stays consistent with the manual operator path.

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
    pub fn label(self) -> &'static str {
        match self {
            ProcState::Running(pid) => return Box::leak(format!("running pid={pid}").into_boxed_str()) as &str,
            ProcState::Stopped => "stopped",
            ProcState::Unknown => "unknown",
        }
    }
    pub fn is_running(self) -> bool { matches!(self, ProcState::Running(_)) }
}

#[derive(Debug)]
pub enum OpResult {
    Ok(String),
    Failed(String),
}

pub fn status(role: Role) -> ProcState {
    // `tasklist /FI "IMAGENAME eq server_realms.exe"` works on Windows.
    // We filter by role via the `auth.toml` listen port vs `channel.toml`
    // listen port when both are running, but for v1 we just say
    // "running" if any server_realms.exe process is alive (the
    // operator can disambiguate with the per-process log).
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
    // tasklist /NH output (Win10+): "server_realms.exe          1234 Console  1   12,345 K"
    // Split on whitespace and take the first numeric column after the name.
    let mut iter = line.split_whitespace();
    let name = iter.next()?;
    if !name.eq_ignore_ascii_case("server_realms.exe") {
        return None;
    }
    iter.next()?; // pid (skipped return below if non-numeric)
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
    // Give the OS a moment to release the ports.
    std::thread::sleep(std::time::Duration::from_millis(500));
    start(deploy_dir)
}

pub fn backup(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "backup_win.ps1", &[])
}

pub fn restore(deploy_dir: &Path, dump_name: &str) -> OpResult {
    // restore_drill.ps1 takes the dump filename as $1.
    run_script(deploy_dir, "restore_drill.ps1", &[dump_name])
}

fn run_script(deploy_dir: &Path, script: &str, args: &[&str]) -> OpResult {
    // The scripts live at the repo root (../../scripts relative to
    // the deploy dir). We try the repo-relative path first; if not
    // found, we fall back to the deploy-relative `scripts/` (which
    // the deploy README will install alongside the binary).
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
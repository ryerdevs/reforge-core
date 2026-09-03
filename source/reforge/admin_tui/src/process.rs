//! Process controller: start/stop/status/restart of auth + channel.

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

    pub fn port(self) -> u16 {
        match self {
            Role::Auth => 30001,
            Role::Channel => 30003,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Running(u32),
    Stopped,
    Unknown,
}

#[derive(Debug)]
pub enum OpResult {
    Ok(String),
    Failed(String),
}

pub fn status(role: Role) -> ProcState {
    let listeners = match listening_pids(role.port()) {
        Ok(pids) => pids,
        Err(_) => return ProcState::Unknown,
    };
    let Some(first_listener) = listeners.first().copied() else {
        return ProcState::Stopped;
    };

    // The port identifies the role. Tasklist only verifies that the listener
    // belongs to this server rather than some unrelated process.
    match server_pids() {
        Ok(pids) if pids.iter().any(|pid| listeners.contains(pid)) => ProcState::Running(
            listeners
                .into_iter()
                .find(|pid| pids.contains(pid))
                .unwrap_or(first_listener),
        ),
        Ok(_) => ProcState::Unknown,
        // A listening role port is still useful status when tasklist is not
        // available (for example, a restricted Windows shell).
        Err(_) => ProcState::Running(first_listener),
    }
}

fn server_pids() -> std::io::Result<Vec<u32>> {
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq server_realms.exe", "/NH"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("tasklist failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().filter_map(parse_tasklist_pid).collect())
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

fn listening_pids(port: u16) -> std::io::Result<Vec<u32>> {
    let output = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("netstat failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_listening_pids(&stdout, port))
}

fn parse_listening_pids(output: &str, port: u16) -> Vec<u32> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let protocol = fields.next()?;
            let local_address = fields.next()?;
            let _foreign_address = fields.next()?;
            let state = fields.next()?;
            let pid = fields.next()?;
            if !protocol.eq_ignore_ascii_case("TCP")
                || !state.eq_ignore_ascii_case("LISTENING")
                || endpoint_port(local_address) != Some(port)
            {
                return None;
            }
            pid.parse().ok()
        })
        .collect()
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint
        .rsplit(':')
        .next()?
        .trim_end_matches(']')
        .parse()
        .ok()
}

pub fn start(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "start_win.ps1", &[])
}

pub fn stop(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "stop_win.ps1", &[])
}

pub fn restart(deploy_dir: &Path) -> OpResult {
    if let OpResult::Failed(message) = stop(deploy_dir) {
        return OpResult::Failed(format!("restart stop: {message}"));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    start(deploy_dir)
}

pub fn backup(deploy_dir: &Path) -> OpResult {
    run_script(deploy_dir, "backup_win.ps1", &[])
}

fn run_script(deploy_dir: &Path, script: &str, args: &[&str]) -> OpResult {
    let script_path = find_script(deploy_dir, script);
    if !script_path.exists() {
        return OpResult::Failed(format!("script not found: {}", script));
    }
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path);
    for a in args {
        cmd.arg(a);
    }
    cmd.current_dir(deploy_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match cmd.output() {
        Ok(output) if output.status.success() => OpResult::Ok(format!("{} ok", script)),
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if detail.is_empty() {
                OpResult::Failed(format!("{} exit {:?}", script, output.status.code()))
            } else {
                OpResult::Failed(format!(
                    "{} exit {:?}: {}",
                    script,
                    output.status.code(),
                    detail
                ))
            }
        }
        Err(e) => OpResult::Failed(format!("{} spawn: {}", script, e)),
    }
}

fn find_script(deploy_dir: &Path, script: &str) -> PathBuf {
    let deployed = deploy_dir.join("scripts").join(script);
    if deployed.exists() {
        return deployed;
    }

    // Check deploy_dir ancestors (e.g. repo_root/source/deploy/win -> repo_root/scripts/)
    for ancestor in deploy_dir.ancestors() {
        let candidate = ancestor.join("scripts").join(script);
        if candidate.exists() {
            return candidate;
        }
    }

    // Check current_exe ancestors
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            let candidate = ancestor.join("scripts").join(script);
            if candidate.exists() {
                return candidate;
            }
        }
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for ancestor in cwd.ancestors() {
        let candidate = ancestor.join("scripts").join(script);
        if candidate.exists() {
            return candidate;
        }
    }
    deployed
}

pub fn git_head(deploy_dir: &Path) -> String {
    let output = Command::new("git")
        .args(["-C"])
        .arg(deploy_dir)
        .args(["rev-parse", "--short", "HEAD"])
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if head.is_empty() {
                String::from("unknown")
            } else {
                head
            }
        }
        _ => String::from("unknown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ports_match_runtime_contract() {
        assert_eq!(Role::Auth.port(), 30001);
        assert_eq!(Role::Channel.port(), 30003);
    }

    #[test]
    fn tasklist_parser_reads_server_pid_only() {
        let output = "server_realms.exe              1234 Console                    1     20,000 K\nother.exe                       5678 Console                    1     10,000 K";
        assert_eq!(
            parse_tasklist_pid(output.lines().next().unwrap()),
            Some(1234)
        );
        assert_eq!(parse_tasklist_pid(output.lines().nth(1).unwrap()), None);
    }

    #[test]
    fn netstat_parser_selects_listening_role_port() {
        let output = "  TCP    0.0.0.0:30001    0.0.0.0:0    LISTENING    1234\n  TCP    [::]:30003       [::]:0       LISTENING    5678\n  TCP    0.0.0.0:30001    10.0.0.1:9   ESTABLISHED  9999";
        assert_eq!(parse_listening_pids(output, 30001), vec![1234]);
        assert_eq!(parse_listening_pids(output, 30003), vec![5678]);
    }

    #[test]
    fn netstat_parser_ignores_similar_ports_and_non_tcp_rows() {
        let output = "  UDP    0.0.0.0:30001    *:*                         1234\n  TCP    0.0.0.0:130001   0.0.0.0:0    LISTENING    5678\n  TCP    0.0.0.0:30001    0.0.0.0:0    LISTENING    nope";
        assert!(parse_listening_pids(output, 30001).is_empty());
    }

    #[test]
    fn find_script_discovers_in_deploy_dir_ancestors() {
        let unique = format!(
            "tui_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let deploy = root.join("source").join("deploy").join("win");
        let scripts = root.join("scripts");
        let _ = std::fs::create_dir_all(&deploy);
        let _ = std::fs::create_dir_all(&scripts);
        let dummy_script = scripts.join("test_script.ps1");
        let _ = std::fs::write(&dummy_script, "# test");

        let found = find_script(&deploy, "test_script.ps1");
        assert_eq!(found, dummy_script);

        let _ = std::fs::remove_dir_all(&root);
    }
}

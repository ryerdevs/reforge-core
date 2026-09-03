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

pub fn is_postgres_running() -> bool {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    let addr = SocketAddr::from(([127, 0, 0, 1], 5432));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

pub fn ensure_postgres_running() -> Result<(), String> {
    if is_postgres_running() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        let _ = Command::new("net")
            .args(["start", "postgresql-metin2"])
            .output();
        let _ = Command::new("sc.exe")
            .args(["start", "postgresql-metin2"])
            .output();
        let _ = Command::new("net").args(["start", "postgresql"]).output();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("systemctl")
            .args(["start", "postgresql"])
            .output();
    }

    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if is_postgres_running() {
            return Ok(());
        }
    }
    Err("PostgreSQL is not responding on 127.0.0.1:5432".to_string())
}

pub fn find_server_realms_exe(deploy_dir: &Path) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "server_realms.exe"
    } else {
        "server_realms"
    };

    let in_deploy = deploy_dir.join(exe_name);
    if in_deploy.is_file() {
        return Some(in_deploy);
    }

    for ancestor in deploy_dir.ancestors() {
        let release = ancestor
            .join("source")
            .join("reforge")
            .join("target")
            .join("release")
            .join(exe_name);
        if release.is_file() {
            return Some(release);
        }
        let debug = ancestor
            .join("source")
            .join("reforge")
            .join("target")
            .join("debug")
            .join(exe_name);
        if debug.is_file() {
            return Some(debug);
        }
    }

    if let Ok(current) = std::env::current_exe() {
        for ancestor in current.ancestors() {
            let candidate = ancestor.join(exe_name);
            if candidate.is_file() && candidate != current {
                return Some(candidate);
            }
            let release = ancestor
                .join("source")
                .join("reforge")
                .join("target")
                .join("release")
                .join(exe_name);
            if release.is_file() {
                return Some(release);
            }
        }
    }
    None
}

pub fn find_config(deploy_dir: &Path, file_name: &str) -> Option<PathBuf> {
    let in_deploy = deploy_dir.join(file_name);
    if in_deploy.is_file() {
        return Some(in_deploy);
    }
    let in_config = deploy_dir.join("config").join(file_name);
    if in_config.is_file() {
        return Some(in_config);
    }
    for ancestor in deploy_dir.ancestors() {
        let candidate = ancestor
            .join("source")
            .join("deploy")
            .join("win")
            .join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
        let example = ancestor
            .join("source")
            .join("deploy")
            .join("win")
            .join("examples")
            .join(format!(
                "{}.example.toml",
                file_name.trim_end_matches(".toml")
            ));
        if example.is_file() {
            return Some(example);
        }
    }
    None
}

fn current_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_of_day = now % 86400;
    let hours = secs_of_day / 3600;
    let mins = (secs_of_day % 3600) / 60;
    let secs = secs_of_day % 60;
    format!("{hours:02}{mins:02}{secs:02}")
}

pub fn start(deploy_dir: &Path) -> OpResult {
    // 1. PostgreSQL check
    if !is_postgres_running()
        && let Err(e) = ensure_postgres_running()
    {
        return OpResult::Failed(format!("PostgreSQL (127.0.0.1:5432): {e}"));
    }

    // 2. server_realms binary
    let Some(exe_path) = find_server_realms_exe(deploy_dir) else {
        return OpResult::Failed(format!(
            "server_realms binary not found in {}",
            deploy_dir.display()
        ));
    };

    // 3. configs
    let Some(auth_cfg) = find_config(deploy_dir, "auth.toml") else {
        return OpResult::Failed("auth.toml config not found".to_string());
    };
    let Some(channel_cfg) = find_config(deploy_dir, "channel.toml") else {
        return OpResult::Failed("channel.toml config not found".to_string());
    };

    // 4. Stop existing
    let _ = stop(deploy_dir);
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 5. Logs dir
    let logs_dir = deploy_dir.join("logs");
    let _ = std::fs::create_dir_all(&logs_dir);
    let ts = current_timestamp();

    // 6. Spawn Auth
    let auth_out = match std::fs::File::create(logs_dir.join(format!("auth.{ts}.out.log"))) {
        Ok(f) => Stdio::from(f),
        Err(e) => return OpResult::Failed(format!("auth log creation: {e}")),
    };
    let auth_err = match std::fs::File::create(logs_dir.join(format!("auth.{ts}.err.log"))) {
        Ok(f) => Stdio::from(f),
        Err(e) => return OpResult::Failed(format!("auth err log creation: {e}")),
    };

    let mut auth_cmd = Command::new(&exe_path);
    auth_cmd
        .arg("--role")
        .arg("auth")
        .arg("--config")
        .arg(&auth_cfg)
        .current_dir(deploy_dir)
        .stdin(Stdio::null())
        .stdout(auth_out)
        .stderr(auth_err);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        auth_cmd.creation_flags(0x0000_0208);
    }

    if let Err(e) = auth_cmd.spawn() {
        return OpResult::Failed(format!("failed to spawn auth: {e}"));
    }

    // 7. Spawn Channel
    let ch_out = match std::fs::File::create(logs_dir.join(format!("channel.{ts}.out.log"))) {
        Ok(f) => Stdio::from(f),
        Err(e) => return OpResult::Failed(format!("channel log creation: {e}")),
    };
    let ch_err = match std::fs::File::create(logs_dir.join(format!("channel.{ts}.err.log"))) {
        Ok(f) => Stdio::from(f),
        Err(e) => return OpResult::Failed(format!("channel err log creation: {e}")),
    };

    let mut ch_cmd = Command::new(&exe_path);
    ch_cmd
        .arg("--role")
        .arg("channel")
        .arg("--config")
        .arg(&channel_cfg)
        .current_dir(deploy_dir)
        .stdin(Stdio::null())
        .stdout(ch_out)
        .stderr(ch_err);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        ch_cmd.creation_flags(0x0000_0208);
    }

    if let Err(e) = ch_cmd.spawn() {
        return OpResult::Failed(format!("failed to spawn channel: {e}"));
    }

    OpResult::Ok(format!("auth + channel launched (logs: {ts})"))
}

pub fn stop(_deploy_dir: &Path) -> OpResult {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "server_realms.exe"])
            .output();
        OpResult::Ok("server_realms processes stopped".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("pkill").args(["-9", "server_realms"]).output();
        OpResult::Ok("server_realms processes stopped".to_string())
    }
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

pub fn run_bootstrap_db(deploy_dir: &Path, command: &str) -> OpResult {
    let script = find_script(deploy_dir, "bootstrap_db.py");
    if !script.exists() {
        return OpResult::Failed(format!(
            "bootstrap_db.py not found from {}",
            deploy_dir.display()
        ));
    }
    let mut cmd = Command::new("python");
    cmd.arg(&script).arg(command);
    if command == "reset" {
        cmd.arg("--force");
    }
    cmd.current_dir(deploy_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match cmd.output() {
        Ok(output) if output.status.success() => {
            let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
            OpResult::Ok(out)
        }
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            OpResult::Failed(format!("db {command} failed: {detail}"))
        }
        Err(e) => OpResult::Failed(format!("db {command} spawn: {e}")),
    }
}

pub fn find_script(deploy_dir: &Path, script: &str) -> PathBuf {
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

    #[test]
    fn find_server_realms_exe_discovers_binary() {
        let unique = format!(
            "tui_exe_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let deploy = root.join("source").join("deploy").join("win");
        let _ = std::fs::create_dir_all(&deploy);
        let exe_name = if cfg!(windows) {
            "server_realms.exe"
        } else {
            "server_realms"
        };
        let dummy_exe = deploy.join(exe_name);
        let _ = std::fs::write(&dummy_exe, [0x4d, 0x5a]);

        let found = find_server_realms_exe(&deploy);
        assert_eq!(found, Some(dummy_exe));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_config_discovers_toml_files() {
        let unique = format!(
            "tui_cfg_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let deploy = root.join("deploy");
        let config_dir = deploy.join("config");
        let _ = std::fs::create_dir_all(&config_dir);
        let dummy_cfg = config_dir.join("auth.toml");
        let _ = std::fs::write(&dummy_cfg, "port = 30001\n");

        let found = find_config(&deploy, "auth.toml");
        assert_eq!(found, Some(dummy_cfg));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn run_bootstrap_db_fails_gracefully_on_invalid_command() {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let res = run_bootstrap_db(&cwd, "invalid_command_xyz");
        match res {
            super::OpResult::Failed(msg) => {
                assert!(
                    msg.contains("invalid_command_xyz")
                        || msg.contains("invalid")
                        || msg.contains("error")
                        || msg.contains("db invalid_command_xyz")
                );
            }
            _ => panic!("expected Failed on invalid command"),
        }
    }
}

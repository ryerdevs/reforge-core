//! Tail of the auth + channel logs from the deploy directory.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_LINES: usize = 500;

pub fn latest_log(deploy_dir: &Path, role: super::process::Role) -> PathBuf {
    let logs_dir = deploy_dir.join("logs");
    let prefix = format!("{}.", role.label());
    let suffix = ".out.log";
    let candidates = fs::read_dir(&logs_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            if !is_timestamped_log(name, &prefix, suffix) {
                return None;
            }
            let modified = path.metadata().ok()?.modified().ok()?;
            Some((path, modified))
        });

    newest_path(candidates).unwrap_or_else(|| logs_dir.join(format!("{}.out.log", role.label())))
}

fn is_timestamped_log(name: &str, prefix: &str, suffix: &str) -> bool {
    let Some(timestamp) = name
        .strip_prefix(prefix)
        .and_then(|n| n.strip_suffix(suffix))
    else {
        return false;
    };
    !timestamp.is_empty() && timestamp.chars().all(|ch| ch.is_ascii_digit())
}

fn newest_path<I>(candidates: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = (PathBuf, SystemTime)>,
{
    candidates
        .into_iter()
        .max_by(|(left_path, left_time), (right_path, right_time)| {
            left_time
                .cmp(right_time)
                .then_with(|| left_path.cmp(right_path))
        })
        .map(|(path, _)| path)
}

pub fn tail(path: &Path) -> std::io::Result<Vec<String>> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(Vec::new()),
    };
    let len = file.metadata()?.len();
    // Read the last 64 KB to keep the buffer bounded; larger files
    // are truncated from the top.
    let window = len.min(64 * 1024);
    let start = len - window;
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity(window as usize);
    file.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    // Keep only the last MAX_LINES.
    if lines.len() > MAX_LINES {
        let drop = lines.len() - MAX_LINES;
        lines.drain(0..drop);
    }
    Ok(lines)
}

pub fn list_dumps() -> std::io::Result<Vec<String>> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(env_backup) = std::env::var("REFORGE_BACKUP_DIR") {
        candidate_dirs.push(PathBuf::from(env_backup));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidate_dirs.push(cwd.join("backups"));
        candidate_dirs.push(cwd.join("source").join("reforge").join("backups"));
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors() {
            let b = ancestor.join("backups");
            if b.is_dir() {
                candidate_dirs.push(b);
            }
        }
    }
    let legacy = PathBuf::from(r"C:\projects\metin2-extra\backups");
    if legacy.is_dir() {
        candidate_dirs.push(legacy);
    }

    let mut out: Vec<String> = Vec::new();
    for dir in &candidate_dirs {
        if dir.is_dir()
            && let Ok(rd) = fs::read_dir(dir)
        {
            for ent in rd.flatten() {
                let p = ent.path();
                if p.extension().and_then(|e| e.to_str()) == Some("dump")
                    && let Some(name) = p.file_name().and_then(|n| n.to_str())
                {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    out.reverse();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamped_name_requires_exact_role_and_numeric_stamp() {
        assert!(is_timestamped_log(
            "auth.001531.out.log",
            "auth.",
            ".out.log"
        ));
        assert!(!is_timestamped_log("auth.out.log", "auth.", ".out.log"));
        assert!(!is_timestamped_log(
            "auth2.001531.out.log",
            "auth.",
            ".out.log"
        ));
        assert!(!is_timestamped_log(
            "auth.latest.out.log",
            "auth.",
            ".out.log"
        ));
    }

    #[test]
    fn newest_path_uses_mtime_and_has_deterministic_ties() {
        let older = PathBuf::from("auth.001531.out.log");
        let newer = PathBuf::from("auth.003013.out.log");
        let newest = newest_path([
            (older.clone(), SystemTime::UNIX_EPOCH),
            (
                newer.clone(),
                SystemTime::UNIX_EPOCH + Duration::from_secs(1),
            ),
        ]);
        assert_eq!(newest, Some(newer));

        let tied = newest_path([
            (PathBuf::from("auth.001531.out.log"), SystemTime::UNIX_EPOCH),
            (PathBuf::from("auth.003013.out.log"), SystemTime::UNIX_EPOCH),
        ]);
        assert_eq!(tied, Some(PathBuf::from("auth.003013.out.log")));
    }

    #[test]
    fn tail_missing_file_is_empty() {
        let path = std::env::temp_dir().join(format!(
            "reforge-admin-tui-missing-{}.log",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        assert!(tail(&path).unwrap().is_empty());
    }

    #[test]
    fn latest_log_falls_back_to_legacy_name() {
        let directory = std::env::temp_dir().join(format!(
            "reforge-admin-tui-logs-{}-fallback",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let logs_dir = directory.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        let legacy = logs_dir.join("auth.out.log");
        fs::write(&legacy, "legacy\n").unwrap();

        assert_eq!(
            latest_log(&directory, super::super::process::Role::Auth),
            legacy
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn latest_log_prefers_timestamped_output_over_legacy() {
        let directory = std::env::temp_dir().join(format!(
            "reforge-admin-tui-logs-{}-timestamped",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        let logs_dir = directory.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(logs_dir.join("auth.out.log"), "legacy\n").unwrap();
        let timestamped = logs_dir.join("auth.001531.out.log");
        fs::write(&timestamped, "timestamped\n").unwrap();

        assert_eq!(
            latest_log(&directory, super::super::process::Role::Auth),
            timestamped
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

//! Tail of the auth + channel logs from the deploy directory.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 500;

pub fn log_path(deploy_dir: &Path, role: super::process::Role) -> PathBuf {
    deploy_dir.join("logs").join(format!("{}.out.log", role.label()))
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
    // Dumps may live in two places: the deploy-local `backups/`
    // (recommended) or the developer's `C:\projects\metin2-extra\backups`
    // (legacy). We union the two.
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };
    let mut out: Vec<String> = Vec::new();
    for dir in &[
        cwd.join("backups"),
        cwd.join("source").join("reforge").join("backups"),
        PathBuf::from(r"C:\projects\metin2-extra\backups"),
    ] {
        if dir.is_dir() {
            if let Ok(rd) = fs::read_dir(dir) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("dump") {
                        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                            out.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out.reverse();
    Ok(out)
}
//! App state machine shared with the TUI loop.

#![allow(dead_code)] // last_log_size is wired in commit 4 (incremental tail)

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::logs;
use crate::process::{self, ProcState, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Logs,
    Restore,
}

pub struct App {
    pub deploy_dir: PathBuf,
    pub screen: Screen,
    pub auth_state: ProcState,
    pub channel_state: ProcState,
    pub log_target: Role,
    pub log_lines: Vec<String>,
    pub log_offset: usize,
    pub dumps: Vec<String>,
    pub status_message: String,
    pub last_tick: Instant,
    #[allow(dead_code)]
    pub last_log_size: u64,
}

impl App {
    pub fn new(deploy_dir: PathBuf) -> Self {
        let mut app = App {
            deploy_dir,
            screen: Screen::Main,
            auth_state: process::status(Role::Auth),
            channel_state: process::status(Role::Channel),
            log_target: Role::Auth,
            log_lines: Vec::new(),
            log_offset: 0,
            dumps: Vec::new(),
            status_message: String::from("ready"),
            last_tick: Instant::now(),
            last_log_size: 0,
        };
        app.refresh_logs();
        app
    }

    pub fn refresh_logs(&mut self) {
        let path = logs::log_path(&self.deploy_dir, self.log_target);
        if let Ok(lines) = logs::tail(&path) {
            self.log_offset = lines.len();
            self.log_lines = lines;
        }
    }

    pub fn refresh_dumps(&mut self) {
        if let Ok(d) = logs::list_dumps() {
            self.dumps = d;
        }
    }

    pub fn refresh_status(&mut self) {
        self.auth_state = process::status(Role::Auth);
        self.channel_state = process::status(Role::Channel);
    }

    pub fn tick(&mut self) {
        if self.last_tick.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.refresh_status();
        self.refresh_logs();
        self.last_tick = Instant::now();
    }
}
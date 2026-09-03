//! App state machine shared with the TUI loop.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crate::logs;
use crate::process::{self, ProcState, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Logs,
}

struct PendingOperation {
    label: &'static str,
    result: Receiver<process::OpResult>,
}

struct RefreshResult {
    pg_state: bool,
    auth_state: ProcState,
    channel_state: ProcState,
    auth_lines: Vec<String>,
    channel_lines: Vec<String>,
}

pub struct App {
    pub deploy_dir: PathBuf,
    pub screen: Screen,
    pub pg_state: bool,
    pub auth_state: ProcState,
    pub channel_state: ProcState,
    pub head: String,
    pub log_target: Role,
    pub log_lines: Vec<String>,
    pub log_offset: usize,
    pub auth_log_preview: String,
    pub channel_log_preview: String,
    pub status_message: String,
    pub last_tick: Instant,
    pending_operation: Option<PendingOperation>,
    refresh_result: Option<Receiver<RefreshResult>>,
}

impl App {
    pub fn new(deploy_dir: PathBuf) -> Self {
        let head = process::git_head(&deploy_dir);
        let mut app = App {
            deploy_dir,
            screen: Screen::Main,
            pg_state: process::is_postgres_running(),
            auth_state: process::status(Role::Auth),
            channel_state: process::status(Role::Channel),
            head,
            log_target: Role::Auth,
            log_lines: Vec::new(),
            log_offset: 0,
            auth_log_preview: String::from("no output yet"),
            channel_log_preview: String::from("no output yet"),
            status_message: String::from("ready"),
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        };
        app.refresh_all_logs();
        app
    }

    pub fn refresh_logs(&mut self) {
        let target = self.log_target;
        let lines = logs::tail(&logs::latest_log(&self.deploy_dir, target)).unwrap_or_default();
        self.set_preview(target, &lines);
        self.replace_log_lines(lines);
    }

    pub fn start_operation<F>(&mut self, label: &'static str, operation: F)
    where
        F: FnOnce(&std::path::Path) -> process::OpResult + Send + 'static,
    {
        if self.pending_operation.is_some() {
            self.status_message = String::from("another operation is already running");
            return;
        }

        let deploy_dir = self.deploy_dir.clone();
        let (sender, result) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(operation(&deploy_dir));
        });
        self.pending_operation = Some(PendingOperation { label, result });
        self.status_message = format!("{label}: running...");
    }

    pub fn operation_running(&self) -> Option<&'static str> {
        self.pending_operation
            .as_ref()
            .map(|operation| operation.label)
    }

    pub fn select_log_target(&mut self, target: Role) {
        self.log_target = target;
        self.log_offset = 0;
        self.refresh_logs();
    }

    pub fn scroll_up(&mut self) {
        self.log_offset = (self.log_offset + 1).min(self.log_lines.len());
    }

    pub fn scroll_down(&mut self) {
        self.log_offset = self.log_offset.saturating_sub(1);
    }

    pub fn follow_tail(&mut self) {
        self.log_offset = 0;
    }

    pub fn tick(&mut self) {
        self.poll_operation();
        self.poll_refresh();
        if self.last_tick.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_tick = Instant::now();
        self.start_refresh();
    }

    fn refresh_all_logs(&mut self) {
        for (role, lines) in [
            (
                Role::Auth,
                logs::tail(&logs::latest_log(&self.deploy_dir, Role::Auth)).unwrap_or_default(),
            ),
            (
                Role::Channel,
                logs::tail(&logs::latest_log(&self.deploy_dir, Role::Channel)).unwrap_or_default(),
            ),
        ] {
            self.set_preview(role, &lines);
            if role == self.log_target {
                self.replace_log_lines(lines);
            }
        }
    }

    fn set_preview(&mut self, role: Role, lines: &[String]) {
        let preview = lines
            .last()
            .cloned()
            .unwrap_or_else(|| String::from("no output yet"));
        match role {
            Role::Auth => self.auth_log_preview = preview,
            Role::Channel => self.channel_log_preview = preview,
        }
    }

    fn replace_log_lines(&mut self, lines: Vec<String>) {
        let following_tail = self.log_offset == 0;
        self.log_lines = lines;
        self.log_offset = if following_tail {
            0
        } else {
            self.log_offset.min(self.log_lines.len())
        };
    }

    fn start_refresh(&mut self) {
        if self.refresh_result.is_some() {
            return;
        }
        let deploy_dir = self.deploy_dir.clone();
        let (sender, result) = mpsc::channel();
        thread::spawn(move || {
            let _ = sender.send(RefreshResult {
                pg_state: process::is_postgres_running(),
                auth_state: process::status(Role::Auth),
                channel_state: process::status(Role::Channel),
                auth_lines: logs::tail(&logs::latest_log(&deploy_dir, Role::Auth))
                    .unwrap_or_default(),
                channel_lines: logs::tail(&logs::latest_log(&deploy_dir, Role::Channel))
                    .unwrap_or_default(),
            });
        });
        self.refresh_result = Some(result);
    }

    fn poll_refresh(&mut self) {
        let Some(result) = self.refresh_result.take() else {
            return;
        };
        match result.try_recv() {
            Ok(refresh) => {
                self.pg_state = refresh.pg_state;
                self.auth_state = refresh.auth_state;
                self.channel_state = refresh.channel_state;
                self.set_preview(Role::Auth, &refresh.auth_lines);
                self.set_preview(Role::Channel, &refresh.channel_lines);
                let lines = match self.log_target {
                    Role::Auth => refresh.auth_lines,
                    Role::Channel => refresh.channel_lines,
                };
                self.replace_log_lines(lines);
            }
            Err(TryRecvError::Empty) => self.refresh_result = Some(result),
            Err(TryRecvError::Disconnected) => {
                self.status_message = String::from("status refresh failed");
            }
        }
    }

    fn poll_operation(&mut self) {
        let Some(operation) = self.pending_operation.take() else {
            return;
        };
        match operation.result.try_recv() {
            Ok(result) => {
                self.status_message = match result {
                    process::OpResult::Ok(message) => format!("{}: {message}", operation.label),
                    process::OpResult::Failed(message) => {
                        format!("{}: FAIL {message}", operation.label)
                    }
                };
                self.start_refresh();
            }
            Err(TryRecvError::Empty) => self.pending_operation = Some(operation),
            Err(TryRecvError::Disconnected) => {
                self.status_message = format!("{}: FAIL worker disconnected", operation.label);
            }
        }
    }
}

// TODO metrics: expose WorldMetrics via channel IPC or file tick_ms.csv tail

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_logs_keeps_follow_tail_at_zero() {
        let mut app = App {
            deploy_dir: PathBuf::from("."),
            screen: Screen::Logs,
            pg_state: false,
            auth_state: ProcState::Stopped,
            channel_state: ProcState::Stopped,
            head: String::from("test"),
            log_target: Role::Auth,
            log_lines: vec![String::from("old")],
            log_offset: 0,
            auth_log_preview: String::new(),
            channel_log_preview: String::new(),
            status_message: String::new(),
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        };
        app.replace_log_lines(vec![String::from("new")]);
        assert_eq!(app.log_offset, 0);
    }

    #[test]
    fn replacing_logs_clamps_manual_scroll_without_resetting_it() {
        let mut app = App {
            deploy_dir: PathBuf::from("."),
            screen: Screen::Logs,
            pg_state: false,
            auth_state: ProcState::Stopped,
            channel_state: ProcState::Stopped,
            head: String::from("test"),
            log_target: Role::Auth,
            log_lines: vec![
                String::from("one"),
                String::from("two"),
                String::from("three"),
            ],
            log_offset: 2,
            auth_log_preview: String::new(),
            channel_log_preview: String::new(),
            status_message: String::new(),
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        };
        app.replace_log_lines(vec![String::from("new")]);
        assert_eq!(app.log_offset, 1);
    }

    #[test]
    fn operation_reports_running_until_worker_returns() {
        let mut app = App {
            deploy_dir: PathBuf::from("."),
            screen: Screen::Main,
            pg_state: false,
            auth_state: ProcState::Stopped,
            channel_state: ProcState::Stopped,
            head: String::from("test"),
            log_target: Role::Auth,
            log_lines: Vec::new(),
            log_offset: 0,
            auth_log_preview: String::new(),
            channel_log_preview: String::new(),
            status_message: String::new(),
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        };
        let (started_sender, started_receiver) = mpsc::channel();
        app.start_operation("test", move |_| {
            started_sender.send(()).unwrap();
            process::OpResult::Ok(String::from("done"))
        });
        assert_eq!(app.operation_running(), Some("test"));
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        for _ in 0..100 {
            app.poll_operation();
            if app.operation_running().is_none() {
                break;
            }
            thread::yield_now();
        }
        assert_eq!(app.operation_running(), None);
        assert_eq!(app.status_message, "test: done");
    }
}

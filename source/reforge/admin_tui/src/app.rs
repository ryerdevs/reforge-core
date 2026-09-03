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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Cockpit,
    Logs,
    Players,
    Config,
    Doctor,
}

impl Tab {
    pub fn all() -> &'static [Tab] {
        &[
            Tab::Cockpit,
            Tab::Logs,
            Tab::Players,
            Tab::Config,
            Tab::Doctor,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Cockpit => " 1. Cockpit ",
            Tab::Logs => " 2. Logs en Vivo ",
            Tab::Players => " 3. Jugadores & Mundo ",
            Tab::Config => " 4. Configuración ",
            Tab::Doctor => " 5. Doctor & Mantenimiento ",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Cockpit => 0,
            Tab::Logs => 1,
            Tab::Players => 2,
            Tab::Config => 3,
            Tab::Doctor => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionItem {
    StartServer,
    StopServer,
    RestartServer,
    TogglePostgres,
    RunDoctor,
    CreateBackup,
    ViewLogs,
    Quit,
}

impl ActionItem {
    pub fn all() -> &'static [ActionItem] {
        &[
            ActionItem::StartServer,
            ActionItem::StopServer,
            ActionItem::RestartServer,
            ActionItem::TogglePostgres,
            ActionItem::RunDoctor,
            ActionItem::CreateBackup,
            ActionItem::ViewLogs,
            ActionItem::Quit,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            ActionItem::StartServer => "1. Iniciar Servidor Completo",
            ActionItem::StopServer => "2. Detener Servidor",
            ActionItem::RestartServer => "3. Reiniciar Servidor",
            ActionItem::TogglePostgres => "4. Iniciar / Gestionar PostgreSQL (P)",
            ActionItem::RunDoctor => "5. Diagnóstico del Sistema (Doctor) (D)",
            ActionItem::CreateBackup => "6. Crear Copia de Respaldo (.dump) (B)",
            ActionItem::ViewLogs => "7. Ver Registros en Vivo (L)",
            ActionItem::Quit => "8. Salir del Panel (Q)",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ActionItem::StartServer => "🚀",
            ActionItem::StopServer => "🛑",
            ActionItem::RestartServer => "🔄",
            ActionItem::TogglePostgres => "🐘",
            ActionItem::RunDoctor => "🩺",
            ActionItem::CreateBackup => "💾",
            ActionItem::ViewLogs => "📜",
            ActionItem::Quit => "🚪",
        }
    }
}

struct PendingOperation {
    label: &'static str,
    result: Receiver<process::OpResult>,
}

struct RefreshResult {
    pg_state: bool,
    pg_latency_ms: Option<u64>,
    auth_state: ProcState,
    channel_state: ProcState,
    auth_lines: Vec<String>,
    channel_lines: Vec<String>,
}

pub struct App {
    pub deploy_dir: PathBuf,
    pub screen: Screen,
    pub current_tab: Tab,
    pub selected_action: usize,
    pub pg_state: bool,
    pub pg_latency_ms: Option<u64>,
    pub auth_state: ProcState,
    pub channel_state: ProcState,
    pub head: String,
    pub log_target: Role,
    pub log_lines: Vec<String>,
    pub log_offset: usize,
    pub auth_log_preview: String,
    pub channel_log_preview: String,
    pub status_message: String,
    pub recent_events: Vec<(String, String)>,
    pub rate_exp: u32,
    pub rate_yang: u32,
    pub rate_drop: u32,
    pub selected_player_index: usize,
    pub start_time: Option<Instant>,
    pub last_tick: Instant,
    pending_operation: Option<PendingOperation>,
    refresh_result: Option<Receiver<RefreshResult>>,
}

fn measure_pg_latency() -> Option<u64> {
    use std::net::{SocketAddr, TcpStream};
    let addr = SocketAddr::from(([127, 0, 0, 1], 5432));
    let t0 = Instant::now();
    if TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok() {
        Some(t0.elapsed().as_millis().max(1) as u64)
    } else {
        None
    }
}

fn current_time_str() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs_of_day = now % 86400;
    let hours = secs_of_day / 3600;
    let mins = (secs_of_day % 3600) / 60;
    let secs = secs_of_day % 60;
    format!("{hours:02}:{mins:02}:{secs:02}")
}

impl App {
    pub fn new(deploy_dir: PathBuf) -> Self {
        let head = process::git_head(&deploy_dir);
        let mut app = App {
            deploy_dir,
            screen: Screen::Main,
            current_tab: Tab::Cockpit,
            selected_action: 0,
            pg_state: process::is_postgres_running(),
            pg_latency_ms: measure_pg_latency(),
            auth_state: process::status(Role::Auth),
            channel_state: process::status(Role::Channel),
            head,
            log_target: Role::Auth,
            log_lines: Vec::new(),
            log_offset: 0,
            auth_log_preview: String::from("no output yet"),
            channel_log_preview: String::from("no output yet"),
            status_message: String::from("ready"),
            recent_events: vec![
                (current_time_str(), String::from("Panel Cockpit iniciado")),
                (
                    current_time_str(),
                    String::from("Monitoreo de cluster activo"),
                ),
            ],
            rate_exp: 100,
            rate_yang: 100,
            rate_drop: 100,
            selected_player_index: 0,
            start_time: None,
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        };
        app.refresh_all_logs();
        app
    }

    pub fn next_tab(&mut self) {
        let all = Tab::all();
        let idx = (self.current_tab.index() + 1) % all.len();
        self.current_tab = all[idx];
        self.sync_screen();
    }

    pub fn prev_tab(&mut self) {
        let all = Tab::all();
        let idx = if self.current_tab.index() == 0 {
            all.len() - 1
        } else {
            self.current_tab.index() - 1
        };
        self.current_tab = all[idx];
        self.sync_screen();
    }

    pub fn select_tab(&mut self, tab: Tab) {
        self.current_tab = tab;
        self.sync_screen();
    }

    fn sync_screen(&mut self) {
        self.screen = match self.current_tab {
            Tab::Logs => Screen::Logs,
            _ => Screen::Main,
        };
        if self.current_tab == Tab::Logs {
            self.refresh_logs();
        }
    }

    pub fn next_action(&mut self) {
        let count = ActionItem::all().len();
        self.selected_action = (self.selected_action + 1).min(count - 1);
    }

    pub fn prev_action(&mut self) {
        self.selected_action = self.selected_action.saturating_sub(1);
    }

    pub fn add_event(&mut self, msg: String) {
        let ts = current_time_str();
        self.recent_events.push((ts, msg));
        if self.recent_events.len() > 8 {
            self.recent_events.remove(0);
        }
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
                pg_latency_ms: measure_pg_latency(),
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
                self.pg_latency_ms = refresh.pg_latency_ms;
                self.auth_state = refresh.auth_state;
                self.channel_state = refresh.channel_state;
                if matches!(self.auth_state, ProcState::Running(_))
                    || matches!(self.channel_state, ProcState::Running(_))
                {
                    if self.start_time.is_none() {
                        self.start_time = Some(Instant::now());
                    }
                } else {
                    self.start_time = None;
                }
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
                    process::OpResult::Ok(message) => {
                        self.add_event(format!("[OK] {}", operation.label));
                        format!("{}: {message}", operation.label)
                    }
                    process::OpResult::Failed(message) => {
                        self.add_event(format!("[FAIL] {}", operation.label));
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
impl App {
    pub fn for_test(screen: Screen) -> Self {
        App {
            deploy_dir: PathBuf::from("."),
            screen,
            current_tab: if screen == Screen::Logs {
                Tab::Logs
            } else {
                Tab::Cockpit
            },
            selected_action: 0,
            pg_state: false,
            pg_latency_ms: None,
            auth_state: ProcState::Stopped,
            channel_state: ProcState::Stopped,
            head: String::from("test"),
            log_target: Role::Auth,
            log_lines: Vec::new(),
            log_offset: 0,
            auth_log_preview: String::new(),
            channel_log_preview: String::new(),
            status_message: String::new(),
            recent_events: Vec::new(),
            rate_exp: 100,
            rate_yang: 100,
            rate_drop: 100,
            selected_player_index: 0,
            start_time: None,
            last_tick: Instant::now(),
            pending_operation: None,
            refresh_result: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_logs_keeps_follow_tail_at_zero() {
        let mut app = App::for_test(Screen::Logs);
        app.log_lines = vec![String::from("old")];
        app.replace_log_lines(vec![String::from("new")]);
        assert_eq!(app.log_offset, 0);
    }

    #[test]
    fn replacing_logs_clamps_manual_scroll_without_resetting_it() {
        let mut app = App::for_test(Screen::Logs);
        app.log_lines = vec![
            String::from("one"),
            String::from("two"),
            String::from("three"),
        ];
        app.log_offset = 2;
        app.replace_log_lines(vec![String::from("new")]);
        assert_eq!(app.log_offset, 1);
    }

    #[test]
    fn operation_reports_running_until_worker_returns() {
        let mut app = App::for_test(Screen::Main);
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

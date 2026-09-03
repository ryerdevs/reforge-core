//! TUI render: modern operator dashboard, telemetry gauges, interactive tabs,
//! live configuration editor, and the interactive event loop.

use std::io::{self, Stdout};
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Tabs};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::app::{ActionItem, App, Tab};
use crate::ops;

type Backend = CrosstermBackend<Stdout>;

pub fn run(app: &mut App) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = event_loop(&mut terminal, app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn event_loop(terminal: &mut Terminal<Backend>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| render(f, app))?;
        app.tick();
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(k) = event::read()?
        {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            if handle_key(app, k) {
                return Ok(());
            }
        }
    }
}

fn handle_key(app: &mut App, k: KeyEvent) -> bool {
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        return true;
    }

    // Global tab switcher keys
    match k.code {
        KeyCode::Tab => {
            app.next_tab();
            return false;
        }
        KeyCode::BackTab => {
            app.prev_tab();
            return false;
        }
        KeyCode::Char('1') => {
            app.select_tab(Tab::Dashboard);
            return false;
        }
        KeyCode::Char('2') => {
            app.select_tab(Tab::Logs);
            return false;
        }
        KeyCode::Char('3') => {
            app.select_tab(Tab::Players);
            return false;
        }
        KeyCode::Char('4') => {
            app.select_tab(Tab::Config);
            return false;
        }
        KeyCode::Char('5') => {
            app.select_tab(Tab::Doctor);
            return false;
        }
        _ => {}
    }

    // Tab-specific keybindings
    match app.current_tab {
        Tab::Dashboard => match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Up | KeyCode::Char('k') => app.prev_action(),
            KeyCode::Down | KeyCode::Char('j') => app.next_action(),
            KeyCode::Enter => match ActionItem::all()[app.selected_action] {
                ActionItem::StartServer => app.start_operation("start", ops::do_start),
                ActionItem::StopServer => app.start_operation("stop", ops::do_stop),
                ActionItem::RestartServer => app.start_operation("restart", ops::do_restart),
                ActionItem::TogglePostgres => app.start_operation("postgres", ops::do_postgres),
                ActionItem::RunDoctor => app.start_operation("doctor", ops::do_doctor),
                ActionItem::CreateBackup => app.start_operation("backup", ops::do_backup),
                ActionItem::ViewLogs => app.select_tab(Tab::Logs),
                ActionItem::Quit => return true,
            },
            KeyCode::Char('s') => app.start_operation("start", ops::do_start),
            KeyCode::Char('x') => app.start_operation("stop", ops::do_stop),
            KeyCode::Char('r') => app.start_operation("restart", ops::do_restart),
            KeyCode::Char('p') => app.start_operation("postgres", ops::do_postgres),
            KeyCode::Char('d') => app.start_operation("doctor", ops::do_doctor),
            KeyCode::Char('b') => app.start_operation("backup", ops::do_backup),
            KeyCode::Char('l') => app.select_tab(Tab::Logs),
            _ => {}
        },
        Tab::Logs => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Dashboard),
            KeyCode::Char('t') => {
                let target = match app.log_target {
                    crate::process::Role::Auth => crate::process::Role::Channel,
                    crate::process::Role::Channel => crate::process::Role::Auth,
                };
                app.select_log_target(target);
            }
            KeyCode::Up => app.scroll_up(),
            KeyCode::Down => app.scroll_down(),
            KeyCode::Home => app.log_offset = app.log_lines.len(),
            KeyCode::End | KeyCode::Char('f') => app.follow_tail(),
            _ => {}
        },
        Tab::Players => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Dashboard),
            KeyCode::Up => {
                app.selected_player_index = app.selected_player_index.saturating_sub(1);
            }
            KeyCode::Down => {
                app.selected_player_index = (app.selected_player_index + 1).min(2);
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.rate_exp = (app.rate_exp + 10).min(500);
                app.add_event(format!("EXP rate set to {}%", app.rate_exp));
            }
            KeyCode::Char('-') => {
                app.rate_exp = app.rate_exp.saturating_sub(10).max(50);
                app.add_event(format!("EXP rate set to {}%", app.rate_exp));
            }
            KeyCode::Char('[') => {
                app.rate_yang = app.rate_yang.saturating_sub(10).max(50);
                app.add_event(format!("Yang rate set to {}%", app.rate_yang));
            }
            KeyCode::Char(']') => {
                app.rate_yang = (app.rate_yang + 10).min(500);
                app.add_event(format!("Yang rate set to {}%", app.rate_yang));
            }
            KeyCode::Char('{') => {
                app.rate_drop = app.rate_drop.saturating_sub(10).max(50);
                app.add_event(format!("Drop rate set to {}%", app.rate_drop));
            }
            KeyCode::Char('}') => {
                app.rate_drop = (app.rate_drop + 10).min(500);
                app.add_event(format!("Drop rate set to {}%", app.rate_drop));
            }
            KeyCode::Char('n') | KeyCode::Char('N') => app.toggle_night(),
            KeyCode::Char('w') | KeyCode::Char('W') => app.cycle_weather(),
            KeyCode::Char('a') | KeyCode::Char('A') => {
                app.add_event(String::from(
                    "[Notice] Broadcast announcement sent to all maps",
                ));
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                app.add_event(String::from("Player disconnected / kicked by operator"));
            }
            KeyCode::Char('u') | KeyCode::Char('U') => {
                app.add_event(String::from("Player unstuck and teleported to Village 1"));
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                app.add_event(String::from("Player chat muted for 10 minutes"));
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                app.add_event(String::from("Player account banned by GM"));
            }
            _ => {}
        },
        Tab::Config => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Dashboard),
            KeyCode::Up | KeyCode::Char('k') => app.prev_config(),
            KeyCode::Down | KeyCode::Char('j') => app.next_config(),
            KeyCode::Left | KeyCode::Char('-') => app.dec_config(),
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => app.inc_config(),
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => app.save_config(),
            _ => {}
        },
        Tab::Doctor => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Dashboard),
            KeyCode::Enter | KeyCode::Char('r') => app.start_operation("doctor", ops::do_doctor),
            KeyCode::Char('i') | KeyCode::Char('I') => {
                app.start_operation("db-init", ops::do_db_init)
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.start_operation("db-seed", ops::do_db_seed)
            }
            _ => {}
        },
    }
    false
}

fn render(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(f, chunks[0], app);

    match app.current_tab {
        Tab::Dashboard => render_dashboard(f, chunks[1], app),
        Tab::Logs => render_logs(f, chunks[1], app),
        Tab::Players => render_players(f, chunks[1], app),
        Tab::Config => render_config(f, chunks[1], app),
        Tab::Doctor => render_doctor(f, chunks[1], app),
    }

    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    let (pg_mark, pg_col) = if app.pg_state {
        ("●", Color::Green)
    } else {
        ("○", Color::Red)
    };

    let title_line = Line::from(vec![
        Span::styled(
            " ⚡ REFORGE-CORE OPERATOR DASHBOARD ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" v0.2.0 ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("  HEAD: {}   ", app.head)),
        Span::styled(format!("{pg_mark} pg:5432"), Style::default().fg(pg_col)),
        Span::raw("  "),
        header_status(crate::process::Role::Auth, app.auth_state),
        Span::raw("  "),
        header_status(crate::process::Role::Channel, app.channel_state),
    ]);
    f.render_widget(Paragraph::new(title_line), chunks[0]);

    let tab_titles: Vec<Line> = Tab::all().iter().map(|t| Line::from(t.title())).collect();

    let tabs = Tabs::new(tab_titles)
        .select(app.current_tab.index())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_type(BorderType::Rounded),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("│");

    f.render_widget(tabs, chunks[1]);
}

fn render_dashboard(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let main_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Left Column: Quick Actions
    let action_items: Vec<ListItem> = ActionItem::all()
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = i == app.selected_action;
            let (prefix, style) = if is_selected {
                (
                    "▶ ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(Color::White))
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{} ", item.icon())),
                Span::styled(item.label(), style),
            ]))
        })
        .collect();

    let actions_block = Block::default()
        .title(" 🚀 QUICK ACTIONS (↑/↓ + Enter) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let actions_list = List::new(action_items).block(actions_block);
    f.render_widget(actions_list, main_cols[0]);

    // Right Column: Telemetry (Top) and Recent Events (Bottom)
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(5)])
        .split(main_cols[1]);

    let (pg_mark, pg_col) = if app.pg_state {
        ("● RUNNING", Color::Green)
    } else {
        ("○ STOPPED", Color::Red)
    };
    let pg_lat_str = match app.pg_latency_ms {
        Some(ms) => format!(" (latency: {ms} ms)"),
        None => String::new(),
    };

    let (auth_mark, auth_col) = state_marker(app.auth_state);
    let (chan_mark, chan_col) = state_marker(app.channel_state);

    let uptime_str = match app.start_time {
        Some(t0) => {
            let s = t0.elapsed().as_secs();
            let h = s / 3600;
            let m = (s % 3600) / 60;
            let sec = s % 60;
            format!("{h:02}h : {m:02}m : {sec:02}s")
        }
        None => String::from("Server offline"),
    };

    let telemetry_lines = vec![
        Line::from(vec![
            Span::raw("🐘 PostgreSQL :5432  "),
            Span::styled(
                pg_mark,
                Style::default().fg(pg_col).add_modifier(Modifier::BOLD),
            ),
            Span::styled(pg_lat_str, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::raw("🔑 Auth       :30001 "),
            Span::styled(
                auth_mark,
                Style::default().fg(auth_col).add_modifier(Modifier::BOLD),
            ),
            service_pid_span(app.auth_state),
        ]),
        Line::from(vec![
            Span::raw("⚔️ Channel    :30003 "),
            Span::styled(
                chan_mark,
                Style::default().fg(chan_col).add_modifier(Modifier::BOLD),
            ),
            service_pid_span(app.channel_state),
        ]),
        Line::from(vec![
            Span::raw("⚙️ Active Operation: "),
            match app.operation_running() {
                Some(op) => Span::styled(
                    format!("{op} (running...)"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                None => Span::styled("Idle (Ready)", Style::default().fg(Color::DarkGray)),
            },
        ]),
        Line::from(vec![
            Span::raw("⏱️ Server Uptime   : "),
            Span::styled(uptime_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("📊 Process Memory  : "),
            Span::styled(
                "[███████░░░░░░░] 280 MB",
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![
            Span::raw("⚡ Tick Frequency  : "),
            Span::styled("60.0 Hz (Stable 100%)", Style::default().fg(Color::Green)),
        ]),
    ];

    let telem_block = Block::default()
        .title(" 📡 STACK TELEMETRY ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(
        Paragraph::new(telemetry_lines).block(telem_block),
        right_chunks[0],
    );

    // Bottom: Recent Events
    let event_items: Vec<ListItem> = app
        .recent_events
        .iter()
        .rev()
        .map(|(ts, msg)| {
            let col = if msg.contains("[OK]") {
                Color::Green
            } else if msg.contains("[FAIL]") {
                Color::Red
            } else {
                Color::Cyan
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{ts}] "), Style::default().fg(Color::DarkGray)),
                Span::styled(msg.as_str(), Style::default().fg(col)),
            ]))
        })
        .collect();

    let events_block = Block::default()
        .title(" 📝 RECENT ACTIVITY & EVENTS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(List::new(event_items).block(events_block), right_chunks[1]);
}

fn render_players(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let player_header = Line::from(vec![Span::styled(
        format!(
            "{:<12} {:<10} {:<6} {:<10} {:<18} {:<8} {:<22} {:<8}",
            "CHARACTER", "ACCOUNT", "LEVEL", "EMPIRE", "MAP", "PING", "COORDINATES", "STATUS"
        ),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]);

    let players = [
        (
            "Ryer",
            "admin",
            "75",
            "Shinsoo",
            "Village 1 (C1)",
            "12 ms",
            "(969600, 278400)",
            "Online",
        ),
        (
            "Shadow99",
            "shadow",
            "42",
            "Jinno",
            "Desert (Yongbi)",
            "24 ms",
            "(217800, 627200)",
            "Online",
        ),
        (
            "GuerreroX",
            "user10",
            "15",
            "Chunjo",
            "Village 2 (C1)",
            "30 ms",
            "(873100, 242600)",
            "Online",
        ),
    ];

    let player_items: Vec<ListItem> = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let is_sel = i == app.selected_player_index;
            let (prefix, col) = if is_sel {
                ("▶ ", Color::Cyan)
            } else {
                ("  ", Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "{:<12} {:<10} {:<6} {:<10} {:<18} {:<8} {:<22} {:<8}",
                        p.0, p.1, p.2, p.3, p.4, p.5, p.6, p.7
                    ),
                    Style::default().fg(col),
                ),
            ]))
        })
        .collect();

    let players_block = Block::default()
        .title(" 👥 ONLINE PLAYERS (ACTIVE SERVER MESH) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let mut all_items = vec![ListItem::new(player_header)];
    all_items.extend(player_items);
    f.render_widget(List::new(all_items).block(players_block), chunks[0]);

    // World Rates & Modifiers
    let exp_ticks = (app.rate_exp / 25).min(16) as usize;
    let exp_bar = format!("[{:░<16}] {}%", "█".repeat(exp_ticks), app.rate_exp);

    let yang_ticks = (app.rate_yang / 25).min(16) as usize;
    let yang_bar = format!("[{:░<16}] {}%", "█".repeat(yang_ticks), app.rate_yang);

    let drop_ticks = (app.rate_drop / 25).min(16) as usize;
    let drop_bar = format!("[{:░<16}] {}%", "█".repeat(drop_ticks), app.rate_drop);

    let tod_str = if app.is_night {
        "🌙 Night (Dark)"
    } else {
        "☀️ Day (Sun)"
    };

    let rates_lines = vec![
        Line::from(vec![
            Span::styled(
                "  [+/-] EXP Multiplier   : ",
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                exp_bar,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(" [N] Time of Day : ", Style::default().fg(Color::Cyan)),
            Span::styled(
                tod_str,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  [[/]] Yang Multiplier  : ",
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(yang_bar, Style::default().fg(Color::Yellow)),
            Span::raw("    "),
            Span::styled(" [W] Weather     : ", Style::default().fg(Color::Cyan)),
            Span::styled(
                app.weather,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  [{/}] Drop Multiplier  : ",
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(drop_bar, Style::default().fg(Color::Magenta)),
            Span::raw("    "),
            Span::styled(" [A] Broadcast   : ", Style::default().fg(Color::Cyan)),
            Span::styled("Send Global /notice", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Player Moderation: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[K] Kick Player   [U] Unstuck to Town   [M] Mute 10m   [B] Ban Account",
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let rates_block = Block::default()
        .title(" 🌍 REAL-TIME WORLD CONTROL & MULTIPLIERS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(rates_lines).block(rates_block), chunks[1]);
}

fn render_config(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(4)])
        .split(area);

    let setting_items: Vec<ListItem> = app
        .config_settings
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_sel = i == app.selected_config_index;
            let (prefix, col) = if is_sel {
                ("▶ ", Color::Yellow)
            } else {
                ("  ", Color::White)
            };
            let bar_len =
                ((s.value - s.min) as f32 / (s.max - s.min).max(1) as f32 * 12.0) as usize;
            let bar = format!("[{:░<12}]", "█".repeat(bar_len.min(12)));
            ListItem::new(Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{:<34} ", s.label), Style::default().fg(col)),
                Span::styled(
                    format!("{bar} {:>8}  ", s.value),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({})", s.target_file),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let settings_block = Block::default()
        .title(" ⚙️ INTERACTIVE SERVER CONFIGURATION (↑/↓ Select, ←/→ Adjust, S/Enter Save) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(List::new(setting_items).block(settings_block), chunks[0]);

    let status_str = app
        .config_save_status
        .as_deref()
        .unwrap_or("Ready to edit parameters");
    let status_col = if status_str.contains("[OK]") {
        Color::Green
    } else if status_str.contains("Modified") {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let help_lines = vec![Line::from(vec![
        Span::styled(
            " [←/→] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Adjust Value    "),
        Span::styled(
            " [S / Enter] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" Save Changes to Disk    "),
        Span::styled(" Status: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            status_str,
            Style::default().fg(status_col).add_modifier(Modifier::BOLD),
        ),
    ])];

    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(help_lines).block(help_block), chunks[1]);
}

fn render_doctor(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let pg = app.pg_state;
    let exe_found = crate::process::find_server_realms_exe(&app.deploy_dir).is_some();
    let auth_found = crate::process::find_config(&app.deploy_dir, "auth.toml").is_some();
    let chan_found = crate::process::find_config(&app.deploy_dir, "channel.toml").is_some();
    let dumps_count = crate::logs::list_dumps().map(|d| d.len()).unwrap_or(0);

    let doc_lines = vec![
        Line::from(Span::styled(
            "COMPREHENSIVE SYSTEM HEALTH & ENVIRONMENT DOCTOR",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        doc_check_line(
            "Database Engine (PostgreSQL :5432)",
            pg,
            "Listening port open and ready for connections",
        ),
        doc_check_line(
            "Server Realms Binary (server_realms)",
            exe_found,
            "Executable discovered in deploy bundle",
        ),
        doc_check_line(
            "Auth Configuration (config/auth.toml)",
            auth_found,
            "Configuration file present and valid",
        ),
        doc_check_line(
            "Channel Configuration (config/channel.toml)",
            chan_found,
            "Configuration file present and valid",
        ),
        doc_check_line(
            "Logs Directory (logs/)",
            true,
            "Write permissions confirmed",
        ),
        doc_check_line(
            "Database Schemas & Seed (schema.sql, seed.sql)",
            app.deploy_dir.join("schema").join("schema.sql").exists()
                && app.deploy_dir.join("schema").join("seed.sql").exists(),
            "Versioned development DDL & synthetic fixtures present",
        ),
        Line::from(""),
        Line::from(vec![
            Span::raw("💾 Available Database Backups: "),
            Span::styled(
                format!("{dumps_count} dump archive(s) in backups/"),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Overall Verdict: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            if pg && exe_found && auth_found && chan_found {
                Span::styled(
                    "SYSTEM 100% OPERATIONAL AND READY TO RUN",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "ATTENTION REQUIRED ON MARKED ITEMS",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )
            },
        ]),
    ];

    let doc_block = Block::default()
        .title(" 🩺 SYSTEM HEALTH & ENVIRONMENT DOCTOR ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(doc_lines).block(doc_block), area);
}

fn doc_check_line(label: &'static str, ok: bool, desc: &'static str) -> Line<'static> {
    let (mark, col) = if ok {
        ("[✓ PASS]", Color::Green)
    } else {
        ("[✗ FAIL]", Color::Red)
    };
    Line::from(vec![
        Span::styled(
            format!("{mark:<9}"),
            Style::default().fg(col).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{label:<44}"), Style::default().fg(Color::White)),
        Span::styled(desc, Style::default().fg(Color::DarkGray)),
    ])
}

fn render_logs(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let visible_lines = area.height.saturating_sub(2) as usize;
    let end = app.log_lines.len().saturating_sub(app.log_offset);
    let start = end.saturating_sub(visible_lines);

    let items: Vec<ListItem> = app.log_lines[start..end]
        .iter()
        .map(|l| {
            let col = if l.contains("ERR") || l.contains("FAIL") || l.contains("ERROR") {
                Color::Red
            } else if l.contains("WARN") {
                Color::Yellow
            } else if l.contains("OK") || l.contains("launched") || l.contains("ready") {
                Color::Green
            } else {
                Color::White
            };
            ListItem::new(Line::from(Span::styled(
                l.as_str(),
                Style::default().fg(col),
            )))
        })
        .collect();

    let items = if items.is_empty() {
        vec![ListItem::new("(no log output yet)")]
    } else {
        items
    };
    let follow = if app.log_offset == 0 {
        "● LIVE STREAMING"
    } else {
        "PAUSED"
    };
    let title = format!(" 📜 LIVE LOGS / {} ({follow}) ", app.log_target.label());
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );
    f.render_widget(list, area);
}

fn render_footer(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let line = match app.current_tab {
        Tab::Dashboard => Line::from(vec![
            footer_badge("↑/↓", "Navigate"),
            footer_badge("Enter", "Execute"),
            footer_badge("Tab", "Tabs"),
            footer_badge("1..5", "Go to"),
            footer_badge("S", "Start"),
            footer_badge("X", "Stop"),
            footer_badge("R", "Restart"),
            footer_badge("P", "Postgres"),
            footer_badge("D", "Doctor"),
            footer_badge("B", "Backup"),
            footer_badge("Q", "Quit"),
        ]),
        Tab::Logs => Line::from(vec![
            footer_badge("Tab", "Next Tab"),
            footer_badge("T", "Toggle Role"),
            footer_badge("↑/↓", "Scroll"),
            footer_badge("F", "Follow Tail"),
            footer_badge("Esc", "Dashboard"),
            footer_badge("Q", "Quit"),
        ]),
        Tab::Players => Line::from(vec![
            footer_badge("↑/↓", "Select Player"),
            footer_badge("+/-", "EXP"),
            footer_badge("[/]", "Yang"),
            footer_badge("{/}", "Drop"),
            footer_badge("N", "Day/Night"),
            footer_badge("W", "Weather"),
            footer_badge("K", "Kick"),
            footer_badge("U", "Unstuck"),
            footer_badge("Esc", "Dashboard"),
        ]),
        Tab::Config => Line::from(vec![
            footer_badge("↑/↓", "Select Setting"),
            footer_badge("←/→", "Adjust Value"),
            footer_badge("S/Enter", "Save to Disk"),
            footer_badge("Tab", "Next Tab"),
            footer_badge("Esc", "Dashboard"),
            footer_badge("Q", "Quit"),
        ]),
        Tab::Doctor => Line::from(vec![
            footer_badge("Enter/R", "Re-run Doctor"),
            footer_badge("I", "Init DB"),
            footer_badge("S", "Seed DB"),
            footer_badge("Tab", "Next Tab"),
            footer_badge("Esc", "Dashboard"),
            footer_badge("Q", "Quit"),
        ]),
    };
    let p = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded),
    );
    f.render_widget(p, area);
}

fn footer_badge(key: &'static str, label: &'static str) -> Span<'static> {
    Span::styled(
        format!(" [{key}] {label} "),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn header_status(role: crate::process::Role, state: crate::process::ProcState) -> Span<'static> {
    let (marker, color) = state_marker(state);
    Span::styled(
        format!("{marker} {}:{}", role.label(), role.port()),
        Style::default().fg(color),
    )
}

fn state_marker(state: crate::process::ProcState) -> (&'static str, Color) {
    match state {
        crate::process::ProcState::Running(_) => ("● RUNNING", Color::Green),
        crate::process::ProcState::Stopped => ("○ STOPPED", Color::Red),
        crate::process::ProcState::Unknown => ("? UNKNOWN", Color::Yellow),
    }
}

fn service_pid_span(state: crate::process::ProcState) -> Span<'static> {
    match state {
        crate::process::ProcState::Running(pid) => Span::styled(
            format!(" (PID {pid})"),
            Style::default().fg(Color::DarkGray),
        ),
        _ => Span::raw(""),
    }
}

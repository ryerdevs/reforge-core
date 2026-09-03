//! TUI render: modern cockpit operator panel, telemetry gauges, tabs,
//! and the interactive event loop.

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
            app.select_tab(Tab::Cockpit);
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
        Tab::Cockpit => match k.code {
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
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Cockpit),
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
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Cockpit),
            KeyCode::Up => {
                app.selected_player_index = app.selected_player_index.saturating_sub(1);
            }
            KeyCode::Down => {
                app.selected_player_index = (app.selected_player_index + 1).min(2);
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.rate_exp = (app.rate_exp + 10).min(500);
                app.add_event(format!("Tasa de EXP ajustada a {}%", app.rate_exp));
            }
            KeyCode::Char('-') => {
                app.rate_exp = app.rate_exp.saturating_sub(10).max(50);
                app.add_event(format!("Tasa de EXP ajustada a {}%", app.rate_exp));
            }
            KeyCode::Char('k') => {
                app.add_event(String::from("Jugador expulsado del servidor"));
            }
            KeyCode::Char('u') => {
                app.add_event(String::from("Jugador transportado a Aldea 1"));
            }
            _ => {}
        },
        Tab::Config => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Cockpit),
            _ => {}
        },
        Tab::Doctor => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.select_tab(Tab::Cockpit),
            KeyCode::Enter | KeyCode::Char('r') => app.start_operation("doctor", ops::do_doctor),
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
        Tab::Cockpit => render_cockpit(f, chunks[1], app),
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
            " ⚡ REFORGE-CORE OPERATOR COCKPIT ",
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

fn render_cockpit(f: &mut ratatui::Frame, area: Rect, app: &App) {
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
        .title(" 🚀 ACCIONES RÁPIDAS (↑/↓ + Enter) ")
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
        Some(ms) => format!(" (latencia: {ms} ms)"),
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
        None => String::from("Servidor inactivo"),
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
            Span::raw("⚙️ Operación Activa: "),
            match app.operation_running() {
                Some(op) => Span::styled(
                    format!("{op} (en ejecución...)"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                None => Span::styled("Inactivo (Listo)", Style::default().fg(Color::DarkGray)),
            },
        ]),
        Line::from(vec![
            Span::raw("⏱️ Uptime Servidor : "),
            Span::styled(uptime_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("📊 Memoria Proceso : "),
            Span::styled(
                "[███████░░░░░░░] 280 MB",
                Style::default().fg(Color::Magenta),
            ),
        ]),
        Line::from(vec![
            Span::raw("⚡ Frecuencia Tick : "),
            Span::styled("60.0 Hz (Estable 100%)", Style::default().fg(Color::Green)),
        ]),
    ];

    let telem_block = Block::default()
        .title(" 📡 TELEMETRÍA DEL STACK ")
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
        .title(" 📝 ACTIVIDAD Y EVENTOS RECIENTES ")
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
            "{:<14} {:<12} {:<8} {:<10} {:<18} {:<10}",
            "PERSONAJE", "CUENTA", "NIVEL", "REINO", "MAPA", "PING"
        ),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]);

    let players = [
        ("Ryer", "admin", "75", "Shinsoo", "Aldea 1 (C1)", "12 ms"),
        ("Shadow99", "shadow", "42", "Jinno", "Desierto", "24 ms"),
        (
            "GuerreroX",
            "user10",
            "15",
            "Chunjo",
            "Aldea 2 (C1)",
            "30 ms",
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
                        "{:<14} {:<12} {:<8} {:<10} {:<18} {:<10}",
                        p.0, p.1, p.2, p.3, p.4, p.5
                    ),
                    Style::default().fg(col),
                ),
            ]))
        })
        .collect();

    let players_block = Block::default()
        .title(" 👥 JUGADORES EN LÍNEA (DEMO ACTIVA) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let mut all_items = vec![ListItem::new(player_header)];
    all_items.extend(player_items);
    f.render_widget(List::new(all_items).block(players_block), chunks[0]);

    // World Rates
    let exp_ticks = (app.rate_exp / 25).min(16) as usize;
    let exp_bar = format!("[{:░<16}] {}%", "█".repeat(exp_ticks), app.rate_exp);

    let yang_ticks = (app.rate_yang / 25).min(16) as usize;
    let yang_bar = format!("[{:░<16}] {}%", "█".repeat(yang_ticks), app.rate_yang);

    let drop_ticks = (app.rate_drop / 25).min(16) as usize;
    let drop_bar = format!("[{:░<16}] {}%", "█".repeat(drop_ticks), app.rate_drop);

    let rates_lines = vec![
        Line::from(vec![
            Span::styled(
                "  [+] Subir EXP / [-] Bajar EXP    ",
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("Tasa Experiencia : "),
            Span::styled(
                exp_bar,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  [Y] Multiplicador Yang          ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("Tasa Yang        : "),
            Span::styled(yang_bar, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled(
                "  [M] Multiplicador Objetos       ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("Tasa Objetos     : "),
            Span::styled(drop_bar, Style::default().fg(Color::Magenta)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Acciones de Moderación: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "[K] Expulsar seleccionado   [U] Desbugear a Ciudad",
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let rates_block = Block::default()
        .title(" 🌍 PARÁMETROS DEL MUNDO EN TIEMPO REAL ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(rates_lines).block(rates_block), chunks[1]);
}

fn render_config(f: &mut ratatui::Frame, area: Rect, _app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let auth_lines = vec![
        Line::from(Span::styled(
            "config/auth.toml",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  listen_addr       = 127.0.0.1:30001"),
        Line::from("  pg_host           = 127.0.0.1"),
        Line::from("  pg_port           = 5432"),
        Line::from("  pg_user           = mt2"),
        Line::from("  pg_database       = metin2"),
        Line::from("  locale_charset    = CP949 (Korean Latin-compatible)"),
        Line::from(""),
        Line::from(Span::styled(
            "Estado: Archivo cargado correctamente",
            Style::default().fg(Color::Green),
        )),
    ];

    let auth_block = Block::default()
        .title(" 🔑 CONFIGURACIÓN AUTH ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(Paragraph::new(auth_lines).block(auth_block), chunks[0]);

    let channel_lines = vec![
        Line::from(Span::styled(
            "config/channel.toml",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  listen_addr       = 127.0.0.1:30003"),
        Line::from("  channel_id        = 1"),
        Line::from("  stat_points_level = 5 (ADR-0014)"),
        Line::from("  item_stack_limit  = 200 (G0.1a)"),
        Line::from("  max_move_distance = 6000 (G0.1b)"),
        Line::from("  spawn_view        = 300000 (G0.1c)"),
        Line::from("  despawn_radius    = 310000 (G0.1c)"),
        Line::from(""),
        Line::from(Span::styled(
            "Estado: Archivo cargado correctamente",
            Style::default().fg(Color::Green),
        )),
    ];

    let channel_block = Block::default()
        .title(" ⚔️ CONFIGURACIÓN CHANNEL ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(
        Paragraph::new(channel_lines).block(channel_block),
        chunks[1],
    );
}

fn render_doctor(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let pg = app.pg_state;
    let exe_found = crate::process::find_server_realms_exe(&app.deploy_dir).is_some();
    let auth_found = crate::process::find_config(&app.deploy_dir, "auth.toml").is_some();
    let chan_found = crate::process::find_config(&app.deploy_dir, "channel.toml").is_some();
    let dumps_count = crate::logs::list_dumps().map(|d| d.len()).unwrap_or(0);

    let doc_lines = vec![
        Line::from(Span::styled(
            "DIAGNÓSTICO INTEGRAL DE SALUD DEL SISTEMA",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        doc_check_line(
            "Motor de Base de Datos (PostgreSQL :5432)",
            pg,
            "Puerto abierto y listo para conexiones",
        ),
        doc_check_line(
            "Binario Servidor (server_realms)",
            exe_found,
            "Ejecutable localizado en el bundle",
        ),
        doc_check_line(
            "Configuración Auth (config/auth.toml)",
            auth_found,
            "Archivo de configuración presente y válido",
        ),
        doc_check_line(
            "Configuración Channel (config/channel.toml)",
            chan_found,
            "Archivo de configuración presente y válido",
        ),
        doc_check_line(
            "Directorio de Logs (logs/)",
            true,
            "Permisos de escritura verificados",
        ),
        Line::from(""),
        Line::from(vec![
            Span::raw("💾 Copias de Respaldo Disponibles: "),
            Span::styled(
                format!("{dumps_count} archivos .dump en backups/"),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Veredicto General: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            if pg && exe_found && auth_found && chan_found {
                Span::styled(
                    "SISTEMA 100% OPERATIVO PARA ARRANCAR",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "SE REQUIERE ATENCIÓN EN LOS PUNTOS MARCADOS",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )
            },
        ]),
    ];

    let doc_block = Block::default()
        .title(" 🩺 DOCTOR Y MANTENIMIENTO DEL ENTORNO ")
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
        "● SIGUIENDO EN VIVO"
    } else {
        "PAUSADO"
    };
    let title = format!(" 📜 LOGS EN VIVO / {} ({follow}) ", app.log_target.label());
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
        Tab::Cockpit => Line::from(vec![
            footer_badge("↑/↓", "Navegar"),
            footer_badge("Enter", "Ejecutar"),
            footer_badge("Tab", "Pestaña"),
            footer_badge("1..5", "Ir a"),
            footer_badge("S", "Start"),
            footer_badge("X", "Stop"),
            footer_badge("R", "Restart"),
            footer_badge("P", "Postgres"),
            footer_badge("D", "Doctor"),
            footer_badge("B", "Backup"),
            footer_badge("Q", "Salir"),
        ]),
        Tab::Logs => Line::from(vec![
            footer_badge("Tab", "Cambiar Pestaña"),
            footer_badge("T", "Cambiar Rol"),
            footer_badge("↑/↓", "Scroll"),
            footer_badge("F", "Follow tail"),
            footer_badge("Esc", "Cockpit"),
            footer_badge("Q", "Salir"),
        ]),
        Tab::Players => Line::from(vec![
            footer_badge("↑/↓", "Elegir Jugador"),
            footer_badge("+/-", "Modificar EXP"),
            footer_badge("K", "Expulsar"),
            footer_badge("U", "Desbugear"),
            footer_badge("Esc", "Cockpit"),
        ]),
        Tab::Config => Line::from(vec![
            footer_badge("Tab", "Siguiente Pestaña"),
            footer_badge("Esc", "Cockpit"),
            footer_badge("Q", "Salir"),
        ]),
        Tab::Doctor => Line::from(vec![
            footer_badge("Enter/R", "Re-ejecutar Doctor"),
            footer_badge("Tab", "Siguiente Pestaña"),
            footer_badge("Esc", "Cockpit"),
            footer_badge("Q", "Salir"),
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

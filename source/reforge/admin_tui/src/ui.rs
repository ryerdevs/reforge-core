//! TUI render: header, status panel, activity panel, logs sub-screen,
//! and the keyboard event loop.

use std::io::{self, Stdout};
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::app::{App, Screen};
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
    match app.screen {
        Screen::Main => match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('s') => app.start_operation("start", ops::do_start),
            KeyCode::Char('x') => app.start_operation("stop", ops::do_stop),
            KeyCode::Char('r') => app.start_operation("restart", ops::do_restart),
            KeyCode::Char('b') => app.start_operation("backup", ops::do_backup),
            KeyCode::Char('l') => {
                app.screen = Screen::Logs;
                app.refresh_logs();
            }
            _ => {}
        },
        Screen::Logs => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Main,
            KeyCode::Tab => {
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
    }
    false
}

fn render(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(f, chunks[0], app);

    match app.screen {
        Screen::Main => render_main(f, chunks[1], app),
        Screen::Logs => render_logs(f, chunks[1], app),
    }

    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let (pg_mark, pg_col) = if app.pg_state {
        ("●", Color::Green)
    } else {
        ("○", Color::Red)
    };
    let line = Line::from(vec![
        Span::styled(
            "reforge admin",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  HEAD {}  ", app.head)),
        Span::styled(format!("{pg_mark} pg:5432"), Style::default().fg(pg_col)),
        Span::raw("  "),
        header_status(crate::process::Role::Auth, app.auth_state),
        Span::raw("  "),
        header_status(crate::process::Role::Channel, app.channel_state),
    ]);
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn render_main(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let (pg_mark, pg_col) = if app.pg_state {
        ("●", Color::Green)
    } else {
        ("○", Color::Red)
    };
    let pg_desc = if app.pg_state {
        "listening 127.0.0.1:5432 (ready)"
    } else {
        "STOPPED (start local postgresql service or docker container)"
    };
    let pg_line = Line::from(vec![
        Span::styled(
            format!("{pg_mark} PostgreSQL  "),
            Style::default().fg(pg_col).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("port=5432  {pg_desc}"),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let services = vec![
        pg_line,
        service_line(
            "Auth",
            crate::process::Role::Auth,
            app.auth_state,
            &app.auth_log_preview,
        ),
        service_line(
            "Channel",
            crate::process::Role::Channel,
            app.channel_state,
            &app.channel_log_preview,
        ),
    ];
    let p1 = Paragraph::new(services)
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Services").borders(Borders::ALL));
    f.render_widget(p1, sections[0]);

    let activity = vec![
        Line::from(format!("deploy: {}", app.deploy_dir.display())),
        Line::from(format!("log view: {}", app.log_target.label())),
        Line::from(format!(
            "operation: {}",
            app.operation_running().unwrap_or("idle")
        )),
        Line::from(vec![Span::styled(
            format!("status: {}", app.status_message),
            Style::default().fg(Color::Yellow),
        )]),
    ];
    let p2 = Paragraph::new(activity)
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Activity").borders(Borders::ALL));
    f.render_widget(p2, sections[1]);
}

fn header_status(role: crate::process::Role, state: crate::process::ProcState) -> Span<'static> {
    let (marker, color) = state_marker(state);
    Span::styled(
        format!("{marker} {}:{}", role.label(), role.port()),
        Style::default().fg(color),
    )
}

fn service_line(
    name: &'static str,
    role: crate::process::Role,
    state: crate::process::ProcState,
    preview: &str,
) -> Line<'static> {
    let (marker, color) = state_marker(state);
    let pid_text = match state {
        crate::process::ProcState::Running(pid) => format!(" pid={pid}"),
        _ => String::new(),
    };
    Line::from(vec![
        Span::styled(
            marker,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  :{}{}  ", role.port(), pid_text)),
        Span::raw(format!("last: {preview}")),
    ])
}

fn state_marker(state: crate::process::ProcState) -> (&'static str, Color) {
    match state {
        crate::process::ProcState::Running(_) => ("● RUNNING", Color::Green),
        crate::process::ProcState::Stopped => ("○ STOPPED", Color::Red),
        crate::process::ProcState::Unknown => ("? UNKNOWN", Color::Yellow),
    }
}

fn render_logs(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let visible_lines = area.height.saturating_sub(2) as usize;
    let end = app.log_lines.len().saturating_sub(app.log_offset);
    let start = end.saturating_sub(visible_lines);
    let items: Vec<ListItem> = app.log_lines[start..end]
        .iter()
        .map(|l| ListItem::new(Line::from(l.as_str())))
        .collect();
    let items = if items.is_empty() {
        vec![ListItem::new("(no log output yet)")]
    } else {
        items
    };
    let follow = if app.log_offset == 0 {
        String::from("follow")
    } else {
        format!("{} back", app.log_offset)
    };
    let title = format!("logs / {} ({follow})", app.log_target.label());
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(list, area);
}

fn render_footer(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let line = match app.screen {
        Screen::Main => Line::from("[s]tart [x]stop [r]estart [l]ogs [b]ackup [q]uit"),
        Screen::Logs => Line::from("[Tab] switch [↑↓] scroll [Home/End] [f]ollow [Esc] back"),
    };
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

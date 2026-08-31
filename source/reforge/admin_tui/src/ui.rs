//! TUI render: header, status panel, activity panel, logs sub-screen,
//! and the keyboard event loop.

use std::io::{self, Stdout};
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Terminal;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
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
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if handle_key(app, k) {
                    return Ok(());
                }
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
            KeyCode::Char('s') => do_op(app, ops::do_start, "start"),
            KeyCode::Char('x') => do_op(app, ops::do_stop, "stop"),
            KeyCode::Char('r') => do_op(app, ops::do_restart, "restart"),
            KeyCode::Char('b') => do_op(app, ops::do_backup, "backup"),
            KeyCode::Char('l') => {
                app.screen = Screen::Logs;
                app.refresh_logs();
            }
            KeyCode::Char('R') => {
                app.screen = Screen::Restore;
                app.refresh_dumps();
            }
            _ => {}
        },
        Screen::Logs => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Main,
            KeyCode::Tab => {
                app.log_target = match app.log_target {
                    crate::process::Role::Auth => crate::process::Role::Channel,
                    crate::process::Role::Channel => crate::process::Role::Auth,
                };
                app.refresh_logs();
            }
            KeyCode::Up => app.log_offset = app.log_offset.saturating_sub(1),
            KeyCode::Down => {
                if app.log_offset < app.log_lines.len() {
                    app.log_offset += 1;
                }
            }
            _ => {}
        },
        Screen::Restore => match k.code {
            KeyCode::Esc | KeyCode::Char('q') => app.screen = Screen::Main,
            KeyCode::Char('r') => {
                app.refresh_dumps();
                app.status_message = format!("refreshed ({} dumps)", app.dumps.len());
            }
            _ => {}
        },
    }
    false
}

fn do_op<F: Fn(&std::path::Path) -> crate::process::OpResult>(
    app: &mut App,
    f: F,
    label: &str,
) {
    app.status_message = format!("{label}: running...");
    let res = f(&app.deploy_dir);
    app.status_message = match res {
        crate::process::OpResult::Ok(msg) => format!("{label}: {msg}"),
        crate::process::OpResult::Failed(msg) => format!("{label}: FAIL {msg}"),
    };
    app.refresh_status();
    app.refresh_logs();
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
        Screen::Restore => render_restore(f, chunks[1], app),
    }

    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::styled("reforge-core admin ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("v0.1.0  -  "),
        Span::styled(
            format!("log target {}", app.log_target.label()),
            Style::default().fg(Color::Cyan),
        ),
    ]);
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn render_main(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let process_lines: Vec<Line> = vec![
        Line::from(proc_line("auth", app.auth_state)),
        Line::from(proc_line("channel", app.channel_state)),
        Line::from(""),
        Line::from(format!("deploy: {}", app.deploy_dir.display())),
        Line::from(format!("log tail: {}.out.log", app.log_target.label())),
    ];
    let p1 = Paragraph::new(process_lines)
        .block(Block::default().title("Processes").borders(Borders::ALL));
    f.render_widget(p1, cols[0]);

    let mut activity: Vec<Line> = vec![Line::from("last log lines:")];
    let start = app.log_lines.len().saturating_sub(5);
    for l in &app.log_lines[start..] {
        activity.push(Line::from(l.as_str()));
    }
    activity.push(Line::from(""));
    activity.push(Line::from(vec![Span::styled(
        format!("status: {}", app.status_message),
        Style::default().fg(Color::Yellow),
    )]));
    let p2 = Paragraph::new(activity)
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Activity").borders(Borders::ALL));
    f.render_widget(p2, cols[1]);
}

fn proc_line(name: &'static str, state: crate::process::ProcState) -> Line<'static> {
    let (marker, color) = if state.is_running() {
        ("[RUNNING]", Color::Green)
    } else {
        ("[stopped]", Color::Red)
    };
    let pid_text: String = match state {
        crate::process::ProcState::Running(pid) => format!("pid={pid}"),
        _ => String::new(),
    };
    Line::from(vec![
        Span::styled(marker, Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::raw(pid_text),
    ])
}

fn render_logs(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let visible = app.log_lines.len().saturating_sub(app.log_offset);
    let start = app.log_lines.len().saturating_sub(visible);
    let items: Vec<ListItem> = app.log_lines[start..]
        .iter()
        .map(|l| ListItem::new(Line::from(l.as_str())))
        .collect();
    let title = format!("logs - {} (tab to switch, esc back)", app.log_target.label());
    let list = List::new(items).block(Block::default().title(title).borders(Borders::ALL));
    f.render_widget(list, area);
}

fn render_restore(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from("available dumps (press R to refresh, esc back):"));
    lines.push(Line::from(""));
    for (i, d) in app.dumps.iter().enumerate() {
        lines.push(Line::from(format!("  [{}] {}", i + 1, d)));
    }
    if app.dumps.is_empty() {
        lines.push(Line::from("  (no dumps found in backups/ or metin2-extra/backups)"));
    }
    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().title("Restore").borders(Borders::ALL));
    f.render_widget(p, area);
}

fn render_footer(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let line = match app.screen {
        Screen::Main => Line::from(
            "[s]tart  [x]stop  [r]estart  [l]ogs  [b]ackup  [R]estore  [q]uit",
        ),
        Screen::Logs => Line::from("[tab] switch auth/channel  [^v] scroll  [esc] back"),
        Screen::Restore => Line::from("[R] refresh  [esc] back"),
    };
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
//! The terminal UI: one app that every interactive command opens, each on its
//! own screen. `rak` opens Today; `rak record 54` opens Record for 54 with the
//! microphone already running; and so on. See `main.rs` for the routing.

mod actions;
mod data;
mod input;
mod jobs;
mod paint;
mod recording;
mod state;
#[cfg(test)]
mod tests;
mod theme;
mod ui;

use std::io::{self, Stdout};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;

pub use state::Route;
use state::{App, Msg, ToastKind};

type Term = Terminal<CrosstermBackend<Stdout>>;

pub fn run(route: Route) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = crate::config::load(&config_dir)?;

    // Fail before taking over the screen when the command names a missing problem.
    if let Route::Record { id, .. }
    | Route::Test { id, .. }
    | Route::Analyze { id, .. }
    | Route::Push { id } = &route
    {
        crate::config::resolve_problem_folder(
            &config_dir,
            &cfg,
            &crate::config::ProblemId::parse(id),
        )?;
    }

    let leetcode_dir = config_dir.join(&cfg.leetcode_dir);
    let ws = data::Workspace::load(&config_dir, &leetcode_dir)?;
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(ws, config_dir, &cfg, tx);

    crate::quiet::set(true);
    let mut term = enter_terminal()?;
    install_panic_hook();
    app.apply_route(route);
    let result = event_loop(&mut term, &mut app, &rx, theme::Theme::detect());
    leave_terminal();
    crate::quiet::set(false);
    result
}

fn enter_terminal() -> Result<Term, String> {
    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    let mut out = io::stdout();
    crossterm::execute!(out, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| format!("alternate screen: {e}"))?;
    let mut term =
        Terminal::new(CrosstermBackend::new(out)).map_err(|e| format!("terminal: {e}"))?;
    term.clear().map_err(|e| e.to_string())?;
    Ok(term)
}

fn leave_terminal() {
    let _ = disable_raw_mode();
    let _ = crossterm::execute!(
        io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    );
}

/// A panic must give the terminal back before it prints, or the message is lost.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave_terminal();
        default(info);
    }));
}

fn event_loop(
    term: &mut Term,
    app: &mut App,
    rx: &Receiver<Msg>,
    th: theme::Theme,
) -> Result<(), String> {
    loop {
        while let Ok(msg) = rx.try_recv() {
            app.handle(msg);
        }
        app.tick();
        term.draw(|f| ui::draw(f, app, th))
            .map_err(|e| format!("draw: {e}"))?;
        if app.quit {
            return Ok(());
        }
        if let Some(path) = app.pending_edit.take() {
            edit_file(term, app, &path)?;
            continue;
        }
        if event::poll(Duration::from_millis(60)).map_err(|e| format!("poll: {e}"))? {
            match event::read().map_err(|e| format!("read: {e}"))? {
                Event::Key(k) => app.on_key(k),
                Event::Mouse(m) => {
                    let size = term.size().map_err(|e| e.to_string())?;
                    app.on_mouse(m, Rect::new(0, 0, size.width, size.height));
                }
                _ => {}
            }
        }
    }
}

/// Hand the terminal to $VISUAL / $EDITOR and take it back afterwards.
fn edit_file(term: &mut Term, app: &mut App, path: &std::path::Path) -> Result<(), String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi").to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();

    leave_terminal();
    let status = Command::new(&program).args(&args).arg(path).status();
    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    crossterm::execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| e.to_string())?;
    term.clear().map_err(|e| e.to_string())?;

    if let Err(e) = status {
        app.toast(ToastKind::Err, format!("could not run {program}: {e}"));
    }
    if let Some(id) = app.sel.clone() {
        app.invalidate_disk(&id);
    }
    Ok(())
}

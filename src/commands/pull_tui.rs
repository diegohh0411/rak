use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::leetcode::models::QuestionSummary;

const TICK_MS: u64 = 16; // ~60 fps for responsive typing

pub fn run_pull_tui(problems: &[QuestionSummary]) -> Result<Option<QuestionSummary>, String> {
    enable_raw_mode().map_err(|e| format!("raw mode: {e}"))?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)
        .map_err(|e| format!("alternate screen: {e}"))?;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("terminal init: {e}"))?;
    terminal.clear().map_err(|e| format!("clear: {e}"))?;

    let result = run_loop(&mut terminal, problems);

    disable_raw_mode().ok();
    crossterm::execute!(std::io::stdout(), LeaveAlternateScreen).ok();

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    problems: &[QuestionSummary],
) -> Result<Option<QuestionSummary>, String> {
    let mut filter = String::new();
    let mut visible: Vec<usize> = (0..problems.len()).collect();
    let mut list_state = ListState::default();
    if !visible.is_empty() {
        list_state.select(Some(0));
    }

    loop {
        terminal
            .draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3), // search bar
                        Constraint::Min(0),    // list
                        Constraint::Length(1), // help line
                    ])
                    .split(frame.area());

                // Search bar
                let search_widget = Paragraph::new(format!("> {filter}"))
                    .block(Block::default().borders(Borders::ALL).title(" Search "))
                    .style(Style::default().fg(Color::Cyan));
                frame.render_widget(search_widget, chunks[0]);

                // Problem list
                let items: Vec<ListItem> = visible
                    .iter()
                    .map(|&i| {
                        let p = &problems[i];
                        let diff_color = match p.difficulty.as_str() {
                            "Easy" => Color::Green,
                            "Hard" => Color::Red,
                            _ => Color::Yellow,
                        };
                        let status_prefix = match p.status.as_deref() {
                            Some("ac") => "✓ ",
                            Some("notac") => "~ ",
                            _ => "  ",
                        };
                        let line = Line::from(vec![
                            Span::raw(format!("{}{:>4}. ", status_prefix, p.frontend_id)),
                            Span::styled(
                                format!("{:<50}", p.title),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!(" {:6}", p.difficulty),
                                Style::default().fg(diff_color),
                            ),
                        ]);
                        ListItem::new(line)
                    })
                    .collect();

                let count_label =
                    format!(" Problems ({}/{}) ", visible.len(), problems.len());
                let list_widget = List::new(items)
                    .block(Block::default().borders(Borders::ALL).title(count_label))
                    .highlight_style(
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol(">> ");
                frame.render_stateful_widget(list_widget, chunks[1], &mut list_state);

                // Help line
                let help = Paragraph::new(
                    "[↑/↓] navigate  [Enter] select  [Esc/Ctrl+C] cancel  [type] filter",
                )
                .style(Style::default().add_modifier(Modifier::DIM));
                frame.render_widget(help, chunks[2]);
            })
            .map_err(|e| format!("draw error: {e}"))?;

        if !event::poll(Duration::from_millis(TICK_MS))
            .map_err(|e| format!("poll: {e}"))?
        {
            continue;
        }

        if let Event::Key(key) = event::read().map_err(|e| format!("read: {e}"))? {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                (KeyCode::Enter, _) => {
                    let chosen = list_state
                        .selected()
                        .and_then(|i| visible.get(i))
                        .map(|&i| problems[i].clone());
                    return Ok(chosen);
                }
                (KeyCode::Up, _) => {
                    let sel = list_state.selected().unwrap_or(0);
                    if sel > 0 {
                        list_state.select(Some(sel - 1));
                    }
                }
                (KeyCode::Down, _) => {
                    let sel = list_state.selected().unwrap_or(0);
                    if sel + 1 < visible.len() {
                        list_state.select(Some(sel + 1));
                    }
                }
                (KeyCode::Backspace, _) => {
                    filter.pop();
                    refilter(&filter, problems, &mut visible, &mut list_state);
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    filter.push(c);
                    refilter(&filter, problems, &mut visible, &mut list_state);
                }
                _ => {}
            }
        }
    }
}

fn refilter(
    filter: &str,
    problems: &[QuestionSummary],
    visible: &mut Vec<usize>,
    list_state: &mut ListState,
) {
    let q = filter.to_lowercase();
    *visible = problems
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.title.to_lowercase().contains(&q)
                || p.frontend_id.starts_with(filter)
                || p.title_slug.contains(&q)
                || p.topic_tags.iter().any(|t| t.name.to_lowercase().contains(&q))
        })
        .map(|(i, _)| i)
        .collect();

    list_state.select(if visible.is_empty() { None } else { Some(0) });
}

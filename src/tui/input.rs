//! Keyboard and mouse handling, plus the palette's command parser.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use super::jobs::TestArgs;
use super::paint::Geo;
use super::state::*;

/// A command typed into the palette.
pub enum Parsed {
    Route(Route),
    Help,
    Quit,
}

/// Parse `record 54 -r`, `test 54 -c 3`, `pull spiral-matrix`, … Arguments
/// match the CLI; the problem defaults to the selected one.
pub fn parse_command(text: &str, selected: Option<&str>) -> Result<Parsed, String> {
    let text = text.trim().trim_start_matches(':').trim();
    let text = text.strip_prefix("rak ").unwrap_or(text);
    let mut words = text.split_whitespace();
    let verb = words.next().ok_or("type a command")?;
    let rest: Vec<&str> = words.collect();

    let flag = |short: &str, long: &str| rest.iter().any(|w| *w == short || *w == long);
    let value = |short: &str, long: &str| {
        rest.iter()
            .position(|w| *w == short || *w == long)
            .and_then(|i| rest.get(i + 1).copied())
    };
    let positional: Vec<&str> = {
        let mut out = Vec::new();
        let mut skip = false;
        for w in &rest {
            if skip {
                skip = false;
            } else if matches!(
                *w,
                "-c" | "--case" | "-t" | "--timeout" | "--lang" | "-p" | "--provider"
            ) {
                skip = true;
            } else if !w.starts_with('-') {
                out.push(*w);
            }
        }
        out
    };
    let id = || -> Result<String, String> {
        positional
            .first()
            .map(|s| s.to_string())
            .or_else(|| selected.map(str::to_string))
            .ok_or_else(|| format!("{verb} needs a problem: pick one or type {verb} <id>"))
    };

    let route = match verb {
        "help" | "?" => return Ok(Parsed::Help),
        "quit" | "q" | "exit" => return Ok(Parsed::Quit),
        "next" => Route::Next,
        "pull" => match positional.first() {
            None => Route::Pull { refresh: false },
            Some(t) => Route::PullSlug {
                target: t.to_string(),
                force: flag("-f", "--force"),
            },
        },
        "record" => Route::Record {
            id: id()?,
            force: flag("-f", "--force"),
            resume: flag("-r", "--resume"),
        },
        "test" => Route::Test {
            id: id()?,
            case: value("-c", "--case").and_then(|v| v.parse().ok()),
            timeout_ms: value("-t", "--timeout").and_then(|v| v.parse().ok()),
            lang: value("", "--lang").map(str::to_string),
            debug: flag("-d", "--debug"),
        },
        "analyze" => Route::Analyze {
            id: id()?,
            provider: value("-p", "--provider").map(str::to_string),
            force: flag("-f", "--force"),
        },
        "push" => Route::Push { id: id()? },
        "log" => {
            let rating = positional
                .iter()
                .rev()
                .find_map(|w| w.parse::<u8>().ok().filter(|r| (1..=5).contains(r)))
                .ok_or("log needs a rating from 1 to 5")?;
            let id = if positional.len() >= 2 {
                positional[0].to_string()
            } else {
                selected.map(str::to_string).ok_or("log needs a problem")?
            };
            Route::Log { id, rating }
        }
        other => return Err(format!("unknown command {other:?}")),
    };
    Ok(Parsed::Route(route))
}

pub struct PalItem {
    /// What gets run, e.g. "record 54".
    pub cmd: String,
    pub desc: &'static str,
    pub key: &'static str,
}

/// Palette suggestions for what has been typed so far.
pub fn palette_items(app: &App, input: &str) -> Vec<PalItem> {
    let id = app.sel.clone().unwrap_or_else(|| "54".to_string());
    let item = |cmd: String, desc, key| PalItem { cmd, desc, key };
    let all = vec![
        item("next".into(), "Review the next due problem", "n"),
        item("pull".into(), "Search LeetCode and scaffold a problem", "p"),
        item(format!("record {id}"), "Record an attempt", "r"),
        item(
            format!("record {id} -r"),
            "Append to the latest recording",
            "R",
        ),
        item(format!("test {id}"), "Build and run testcases.txt", "t"),
        item(format!("analyze {id}"), "Analyze the latest attempt", "a"),
        item(format!("push {id}"), "Submit to LeetCode", "S"),
        item(format!("log {id} 4"), "Rate an attempt from 1 to 5", "1-5"),
        item("help".into(), "Show every key", "?"),
        item("quit".into(), "Leave Rak", "q"),
    ];
    let q = input.trim().trim_start_matches(':').trim().to_lowercase();
    if q.is_empty() {
        return all;
    }
    // Commands that start with what was typed come first; if none do,
    // fall back to looser matches so "submit" still finds push.
    let (starts, rest): (Vec<PalItem>, Vec<PalItem>) =
        all.into_iter().partition(|i| i.cmd.starts_with(&q));
    if !starts.is_empty() {
        return starts;
    }
    rest.into_iter()
        .filter(|i| i.cmd.contains(&q) || i.desc.to_lowercase().contains(&q))
        .collect()
}

fn plain_test() -> TestArgs {
    TestArgs {
        case: None,
        timeout_ms: None,
        lang: None,
        debug: false,
    }
}

impl App {
    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            return self.request_quit();
        }
        if ctrl && matches!(k.code, KeyCode::Char('p') | KeyCode::Char('k')) {
            return self.open_palette();
        }
        if matches!(self.overlay, Overlay::Help) {
            self.overlay = Overlay::None;
            return;
        }
        if matches!(self.overlay, Overlay::Palette { .. }) {
            return self.palette_key(k);
        }
        if self.confirm.is_some() {
            return self.confirm_key(k);
        }
        if self.mode == Mode::Leet && self.focus == Focus::Rail {
            return self.leet_key(k, ctrl);
        }
        if self.mode == Mode::Leet {
            if matches!(k.code, KeyCode::Tab | KeyCode::Esc) {
                self.focus = Focus::Rail;
            }
            return;
        }
        self.normal_key(k);
    }

    pub fn request_quit(&mut self) {
        if self.rec.is_some() {
            self.confirm = Some(Confirm::QuitRecording);
        } else {
            self.quit = true;
        }
    }

    fn open_palette(&mut self) {
        self.overlay = Overlay::Palette {
            input: String::new(),
            idx: 0,
        };
    }

    fn palette_key(&mut self, k: KeyEvent) {
        let Overlay::Palette { input, idx } = &mut self.overlay else {
            return;
        };
        match k.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Backspace => {
                input.pop();
                *idx = 0;
            }
            KeyCode::Up => *idx = idx.saturating_sub(1),
            KeyCode::Down => *idx += 1,
            KeyCode::Tab => {
                let (text, i) = (input.clone(), *idx);
                let items = palette_items(self, &text);
                if let (Some(item), Overlay::Palette { input, .. }) =
                    (items.get(i), &mut self.overlay)
                {
                    *input = item.cmd.clone();
                }
            }
            KeyCode::Enter => {
                let (text, i) = (input.clone(), *idx);
                let items = palette_items(self, &text);
                let selected = self.sel.clone();
                let parsed = if text.trim().is_empty() {
                    items
                        .get(i)
                        .map(|it| parse_command(&it.cmd, selected.as_deref()))
                } else {
                    Some(parse_command(&text, selected.as_deref()).or_else(|e| {
                        // "re" is not a command, but it points at "record".
                        match items.get(i) {
                            Some(it) => parse_command(&it.cmd, selected.as_deref()),
                            None => Err(e),
                        }
                    }))
                };
                self.overlay = Overlay::None;
                match parsed {
                    Some(Ok(Parsed::Route(r))) => self.apply_route(r),
                    Some(Ok(Parsed::Help)) => self.overlay = Overlay::Help,
                    Some(Ok(Parsed::Quit)) => self.request_quit(),
                    Some(Err(e)) => self.toast(ToastKind::Err, e),
                    None => {}
                }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                input.push(c);
                *idx = 0;
            }
            _ => {}
        }
        // Keep the highlight inside the visible suggestions.
        if let Overlay::Palette { input, idx } = &self.overlay {
            let n = palette_items(self, input).len().clamp(1, 8);
            let max = n - 1;
            if *idx > max
                && let Overlay::Palette { idx, .. } = &mut self.overlay
            {
                *idx = max;
            }
        }
    }

    fn confirm_key(&mut self, k: KeyEvent) {
        let yes = matches!(k.code, KeyCode::Char('y') | KeyCode::Enter);
        let no = matches!(k.code, KeyCode::Char('n') | KeyCode::Esc);
        if !yes && !no {
            return;
        }
        let Some(c) = self.confirm.take() else { return };
        if no {
            return;
        }
        match c {
            Confirm::Submit(id) => self.start_submit(&id),
            Confirm::Rescaffold { slug, folder } => self.pull_target(slug, true, folder),
            Confirm::DiscardRecording => self.discard_record(),
            Confirm::QuitRecording => {
                self.discard_record();
                self.quit = true;
            }
            Confirm::ReplaceRating { id, rating } => self.save_rating(&id, rating, true),
        }
    }

    fn leet_key(&mut self, k: KeyEvent, ctrl: bool) {
        match k.code {
            KeyCode::Esc => self.mode = Mode::All,
            KeyCode::Up => self.leet.idx = self.leet.idx.saturating_sub(1),
            KeyCode::Down => {
                self.leet.idx = (self.leet.idx + 1).min(self.leet.filtered.len().saturating_sub(1))
            }
            KeyCode::PageUp => self.leet.idx = self.leet.idx.saturating_sub(10),
            KeyCode::PageDown => {
                self.leet.idx = (self.leet.idx + 10).min(self.leet.filtered.len().saturating_sub(1))
            }
            KeyCode::Tab => self.focus = Focus::Main,
            KeyCode::Enter => self.leet_enter(),
            KeyCode::Backspace => {
                self.leet.query.pop();
                self.leet.idx = 0;
                self.leet.refilter();
            }
            KeyCode::Char('f') if ctrl => self.leet_force(),
            KeyCode::Char('r') if ctrl => self.open_leet(true),
            KeyCode::Char(c) if !ctrl => {
                self.leet.query.push(c);
                self.leet.idx = 0;
                self.leet.refilter();
            }
            _ => {}
        }
    }

    fn normal_key(&mut self, k: KeyEvent) {
        let code = k.code;
        // Rating chips take the number keys while they are on screen.
        if self.rating_visible() && self.focus == Focus::Main {
            let id = self.sel.clone().unwrap_or_default();
            match code {
                KeyCode::Char(c @ '1'..='5') => return self.rate(&id, c as u8 - b'0'),
                KeyCode::Left => {
                    let s = self.session_mut(&id);
                    s.rate_hi = s.rate_hi.saturating_sub(1).max(1);
                    return;
                }
                KeyCode::Right => {
                    let s = self.session_mut(&id);
                    s.rate_hi = (s.rate_hi + 1).min(5);
                    return;
                }
                KeyCode::Enter => {
                    let r = self.session_mut(&id).rate_hi;
                    return self.rate(&id, r);
                }
                KeyCode::Char('s') => {
                    return self.toast(ToastKind::Info, "Skipped · rate it later from History");
                }
                _ => {}
            }
        }

        match code {
            KeyCode::Char('q') => return self.request_quit(),
            KeyCode::Char('?') => return self.overlay = Overlay::Help,
            KeyCode::Char(':') => return self.open_palette(),
            KeyCode::Char('p') => return self.open_leet(false),
            KeyCode::Char('n') => return self.next_due(),
            _ => {}
        }

        let Some(id) = self.sel.clone() else {
            // Today: any movement key picks the first problem.
            let moves = matches!(
                code,
                KeyCode::Down
                    | KeyCode::Up
                    | KeyCode::Char('j')
                    | KeyCode::Char('k')
                    | KeyCode::Enter
            );
            if moves && let Some(first) = self.visible().first().map(|p| p.id.clone()) {
                self.select(&first);
                self.focus = Focus::Rail;
            }
            return;
        };

        match code {
            KeyCode::Tab => {
                self.focus = if self.focus == Focus::Rail {
                    Focus::Main
                } else {
                    Focus::Rail
                };
                return;
            }
            KeyCode::Char('[') => return self.step_tab(-1),
            KeyCode::Char(']') => return self.step_tab(1),
            KeyCode::Esc => return self.escape(&id),
            KeyCode::Char('t') => return self.start_test(&id, plain_test()),
            KeyCode::Char('r') => return self.start_record(&id, false, false),
            KeyCode::Char('R') => return self.start_record(&id, true, false),
            KeyCode::Char('a') => return self.start_analyze(&id, None, true),
            KeyCode::Char('S') => {
                self.tab = Tab::Submit;
                self.focus = Focus::Main;
                self.confirm = Some(Confirm::Submit(id));
                return;
            }
            KeyCode::Char('e') => return self.edit_current(&id),
            _ => {}
        }

        if self.focus == Focus::Rail {
            match code {
                KeyCode::Down | KeyCode::Char('j') => self.move_rail(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_rail(-1),
                KeyCode::PageDown => self.move_rail(10),
                KeyCode::PageUp => self.move_rail(-10),
                KeyCode::Enter => self.focus = Focus::Main,
                _ => {}
            }
            return;
        }
        self.main_key(k, &id);
    }

    fn main_key(&mut self, k: KeyEvent, id: &str) {
        let code = k.code;
        let recording_here = self.recording_id() == Some(id);
        if self.tab == Tab::Record && recording_here {
            match code {
                KeyCode::Char(' ') => {
                    if let Some(r) = self.rec.as_mut() {
                        r.active.toggle_pause();
                    }
                }
                KeyCode::Enter => self.stop_record(),
                KeyCode::Char('d') => {
                    if let Some(name) = self.rec.as_mut().and_then(|r| r.active.switch_device()) {
                        self.toast(ToastKind::Info, format!("Input → {name}"));
                    }
                }
                _ => {}
            }
            return;
        }
        match self.tab {
            Tab::Question | Tab::Analysis => match code {
                KeyCode::Down | KeyCode::Char('j') => self.scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll(-1),
                KeyCode::PageDown => self.scroll(10),
                KeyCode::PageUp => self.scroll(-10),
                KeyCode::Home | KeyCode::Char('g') => self.scroll(isize::MIN / 2),
                KeyCode::End | KeyCode::Char('G') => self.scroll(isize::MAX / 2),
                _ => {}
            },
            Tab::Tests => {
                let n = self.session(id).map_or(0, |s| s.test.rows.len());
                match code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        let s = self.session_mut(id);
                        s.case_sel = (s.case_sel + 1).min(n.saturating_sub(1));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let s = self.session_mut(id);
                        s.case_sel = s.case_sel.saturating_sub(1);
                    }
                    KeyCode::Enter => self.rerun_case(id),
                    KeyCode::Char('d') => self.start_test(
                        id,
                        TestArgs {
                            debug: true,
                            ..plain_test()
                        },
                    ),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn rate(&mut self, id: &str, rating: u8) {
        let rated = self.ws.find(id).and_then(|p| p.rated_today(self.ws.today));
        if rated.is_some() && self.tab != Tab::History {
            return self.toast(ToastKind::Info, "Already rated today · History changes it");
        }
        self.save_rating(id, rating, false);
    }

    fn rerun_case(&mut self, id: &str) {
        let Some(s) = self.session(id) else { return };
        let Some(row) = s.test.rows.get(s.case_sel) else {
            return;
        };
        let n = row.n;
        let now = self.now();
        let s = self.session_mut(id);
        s.test.state = TestState::Building;
        s.test.started = now;
        s.test.partial = true;
        super::jobs::test(
            self.tx.clone(),
            id.to_string(),
            TestArgs {
                case: Some(n),
                ..plain_test()
            },
        );
    }

    fn edit_current(&mut self, id: &str) {
        let Some(folder) = self.folder_of(id) else {
            return self.toast(ToastKind::Err, format!("{id} has no folder yet"));
        };
        let file = match self.tab {
            Tab::Analysis => "analysis.md".to_string(),
            Tab::Question => "question.md".to_string(),
            _ => self
                .session(id)
                .and_then(|s| s.disk.as_ref())
                .and_then(|d| d.solution_file.clone())
                .unwrap_or_else(|| "solution.cpp".to_string()),
        };
        self.edit(folder.join(file));
    }

    fn step_tab(&mut self, delta: isize) {
        self.tab = self.tab.step(delta);
        self.focus = Focus::Main;
    }

    fn escape(&mut self, id: &str) {
        if self.focus == Focus::Main {
            if self.tab == Tab::Record && self.recording_id() == Some(id) {
                self.confirm = Some(Confirm::DiscardRecording);
            } else {
                self.focus = Focus::Rail;
            }
        } else if self.mode == Mode::Due {
            self.mode = Mode::All;
        } else {
            self.sel = None;
        }
    }

    fn move_rail(&mut self, delta: isize) {
        let ids: Vec<String> = self.visible().iter().map(|p| p.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let at = self
            .sel
            .as_ref()
            .and_then(|s| ids.iter().position(|i| i == s))
            .unwrap_or(0) as isize;
        let next = (at + delta).clamp(0, ids.len() as isize - 1) as usize;
        self.select(&ids[next].clone());
    }

    fn scroll(&mut self, delta: isize) {
        let Some(id) = self.sel.clone() else { return };
        let max = self.max_scroll.get() as isize;
        let tab = self.tab;
        let s = self.session_mut(&id);
        let field = if tab == Tab::Analysis {
            &mut s.scroll_analysis
        } else {
            &mut s.scroll_question
        };
        *field = (*field as isize).saturating_add(delta).clamp(0, max) as usize;
    }

    pub fn on_mouse(&mut self, m: MouseEvent, size: Rect) {
        if self.overlay_open() {
            return;
        }
        let side_w = Geo::new(size).side_w;
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self
                    .hits
                    .iter()
                    .rev()
                    .find(|h| h.rect.contains((m.column, m.row).into()))
                    .map(|h| h.action.clone());
                if let Some(a) = hit {
                    self.do_action(a);
                }
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let delta: isize = if m.kind == MouseEventKind::ScrollDown {
                    1
                } else {
                    -1
                };
                if m.column < side_w {
                    if self.mode == Mode::Leet {
                        let max = self.leet.filtered.len().saturating_sub(1) as isize;
                        self.leet.idx = (self.leet.idx as isize + delta).clamp(0, max) as usize;
                    } else {
                        self.move_rail(delta);
                    }
                } else if matches!(self.tab, Tab::Question | Tab::Analysis) {
                    self.scroll(delta * 3);
                }
            }
            _ => {}
        }
    }

    fn do_action(&mut self, a: Action) {
        match a {
            Action::SelectProblem(id) => {
                self.select(&id);
                self.focus = Focus::Rail;
            }
            Action::OpenDue(id) => {
                self.select(&id);
                self.tab = Tab::History;
                self.focus = Focus::Main;
            }
            Action::SelectLeet(i) => {
                self.leet.idx = i;
                self.focus = Focus::Rail;
            }
            Action::SetMode(Mode::Leet) => self.open_leet(false),
            Action::SetMode(Mode::Due) => {
                self.mode = Mode::Due;
                self.focus = Focus::Rail;
                if self.problem().is_none_or(|p| !p.is_due())
                    && let Some(first) = self.visible().first().map(|p| p.id.clone())
                {
                    self.select(&first);
                }
            }
            Action::SetMode(Mode::All) => {
                self.mode = Mode::All;
                self.focus = Focus::Rail;
            }
            Action::SetTab(t) => {
                self.tab = t;
                self.focus = Focus::Main;
            }
            Action::Rate(r) => {
                if let Some(id) = self.sel.clone() {
                    self.rate(&id, r);
                }
            }
            Action::SelectCase(i) => {
                if let Some(id) = self.sel.clone() {
                    self.session_mut(&id).case_sel = i;
                    self.focus = Focus::Main;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(text: &str, sel: Option<&str>) -> Route {
        match parse_command(text, sel) {
            Ok(Parsed::Route(r)) => r,
            _ => panic!("not a route: {text}"),
        }
    }

    #[test]
    fn parses_record_with_flags_and_default_problem() {
        match route("record -r", Some("54")) {
            Route::Record { id, resume, force } => {
                assert_eq!(id, "54");
                assert!(resume && !force);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_test_case_and_debug() {
        match route("test 54 -c 3 -d", None) {
            Route::Test {
                id, case, debug, ..
            } => {
                assert_eq!((id.as_str(), case, debug), ("54", Some(3), true));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_log_and_pull() {
        assert!(matches!(
            route("log 54 4", None),
            Route::Log { rating: 4, .. }
        ));
        assert!(matches!(route(":rak pull", None), Route::Pull { .. }));
        assert!(matches!(route("pull today", None), Route::PullSlug { .. }));
        assert!(parse_command("log 54 9", None).is_err());
        assert!(parse_command("bogus", None).is_err());
        assert!(parse_command("record", None).is_err());
    }
}

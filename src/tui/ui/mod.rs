//! The shell every screen shares: a sidebar with the problem list, a title
//! line and tabs, the content, a command bar, and a footer.

mod pull;
mod tabs;
mod today;

use ratatui::Frame;
use ratatui::style::Color;

use super::input::palette_items;
use super::paint::*;
use super::state::*;
use super::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App, th: Theme) {
    let area = frame.area();
    let g = Geo::new(area);
    let t = app.now();
    let mut hits = Vec::new();
    {
        let mut cv = Cv {
            buf: frame.buffer_mut(),
            th,
            app,
            g,
            t,
            hits: &mut hits,
        };
        cv.render();
    }
    app.hits = hits;
}

pub fn due_color(th: &Theme, due: Option<i64>) -> Color {
    match due {
        None => th.info,
        Some(d) if d > 0 => th.err,
        Some(0) => th.warn,
        Some(_) => th.dim,
    }
}

pub fn due_label(due: Option<i64>) -> String {
    match due {
        None => "new".to_string(),
        Some(d) if d > 0 => format!("{d}d late"),
        Some(0) => "today".to_string(),
        Some(d) => format!("in {}d", -d),
    }
}

pub fn diff_color(th: &Theme, difficulty: &str) -> Color {
    match difficulty {
        "Easy" => th.ok,
        "Medium" => th.warn,
        "Hard" => th.err,
        _ => th.dim,
    }
}

impl Cv<'_> {
    fn render(&mut self) {
        let g = self.g;
        let th = self.th;
        self.fill(0, 0, g.w, g.h, th.bg);
        if g.w < MIN_W || g.h < MIN_H {
            let msg = format!(
                "Terminal too small ({}×{}); rak needs {MIN_W}×{MIN_H}",
                g.w, g.h
            );
            self.put(2, 1, &msg, st(th.dim));
            return;
        }
        self.fill(0, 0, g.side_w, g.h, th.panel);
        self.sidebar();
        self.header();
        if self.app.mode == Mode::Leet {
            self.leet_preview();
        } else if self.app.sel.is_none() {
            self.today();
        } else {
            self.tab_content();
        }
        self.confirm_block();
        self.command_bar();
        self.footer();
        self.toast();
        match &self.app.overlay {
            Overlay::Palette { .. } => self.suggestions(),
            Overlay::Help => self.help(),
            Overlay::None => {}
        }
    }

    /// Whether row `y` is still above the command bar.
    pub fn fits(&self, y: u16) -> bool {
        y + 1 < self.g.bar_y
    }

    // ---- sidebar ------------------------------------------------------

    fn sidebar(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let focus = app.focus == Focus::Rail && !app.overlay_open();
        self.put(2, 1, "rak", bold(th.fg));
        if !g.narrow() {
            let v = format!("v{}", env!("CARGO_PKG_VERSION"));
            self.put(g.side_w - 2 - v.len() as u16, 1, &v, st(th.faint));

            let due = app.ws.due_count();
            let mut x = 2;
            for (mode, label) in [
                (Mode::All, "All".to_string()),
                (Mode::Due, format!("Due {due}")),
                (Mode::Leet, "LeetCode".to_string()),
            ] {
                let text = format!(" {label} ");
                let w = width(&text) as u16;
                self.hit(x, 3, w, Action::SetMode(mode));
                let style = if app.mode == mode {
                    if focus {
                        on(th.ink, th.acc).add_modifier(ratatui::style::Modifier::BOLD)
                    } else {
                        bold(th.fg).bg(th.raised)
                    }
                } else {
                    st(th.dim)
                };
                x = self.put(x, 3, &text, style) + 1;
            }
            if app.mode == Mode::Leet {
                self.fill(1, 5, g.side_w - 2, 1, th.raised);
                self.vbar(1, 5, 1, if focus { th.acc } else { th.faint });
                if app.leet.query.is_empty() {
                    self.put(3, 5, "Search 3,000+ problems", st(th.faint));
                } else {
                    let e = self.put(3, 5, &trunc(&app.leet.query, 22), st(th.fg));
                    if focus && blink(self.t) {
                        self.put(e, 5, " ", on(th.bg, th.fg));
                    }
                }
                if focus && app.leet.query.is_empty() && blink(self.t) {
                    self.put(2, 5, " ", on(th.bg, th.fg));
                }
            } else {
                let n = app.visible().len();
                let label = format!("{n} {}", if n == 1 { "problem" } else { "problems" });
                self.put(2, 5, &label, st(th.faint));
            }
        }

        let first = 7u16;
        let last = g.h.saturating_sub(5);
        let rows = last.saturating_sub(first) as usize + 1;
        if app.mode == Mode::Leet {
            self.leet_rows(first, rows, focus);
        } else {
            self.problem_rows(first, rows, focus);
        }

        if !g.narrow() {
            let dir = app
                .leetcode_dir
                .parent()
                .and_then(|_| app.leetcode_dir.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("leetcode");
            self.put(2, g.h - 3, &trunc(&format!("~/{dir}"), 24), st(th.dim));
            let due = app.ws.due_count();
            self.put(2, g.h - 2, "●", st(if due > 0 { th.warn } else { th.ok }));
            let status = format!("{due} due · {} tracked", app.ws.problems.len());
            self.put(4, g.h - 2, &trunc(&status, 24), st(th.dim));
        } else {
            let due = app.ws.due_count();
            self.put(2, g.h - 2, "●", st(if due > 0 { th.warn } else { th.ok }));
            self.put(4, g.h - 2, &due.to_string(), st(th.dim));
        }
    }

    fn problem_rows(&mut self, first: u16, rows: usize, focus: bool) {
        let (g, th, app) = (self.g, self.th, self.app);
        let items = app.visible();
        if items.is_empty() {
            let msg = if app.mode == Mode::Due {
                "Nothing due"
            } else {
                "No problems yet"
            };
            self.put(2, first, msg, st(th.dim));
            if !g.narrow() && app.mode == Mode::All {
                self.put(2, first + 1, "p pulls one", st(th.faint));
            }
            return;
        }
        let at = app
            .sel
            .as_ref()
            .and_then(|s| items.iter().position(|p| &p.id == s))
            .unwrap_or(0);
        let off = at
            .saturating_sub(rows / 2)
            .min(items.len().saturating_sub(rows));
        for (i, p) in items.iter().skip(off).take(rows).enumerate() {
            let y = first + i as u16;
            let selected = app.sel.as_deref() == Some(p.id.as_str());
            if selected {
                self.fill(0, y, g.side_w, 1, th.raised);
                self.vbar(0, y, 1, if focus { th.acc } else { th.faint });
            }
            if p.is_new() {
                self.put(2, y, "+", st(th.info));
            } else if p.is_due() {
                self.put(2, y, "●", st(due_color(&th, p.due)));
            }
            let id_style = if selected { bold(th.fg) } else { st(th.dim) };
            self.put(4, y, &pad(&p.id, 5), id_style);
            if !g.narrow() {
                let ts = if selected { bold(th.fg) } else { st(th.fg) };
                self.put(10, y, &pad(&p.title, (g.side_w - 11) as usize), ts);
            }
            self.hit(0, y, g.side_w, Action::SelectProblem(p.id.clone()));
        }
    }

    fn leet_rows(&mut self, first: u16, rows: usize, focus: bool) {
        let (g, th, app) = (self.g, self.th, self.app);
        let leet = &app.leet;
        let LeetList::Ready(list) = &leet.list else {
            let msg = match &leet.list {
                LeetList::Failed(_) => "Could not load",
                _ => "Loading…",
            };
            if !g.narrow() {
                self.put(2, first, msg, st(th.dim));
            }
            return;
        };
        if leet.filtered.is_empty() {
            if !g.narrow() {
                self.put(2, first, "No problems match", st(th.dim));
            }
            return;
        }
        let off = leet
            .idx
            .saturating_sub(rows / 2)
            .min(leet.filtered.len().saturating_sub(rows));
        for (i, &li) in leet.filtered.iter().skip(off).take(rows).enumerate() {
            let p = &list[li];
            let y = first + i as u16;
            let idx = off + i;
            let selected = idx == leet.idx;
            if selected {
                self.fill(0, y, g.side_w, 1, th.raised);
                self.vbar(0, y, 1, if focus { th.acc } else { th.faint });
            }
            let local = app
                .ws
                .find(&p.frontend_id)
                .is_some_and(|l| l.folder.is_some());
            if local {
                self.put(2, y, "✓", st(th.ok));
            }
            self.put(
                4,
                y,
                &pad(&p.frontend_id, 5),
                if selected { bold(th.fg) } else { st(th.dim) },
            );
            if !g.narrow() {
                let ts = if selected { bold(th.fg) } else { st(th.fg) };
                self.put(10, y, &pad(&p.title, (g.side_w - 11) as usize), ts);
            }
            self.hit(0, y, g.side_w, Action::SelectLeet(idx));
        }
    }

    // ---- title line and tabs --------------------------------------

    fn header(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let main_focus = app.focus == Focus::Main && !app.overlay_open();
        let right = |cv: &mut Cv, segs: Vec<(String, Color)>| {
            let total: usize = segs.iter().map(|(t, _)| width(t)).sum::<usize>()
                + 3 * segs.len().saturating_sub(1);
            let mut x = g.cr.saturating_sub(total as u16);
            for (i, (t, c)) in segs.iter().enumerate() {
                if i > 0 {
                    x = cv.put(x, 1, " · ", st(cv.th.faint));
                }
                x = cv.put(x, 1, t, st(*c));
            }
        };
        let rule = |cv: &mut Cv| {
            let w = g.cr - g.bx;
            cv.put(g.bx, 4, &"─".repeat(w as usize), st(cv.th.line));
        };

        if app.mode == Mode::Leet {
            match app.leet.current() {
                Some(p) => {
                    let x = self.put(g.cx, 1, &p.frontend_id, st(th.dim));
                    self.put(x + 2, 1, &trunc(&p.title, 38), bold(th.fg));
                    let mut segs = vec![(p.difficulty.clone(), diff_color(&th, &p.difficulty))];
                    if p.status.as_deref() == Some("ac") {
                        segs.push(("solved on LeetCode".to_string(), th.ok));
                    }
                    right(self, segs);
                }
                None => {
                    self.put(g.cx, 1, "Pull", bold(th.fg));
                }
            }
            rule(self);
            self.put(
                g.cx,
                3,
                "Preview",
                if main_focus {
                    bold(th.acc)
                } else {
                    bold(th.fg)
                },
            );
            self.put(
                g.cx,
                4,
                "━━━━━━━",
                st(if main_focus { th.acc } else { th.dim }),
            );
            return;
        }

        let Some(p) = app.problem() else {
            self.put(g.cx, 1, "Today", bold(th.fg));
            let day = chrono::Local::now().format("%A, %b %-d").to_string();
            right(self, vec![(day, th.dim)]);
            let tracked = app.ws.problems.len();
            let line = format!(
                "{} due · {tracked} tracked · {} mastered",
                app.ws.due_count(),
                app.ws.mastered_count()
            );
            self.put(g.cx, 3, &line, st(th.dim));
            rule(self);
            return;
        };

        let x = self.put(g.cx, 1, &p.id, st(th.dim));
        self.put(x + 2, 1, &trunc(&p.title, 36), bold(th.fg));
        let mut segs: Vec<(String, Color)> = Vec::new();
        if !p.difficulty.is_empty() {
            segs.push((p.difficulty.clone(), diff_color(&th, &p.difficulty)));
        }
        if p.is_new() {
            segs.push(("not logged yet".to_string(), th.info));
        } else {
            segs.push((format!("box {}", p.box_num), th.dim));
            segs.push((
                match p.due {
                    Some(d) if d >= 0 => format!("due {}", due_label(p.due)),
                    Some(d) => format!("next in {}d", -d),
                    None => String::new(),
                },
                due_color(&th, p.due),
            ));
        }
        right(self, segs);

        rule(self);
        let mut x = g.cx;
        for tab in TABS {
            let on_tab = tab == app.tab;
            let label = tab.label();
            let dot = app.tab_busy(tab);
            let w = width(label) as u16 + if dot.is_some() { 2 } else { 0 };
            self.hit(x, 3, w, Action::SetTab(tab));
            let style = if on_tab {
                bold(if main_focus { th.acc } else { th.fg })
            } else {
                st(th.dim)
            };
            let mut end = self.put(x, 3, label, style);
            if on_tab {
                self.put(
                    x,
                    4,
                    &"━".repeat(width(label)),
                    st(if main_focus { th.acc } else { th.dim }),
                );
            }
            if let Some(recording) = dot {
                end = self.put(end + 1, 3, "●", st(if recording { th.err } else { th.acc }));
            }
            x = end + 3;
        }
    }

    // ---- command bar and footer -----------------------------------

    fn command_bar(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let w = g.cr - g.bx;
        let open = matches!(app.overlay, Overlay::Palette { .. });
        self.fill(g.bx, g.bar_y, w, 3, th.raised);
        self.vbar(g.bx, g.bar_y, 3, if open { th.acc } else { th.vio });
        if let Overlay::Palette { input, .. } = &app.overlay {
            let e = self.put(g.cx, g.bar_y, &trunc(input, w as usize - 6), st(th.fg));
            if input.is_empty() {
                self.put(
                    g.cx + 1,
                    g.bar_y,
                    "record 54, test, next, pull…",
                    st(th.faint),
                );
            }
            if blink(self.t) {
                self.put(e, g.bar_y, " ", on(th.bg, th.fg));
            }
        } else {
            self.put(g.cx, g.bar_y, "Press : to type a command", st(th.faint));
        }

        let info = self.info_line();
        let mut x = g.cx;
        for (i, text) in info.iter().enumerate() {
            if i > 0 {
                x = self.put(x, g.bar_y + 2, " · ", st(th.faint));
            }
            let style = match i {
                0 => st(th.acc),
                1 => st(th.fg),
                _ => st(th.dim),
            };
            x = self.put(
                x,
                g.bar_y + 2,
                &trunc(text, g.cr.saturating_sub(x + 1) as usize),
                style,
            );
        }
    }

    /// The line under the prompt: the current tab, then the details that matter for it.
    fn info_line(&self) -> Vec<String> {
        let app = self.app;
        let item = |t: &str| t.to_string();
        if app.mode == Mode::Leet {
            return vec![
                item("Pull"),
                item("LeetCode"),
                item("daily list cached 7 days"),
            ];
        }
        let Some(p) = app.problem() else {
            return vec![
                item("Today"),
                item("rak"),
                item(&format!(
                    "{} / {}",
                    app.transcribe_provider, app.analyze_provider
                )),
            ];
        };
        let folder = p
            .folder
            .as_ref()
            .and_then(|f| f.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("no folder")
            .to_string();
        match app.tab {
            Tab::Question => vec![item("Question"), item(&format!("{folder}/")), item("cpp")],
            Tab::Tests => vec![
                item("Tests"),
                item("g++ -O2 -std=c++20"),
                item("limit from rak.toml"),
            ],
            Tab::Record => {
                let dev = app
                    .rec
                    .as_ref()
                    .map(|r| r.active.device_name.clone())
                    .unwrap_or_else(|| "default input".to_string());
                vec![
                    item("Record"),
                    item(&dev),
                    item(&format!(
                        "{} → {}",
                        app.transcribe_provider, app.analyze_provider
                    )),
                ]
            }
            Tab::Analysis => vec![
                item("Analysis"),
                item(&app.analyze_provider),
                item("analysis.md"),
            ],
            Tab::Submit => vec![item("Submit"), item("leetcode.com"), item("solution file")],
            Tab::History => vec![
                item("History"),
                item(&format!("box {}", p.box_num)),
                item(&format!("every {} days", super::data::interval(p.box_num))),
            ],
        }
    }

    fn footer(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let y = g.foot_y;
        let mut x = g.bx + 1;
        if let Some(r) = &app.rec {
            if r.active.paused {
                x = self.put(x, y, "❚❚", st(th.warn));
                x = self.put(
                    x + 1,
                    y,
                    &format!("paused {}", clock(r.active.elapsed().as_secs())),
                    st(th.warn),
                );
            } else {
                x = self.put(x, y, if blink(self.t) { "●" } else { "○" }, st(th.err));
                x = self.put(
                    x + 1,
                    y,
                    &format!("REC {}", clock(r.active.elapsed().as_secs())),
                    st(th.fg),
                );
            }
            x += 3;
        }
        let jobs = app.jobs();
        if !jobs.is_empty() {
            x = self.squares(x, y);
            let text = jobs.iter().take(2).cloned().collect::<Vec<_>>().join(" · ");
            x = self.put(x + 1, y, &text, st(th.dim));
            x += 3;
        }

        let mut items = self.key_hints();
        if !matches!(app.overlay, Overlay::Palette { .. }) {
            items.push((":", "commands"));
        }
        let w = Cv::keys_width(&items);
        if g.cr.saturating_sub(w) > x {
            self.keys(g.cr - w, y, &items);
        }
    }

    fn key_hints(&self) -> Vec<(&'static str, &'static str)> {
        let app = self.app;
        if matches!(app.overlay, Overlay::Palette { .. }) {
            return vec![
                ("↑↓", "choose"),
                ("tab", "complete"),
                ("↵", "run"),
                ("esc", "close"),
            ];
        }
        if matches!(app.overlay, Overlay::Help) {
            return vec![("esc", "close")];
        }
        if let Some(c) = &app.confirm {
            return match c {
                Confirm::Submit(_) => vec![("↵", "submit"), ("esc", "cancel")],
                Confirm::Rescaffold { .. } => vec![("y", "re-scaffold"), ("n", "cancel")],
                Confirm::DiscardRecording | Confirm::QuitRecording => {
                    vec![("y", "discard"), ("n", "keep")]
                }
                Confirm::ReplaceRating { .. } => vec![("y", "replace"), ("n", "keep")],
            };
        }
        if app.mode == Mode::Leet {
            return if app.focus == Focus::Rail {
                vec![
                    ("↑↓", "move"),
                    ("↵", "pull"),
                    ("^f", "re-scaffold"),
                    ("esc", "back"),
                ]
            } else {
                vec![("tab", "to list"), ("esc", "back")]
            };
        }
        if app.sel.is_none() {
            return vec![("↑↓", "pick"), ("n", "next due"), ("p", "pull")];
        }
        if app.focus == Focus::Rail {
            return vec![
                ("↑↓", "move"),
                ("↵", "open"),
                ("n", "next due"),
                ("p", "pull"),
            ];
        }
        if app.rating_visible() {
            return vec![
                ("1-5", "rate"),
                ("←→", "choose"),
                ("↵", "save"),
                ("s", "skip"),
            ];
        }
        let recording_here = app.recording_id() == app.sel.as_deref();
        match app.tab {
            Tab::Record if recording_here => {
                let pause = if app.rec.as_ref().is_some_and(|r| r.active.paused) {
                    "resume"
                } else {
                    "pause"
                };
                vec![
                    ("space", pause),
                    ("d", "device"),
                    ("↵", "stop"),
                    ("esc", "discard"),
                ]
            }
            Tab::Record => vec![("r", "record"), ("R", "append"), ("[ ]", "tab")],
            Tab::Tests => vec![
                ("↑↓", "case"),
                ("↵", "rerun"),
                ("d", "debug"),
                ("t", "run all"),
            ],
            Tab::Analysis => vec![("↑↓", "scroll"), ("a", "re-run"), ("e", "open")],
            Tab::Submit => vec![("S", "submit"), ("t", "test first"), ("[ ]", "tab")],
            Tab::History => vec![("n", "next due"), ("r", "record"), ("[ ]", "tab")],
            Tab::Question => vec![
                ("↑↓", "scroll"),
                ("t", "test"),
                ("r", "record"),
                ("e", "edit"),
            ],
        }
    }

    fn toast(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let Some(t) = &app.toast else { return };
        let text = trunc(&t.text, (g.w as usize).saturating_sub(12).min(70));
        let w = width(&text) as u16 + 5;
        let x = g.w.saturating_sub(w + 2);
        let color = match t.kind {
            ToastKind::Ok => th.ok,
            ToastKind::Err => th.err,
            ToastKind::Warn => th.warn,
            ToastKind::Info => th.faint,
        };
        self.fill(x, 0, w, 3, th.elev);
        self.vbar(x, 0, 3, color);
        self.put(x + 2, 1, &text, st(th.fg));
    }

    // ---- confirmations and overlays -----------------------------

    fn confirm_block(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let Some(c) = &app.confirm else { return };
        let (bar, question, keys): (Color, String, Vec<(&str, &str)>) = match c {
            Confirm::Submit(id) => (
                th.acc,
                format!("Submit {id}'s solution to LeetCode?"),
                vec![("↵", "submit"), ("esc", "cancel")],
            ),
            Confirm::Rescaffold { folder, .. } => (
                th.warn,
                format!("Re-scaffold {folder}/? solution.cpp is overwritten."),
                vec![("y", "re-scaffold"), ("n", "keep my code")],
            ),
            Confirm::DiscardRecording => (
                th.err,
                "Discard this recording? The audio is deleted.".to_string(),
                vec![("y", "discard"), ("n", "keep recording")],
            ),
            Confirm::QuitRecording => (
                th.err,
                "A recording is running. Quit and discard it?".to_string(),
                vec![("y", "quit and discard"), ("n", "keep recording")],
            ),
            Confirm::ReplaceRating { rating, .. } => (
                th.warn,
                format!("Today is already rated. Replace it with {rating}?"),
                vec![("y", "replace"), ("n", "keep")],
            ),
        };
        let y = g.bar_y.saturating_sub(5);
        self.block(y, 4, bar);
        self.put(g.cx, y + 1, &trunc(&question, g.text_w()), bold(th.fg));
        self.keys(g.cx, y + 2, &keys);
    }

    fn suggestions(&mut self) {
        let (g, th, app) = (self.g, self.th, self.app);
        let Overlay::Palette { input, idx } = &app.overlay else {
            return;
        };
        let items = palette_items(app, input);
        let n = items.len().clamp(1, 8) as u16;
        let y = g.bar_y.saturating_sub(n + 2);
        let w = g.cr - g.bx;
        self.fill(g.bx, y, w, n + 2, th.elev);
        if items.is_empty() {
            self.put(g.cx, y + 1, "No command matches", st(th.dim));
            return;
        }
        for (i, it) in items.iter().take(8).enumerate() {
            let row = y + 1 + i as u16;
            let sel = i == *idx;
            if sel {
                self.fill(g.bx, row, w, 1, th.acc);
            }
            let (c1, c2, c3) = if sel {
                (th.ink, th.ink, th.ink)
            } else {
                (th.fg, th.dim, th.faint)
            };
            let style1 = if sel { bold(c1) } else { st(c1) };
            self.put(g.cx, row, &pad(&it.cmd, 18), style1);
            self.put(
                g.cx + 19,
                row,
                &trunc(it.desc, w.saturating_sub(34) as usize),
                st(c2),
            );
            self.put(
                g.cr.saturating_sub(width(it.key) as u16 + 1),
                row,
                it.key,
                st(c3),
            );
        }
    }

    fn help(&mut self) {
        let (g, th) = (self.g, self.th);
        let w = 58u16.min(g.w - 4);
        let h = 22u16.min(g.h - 2);
        let x0 = (g.w - w) / 2;
        let y0 = (g.h - h) / 2;
        self.fill(x0, y0, w, h, th.elev);
        self.put(x0 + 3, y0 + 1, "Keys", bold(th.fg));
        self.put(x0 + w - 6, y0 + 1, "esc", st(th.dim));
        let groups: [(&str, &[(&str, &str)]); 4] = [
            (
                "Anywhere",
                &[
                    (":", "Command palette"),
                    ("?", "Help"),
                    ("esc", "Back"),
                    ("tab", "Switch pane"),
                    ("q", "Quit"),
                ],
            ),
            (
                "Move",
                &[
                    ("j k", "Up / down"),
                    ("[ ]", "Previous / next tab"),
                    ("↵", "Open"),
                    ("n", "Next due"),
                ],
            ),
            (
                "On the problem",
                &[
                    ("p", "Pull from LeetCode"),
                    ("t", "Test"),
                    ("r R", "Record / append"),
                    ("a", "Analyze"),
                    ("S", "Submit"),
                    ("1-5", "Rate"),
                ],
            ),
            (
                "In a tab",
                &[
                    ("space", "Pause recording"),
                    ("d", "Device / debug build"),
                    ("↵", "Stop / rerun / save"),
                    ("e", "Open in $EDITOR"),
                ],
            ),
        ];
        let mut y = y0 + 3;
        for (title, rows) in groups {
            if y + rows.len() as u16 + 1 >= y0 + h {
                break;
            }
            self.put(x0 + 3, y, title, bold(th.vio));
            y += 1;
            for (k, l) in rows.iter() {
                self.put(x0 + 3, y, l, st(th.fg));
                self.put(x0 + w - 3 - width(k) as u16, y, k, st(th.dim));
                y += 1;
            }
            y += 1;
        }
        self.put(
            x0 + 3,
            y0 + h - 2,
            "S and R act outside or overwrite: they ask first.",
            st(th.faint),
        );
    }

    /// Draw a rating block: chips, the chosen rating's meaning, and where it leads.
    pub fn rating_block(&mut self, y: u16, title: &str) {
        let (g, th, app) = (self.g, self.th, self.app);
        let Some(p) = app.problem() else { return };
        let hi = app.session(&p.id).map_or(4, |s| s.rate_hi);
        let focus = app.focus == Focus::Main;
        self.block(y, 6, if focus { th.acc } else { th.faint });
        self.put(g.cx, y + 1, title, bold(th.fg));
        let mut x = g.cx;
        for r in 1..=5u8 {
            let label = format!(" {r} ");
            self.hit(x, y + 2, 3, Action::Rate(r));
            let style = if r == hi {
                bold(th.ink).bg(th.acc)
            } else {
                on(th.fg, th.elev)
            };
            x = self.put(x, y + 2, &label, style) + 1;
        }
        let meaning = match hi {
            5 => "Solved perfectly, no issues",
            4 => "Solved with minor hesitation",
            3 => "Solved, but looked up syntax",
            2 => "Struggled, needed major help",
            _ => "Couldn't solve without AI or the web",
        };
        self.put(g.cx, y + 3, meaning, st(th.fg));
        let (nb, _) = super::data::outcome(p.box_num, p.streak, hi, p.is_new());
        let lands = if p.is_new() {
            format!("box {nb}")
        } else if nb == p.box_num && nb != 1 {
            format!("stays in box {nb}")
        } else if nb == 1 && hi <= 2 {
            "back to box 1".to_string()
        } else {
            format!("box {nb}")
        };
        let when = (app.ws.today + chrono::Duration::days(super::data::interval(nb)))
            .format("%a %b %-d")
            .to_string();
        self.put(
            g.cx,
            y + 4,
            &format!("→ {lands} · next review {when}"),
            st(th.dim),
        );
    }
}

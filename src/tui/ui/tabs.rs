//! The six tabs of a problem: Question, Tests, Record, Analysis, Submit, History.

use std::time::Duration;

use ratatui::style::Color;

use super::pull::strip_title;
use super::{diff_color, due_label};
use crate::judge::verdict::Verdict;
use crate::tui::data::{self, Problem};
use crate::tui::paint::*;
use crate::tui::state::*;

fn fmt_time(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

fn verdict_label(v: &Verdict) -> &'static str {
    match v {
        Verdict::Accepted => "Accepted",
        Verdict::NoExpected { .. } => "No expected",
        Verdict::WrongAnswer { .. } => "Wrong Answer",
        Verdict::RuntimeError { .. } => "Runtime Error",
        Verdict::InputError { .. } => "Invalid Input",
        Verdict::TimeLimitExceeded => "Time Limit",
    }
}

/// A piece of the case detail block.
enum Part {
    Field(&'static str, String, Color, usize),
    Diff(String, String),
    Note(&'static str, Color),
}

/// One line of the attempts list: number, date, length, live marker, status.
type AttemptRow<'a> = (usize, String, String, String, Option<(&'a str, Color)>);

impl Cv<'_> {
    pub(super) fn tab_content(&mut self) {
        let Some(p) = self.app.problem() else { return };
        if p.folder.is_none() && self.app.tab != Tab::History {
            self.empty(
                self.g.top,
                "This problem has no folder in the workspace.",
                "p",
                "pulls it from LeetCode",
            );
            return;
        }
        match self.app.tab {
            Tab::Question => self.tab_question(p),
            Tab::Tests => self.tab_tests(p),
            Tab::Record => self.tab_record(p),
            Tab::Analysis => self.tab_analysis(p),
            Tab::Submit => self.tab_submit(p),
            Tab::History => self.tab_history(p),
        }
    }

    /// A first line, then a key and what it does.
    fn empty(&mut self, y: u16, line1: &str, key: &str, line2: &str) {
        let (g, th) = (self.g, self.th);
        self.put(g.cx, y, &trunc(line1, g.text_w()), st(th.fg));
        let x = self.put(g.cx, y + 1, key, bold(th.fg));
        self.put(
            x + 1,
            y + 1,
            &trunc(line2, g.text_w().saturating_sub(width(key) + 1)),
            st(th.dim),
        );
    }

    fn scrollbar(&mut self, y: u16, rows: u16, total: usize, skip: usize) {
        let (g, th) = (self.g, self.th);
        if total <= rows as usize || rows == 0 {
            return;
        }
        let x = g.cr + 1;
        let thumb = ((rows as usize * rows as usize) / total).max(1) as u16;
        let max_skip = total - rows as usize;
        let pos = (skip.min(max_skip) * (rows - thumb) as usize / max_skip.max(1)) as u16;
        for i in 0..rows {
            let on_thumb = i >= pos && i < pos + thumb;
            let (ch, color) = if on_thumb {
                ("┃", th.dim)
            } else {
                ("│", th.line)
            };
            self.put(x, y + i, ch, st(color));
        }
    }

    // ---- Question ---------------------------------------------------

    fn tab_question(&mut self, p: &Problem) {
        let (g, th, app) = (self.g, self.th, self.app);
        let s = app.session(&p.id);
        let disk = s.and_then(|s| s.disk.as_ref());
        let Some(question) = disk.and_then(|d| d.question.as_deref()) else {
            self.empty(
                g.top,
                "There is no question.md in this folder.",
                "e",
                "opens the folder's files in your editor",
            );
            return;
        };
        let rows = g.content_h().saturating_sub(2);
        let scroll = s.map_or(0, |s| s.scroll_question);
        let total = self.markdown(g.top, rows, &strip_title(question), scroll);
        app.max_scroll.set(total.saturating_sub(rows));
        self.scrollbar(g.top, rows as u16, total, scroll);

        let folder = p
            .folder
            .as_ref()
            .and_then(|f| f.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let cases = disk.map_or(0, |d| d.case_count);
        let sol = disk
            .and_then(|d| d.solution_file.clone())
            .unwrap_or_else(|| "solution.cpp".into());
        let line = format!("{folder}/ · {sol} · {cases} test cases");
        self.put(g.cx, g.bar_y - 2, &trunc(&line, g.text_w()), st(th.faint));
    }

    // ---- Tests ------------------------------------------------------

    fn tab_tests(&mut self, p: &Problem) {
        let (g, th, app) = (self.g, self.th, self.app);
        let s = app.session(&p.id);
        let run = s.map(|s| &s.test);
        let cases = s.and_then(|s| s.disk.as_ref()).map_or(0, |d| d.case_count);
        let Some(run) = run.filter(|r| r.state != TestState::Idle) else {
            self.empty(
                g.top,
                "No test run yet.",
                "t",
                &format!("builds the solution and runs {cases} cases from testcases.txt"),
            );
            return;
        };

        if run.state == TestState::Failed {
            self.put(g.cx, g.top, "Tests could not run", bold(th.err));
            let rows = g.content_h().saturating_sub(4);
            let msg = run.error.clone().unwrap_or_default();
            for (i, l) in msg.lines().take(rows).enumerate() {
                let color = if l.contains("error") { th.err } else { th.dim };
                self.put(g.cx, g.top + 2 + i as u16, &trunc(l, g.text_w()), st(color));
            }
            return;
        }

        let y = g.top;
        let done = run.state == TestState::Done;
        if done {
            let failed = run.failed_count();
            let passed = run.rows.len() - failed;
            let mut x = self.put(g.cx, y, &format!("{passed} passed"), bold(th.ok));
            x = self.put(x, y, " · ", st(th.faint));
            x = self.put(
                x,
                y,
                &format!("{failed} failed"),
                bold(if failed > 0 { th.err } else { th.dim }),
            );
            let slowest = run.rows.iter().map(|r| r.elapsed).max().unwrap_or_default();
            let build = run.build.map(fmt_time).unwrap_or_default();
            let tail = format!(" · built in {build} · slowest {}", fmt_time(slowest));
            self.put(
                x,
                y,
                &trunc(&tail, g.text_w().saturating_sub(24)),
                st(th.dim),
            );
        } else {
            let x = self.squares(g.cx, y);
            let text = match run.state {
                TestState::Building if run.debug => "Building with sanitizers".to_string(),
                TestState::Building if run.partial => "Re-running".to_string(),
                TestState::Building => "Building".to_string(),
                _ => format!(
                    "Running case {} of {}",
                    (run.rows.len() + 1).min(run.numbers.len().max(1)),
                    run.numbers.len()
                ),
            };
            self.put(x + 1, y, &text, st(th.fg));
        }

        self.put(g.cx, y + 2, "Case  Verdict        Time", st(th.dim));
        let main_focus = app.focus == Focus::Main && !app.overlay_open();
        let sel = s.map_or(0, |s| s.case_sel);
        let mut spinner_used = false;
        for (i, n) in run.numbers.iter().enumerate() {
            let row_y = y + 3 + i as u16;
            if !self.fits(row_y) {
                break;
            }
            let row = run.rows.iter().find(|r| r.n == *n);
            let selected = i == sel && row.is_some();
            if selected {
                self.fill(g.bx, row_y, 28, 1, th.raised);
                self.vbar(g.bx, row_y, 1, if main_focus { th.acc } else { th.faint });
            }
            match row {
                Some(r) => {
                    let failure = r.result.verdict.is_failure();
                    let warn = matches!(r.result.verdict, Verdict::NoExpected { .. });
                    let color = if failure {
                        th.err
                    } else if warn {
                        th.warn
                    } else {
                        th.ok
                    };
                    let mark = if failure {
                        "✗"
                    } else if warn {
                        "?"
                    } else {
                        "✓"
                    };
                    self.put(g.cx, row_y, mark, st(color));
                    self.put(
                        g.cx + 2,
                        row_y,
                        &n.to_string(),
                        if selected { bold(th.fg) } else { st(th.fg) },
                    );
                    self.put(
                        g.cx + 6,
                        row_y,
                        &pad(verdict_label(&r.result.verdict), 14),
                        st(color),
                    );
                    self.put(g.cx + 21, row_y, &fmt_time(r.elapsed), st(th.dim));
                    self.hit(g.bx, row_y, 28, Action::SelectCase(i));
                }
                None if !done && !spinner_used => {
                    spinner_used = true;
                    self.put(g.cx, row_y, &spinner(self.t).to_string(), st(th.acc));
                    self.put(g.cx + 2, row_y, &n.to_string(), st(th.fg));
                    self.put(g.cx + 6, row_y, "running", st(th.dim));
                }
                None => {
                    self.put(g.cx + 2, row_y, &n.to_string(), st(th.faint));
                    self.put(g.cx + 6, row_y, "waiting", st(th.faint));
                }
            }
        }

        let dx = g.cx + 31;
        if g.cr.saturating_sub(dx) < 26 {
            return;
        }
        let Some(row) = run
            .rows
            .get(sel)
            .filter(|_| done || run.partial || run.rows.len() > sel)
        else {
            if !done {
                self.put(
                    dx,
                    y + 3,
                    "Details appear as each case finishes.",
                    st(th.faint),
                );
            }
            return;
        };
        self.case_detail(dx - 2, y + 2, g.cr - (dx - 2), row, run.debug);
    }

    fn case_detail(&mut self, bx: u16, y: u16, w: u16, row: &CaseRow, _debug: bool) {
        let (g, th) = (self.g, self.th);
        let failure = row.result.verdict.is_failure();
        let accent = if failure { th.err } else { th.ok };
        let tw = (w as usize).saturating_sub(4);

        // Decide what to show first, so the block can be sized to fit it.
        let mut parts: Vec<Part> = vec![Part::Field(
            "Input",
            row.input.trim_end().replace('\n', " ⏎ "),
            th.cya,
            2,
        )];
        match &row.result.verdict {
            Verdict::WrongAnswer { expected, actual } => {
                parts.push(Part::Field("Expected", expected.clone(), th.fg, 2));
                parts.push(Part::Diff(expected.clone(), actual.clone()));
            }
            Verdict::Accepted => {
                if let Some(e) = &row.expected {
                    parts.push(Part::Field("Output", e.trim_end().to_string(), th.fg, 2));
                }
            }
            Verdict::NoExpected { actual } => {
                parts.push(Part::Field("Output", actual.clone(), th.fg, 2));
            }
            Verdict::RuntimeError { reason } | Verdict::InputError { reason } => {
                parts.push(Part::Field("Error", reason.clone(), th.err, 4));
            }
            Verdict::TimeLimitExceeded => parts.push(Part::Note("Over the time limit", th.warn)),
        }
        let debug = row.result.debug.trim();
        if failure && !debug.is_empty() {
            parts.push(Part::Field("Stdout", debug.to_string(), th.dim, 3));
        } else if !failure && !debug.is_empty() {
            parts.push(Part::Note(
                "Stdout hidden because the case passed",
                th.faint,
            ));
        }
        let stderr = row.result.stderr.trim();
        if !stderr.is_empty() && !matches!(row.result.verdict, Verdict::InputError { .. }) {
            parts.push(Part::Field("Stderr", stderr.to_string(), th.dim, 3));
        }

        let need: u16 = 4 + parts
            .iter()
            .map(|p| match p {
                Part::Field(_, text, _, max) => 2 + text.lines().count().clamp(1, *max) as u16,
                Part::Diff(..) => 3,
                Part::Note(..) => 2,
            })
            .sum::<u16>();
        let h = need.min(g.bar_y.saturating_sub(y + 2));
        self.block_at(bx, y, w, h, accent);
        let x = bx + 2;
        let bottom = y + h;
        let label = format!("Case {} · {}", row.n, verdict_label(&row.result.verdict));
        self.put(x, y + 1, &trunc(&label, tw), bold(accent));
        let mut cy = y + 3;
        for part in &parts {
            match part {
                Part::Field(label, text, color, max) => {
                    cy = self.field(x, cy, bottom, tw, label, text, *color, *max);
                }
                Part::Diff(expected, actual) => {
                    if cy + 1 < bottom {
                        self.put(x, cy, "Got", st(th.dim));
                        self.diff_line(x, cy + 1, tw, expected, actual);
                        cy += 3;
                    }
                }
                Part::Note(text, color) => {
                    if cy < bottom {
                        self.put(x, cy, text, st(*color));
                        cy += 2;
                    }
                }
            }
        }
    }

    /// A dim label with up to `max` lines of `text` under it; returns the next free row.
    #[allow(clippy::too_many_arguments)]
    fn field(
        &mut self,
        x: u16,
        y: u16,
        bottom: u16,
        w: usize,
        label: &str,
        text: &str,
        color: Color,
        max: usize,
    ) -> u16 {
        let th = self.th;
        if y + 1 >= bottom {
            return y;
        }
        self.put(x, y, label, st(th.dim));
        let mut cy = y + 1;
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().take(max).enumerate() {
            if cy >= bottom {
                break;
            }
            let more = i + 1 == max && lines.len() > max;
            let shown = if more {
                format!("{l} …")
            } else {
                l.to_string()
            };
            self.put(x, cy, &trunc(&shown, w), st(color));
            cy += 1;
        }
        cy + 1
    }

    /// `actual` with everything from the first difference against `expected` highlighted.
    fn diff_line(&mut self, x: u16, y: u16, w: usize, expected: &str, actual: &str) {
        let th = self.th;
        let first = actual.lines().next().unwrap_or("");
        let exp: Vec<char> = expected.lines().next().unwrap_or("").chars().collect();
        let act: Vec<char> = first.chars().collect();
        let k = act
            .iter()
            .zip(exp.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let head: String = act[..k].iter().collect();
        let tail: String = act[k..].iter().collect();
        let head = trunc(&head, w);
        let mut cx = self.put(x, y, &head, st(th.fg));
        let room = w.saturating_sub(width(&head));
        let tail = if tail.is_empty() {
            "…".to_string()
        } else {
            tail
        };
        let hi = bold(th.ink).bg(th.err);
        if room > 0 {
            cx = self.put(cx, y, &trunc(&tail, room), hi);
        }
        let _ = cx;
    }

    // ---- Record -----------------------------------------------------

    fn tab_record(&mut self, p: &Problem) {
        let (g, app) = (self.g, self.app);
        let rec = app.rec.as_ref().filter(|r| r.id == p.id);
        if let Some(rec) = rec {
            return self.recording_view(p, rec);
        }
        let s = app.session(&p.id);
        let next = s
            .and_then(|s| s.disk.as_ref())
            .map_or(1, |d| d.next_attempt());
        if let Some(pipe) = s.and_then(|s| s.pipe.as_ref()) {
            return self.pipeline_view(p, pipe);
        }
        self.empty(
            g.top,
            "Not recording.",
            "r",
            &format!("starts attempt-{next}"),
        );
        if next > 1 {
            let x = self.put(g.cx, g.top + 2, "R", bold(self.th.fg));
            self.put(
                x + 1,
                g.top + 2,
                &format!("adds to attempt-{}", next - 1),
                st(self.th.dim),
            );
        }
        self.attempts_section(p, g.top + 4, None);
    }

    fn recording_view(&mut self, p: &Problem, rec: &Rec) {
        let (g, th, app) = (self.g, self.th, self.app);
        let a = &rec.active;
        let mut y = g.top;
        let paused = a.paused;
        if paused {
            self.put(g.cx, y, "❚❚", st(th.warn));
        } else {
            self.put(g.cx, y, if blink(self.t) { "●" } else { "○" }, st(th.err));
        }
        let mut x = self.put(g.cx + 3, y, &clock(a.elapsed().as_secs()), bold(th.fg));
        x = self.put(
            x + 2,
            y,
            if paused { "paused" } else { "recording" },
            st(if paused { th.warn } else { th.err }),
        );
        let what = if a.resuming() {
            format!("· adding to attempt-{}", a.attempt)
        } else {
            format!("· attempt-{}", a.attempt)
        };
        self.put(x + 1, y, &what, st(th.dim));
        y += 1;
        let x = self.put(g.cx + 3, y, &trunc(&a.device_name, 30), st(th.dim));
        if a.device_lost {
            self.put(x + 2, y, "no audio · d switches input", st(th.err));
        }
        y += 2;

        let sh = (g.content_h() as i32 - 11).clamp(3, 6) as u16;
        let n = g.text_w().div_ceil(2);
        let vals = a.bands(n);
        let sh2 = sh;
        let acc = th.acc;
        let (f1, f2) = (th.acc_fade(0.35), th.acc_fade(0.6));
        self.vbars(g.cx, y, sh, &vals, move |r, h| {
            if r * 3 >= h * 2 {
                acc
            } else if r * 3 >= h {
                f1
            } else {
                f2
            }
        });
        let _ = sh2;
        y += sh + 1;

        self.attempts_section(p, y, Some(rec));
        let y2 = y + 5;
        if app.confirm.is_none() && self.fits(y2 + 2) {
            self.section(y2, "When you stop");
            let t = format!(
                "transcribe {} → analyze {} → rate",
                app.transcribe_provider, app.analyze_provider
            );
            self.put(g.cx, y2 + 1, &trunc(&t, g.text_w()), st(th.dim));
            self.put(
                g.cx,
                y2 + 2,
                "It all happens in this tab. Switching away never stops it.",
                st(th.faint),
            );
        }
    }

    fn attempts_section(&mut self, p: &Problem, y: u16, rec: Option<&Rec>) {
        let (g, th, app) = (self.g, self.th, self.app);
        if !self.fits(y + 1) {
            return;
        }
        self.section(y, "Attempts");
        let disk = app.session(&p.id).and_then(|s| s.disk.as_ref());
        let mut rows: Vec<AttemptRow> = Vec::new();
        if let Some(d) = disk {
            for a in &d.attempts {
                let live = rec.is_some_and(|r| r.active.attempt == a.n);
                if live {
                    continue;
                }
                let date = a.date.map_or(String::new(), |d| {
                    if d == app.ws.today {
                        "today".to_string()
                    } else {
                        d.format("%b %-d").to_string()
                    }
                });
                let len = a.secs.map_or(String::new(), clock);
                let status = if a.transcript {
                    Some(("✓ transcript", th.ok))
                } else {
                    None
                };
                rows.push((a.n, date, len, String::new(), status));
            }
        }
        if let Some(r) = rec {
            let status = if r.active.paused {
                ("paused", th.warn)
            } else {
                ("recording", th.err)
            };
            rows.push((
                r.active.attempt,
                "today".into(),
                clock(r.active.elapsed().as_secs()),
                "live".into(),
                Some(status),
            ));
        }
        rows.sort_by_key(|r| r.0);
        let room = (g.bar_y.saturating_sub(y + 2)).min(3) as usize;
        let skip = rows.len().saturating_sub(room);
        for (i, (n, date, len, live, status)) in rows.iter().skip(skip).enumerate() {
            let ry = y + 1 + i as u16;
            if !self.fits(ry) {
                break;
            }
            self.put(
                g.cx,
                ry,
                &format!("attempt-{n}"),
                if live.is_empty() {
                    st(th.fg)
                } else {
                    bold(th.fg)
                },
            );
            self.put(g.cx + 11, ry, &date.to_string(), st(th.dim));
            self.put(
                g.cx + 20,
                ry,
                len,
                st(if live.is_empty() { th.dim } else { th.err }),
            );
            if let Some((t, c)) = status {
                self.put(g.cx + 27, ry, t, st(*c));
            }
        }
        if rows.is_empty() {
            self.put(g.cx, y + 1, "No recordings yet", st(th.faint));
        } else if let Some(d) = disk.filter(|d| d.analysis.is_some())
            && self.fits(y + 1 + rows.len().min(room) as u16)
        {
            let ay = y + 1 + rows.len().min(room) as u16;
            self.put(g.cx, ay, "analysis.md", st(th.fg));
            let date = d
                .analysis_date
                .map_or(String::new(), |d| d.format("%b %-d").to_string());
            self.put(g.cx + 11, ay, &date, st(th.dim));
            self.put(g.cx + 27, ay, "✓ analysis", st(th.ok));
        }
    }

    fn pipeline_view(&mut self, p: &Problem, pipe: &Pipe) {
        let (g, th, app) = (self.g, self.th, self.app);
        let y = g.top;
        let failed_at_analysis =
            matches!(&pipe.stage, PipeStage::Failed(e) if e.starts_with("analysis"));
        let step = |cv: &mut Cv, row: u16, state: char, text: &str, color: Color| {
            let mark = if state == 's' {
                spinner(cv.t).to_string()
            } else {
                state.to_string()
            };
            cv.put(g.cx, y + row, &mark, st(color));
            cv.put(
                g.cx + 2,
                y + row,
                &trunc(text, g.text_w().saturating_sub(2)),
                st(if color == th.faint { th.faint } else { th.fg }),
            );
        };
        step(
            self,
            0,
            '✓',
            &format!(
                "Recorded attempt-{} · {}",
                pipe.attempt,
                clock(pipe.len.as_secs())
            ),
            th.ok,
        );
        match &pipe.stage {
            PipeStage::Transcribing => {
                step(
                    self,
                    1,
                    's',
                    &format!("Transcribing with {}", app.transcribe_provider),
                    th.acc,
                );
                step(
                    self,
                    2,
                    '·',
                    &format!("Analyze with {}", app.analyze_provider),
                    th.faint,
                );
            }
            PipeStage::Analyzing => {
                step(self, 1, '✓', "Transcribed", th.ok);
                step(
                    self,
                    2,
                    's',
                    &format!("Analyzing with {}", app.analyze_provider),
                    th.acc,
                );
            }
            PipeStage::Done => {
                step(self, 1, '✓', "Transcribed", th.ok);
                step(self, 2, '✓', "Analysis ready", th.ok);
                let x = self.put(g.cx + 20, y + 2, "a", bold(th.fg));
                self.put(x + 1, y + 2, "reads it", st(th.dim));
            }
            PipeStage::Failed(e) => {
                if failed_at_analysis {
                    step(self, 1, '✓', "Transcribed", th.ok);
                    step(self, 2, '✗', e, th.err);
                } else {
                    step(self, 1, '✗', e, th.err);
                    step(self, 2, '·', "Analysis skipped", th.faint);
                }
            }
        }
        let finished = matches!(pipe.stage, PipeStage::Done | PipeStage::Failed(_));
        if !finished {
            step(self, 3, '·', "Rate it", th.faint);
            self.attempts_section(p, y + 5, None);
            return;
        }
        let rated = p.rated_today(app.ws.today);
        if let Some(r) = rated {
            step(self, 3, '✓', &format!("Rated {r} today"), th.ok);
            self.attempts_section(p, y + 5, None);
        } else {
            step(self, 3, '●', "Rate it", th.acc);
            if app.confirm.is_none() && self.fits(y + 5 + 5) {
                self.rating_block(y + 5, "Rate this attempt");
                self.attempts_section(p, y + 12, None);
            }
        }
    }

    // ---- Analysis ---------------------------------------------------

    fn tab_analysis(&mut self, p: &Problem) {
        let (g, th, app) = (self.g, self.th, self.app);
        let s = app.session(&p.id);
        let disk = s.and_then(|s| s.disk.as_ref());
        let busy = s.is_some_and(|s| {
            s.analyzing.is_some()
                || matches!(
                    s.pipe.as_ref().map(|p| &p.stage),
                    Some(PipeStage::Analyzing)
                )
        });
        let mut y = g.top;
        if busy {
            let x = self.squares(g.cx, y);
            self.put(
                x + 1,
                y,
                &format!("Analyzing with {}", app.analyze_provider),
                st(th.fg),
            );
            y += 2;
        } else if let Some(e) = s.and_then(|s| s.analysis_error.as_ref()) {
            self.put(g.cx, y, "Analysis failed", bold(th.err));
            for (i, l) in e.lines().take(3).enumerate() {
                self.put(g.cx, y + 1 + i as u16, &trunc(l, g.text_w()), st(th.dim));
            }
            y += 5;
        }
        let Some(text) = disk.and_then(|d| d.analysis.as_deref()) else {
            if !busy && s.is_none_or(|s| s.analysis_error.is_none()) {
                self.empty(
                    y,
                    "No analysis yet.",
                    "a",
                    "analyzes the latest recording. It also runs after you stop one.",
                );
            }
            return;
        };
        let date = disk
            .and_then(|d| d.analysis_date)
            .map_or(String::new(), |d| d.format("%b %-d").to_string());
        self.put(
            g.cx,
            y,
            &format!("analysis.md · {} · {date}", app.analyze_provider),
            st(th.faint),
        );
        y += 2;
        let rows = (g.bar_y.saturating_sub(1 + y)) as usize;
        let scroll = s.map_or(0, |s| s.scroll_analysis);
        let total = self.markdown(y, rows, text, scroll);
        app.max_scroll.set(total.saturating_sub(rows));
        self.scrollbar(y, rows as u16, total, scroll);
    }

    // ---- Submit -----------------------------------------------------

    fn tab_submit(&mut self, p: &Problem) {
        let (g, th, app) = (self.g, self.th, self.app);
        let s = app.session(&p.id);
        let run = s.map(|s| &s.submit);
        let y = g.top;
        match run {
            None | Some(SubmitRun::Idle) => {
                self.empty(
                    y,
                    "Not submitted yet.",
                    "S",
                    "sends the solution to LeetCode after you confirm",
                );
            }
            Some(SubmitRun::Working(stage)) => {
                let x = self.squares(g.cx, y);
                self.put(x + 1, y, &trunc(stage, g.text_w()), st(th.fg));
            }
            Some(SubmitRun::Failed(e)) => {
                self.put(g.cx, y, "Submission failed", bold(th.err));
                for (i, l) in e.lines().take(6).enumerate() {
                    self.put(g.cx, y + 2 + i as u16, &trunc(l, g.text_w()), st(th.dim));
                }
                let x = self.put(g.cx, y + 9, "S", bold(th.fg));
                self.put(x + 1, y + 9, "tries again", st(th.dim));
            }
            Some(SubmitRun::Done(r)) => self.submit_result(p, r),
        }
    }

    fn submit_result(&mut self, p: &Problem, r: &crate::leetcode::models::CheckResult) {
        let (g, th, app) = (self.g, self.th, self.app);
        let ok = r.status_msg == "Accepted";
        let mut y = g.top;
        let mut x = self.put(
            g.cx,
            y,
            &r.status_msg,
            bold(if ok { th.ok } else { th.err }),
        );
        if let (Some(c), Some(t)) = (r.total_correct, r.total_testcases) {
            x = self.put(x + 2, y, &format!("{c}/{t} testcases"), st(th.dim));
        }
        let _ = x;
        y += 2;
        if !ok {
            let err = r
                .full_compile_error
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(r.compile_error.as_deref().filter(|s| !s.is_empty()));
            if let Some(e) = err {
                let rows = g.content_h().saturating_sub(4);
                for (i, l) in e.lines().take(rows).enumerate() {
                    self.put(g.cx, y + i as u16, &trunc(l, g.text_w()), st(th.dim));
                }
            } else {
                self.put(
                    g.cx,
                    y,
                    "Run the tests with t to find the failing case.",
                    st(th.dim),
                );
            }
            return;
        }

        let half = g.text_w() >= 64;
        let bar_w = if half {
            (g.text_w() as u16 - 4) / 2
        } else {
            g.text_w() as u16
        };
        let gauge = |cv: &mut Cv,
                     x: u16,
                     y: u16,
                     title: &str,
                     value: &Option<String>,
                     pct: Option<f64>,
                     color: Color| {
            let mut cx = cv.put(x, y, title, bold(cv.th.fg));
            if let Some(v) = value {
                cx = cv.put(cx + 2, y, v, st(cv.th.fg));
            }
            if let Some(p) = pct {
                cv.put(cx + 2, y, &format!("beats {p:.1}%"), st(color));
                cv.hbar(x, y + 1, bar_w, p / 100.0, color);
            }
        };
        gauge(
            self,
            g.cx,
            y,
            "Runtime",
            &r.status_runtime,
            r.runtime_percentile,
            th.ok,
        );
        if half {
            gauge(
                self,
                g.cx + bar_w + 4,
                y,
                "Memory",
                &r.status_memory,
                r.memory_percentile,
                th.info,
            );
            y += 3;
        } else {
            y += 3;
            gauge(
                self,
                g.cx,
                y,
                "Memory",
                &r.status_memory,
                r.memory_percentile,
                th.info,
            );
            y += 3;
        }

        if app.confirm.is_some() {
            return;
        }
        if p.rated_today(app.ws.today).is_some() {
            self.put(
                g.cx,
                y + 1,
                &format!("✓ Rated {} today", p.rated_today(app.ws.today).unwrap_or(0)),
                st(th.ok),
            );
        } else if self.fits(y + 6) {
            self.rating_block(y + 1, "Rate this attempt");
        }
    }

    // ---- History ----------------------------------------------------

    fn tab_history(&mut self, p: &Problem) {
        let (g, th, app) = (self.g, self.th, self.app);
        self.section(g.top, "Leitner box");
        let mut x = g.cx;
        for b in 1..=5u8 {
            let cur = !p.is_new() && b == p.box_num;
            let label = format!(" {b} ");
            let style = if cur {
                bold(th.ink).bg(th.acc)
            } else {
                on(th.dim, th.raised)
            };
            self.put(x, g.top + 1, &label, style);
            let days = data::interval(b);
            self.put(
                x,
                g.top + 2,
                &format!("{days}d"),
                st(if cur { th.fg } else { th.faint }),
            );
            if b < 5 {
                self.put(x + 4, g.top + 1, "───", st(th.line));
            }
            x += 8;
        }

        let mut y = g.top + 4;
        if p.is_new() {
            self.put(g.cx, y, "Not logged yet.", st(th.fg));
            let k = self.put(g.cx, y + 1, "1-5", bold(th.fg));
            self.put(
                k + 1,
                y + 1,
                "after your first attempt adds it to box 1",
                st(th.dim),
            );
        } else {
            self.section(y, "Attempts");
            y += 1;
            let mut bx = 1u8;
            let mut streak = 0u8;
            let mut rows = Vec::new();
            for (i, a) in p.attempts.iter().enumerate() {
                let (nb, ns) = data::outcome(bx, streak, a.rating, i == 0);
                bx = nb;
                streak = ns;
                rows.push((a.date, a.rating, nb));
            }
            let room = (g.bar_y.saturating_sub(y + 9)).clamp(1, 5) as usize;
            let skip = rows.len().saturating_sub(room);
            for (date, rating, nb) in rows.iter().skip(skip) {
                let when = if *date == app.ws.today {
                    "today".to_string()
                } else {
                    date.format("%b %-d").to_string()
                };
                self.put(
                    g.cx,
                    y,
                    &pad(&when, 8),
                    st(if *date == app.ws.today {
                        th.acc
                    } else {
                        th.dim
                    }),
                );
                self.put(g.cx + 8, y, &stars(*rating), st(th.warn));
                self.put(g.cx + 15, y, &format!("→ box {nb}"), st(th.dim));
                y += 1;
            }
            if p.rated_today(app.ws.today).is_none() {
                self.put(g.cx, y, "today", st(th.acc));
                self.put(g.cx + 8, y, "·····", st(th.faint));
                y += 1;
            }
            let _ = diff_color;
        }

        if app.confirm.is_some() {
            return;
        }
        let block_y = g.bar_y.saturating_sub(8);
        if p.is_new() && p.folder.is_none() {
            return;
        }
        let rated = p.rated_today(app.ws.today);
        let title = match rated {
            Some(r) => format!("Rate today (replaces your {r})"),
            None => "Rate today".to_string(),
        };
        if block_y >= y {
            self.rating_block(block_y, &title);
        } else if let Some(d) = p.due {
            self.put(
                g.cx,
                y + 1,
                &format!("Due {}", due_label(Some(d))),
                st(th.dim),
            );
        }
    }
}

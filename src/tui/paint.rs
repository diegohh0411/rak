//! Drawing primitives on a ratatui buffer: text, flat fills, the thin left
//! bar that marks the active block, progress bars, and a small Markdown
//! renderer for questions and analyses.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::state::{Action, App, Hit};
use super::theme::Theme;

/// Screen geometry derived from the terminal size. Every screen lays itself
/// out from these numbers so wide and narrow terminals share one code path.
#[derive(Clone, Copy)]
pub struct Geo {
    pub w: u16,
    pub h: u16,
    /// Sidebar width: full list, or just ids on narrow terminals.
    pub side_w: u16,
    /// Left edge of a block in the main area (where its bar is drawn).
    pub bx: u16,
    /// Left edge of text inside the main area.
    pub cx: u16,
    /// Right edge (exclusive) of text and blocks.
    pub cr: u16,
    /// First row of tab content.
    pub top: u16,
    /// First row of the command bar (it spans three rows).
    pub bar_y: u16,
    pub foot_y: u16,
}

pub const MIN_W: u16 = 60;
pub const MIN_H: u16 = 24;

impl Geo {
    pub fn new(area: Rect) -> Geo {
        let w = area.width;
        let h = area.height;
        let side_w = if w >= 100 { 30 } else { 10 };
        let bx = side_w + 1;
        let cx = side_w + 3;
        // Keep line lengths readable on very wide terminals.
        let cr = (w.saturating_sub(3)).min(cx + 110);
        Geo {
            w,
            h,
            side_w,
            bx,
            cx,
            cr,
            top: 6,
            bar_y: h.saturating_sub(6),
            foot_y: h.saturating_sub(2),
        }
    }

    pub fn narrow(&self) -> bool {
        self.side_w < 30
    }

    /// Width of text between `cx` and `cr`.
    pub fn text_w(&self) -> usize {
        self.cr.saturating_sub(self.cx) as usize
    }

    /// Rows available to tab content, above the command bar.
    pub fn content_h(&self) -> usize {
        self.bar_y.saturating_sub(self.top + 1) as usize
    }
}

/// A frame being drawn: the buffer plus everything a screen needs to read.
pub struct Cv<'a> {
    pub buf: &'a mut Buffer,
    pub th: Theme,
    pub app: &'a App,
    pub g: Geo,
    /// Seconds since the app started; drives animation.
    pub t: f64,
    pub hits: &'a mut Vec<Hit>,
}

pub fn st(fg: Color) -> Style {
    Style::default().fg(fg)
}

pub fn bold(fg: Color) -> Style {
    Style::default().fg(fg).add_modifier(Modifier::BOLD)
}

pub fn on(fg: Color, bg: Color) -> Style {
    Style::default().fg(fg).bg(bg)
}

pub fn width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// `s` cut to `n` columns, ending in `…` when it was shortened.
pub fn trunc(s: &str, n: usize) -> String {
    if width(s) <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > n.saturating_sub(1) {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// `s` cut or space-padded to exactly `n` columns.
pub fn pad(s: &str, n: usize) -> String {
    let s = trunc(s, n);
    let w = width(&s);
    format!("{s}{}", " ".repeat(n.saturating_sub(w)))
}

pub fn stars(r: u8) -> String {
    let r = r.min(5) as usize;
    format!("{}{}", "★".repeat(r), "☆".repeat(5 - r))
}

pub fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn spinner(t: f64) -> char {
    const F: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    F[(t * 10.0) as usize % F.len()]
}

pub fn blink(t: f64) -> bool {
    ((t * 2.0) as u64).is_multiple_of(2)
}

const PARTIAL: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

impl Cv<'_> {
    pub fn put(&mut self, x: u16, y: u16, s: &str, style: Style) -> u16 {
        let area = self.buf.area;
        if y >= area.bottom() || x >= area.right() || y < area.y {
            return x;
        }
        let max = (area.right() - x) as usize;
        let (nx, _) = self.buf.set_stringn(x, y, s, max, style);
        nx
    }

    /// Clear a rectangle and give it a background.
    pub fn fill(&mut self, x: u16, y: u16, w: u16, h: u16, bg: Color) {
        let area = self.buf.area;
        for yy in y..y.saturating_add(h).min(area.bottom()) {
            for xx in x..x.saturating_add(w).min(area.right()) {
                let cell = &mut self.buf[(xx, yy)];
                cell.set_symbol(" ");
                cell.set_bg(bg);
            }
        }
    }

    /// The thin bar down the left edge of the active block.
    pub fn vbar(&mut self, x: u16, y: u16, h: u16, color: Color) {
        for yy in y..y.saturating_add(h) {
            self.put(x, yy, "▎", st(color));
        }
    }

    /// A raised block with a colored left bar; text goes at `bx + 2`.
    pub fn block(&mut self, y: u16, h: u16, bar: Color) {
        let (bx, w) = (self.g.bx, self.g.cr - self.g.bx);
        self.fill(bx, y, w, h, self.th.raised);
        self.vbar(bx, y, h, bar);
    }

    pub fn block_at(&mut self, x: u16, y: u16, w: u16, h: u16, bar: Color) {
        self.fill(x, y, w, h, self.th.raised);
        self.vbar(x, y, h, bar);
    }

    pub fn section(&mut self, y: u16, title: &str) {
        let (cx, fg) = (self.g.cx, self.th.fg);
        self.put(cx, y, title, bold(fg));
    }

    /// Key hints: white key, grey label.
    pub fn keys(&mut self, mut x: u16, y: u16, items: &[(&str, &str)]) -> u16 {
        for (k, l) in items {
            x = self.put(x, y, k, bold(self.th.fg));
            x = self.put(x + 1, y, l, st(self.th.dim));
            x += 3;
        }
        x
    }

    pub fn keys_width(items: &[(&str, &str)]) -> u16 {
        items
            .iter()
            .map(|(k, l)| (width(k) + 1 + width(l) + 3) as u16)
            .sum::<u16>()
            .saturating_sub(3)
    }

    pub fn hbar(&mut self, x: u16, y: u16, w: u16, frac: f64, fg: Color) {
        let frac = frac.clamp(0.0, 1.0);
        let n = frac * w as f64;
        let full = n.floor() as u16;
        let part = n - full as f64;
        for i in 0..w {
            if i < full {
                self.put(x + i, y, "█", st(fg));
            } else if i == full && part > 0.06 {
                let idx = ((part * 8.0) as usize).min(7);
                self.put(x + i, y, &PARTIAL[idx].to_string(), st(fg));
            } else {
                self.put(x + i, y, "░", st(self.th.faint));
            }
        }
    }

    /// Bottom-aligned vertical bars, one per value in `0..=1`.
    pub fn vbars(
        &mut self,
        x: u16,
        y: u16,
        h: u16,
        vals: &[f64],
        color: impl Fn(u16, u16) -> Color,
    ) {
        for (i, v) in vals.iter().enumerate() {
            let cx = x + i as u16 * 2;
            let hh = v.clamp(0.0, 1.0) * h as f64;
            for r in 0..h {
                let f = hh - r as f64;
                let ch = if f >= 1.0 {
                    '█'
                } else if f > 0.08 {
                    BLOCKS[((f * 8.0) as usize).min(7)]
                } else {
                    continue;
                };
                self.put(cx, y + h - 1 - r, &ch.to_string(), st(color(r, h)));
            }
        }
    }

    /// OpenCode's row of fading squares, for work in progress.
    pub fn squares(&mut self, x: u16, y: u16) -> u16 {
        let n = 6usize;
        let cycle = 2 * n - 2;
        let pos = (self.t * 12.0) as usize % cycle;
        let head = if pos < n { pos } else { cycle - pos };
        for i in 0..n {
            let d = head.abs_diff(i);
            let color = match d {
                0 => self.th.acc,
                1 => self.th.acc_fade(0.35),
                2 => self.th.acc_fade(0.6),
                _ => self.th.acc_fade(0.8),
            };
            self.put(x + i as u16, y, "■", st(color));
        }
        x + n as u16
    }

    pub fn hit(&mut self, x: u16, y: u16, w: u16, action: Action) {
        self.hits.push(Hit {
            rect: Rect::new(x, y, w, 1),
            action,
        });
    }

    /// Draw wrapped Markdown lines starting at `y`, skipping `skip` lines.
    /// Returns how many lines the text has in total.
    pub fn markdown(&mut self, y: u16, rows: usize, src: &str, skip: usize) -> usize {
        let lines = md_layout(src, self.g.text_w());
        let total = lines.len();
        for (i, line) in lines.iter().skip(skip).take(rows).enumerate() {
            let mut x = self.g.cx;
            for (text, kind) in line {
                let style = self.md_style(*kind);
                x = self.put(x, y + i as u16, text, style);
            }
        }
        total
    }

    fn md_style(&self, kind: MdKind) -> Style {
        let th = &self.th;
        match kind {
            MdKind::Text => st(th.fg),
            MdKind::H1 => bold(th.acc),
            MdKind::H2 => bold(th.vio),
            MdKind::Code => st(th.cya),
            MdKind::Bold => bold(th.fg),
            MdKind::Bullet => st(th.acc),
            MdKind::Quote => st(th.dim),
            MdKind::Dim => st(th.dim),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MdKind {
    Text,
    H1,
    H2,
    Code,
    Bold,
    Bullet,
    Quote,
    Dim,
}

type MdLine = Vec<(String, MdKind)>;

/// Lay out a Markdown subset (headings, lists, quotes, fenced code, tables,
/// `code` and **bold**) as wrapped lines of styled runs. Consecutive text
/// lines form one paragraph, as in Markdown.
pub fn md_layout(src: &str, w: usize) -> Vec<MdLine> {
    let w = w.max(20);
    let mut out: Vec<MdLine> = Vec::new();
    let mut in_fence = false;
    let mut pending: Option<Block> = None;
    let tag = regex::Regex::new(r"</?[a-zA-Z][^>]*>").unwrap();

    fn blank(out: &mut Vec<MdLine>) {
        if out.last().is_some_and(|l| !l.is_empty()) {
            out.push(Vec::new());
        }
    }
    fn flush(pending: &mut Option<Block>, out: &mut Vec<MdLine>, w: usize) {
        if let Some(b) = pending.take() {
            wrap(out, &b.text, w, b.kind, &b.marker, b.indent, b.marker_kind);
        }
    }

    for raw in src.lines() {
        let trimmed = raw.trim_end();
        if trimmed.trim_start().starts_with("```") {
            flush(&mut pending, &mut out, w);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push(vec![(trunc(&format!("  {trimmed}"), w), MdKind::Code)]);
            continue;
        }
        if trimmed.starts_with('[') && trimmed.contains("]: ") {
            continue; // link reference definition
        }
        let cleaned = tag.replace_all(trimmed, "");
        let line = cleaned.as_ref();
        if line.trim().is_empty() {
            flush(&mut pending, &mut out, w);
            blank(&mut out);
            continue;
        }
        let t = line.trim_start();
        if let Some((level, text)) = heading(t) {
            flush(&mut pending, &mut out, w);
            blank(&mut out);
            let kind = if level == 1 { MdKind::H1 } else { MdKind::H2 };
            wrap(&mut out, text, w, kind, "", 0, None);
            continue;
        }
        if t.starts_with('|') {
            flush(&mut pending, &mut out, w);
            if !t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
                out.push(vec![(trunc(t, w), MdKind::Dim)]);
            }
            continue;
        }
        if t.starts_with("---") || t.starts_with("***") {
            flush(&mut pending, &mut out, w);
            continue;
        }
        if let Some(rest) = t.strip_prefix("> ") {
            match &mut pending {
                Some(b) if b.kind == MdKind::Quote => {
                    b.text.push(' ');
                    b.text.push_str(rest);
                }
                _ => {
                    flush(&mut pending, &mut out, w);
                    pending = Some(Block::new(rest, MdKind::Quote, "│ ", 2, None));
                }
            }
            continue;
        }
        let indent = (line.len() - t.len()).min(8);
        if let Some(rest) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        {
            flush(&mut pending, &mut out, w);
            let marker = format!("{}• ", " ".repeat(indent));
            pending = Some(Block::new(
                rest,
                MdKind::Text,
                &marker,
                indent + 2,
                Some(MdKind::Bullet),
            ));
            continue;
        }
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && t[digits..].starts_with(". ") {
            flush(&mut pending, &mut out, w);
            pending = Some(Block::new(
                &t[digits + 2..],
                MdKind::Text,
                &t[..digits + 2],
                digits + 2,
                Some(MdKind::Bullet),
            ));
            continue;
        }
        match &mut pending {
            Some(b) => {
                b.text.push(' ');
                b.text.push_str(t);
            }
            None => pending = Some(Block::new(t, MdKind::Text, "", 0, None)),
        }
    }
    flush(&mut pending, &mut out, w);
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// A paragraph, list item or quote being collected line by line.
struct Block {
    text: String,
    kind: MdKind,
    marker: String,
    indent: usize,
    marker_kind: Option<MdKind>,
}

impl Block {
    fn new(
        text: &str,
        kind: MdKind,
        marker: &str,
        indent: usize,
        marker_kind: Option<MdKind>,
    ) -> Block {
        Block {
            text: text.to_string(),
            kind,
            marker: marker.to_string(),
            indent,
            marker_kind,
        }
    }
}

fn heading(t: &str) -> Option<(usize, &str)> {
    let hashes = t.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
        Some((hashes, t[hashes..].trim()))
    } else {
        None
    }
}

/// Word-wrap `text` (with inline `code` and **bold**) into `out`. The first
/// line starts with `marker`; the rest are indented by `indent` columns.
fn wrap(
    out: &mut Vec<MdLine>,
    text: &str,
    w: usize,
    base: MdKind,
    marker: &str,
    indent: usize,
    marker_kind: Option<MdKind>,
) {
    let mut line: MdLine = Vec::new();
    let mut col = 0;
    if !marker.is_empty() {
        line.push((marker.to_string(), marker_kind.unwrap_or(base)));
        col = width(marker);
    }
    let lead = |base: MdKind| -> MdLine {
        let (text, kind) = if base == MdKind::Quote {
            ("│ ".to_string(), MdKind::Quote)
        } else {
            (" ".repeat(indent), MdKind::Text)
        };
        if text.is_empty() {
            Vec::new()
        } else {
            vec![(text, kind)]
        }
    };
    let mut at_start = true;
    for word in styled_words(text, base) {
        let shown = trunc(&word.text, w.saturating_sub(indent).max(8));
        let ww = width(&shown);
        let space = usize::from(!at_start && word.gap);
        if !at_start && col + space + ww > w {
            out.push(std::mem::take(&mut line));
            line = lead(base);
            col = line.iter().map(|(t, _)| width(t)).sum();
        } else if space == 1 {
            line.push((" ".to_string(), MdKind::Text));
            col += 1;
        }
        line.push((shown, word.kind));
        col += ww;
        at_start = false;
    }
    out.push(line);
}

struct Word {
    text: String,
    kind: MdKind,
    /// Whether whitespace separated it from the previous word.
    gap: bool,
}

/// Split into words, each carrying its inline style. `code` spans stay whole.
fn styled_words(text: &str, base: MdKind) -> Vec<Word> {
    let mut words: Vec<Word> = Vec::new();
    let mut cur = String::new();
    let mut code = false;
    let mut strong = false;
    let mut gap = true;
    let chars: Vec<char> = text.chars().collect();
    let kind_of = |code: bool, strong: bool| {
        if code {
            MdKind::Code
        } else if strong && base == MdKind::Text {
            MdKind::Bold
        } else {
            base
        }
    };
    let flush = |cur: &mut String, words: &mut Vec<Word>, gap: &mut bool, kind: MdKind| {
        if !cur.is_empty() {
            words.push(Word {
                text: std::mem::take(cur),
                kind,
                gap: *gap,
            });
            *gap = false;
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '`' {
            flush(&mut cur, &mut words, &mut gap, kind_of(code, strong));
            code = !code;
        } else if !code && c == '*' && chars.get(i + 1) == Some(&'*') {
            flush(&mut cur, &mut words, &mut gap, kind_of(code, strong));
            strong = !strong;
            i += 1;
        } else if c.is_whitespace() && !code {
            flush(&mut cur, &mut words, &mut gap, kind_of(code, strong));
            gap = true;
        } else {
            cur.push(c);
        }
        i += 1;
    }
    flush(&mut cur, &mut words, &mut gap, kind_of(code, strong));
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(lines: &[MdLine]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|(t, _)| t.as_str()).collect::<String>())
            .collect()
    }

    #[test]
    fn trunc_and_pad_respect_width() {
        assert_eq!(trunc("hello", 10), "hello");
        assert_eq!(trunc("hello world", 6), "hello…");
        assert_eq!(pad("ab", 4), "ab  ");
    }

    #[test]
    fn markdown_wraps_and_marks_lists() {
        let lines = md_layout("## Title\n- one two three four five six", 12);
        let text = flat(&lines);
        assert_eq!(text[0], "Title");
        assert!(text[1].starts_with("• one two"), "{text:?}");
        assert!(text[2].starts_with("  "), "{text:?}");
    }

    #[test]
    fn markdown_joins_paragraph_lines_and_keeps_punctuation_attached() {
        let lines = md_layout("Given `nums` and `target`,\nreturn indices.", 60);
        assert_eq!(flat(&lines), vec!["Given nums and target, return indices."]);
    }

    #[test]
    fn markdown_styles_inline_code_and_bold() {
        let lines = md_layout("use `top` and **bold**", 40);
        let kinds: Vec<MdKind> = lines[0].iter().map(|(_, k)| *k).collect();
        assert!(kinds.contains(&MdKind::Code));
        assert!(kinds.contains(&MdKind::Bold));
    }

    #[test]
    fn markdown_skips_link_references_and_fences_code() {
        let lines = md_layout("[link]: http://x\n```\nlet a = 1;\n```", 40);
        assert_eq!(flat(&lines), vec!["  let a = 1;"]);
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock(125), "02:05");
    }
}

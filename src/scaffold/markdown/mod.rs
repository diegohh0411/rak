//! HTML → Markdown conversion that reproduces leetgo's output.
//!
//! leetgo renders question descriptions with JohannesKaufmann/html-to-markdown
//! v2 (base + commonmark + strikethrough + table plugins). This module is a
//! faithful port of that pipeline so `question.md` comes out the same.
//! Both projects are MIT licensed.

mod dom;
mod text;

use dom::{Dom, Id, Kind, ROOT, name_is_block_node, name_is_heading, name_is_inline_node};
use text::*;

/// Convert LeetCode question HTML to Markdown exactly like leetgo's `htmlToMarkdown`.
pub fn html_to_markdown(html: &str) -> String {
    let mut dom = Dom::parse(html);
    pre_render(&mut dom);

    let mut out = String::new();
    Renderer { dom: &dom }.render_node(Ctx::default(), &mut out, ROOT);

    // base: postRenderTrimContent
    let out = trim_space(&out);
    let out = trim_consecutive_newlines(out);
    let out = trim_unnecessary_hard_line_breaks(&out);
    // base: postRenderUnescapeContent
    let out = unescape_content(&out);
    // commonmark: handlePostRenderCodeBlockNewline
    let out = out.replace(CODE_NL, "\n");
    // leetgo: remove special HTML entity characters
    out.replace('\u{00A0}', " ").replace('\u{200B}', "")
}

// ---------------------------------------------------------------------------
// Tag types
// ---------------------------------------------------------------------------

#[derive(PartialEq, Eq, Clone, Copy)]
enum TagType {
    Block,
    Inline,
    Remove,
    Unknown,
}

fn tag_type(name: &str) -> TagType {
    match name {
        "#comment" | "head" | "script" | "style" | "link" | "meta" | "iframe" | "noscript"
        | "textarea" => TagType::Remove,
        // leetgo registers a task-list renderer for <input>, overriding base's "remove".
        "input" => TagType::Inline,
        n if name_is_block_node(n) => TagType::Block,
        n if name_is_inline_node(n) => TagType::Inline,
        _ => TagType::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Pre-render passes
// ---------------------------------------------------------------------------

fn is_bold(d: &Dom, n: Id) -> bool {
    matches!(d.name(n), "strong" | "b")
}
fn is_italic(d: &Dom, n: Id) -> bool {
    matches!(d.name(n), "em" | "i")
}
fn is_bold_or_italic(d: &Dom, n: Id) -> bool {
    is_bold(d, n) || is_italic(d, n)
}
fn is_both_bold_or_italic(d: &Dom, a: Id, b: Id) -> bool {
    (is_bold(d, a) && is_bold(d, b)) || (is_italic(d, a) && is_italic(d, b))
}
fn is_pre(d: &Dom, n: Id) -> bool {
    d.name(n) == "pre"
}
fn is_inline_code(d: &Dom, n: Id) -> bool {
    matches!(d.name(n), "code" | "var" | "samp" | "kbd" | "tt")
}
fn is_link(d: &Dom, n: Id) -> bool {
    d.name(n) == "a"
}
fn is_both_link(d: &Dom, a: Id, b: Id) -> bool {
    is_link(d, a) && is_link(d, b)
}
fn is_heading(d: &Dom, n: Id) -> bool {
    name_is_heading(d.name(n))
}
fn is_strike(d: &Dom, n: Id) -> bool {
    matches!(d.name(n), "del" | "s" | "strike")
}
fn is_both_strike(d: &Dom, a: Id, b: Id) -> bool {
    is_strike(d, a) && is_strike(d, b)
}

fn pre_render(d: &mut Dom) {
    // base: preRenderRemove (PriorityEarly)
    for n in d.all_nodes(ROOT) {
        if tag_type(d.name(n)) == TagType::Remove {
            d.remove(n);
        }
    }
    merge_adjacent_text_nodes(d, ROOT);

    // leetgo: sub/sup → unicode (PriorityEarly)
    for n in d.all_nodes(ROOT) {
        let name = d.name(n);
        if name == "sub" || name == "sup" {
            let is_sub = name == "sub";
            let text = d.collect_text(n);
            let replaced: String = text
                .chars()
                .map(|c| if is_sub { subscript(c) } else { superscript(c) })
                .collect();
            d.set_text(n, replaced);
        }
    }

    // commonmark: handlePreRender (PriorityStandard)
    rename_fake_spans(d, ROOT);
    remove_redundant(d, is_both_bold_or_italic);
    merge_adjacent(d, is_bold_or_italic);
    remove_empty_code(d);
    swap_tags(d, ROOT, is_inline_code, is_pre);
    merge_adjacent(d, is_inline_code);
    add_space(d, is_bold_or_italic, is_inline_code);
    remove_redundant(d, is_both_link);
    swap_tags(d, ROOT, is_bold_or_italic, is_link);
    swap_tags(d, ROOT, is_link, is_heading);
    leaf_block_alternatives(d, ROOT, false, false);
    move_list_items(d, ROOT);

    // strikethrough: handlePreRender (PriorityStandard)
    remove_redundant(d, is_both_strike);
    merge_adjacent(d, is_strike);

    // base: preRenderCollapse (PriorityLate)
    collapse(d, ROOT);

    // commonmark: list end comments (PriorityLate + 100)
    add_list_end_comments(d);
}

fn merge_adjacent_text_nodes(d: &mut Dom, n: Id) {
    let mut prev: Option<Id> = None;
    let mut c = d.first_child(n);
    while let Some(id) = c {
        let next = d.next(id);
        match prev {
            Some(p) if d.kind(id) == Kind::Text && d.kind(p) == Kind::Text => {
                let data = format!("{}{}", d.data(p), d.data(id));
                d.set_data(p, data);
                d.remove(id);
            }
            _ => {
                merge_adjacent_text_nodes(d, id);
                prev = Some(id);
            }
        }
        c = next;
    }
}

fn rename_fake_spans(d: &mut Dom, n: Id) {
    if d.name(n) == "span"
        && d.all_nodes(n).into_iter().any(|c| name_is_block_node(d.name(c)))
    {
        d.set_data(n, "div");
    }
    let mut c = d.first_child(n);
    while let Some(id) = c {
        rename_fake_spans(d, id);
        c = d.next(id);
    }
}

fn remove_redundant(d: &mut Dom, same: fn(&Dom, Id, Id) -> bool) {
    for n in d.all_nodes(ROOT) {
        if !same(d, n, n) {
            continue;
        }
        let mut p = d.parent(n);
        while let Some(pid) = p {
            if same(d, n, pid) {
                d.unwrap_node(n);
                break;
            }
            p = d.parent(pid);
        }
    }
}

fn merge_adjacent(d: &mut Dom, m: fn(&Dom, Id) -> bool) {
    let mut node = Some(ROOT);
    while let Some(n) = node {
        if m(d, n) {
            let mut collected = Vec::new();
            let mut cur = d.next(n);
            while let Some(c) = cur {
                if d.name(c) == "span" {
                    cur = d.next_neighbor_node(c);
                } else if m(d, c) {
                    collected.push(c);
                    cur = d.next_neighbor_node_excluding_own_child(c);
                } else {
                    break;
                }
            }
            for c in collected {
                for child in d.children(c) {
                    d.append_child(n, child);
                }
                d.remove(c);
            }
        }
        node = d.next_neighbor_element(n);
    }
}

fn remove_empty_code(d: &mut Dom) {
    let mut node = Some(ROOT);
    while let Some(n) = node {
        if d.name(n) == "code"
            && !d
                .all_nodes(n)
                .into_iter()
                .any(|c| d.kind(c) == Kind::Text && !d.data(c).is_empty())
        {
            let next = d.next_neighbor_node_excluding_own_child(n);
            d.remove(n);
            node = next;
            continue;
        }
        node = d.next_neighbor_node(n);
    }
}

fn swap_tags(d: &mut Dom, n: Id, outer: fn(&Dom, Id) -> bool, inner: fn(&Dom, Id) -> bool) {
    if outer(d, n) {
        let kids: Vec<Id> = d
            .children(n)
            .into_iter()
            .filter(|&c| !(d.kind(c) == Kind::Text && trim_space(d.data(c)).is_empty()))
            .collect();
        if kids.len() == 1 && inner(d, kids[0]) {
            d.swap_tags(n, kids[0]);
            return;
        }
    }
    let mut c = d.first_child(n);
    while let Some(id) = c {
        swap_tags(d, id, outer, inner);
        c = d.next(id);
    }
}

fn first_child_matching(d: &Dom, start: Id, m: fn(&Dom, Id) -> bool, last: bool) -> Option<Id> {
    let mut node = if last { d.last_child(start) } else { d.first_child(start) };
    while let Some(n) = node {
        if d.name(n) == "span" {
            node = if last { d.prev_neighbor_node(n) } else { d.next_neighbor_node(n) };
        } else if m(d, n) {
            return Some(n);
        } else {
            return None;
        }
    }
    None
}

fn neighbor_text_node(d: &Dom, start: Id, forward: bool) -> Option<Id> {
    let mut node = if forward {
        d.next_neighbor_node_excluding_own_child(start)
    } else {
        d.prev_neighbor_node_excluding_own_child(start)
    };
    while let Some(n) = node {
        if d.kind(n) == Kind::Text {
            return Some(n);
        }
        if d.name(n) == "span" {
            node = if forward { d.next_neighbor_node(n) } else { d.prev_neighbor_node(n) };
            continue;
        }
        return None;
    }
    None
}

fn add_space(d: &mut Dom, outer: fn(&Dom, Id) -> bool, inner: fn(&Dom, Id) -> bool) {
    let mut node = Some(ROOT);
    while let Some(n) = node {
        if outer(d, n) {
            if first_child_matching(d, n, inner, false).is_some()
                && let Some(prev) = neighbor_text_node(d, n, false)
            {
                let data = format!("{} ", d.data(prev));
                d.set_data(prev, data);
            }
            if first_child_matching(d, n, inner, true).is_some()
                && let Some(next) = neighbor_text_node(d, n, true)
            {
                let data = format!(" {}", d.data(next));
                d.set_data(next, data);
            }
        }
        node = d.next_neighbor_element(n);
    }
}

fn markdown_structure(name: &str) -> &'static str {
    match name {
        "#document" | "html" | "head" | "body" | "blockquote" | "ul" | "ol" | "li" => {
            "container_block"
        }
        "hr" | "pre" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "leaf_block",
        "#text" | "span" | "code" | "b" | "strong" | "i" | "em" | "a" | "img" | "br" => "inline",
        _ => "",
    }
}

fn leaf_block_alternatives(d: &mut Dom, n: Id, mut in_leaf: bool, mut in_inline: bool) {
    let structure = markdown_structure(d.name(n));
    if (structure == "container_block" || structure == "leaf_block") && (in_leaf || in_inline) {
        match d.name(n) {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                d.set_data(n, "strong");
                let br = d.new_node(Kind::Element, "br".into());
                let p = d.parent(n).expect("heading has a parent");
                let next = d.next(n);
                d.insert_before(p, br, next);
            }
            "blockquote" => {
                let p = d.parent(n).expect("blockquote has a parent");
                let before = d.new_node(Kind::Text, " \"".into());
                d.insert_before(p, before, Some(n));
                d.set_data(n, "span");
                let after = d.new_node(Kind::Text, "\" ".into());
                let next = d.next(n);
                d.insert_before(p, after, next);
            }
            "pre" => d.set_data(n, "code"),
            "hr" => d.remove(n),
            _ => d.set_data(n, "span"),
        }
    }
    if structure == "leaf_block" {
        in_leaf = true;
    }
    if structure == "inline" {
        in_inline = true;
    }
    // Go uses `defer` inside the loop: children are visited in reverse order.
    for c in d.children(n).into_iter().rev() {
        leaf_block_alternatives(d, c, in_leaf, in_inline);
    }
}

fn move_list_items(d: &mut Dom, n: Id) {
    if d.kind(n) == Kind::Element && (d.data(n) == "ol" || d.data(n) == "ul") {
        let mut previous_li: Option<Id> = None;
        for child in d.children(n) {
            if d.kind(child) == Kind::Element && d.data(child) == "li" {
                previous_li = Some(child);
            } else if d.kind(child) == Kind::Text && trim_space(d.data(child)).is_empty() {
                // keep whitespace between items
            } else if let Some(li) = previous_li {
                d.append_child(li, child);
            } else {
                let li = d.new_node(Kind::Element, "li".into());
                previous_li = Some(d.wrap_node(child, li));
            }
        }
    }
    let mut c = d.first_child(n);
    while let Some(id) = c {
        move_list_items(d, id);
        c = d.next(id);
    }
}

fn add_list_end_comments(d: &mut Dom) {
    let is_list = |d: &Dom, n: Id| matches!(d.name(n), "ul" | "ol");
    let mut node = Some(ROOT);
    while let Some(n) = node {
        if is_list(d, n) && next_is_list(d, n) {
            let comment = d.new_node(Kind::Comment, LIST_END_COMMENT.into());
            let p = d.parent(n).expect("list has a parent");
            let next = d.next(n);
            d.insert_before(p, comment, next);
        }
        node = d.next_neighbor_element(n);
    }
}

const LIST_END_COMMENT: &str = "THE END";

fn next_is_list(d: &Dom, start: Id) -> bool {
    let mut node = d.next_neighbor_node_excluding_own_child(start);
    while let Some(n) = node {
        match d.name(n) {
            "ul" | "ol" => return true,
            "li" | "hr" | "#text" => return false,
            "#comment" if d.data(n) == LIST_END_COMMENT => return false,
            _ => {}
        }
        node = d.next_neighbor_node(n);
    }
    false
}

// ---------------------------------------------------------------------------
// Whitespace collapsing (collapse/collapse.go)
// ---------------------------------------------------------------------------

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "command" | "embed" | "hr" | "img" | "input"
            | "keygen" | "link" | "meta" | "param" | "track" | "wbr"
    )
}

fn is_preformatted(d: &Dom, n: Id) -> bool {
    matches!(d.name(n), "pre" | "code")
}

fn replace_any_whitespace_with_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if matches!(c, ' ' | '\r' | '\n' | '\t') {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

fn collapse_next(d: &Dom, prev: Option<Id>, current: Id) -> Id {
    let came_up = prev.is_some_and(|p| d.parent(p) == Some(current));
    if came_up || is_preformatted(d, current) {
        return d.next(current).or(d.parent(current)).expect("walk stays inside root");
    }
    d.first_child(current)
        .or(d.next(current))
        .or(d.parent(current))
        .expect("walk stays inside root")
}

fn collapse_remove(d: &mut Dom, n: Id) -> Id {
    let next = d.next(n).or(d.parent(n)).expect("node has a parent");
    d.remove(n);
    next
}

fn collapse(d: &mut Dom, element: Id) {
    if d.first_child(element).is_none() || is_preformatted(d, element) {
        return;
    }
    let mut prev_text: Option<Id> = None;
    let mut keep_leading_ws = false;
    let mut prev: Option<Id> = None;
    let mut node = collapse_next(d, prev, element);

    while node != element {
        match d.kind(node) {
            Kind::Text => {
                let mut text = replace_any_whitespace_with_space(d.data(node));
                let prev_ends_space = prev_text.is_none_or(|p| d.data(p).ends_with(' '));
                if prev_ends_space && !keep_leading_ws && text.starts_with(' ') {
                    text.remove(0);
                }
                if text.is_empty() {
                    node = collapse_remove(d, node);
                    continue;
                }
                d.set_data(node, text);
                prev_text = Some(node);
            }
            Kind::Element => {
                let name = d.name(node).to_string();
                if tag_type(&name) == TagType::Block || name == "br" {
                    if let Some(p) = prev_text {
                        let t = d.data(p).strip_suffix(' ').unwrap_or(d.data(p)).to_string();
                        d.set_data(p, t);
                    }
                    prev_text = None;
                    keep_leading_ws = false;
                } else if is_void(&name) || is_preformatted(d, node) || name == "code" {
                    prev_text = None;
                    keep_leading_ws = true;
                } else if prev_text.is_some() {
                    keep_leading_ws = false;
                }
            }
            Kind::Comment => {}
            Kind::Document => {
                node = collapse_remove(d, node);
                continue;
            }
        }
        let next = collapse_next(d, prev, node);
        prev = Some(node);
        node = next;
    }

    if let Some(p) = prev_text {
        let t = d.data(p).strip_suffix(' ').unwrap_or(d.data(p)).to_string();
        let empty = t.is_empty();
        d.set_data(p, t);
        if empty {
            d.remove(p);
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

#[derive(Default, Clone, Copy)]
struct Ctx {
    inside_link: bool,
}

struct Renderer<'a> {
    dom: &'a Dom,
}

impl Renderer<'_> {
    fn render_children(&self, ctx: Ctx, w: &mut String, n: Id) {
        for c in self.dom.children(n) {
            self.render_node(ctx, w, c);
        }
    }

    fn render_to_string(&self, ctx: Ctx, n: Id, children_only: bool) -> String {
        let mut buf = String::new();
        if children_only {
            self.render_children(ctx, &mut buf, n);
        } else {
            self.render_node(ctx, &mut buf, n);
        }
        buf
    }

    fn render_node(&self, ctx: Ctx, w: &mut String, n: Id) {
        let d = self.dom;
        if d.kind(n) == Kind::Text {
            // base: characterEntityReplacer + escaping
            let content = d.data(n).replace('<', "&lt;").replace('>', "&gt;");
            let mut content = escape_content(&content);
            // commonmark: keep `]` escaped inside link text
            if ctx.inside_link {
                content = content.replace("\x07]", "\\]");
            }
            w.push_str(&content);
            return;
        }

        let handled = match d.name(n) {
            "input" => self.render_input(w, n),
            "strong" | "b" => self.render_delimited(ctx, w, n, "**"),
            "em" | "i" => self.render_delimited(ctx, w, n, "*"),
            "del" | "s" | "strike" => self.render_delimited(ctx, w, n, "~~"),
            "hr" => {
                w.push_str("\n\n* * *\n\n");
                true
            }
            "br" => {
                w.push_str("  \n");
                true
            }
            "ul" | "ol" => self.render_list(ctx, w, n),
            "pre" => self.render_block_code(w, n),
            "code" | "var" | "samp" | "kbd" | "tt" => self.render_inline_code(w, n),
            "blockquote" => self.render_blockquote(ctx, w, n),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.render_heading(ctx, w, n),
            "img" => self.render_image(w, n),
            "a" => self.render_link(ctx, w, n),
            "#comment" if d.data(n) == LIST_END_COMMENT => {
                w.push_str("\n\n<!--THE END-->\n\n");
                true
            }
            "table" => self.render_table(ctx, w, n),
            "tr" => {
                w.push_str("\n\n");
                self.render_children(ctx, w, n);
                w.push_str("\n\n");
                true
            }
            _ => false,
        };
        if handled {
            return;
        }

        // converter: handleRenderFallback
        let block = tag_type(d.name(n)) == TagType::Block;
        if block {
            w.push_str("\n\n");
        }
        self.render_children(ctx, w, n);
        if block {
            w.push_str("\n\n");
        }
    }

    /// leetgo's task-list checkbox renderer.
    fn render_input(&self, w: &mut String, n: Id) -> bool {
        let d = self.dom;
        let in_li = d.parent(n).is_some_and(|p| d.name(p) == "li");
        if in_li && d.attr_or(n, "type", "") == "checkbox" {
            w.push_str(if d.attr(n, "checked").is_some() { "[x]" } else { "[ ]" });
            let next_starts_with_space = d
                .next(n)
                .is_some_and(|x| d.kind(x) == Kind::Text && d.data(x).starts_with(' '));
            if !next_starts_with_space {
                w.push(' ');
            }
        }
        true
    }

    fn render_delimited(&self, ctx: Ctx, w: &mut String, n: Id, delim: &str) -> bool {
        let content = self.render_to_string(ctx, n, true);
        w.push_str(&delimiter_for_every_line(&content, delim));
        true
    }

    fn code_without_tags(&self, start: Id) -> (String, String) {
        let d = self.dom;
        let mut buf = String::new();
        let mut info = String::new();
        fn walk(d: &Dom, n: Id, buf: &mut String, info: &mut String) {
            if d.kind(n) == Kind::Element {
                let name = d.data(n);
                if (name == "code" || name == "pre") && info.is_empty() {
                    *info = code_language(d, n);
                }
                if matches!(name, "style" | "script" | "textarea") {
                    return;
                }
                if name == "br" || name == "div" {
                    buf.push('\n');
                }
            }
            if d.kind(n) == Kind::Text {
                buf.push_str(d.data(n));
                return;
            }
            for c in d.children(n) {
                walk(d, c, buf, info);
            }
        }
        walk(d, start, &mut buf, &mut info);
        (buf, info)
    }

    fn render_inline_code(&self, w: &mut String, n: Id) -> bool {
        let (content, _) = self.code_without_tags(n);
        if trim_space(&content).is_empty() {
            w.push('`');
            w.push_str(&content);
            w.push('`');
            return true;
        }
        let mut code = collapse_inline_code(&content);
        let fence = "`".repeat(code_fence_occurrences('`', &code) + 1);
        if code.starts_with('`') {
            code.insert(0, ' ');
        }
        if code.ends_with('`') {
            code.push(' ');
        }
        w.push_str(&fence);
        w.push_str(&code);
        w.push_str(&fence);
        true
    }

    fn render_block_code(&self, w: &mut String, n: Id) -> bool {
        let (mut code, info) = self.code_without_tags(n);
        if code.ends_with('\n') {
            code.pop();
        }
        let fence = calculate_code_fence('`', &code);
        let code = code.replace('\n', &CODE_NL.to_string());
        w.push_str("\n\n");
        w.push_str(&fence);
        w.push_str(&info);
        w.push('\n');
        w.push_str(&code);
        w.push('\n');
        w.push_str(&fence);
        w.push_str("\n\n");
        true
    }

    fn render_list(&self, ctx: Ctx, w: &mut String, n: Id) -> bool {
        let d = self.dom;
        let items: Vec<String> = d
            .children(n)
            .into_iter()
            .map(|c| trim_space(&self.render_to_string(ctx, c, false)).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            return true;
        }
        let is_ul = d.data(n) == "ul";
        let start_at: i64 = d.attr_or(n, "start", "1").parse().unwrap_or(1);
        let width = (start_at + items.len() as i64 - 1).to_string().chars().count();
        let prefix = |i: usize| -> String {
            if is_ul {
                "- ".to_string()
            } else {
                format!("{:0width$}. ", start_at + i as i64, width = width)
            }
        };
        let indent = " ".repeat(prefix(0).chars().count());
        let indented_nl = format!("{CODE_NL}{indent}");

        w.push_str("\n\n");
        for (i, item) in items.iter().enumerate() {
            w.push_str(&prefix(i));
            let item = trim_consecutive_newlines(item);
            let item = trim_unnecessary_hard_line_breaks(&item);
            let item = unescape_content(&item);
            let lines: Vec<&str> = item.split('\n').collect();
            for (j, line) in lines.iter().enumerate() {
                if j != 0 {
                    w.push_str(&indent);
                }
                w.push_str(&line.replace(CODE_NL, &indented_nl));
                if j < lines.len() - 1 {
                    w.push('\n');
                }
            }
            if i < items.len() - 1 {
                w.push('\n');
            }
        }
        w.push_str("\n\n");
        true
    }

    fn render_blockquote(&self, ctx: Ctx, w: &mut String, n: Id) -> bool {
        let content = self.render_to_string(ctx, n, true);
        let content = trim_space(&content);
        if content.is_empty() {
            return true;
        }
        let content = trim_consecutive_newlines(content);
        let content = trim_unnecessary_hard_line_breaks(&content);
        w.push_str("\n\n");
        w.push_str(&prefix_lines(&content, "> "));
        w.push_str("\n\n");
        true
    }

    fn render_heading(&self, ctx: Ctx, w: &mut String, n: Id) -> bool {
        let level = self.dom.name(n)[1..].parse::<usize>().unwrap_or(6);
        let content = self.render_to_string(ctx, n, true);
        if trim_space(&content).is_empty() {
            return true;
        }
        let content = content.replace(['\n', '\r'], " ");
        let mut collapsed = String::new();
        for c in content.chars() {
            if !(c == ' ' && collapsed.ends_with(' ')) {
                collapsed.push(c);
            }
        }
        let mut bytes = trim_space(&collapsed).as_bytes().to_vec();
        let len = bytes.len();
        if len >= 2
            && bytes[len - 1] == b'#'
            && !(len >= 3 && bytes[len - 3] == b'\\')
            && bytes[len - 2].is_ascii()
        {
            bytes[len - 2] = b'\\';
        }
        w.push_str("\n\n");
        w.push_str(&"#".repeat(level));
        w.push(' ');
        w.push_str(&String::from_utf8_lossy(&bytes));
        w.push_str("\n\n");
        true
    }

    fn render_image(&self, w: &mut String, n: Id) -> bool {
        let d = self.dom;
        let src = trim_space(d.attr_or(n, "src", ""));
        if src.is_empty() {
            return false;
        }
        let src = assemble_absolute_url(src);
        let title = d.attr_or(n, "title", "").replace('\n', " ");
        let alt = d.attr_or(n, "alt", "").replace('\n', " ");
        let mut escaped_alt = String::new();
        let mut prev: Option<char> = None;
        for c in alt.chars() {
            if (c == '[' || c == ']') && prev != Some('\\') {
                escaped_alt.push('\\');
            }
            escaped_alt.push(c);
            prev = Some(c);
        }
        w.push_str("![");
        w.push_str(&escaped_alt);
        w.push_str("](");
        w.push_str(&src);
        if !title.is_empty() {
            w.push(' ');
            w.push_str(&surround_by_quotes(&title));
        }
        w.push(')');
        true
    }

    fn render_link(&self, _ctx: Ctx, w: &mut String, n: Id) -> bool {
        let d = self.dom;
        let ctx = Ctx { inside_link: true };
        let href = assemble_absolute_url(trim_space(d.attr_or(n, "href", "")));
        let mut title = d.attr_or(n, "title", "").replace('\n', " ");
        let content = self.render_to_string(ctx, n, true);
        if href.is_empty() {
            title.clear();
        }
        let (left, trimmed, right) = surrounding_spaces(&content);
        let trimmed = escape_multi_line(&trim_consecutive_newlines(trimmed));
        w.push_str(left);
        w.push('[');
        w.push_str(&trimmed);
        w.push_str("](");
        w.push_str(&href);
        if !title.is_empty() {
            w.push(' ');
            w.push_str(&surround_by_quotes(&title));
        }
        w.push(')');
        w.push_str(right);
        true
    }

    // ----- table plugin (minimal cell padding, newlines preserved) -----

    fn render_table(&self, ctx: Ctx, w: &mut String, n: Id) -> bool {
        let Some((alignments, mut rows, caption)) = self.collect_table(ctx, n) else {
            return false;
        };
        let mut counts: Vec<usize> = Vec::new();
        for cells in &rows {
            for (i, cell) in cells.iter().enumerate() {
                if i >= counts.len() {
                    counts.push(1);
                }
                counts[i] = counts[i].max(cell.chars().count());
            }
        }
        for cells in rows.iter_mut() {
            while cells.len() < counts.len() {
                cells.push(String::new());
            }
        }
        let write_row = |w: &mut String, cells: &[String]| {
            for (i, cell) in cells.iter().enumerate() {
                if i == 0 {
                    w.push('|');
                }
                w.push(' ');
                w.push_str(cell);
                w.push_str(" |");
            }
        };
        w.push_str("\n\n");
        write_row(w, &rows[0]);
        w.push('\n');
        for i in 0..counts.len() {
            let align = alignments.get(i).map(String::as_str).unwrap_or("");
            if i == 0 {
                w.push('|');
            }
            w.push(if align == "left" || align == "center" { ':' } else { '-' });
            w.push('-');
            w.push(if align == "right" || align == "center" { ':' } else { '-' });
            w.push('|');
        }
        w.push('\n');
        for cells in &rows[1..] {
            write_row(w, cells);
            w.push('\n');
        }
        if let Some(c) = caption {
            w.push_str("\n\n");
            w.push_str(&c);
        }
        w.push_str("\n\n");
        true
    }

    #[allow(clippy::type_complexity)]
    fn collect_table(
        &self,
        ctx: Ctx,
        n: Id,
    ) -> Option<(Vec<String>, Vec<Vec<String>>, Option<String>)> {
        let d = self.dom;
        if d.attr_or(n, "role", "") == "presentation" {
            return None;
        }
        let problematic_child = d
            .find_first(n, |d, c| {
                let name = d.name(c);
                name_is_heading(name)
                    || matches!(name, "table" | "hr" | "ul" | "ol" | "blockquote")
            })
            .is_some();
        if problematic_child {
            return None;
        }
        let mut p = d.parent(n);
        while let Some(pid) = p {
            if matches!(d.name(pid), "a" | "strong" | "b" | "em" | "i" | "del" | "s" | "strike") {
                return None;
            }
            p = d.parent(pid);
        }

        let header = d
            .find_first(n, |d, c| d.name(c) == "thead")
            .and_then(|thead| d.find_first(thead, |d, c| d.name(c) == "tr"))
            .or_else(|| {
                d.find_first(n, |d, c| d.name(c) == "th")
                    .and_then(|th| d.parent(th))
            });
        let normal: Vec<Id> = d
            .all_nodes(n)
            .into_iter()
            .filter(|&c| d.name(c) == "tr" && Some(c) != header)
            .collect();

        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut grouped_mods: Vec<Vec<(usize, usize, String)>> = Vec::new();
        match header {
            Some(h) => {
                let (cells, mods) = self.collect_cells(ctx, 0, h);
                rows.push(cells);
                grouped_mods.push(mods);
            }
            None => rows.push(Vec::new()),
        }
        for (i, r) in normal.iter().enumerate() {
            let (cells, mods) = self.collect_cells(ctx, i + 1, *r);
            rows.push(cells);
            grouped_mods.push(mods);
        }
        for mods in grouped_mods.into_iter().rev() {
            for (y, x, data) in mods {
                while rows.len() <= y {
                    rows.push(Vec::new());
                }
                while x >= 1 && rows[y].len() < x {
                    rows[y].push(String::new());
                }
                let at = x.min(rows[y].len());
                rows[y].insert(at, data);
            }
        }
        for cells in rows.iter_mut() {
            for cell in cells.iter_mut() {
                if cell.contains('\n') {
                    *cell = cell.replace('\n', "<br />");
                }
            }
        }

        let first_row = header.or_else(|| normal.first().copied());
        let alignments = first_row
            .map(|r| {
                d.find_all(r, |d, c| matches!(d.name(c), "th" | "td"))
                    .into_iter()
                    .map(|c| d.attr_or(c, "align", "").to_string())
                    .collect()
            })
            .unwrap_or_default();
        let caption = d
            .find_first(n, |d, c| d.name(c) == "caption")
            .map(|c| trim_space(&self.render_to_string(ctx, c, false)).to_string());
        Some((alignments, rows, caption))
    }

    fn collect_cells(
        &self,
        ctx: Ctx,
        row_index: usize,
        row: Id,
    ) -> (Vec<String>, Vec<(usize, usize, String)>) {
        let d = self.dom;
        let mut cells = Vec::new();
        let mut mods = Vec::new();
        for (i, cell) in d
            .find_all(row, |d, c| matches!(d.name(c), "th" | "td"))
            .into_iter()
            .enumerate()
        {
            let content = self.render_to_string(ctx, cell, false);
            let content = trim_space(&content).replace("\x07|", "\\|");
            let content = unescape_content(&content);
            let span = |key: &str| -> usize {
                d.attr(cell, key)
                    .and_then(|v| v.parse::<usize>().ok())
                    .filter(|&v| v >= 1)
                    .unwrap_or(1)
            };
            let (row_span, col_span) = (span("rowspan"), span("colspan"));
            if col_span > 1 || row_span > 1 {
                for dx in 1..col_span {
                    mods.push((row_index, i + dx, String::new()));
                }
                for dy in 1..row_span {
                    for dx in 0..col_span {
                        mods.push((row_index + dy, i + dx, String::new()));
                    }
                }
            }
            cells.push(content);
        }
        (cells, mods)
    }
}

fn code_language(d: &Dom, n: Id) -> String {
    for part in d.attr_or(n, "class", "").split(' ') {
        if part.contains("language-") || part.contains("lang-") {
            return part.replacen("language-", "", 1).replacen("lang-", "", 1);
        }
    }
    String::new()
}

fn assemble_absolute_url(raw: &str) -> String {
    let raw = trim_space(raw);
    if raw == "#" {
        return raw.to_string();
    }
    raw.replace('\n', "%0A")
        .replace('\t', "%09")
        .replace(' ', "%20")
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('<', "%3C")
        .replace('>', "%3E")
}

fn superscript(c: char) -> char {
    match c {
        '0' => '\u{2070}', '1' => '\u{00b9}', '2' => '\u{00b2}', '3' => '\u{00b3}',
        '4' => '\u{2074}', '5' => '\u{2075}', '6' => '\u{2076}', '7' => '\u{2077}',
        '8' => '\u{2078}', '9' => '\u{2079}', 'a' => '\u{1D43}', 'b' => '\u{1D47}',
        'c' => '\u{1D9C}', 'd' => '\u{1D48}', 'e' => '\u{1D49}', 'f' => '\u{1DA0}',
        'g' => '\u{1D4D}', 'h' => '\u{02B0}', 'i' => '\u{2071}', 'j' => '\u{02B2}',
        'k' => '\u{1D4F}', 'l' => '\u{02E1}', 'm' => '\u{1D50}', 'n' => '\u{207F}',
        'o' => '\u{1D52}', 'p' => '\u{1D56}', 'q' => '\u{02A0}', 'r' => '\u{02B3}',
        's' => '\u{02E2}', 't' => '\u{1D57}', 'u' => '\u{1D58}', 'v' => '\u{1D5B}',
        'w' => '\u{02B7}', 'x' => '\u{02E3}', 'y' => '\u{02B8}', 'z' => '\u{1DBB}',
        '+' => '\u{207A}', '-' => '\u{207B}', '=' => '\u{207C}', '(' => '\u{207D}',
        ')' => '\u{207E}',
        other => other,
    }
}

fn subscript(c: char) -> char {
    match c {
        '0' => '\u{2080}', '1' => '\u{2081}', '2' => '\u{2082}', '3' => '\u{2083}',
        '4' => '\u{2084}', '5' => '\u{2085}', '6' => '\u{2086}', '7' => '\u{2087}',
        '8' => '\u{2088}', '9' => '\u{2089}', 'a' => '\u{2090}', 'e' => '\u{2091}',
        'h' => '\u{2095}', 'i' => '\u{1d62}', 'j' => '\u{2c7c}', 'k' => '\u{2096}',
        'l' => '\u{2097}', 'm' => '\u{2098}', 'n' => '\u{2099}', 'o' => '\u{2092}',
        'p' => '\u{209a}', 'r' => '\u{1d63}', 's' => '\u{209b}', 't' => '\u{209c}',
        'u' => '\u{1d64}', 'v' => '\u{1d65}', 'x' => '\u{2093}', 'y' => '\u{1d67}',
        '+' => '\u{208A}', '-' => '\u{208B}', '=' => '\u{208C}', '(' => '\u{208D}',
        ')' => '\u{208E}',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::html_to_markdown;

    // Cases copied from leetgo's leetcode/question_test.go (before wrapping).
    #[test]
    fn subsets_keyword_spacing() {
        let html = r#"<p>return <em>all possible</em> <span data-keyword="subset"><em>subsets</em></span> <em>(the power set)</em>.</p>"#;
        assert_eq!(html_to_markdown(html), "return *all possible* *subsets* *(the power set)*.");
    }

    #[test]
    fn inline_code_spacing() {
        let html = r#"<p>return <code>true</code> <em>if</em> <code>s</code> <em>is <strong>valid</strong></em>.</p>"#;
        assert_eq!(html_to_markdown(html), "return `true` *if* `s` *is **valid***.");
    }

    #[test]
    fn sub_and_superscripts() {
        let html = "<p>a<sub>1</sub> &lt;= 10<sup>2</sup></p><p><code>a<sub>1</sub> &lt;= 10<sup>2</sup></code></p><pre><strong>Output:</strong> 10<sup>2</sup>\n</pre><p>Explanation</p>";
        assert_eq!(
            html_to_markdown(html),
            "a₁ &lt;= 10²\n\n`a₁ <= 10²`\n\n```\nOutput: 10²\n```\n\nExplanation"
        );
    }

    #[test]
    fn example_indentation() {
        let html = "<pre><strong>Input:</strong> tree =\n  1\n / \\\n2   3\n</pre><p>Explanation</p>";
        assert_eq!(
            html_to_markdown(html),
            "```\nInput: tree =\n  1\n / \\\n2   3\n```\n\nExplanation"
        );
    }

    #[test]
    fn table_with_strikethrough_and_breaks() {
        let html = "<table><thead><tr><th>State</th><th>Value</th></tr></thead><tbody><tr><td><del>old</del><br>new</td><td><code>0</code></td></tr></tbody></table>";
        assert_eq!(
            html_to_markdown(html),
            "| State | Value |\n|---|---|\n| ~~old~~  <br />new | `0` |"
        );
    }

    #[test]
    fn task_list_checkboxes() {
        let html = r#"<ul><li><input type="checkbox" checked disabled>Read the input</li><li><input type="checkbox" disabled> Return the result</li></ul>"#;
        assert_eq!(html_to_markdown(html), "- [x] Read the input\n- [ ] Return the result");
    }

    #[test]
    fn entities_and_invisible_characters() {
        let html = "<p>所有&nbsp;<code>nums</code>&#8203; 的值都满足 <code>0 &lt;= nums[i]</code>。</p>";
        assert_eq!(html_to_markdown(html), "所有 `nums` 的值都满足 `0 <= nums[i]`。");
    }
}

//! Text helpers and markdown-escaping, ported from html-to-markdown v2
//! (`internal/textutils`, `internal/escape`, `converter/escape.go`).

/// Placeholder written in front of every potentially special character;
/// later either removed or turned into a backslash.
pub const ESC: u8 = 0x07;
pub const ESC_CHAR: char = '\x07';
/// Stands in for newlines inside fenced code blocks until the very end.
pub const CODE_NL: char = '\u{F002}';

pub fn trim_space(s: &str) -> &str {
    s.trim_matches(char::is_whitespace)
}

/// `textutils.SurroundingSpaces`
pub fn surrounding_spaces(s: &str) -> (&str, &str, &str) {
    let right_trimmed = s.trim_end_matches(char::is_whitespace);
    let right = &s[right_trimmed.len()..];
    let trimmed = right_trimmed.trim_start_matches(char::is_whitespace);
    let left = &s[..right_trimmed.len() - trimmed.len()];
    (left, trimmed, right)
}

/// `textutils.DelimiterForEveryLine`
pub fn delimiter_for_every_line(text: &str, delim: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let (l, t, r) = surrounding_spaces(line);
        out.push_str(l);
        if !t.is_empty() {
            out.push_str(delim);
            out.push_str(t);
            out.push_str(delim);
        }
        out.push_str(r);
        if i < lines.len() - 1 {
            out.push('\n');
        }
    }
    out
}

/// `textutils.TrimConsecutiveNewlines`
pub fn trim_consecutive_newlines(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut newlines = 0;
    let mut spaces = String::new();
    for c in input.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push_str(&spaces);
                out.push('\n');
            }
            spaces.clear();
        } else if c == ' ' {
            spaces.push(c);
        } else {
            newlines = 0;
            out.push_str(&spaces);
            out.push(c);
            spaces.clear();
        }
    }
    out.push_str(&spaces);
    out
}

/// `textutils.TrimUnnecessaryHardLineBreaks`
pub fn trim_unnecessary_hard_line_breaks(s: &str) -> String {
    s.replace("  \n\n", "\n\n")
        .replace("  \n  \n", "\n\n")
        .replace("  \n \n", "\n\n")
}

/// `textutils.EscapeMultiLine`
pub fn escape_multi_line(content: &str) -> String {
    let parts: Vec<&str> = content.split('\n').collect();
    if parts.len() == 1 {
        return content.to_string();
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        let t = part.trim_start_matches(char::is_whitespace);
        if t.is_empty() {
            out.push_str("\\\n");
        } else if i == parts.len() - 1 {
            out.push_str(t);
        } else if t.ends_with("  ") {
            out.push_str(t);
            out.push('\n');
        } else {
            out.push_str(t);
            out.push_str("  \n");
        }
    }
    out
}

pub fn prefix_lines(s: &str, prefix: &str) -> String {
    let mut out = String::from(prefix);
    for c in s.chars() {
        out.push(c);
        if c == '\n' {
            out.push_str(prefix);
        }
    }
    out
}

pub fn surround_by_quotes(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let dq = s.contains('"');
    let sq = s.contains('\'');
    if dq && sq {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else if dq {
        format!("'{s}'")
    } else {
        format!("\"{s}\"")
    }
}

pub fn code_fence_occurrences(fence: char, content: &str) -> usize {
    let mut max = 0;
    let mut run = 0;
    for c in content.chars() {
        if c == fence {
            run += 1;
        } else {
            max = max.max(run);
            run = 0;
        }
    }
    max.max(run)
}

pub fn calculate_code_fence(fence: char, content: &str) -> String {
    let n = (code_fence_occurrences(fence, content) + 1).max(3);
    fence.to_string().repeat(n)
}

/// `textutils.CollapseInlineCodeContent`
pub fn collapse_inline_code(s: &str) -> String {
    let s = s.replace(['\n', '\t'], " ");
    let s = trim_space(&s);
    let mut out = String::new();
    let mut count = 0;
    for c in s.chars() {
        if c == ' ' {
            count += 1;
        } else {
            count = 0;
        }
        if count > 1 {
            continue;
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

fn is_markdown_char(c: char) -> bool {
    matches!(
        c,
        '\\' | '*'
            | '_'
            | '-'
            | '+'
            | '.'
            | '>'
            | '|'
            | '$'
            | '#'
            | '='
            | '['
            | ']'
            | '('
            | ')'
            | '!'
            | '~'
            | '`'
            | '"'
            | '\''
    )
}

pub fn escape_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if c == '\0' {
            out.push('\u{FFFD}');
        } else if is_markdown_char(c) {
            out.push(ESC_CHAR);
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

fn go_is_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b' ' | 0x85 | 0xA0)
}

fn next_byte(chars: &[u8], index: usize) -> u8 {
    chars[index + 1..]
        .iter()
        .copied()
        .find(|&b| b != ESC)
        .unwrap_or(0)
}

fn next_rune(chars: &[u8], index: usize) -> Option<char> {
    let i = (index + 1..chars.len()).find(|&i| chars[i] != ESC)?;
    let s = String::from_utf8_lossy(&chars[i..(i + 4).min(chars.len())]).into_owned();
    s.chars().next()
}

fn prev_rune(chars: &[u8], index: usize) -> Option<char> {
    let i = (0..index).rev().find(|&i| chars[i] != ESC)?;
    let start = i.saturating_sub(3);
    let s = String::from_utf8_lossy(&chars[start..=i]).into_owned();
    s.chars().last()
}

/// Walk back to the start of the line; true if only spaces/placeholders precede.
fn only_spaces_before(chars: &[u8], index: usize, allow_digits: bool) -> bool {
    for i in (0..index).rev() {
        match chars[i] {
            b'\n' => return true,
            b' ' | ESC => continue,
            b if allow_digits && b.is_ascii_digit() => continue,
            _ => return false,
        }
    }
    true
}

fn is_italic_or_bold(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'*' && chars[index] != b'_' {
        return -1;
    }
    match next_rune(chars, index) {
        None => -1,
        Some(c) if c.is_whitespace() => -1,
        _ => 1,
    }
}

fn is_block_quote(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'>' || !only_spaces_before(chars, index, false) {
        return -1;
    }
    1
}

fn is_atx_header(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'#' || !only_spaces_before(chars, index, false) {
        return -1;
    }
    let mut n = 1;
    for (i, &b) in chars.iter().enumerate().skip(index + 1) {
        match b {
            b'#' => {
                n += 1;
                if n > 6 {
                    return -1;
                }
            }
            ESC => {}
            b' ' | b'\t' | b'\n' | b'\r' => return (i - index) as i64,
            _ => return -1,
        }
    }
    1
}

fn is_setext_header(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'=' && chars[index] != b'-' {
        return -1;
    }
    let mut newlines = 0;
    for i in (0..index).rev() {
        match chars[i] {
            ESC | b' ' => continue,
            b'\n' => newlines += 1,
            _ => return if newlines == 1 { 1 } else { -1 },
        }
    }
    -1
}

fn is_divider(chars: &[u8], index: usize) -> i64 {
    let c = chars[index];
    if c != b'-' && c != b'_' && c != b'*' {
        return -1;
    }
    if !only_spaces_before(chars, index, false) {
        return -1;
    }
    let mut count = 1;
    let mut last = chars.len();
    for (i, &b) in chars.iter().enumerate().skip(index + 1) {
        match b {
            ESC | b' ' => continue,
            b if b == c => count += 1,
            b'\n' => {
                last = i;
                break;
            }
            _ => return -1,
        }
    }
    if count >= 3 {
        (last - index) as i64
    } else {
        -1
    }
}

fn is_ordered_list(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'.' && chars[index] != b')' {
        return -1;
    }
    match prev_rune(chars, index) {
        Some(c) if c.is_ascii_digit() => {}
        _ => return -1,
    }
    if !only_spaces_before(chars, index, true) {
        return -1;
    }
    let next = next_byte(chars, index);
    if go_is_space(next) || next == 0 {
        1
    } else {
        -1
    }
}

fn is_unordered_list(chars: &[u8], index: usize) -> i64 {
    if !matches!(chars[index], b'-' | b'*' | b'+') || !only_spaces_before(chars, index, false) {
        return -1;
    }
    let next = next_byte(chars, index);
    if go_is_space(next) || next == 0 {
        1
    } else {
        -1
    }
}

fn is_image_or_link(chars: &[u8], index: usize) -> i64 {
    match chars[index] {
        b'!' => {
            if index + 1 < chars.len() && chars[index + 1] == b'[' {
                1
            } else {
                -1
            }
        }
        b'[' => {
            for &b in &chars[index + 1..] {
                if b == b'\n' {
                    return -1;
                }
                if b == b']' {
                    return 1;
                }
            }
            -1
        }
        _ => -1,
    }
}

fn is_fenced_code(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'`' && chars[index] != b'~' {
        return -1;
    }
    for i in (0..index).rev() {
        match chars[i] {
            b' ' | ESC => continue,
            b'\n' => break,
            _ => return -1,
        }
    }
    let mut count = 1;
    let mut i = index + 1;
    while i < chars.len() {
        match chars[i] {
            ESC => {}
            b'`' | b'~' => count += 1,
            _ => break,
        }
        i += 1;
    }
    if count < 3 { -1 } else { (i - index) as i64 }
}

fn is_inline_code(chars: &[u8], index: usize) -> i64 {
    if chars[index] == b'`' { 1 } else { -1 }
}

fn is_backslash(chars: &[u8], index: usize) -> i64 {
    if chars[index] == b'\\' { 1 } else { -1 }
}

fn is_strikethrough(chars: &[u8], index: usize) -> i64 {
    if chars[index] != b'~' {
        return -1;
    }
    match next_rune(chars, index) {
        None => -1,
        Some(c) if c.is_whitespace() => -1,
        _ => 1,
    }
}

const UNESCAPERS: &[fn(&[u8], usize) -> i64] = &[
    is_italic_or_bold,
    is_block_quote,
    is_atx_header,
    is_setext_header,
    is_divider,
    is_ordered_list,
    is_unordered_list,
    is_image_or_link,
    is_fenced_code,
    is_inline_code,
    is_backslash,
    is_strikethrough,
];

/// `converter.unEscapeContent`: placeholders become `\` where the following
/// character would otherwise be interpreted as markdown, and vanish elsewhere.
pub fn unescape_content(s: &str) -> String {
    let chars = s.as_bytes();
    let mut escape_at = vec![false; chars.len()];
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != ESC {
            index += 1;
            continue;
        }
        if index + 1 >= chars.len() {
            break;
        }
        let skip = UNESCAPERS
            .iter()
            .map(|f| f(chars, index + 1))
            .find(|&r| r != -1)
            .unwrap_or(-1);
        if skip == -1 {
            index += 1;
            continue;
        }
        escape_at[index] = true;
        index += skip as usize;
    }
    let mut out = Vec::with_capacity(chars.len());
    for (i, &b) in chars.iter().enumerate() {
        if b != ESC {
            out.push(b);
        } else if escape_at[i] {
            out.push(b'\\');
        }
    }
    String::from_utf8(out).expect("only ASCII bytes were changed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_keeps_link_bracket_escape() {
        let s = escape_content("[1,2]");
        assert_eq!(unescape_content(&s), "\\[1,2]");
    }

    #[test]
    fn unescape_drops_harmless_placeholders() {
        let s = escape_content("a.b (c)");
        assert_eq!(unescape_content(&s), "a.b (c)");
    }

    #[test]
    fn trim_consecutive_newlines_caps_at_two() {
        assert_eq!(trim_consecutive_newlines("a\n\n\n\nb"), "a\n\nb");
    }
}

//! Port of `github.com/muesli/reflow/wordwrap` (v0.3.0) with its defaults:
//! `-` is a breakpoint and existing newlines are kept. Quirks are preserved on
//! purpose (e.g. a `-` does not count towards the line length) so wrapped
//! descriptions match leetgo's.

use unicode_width::UnicodeWidthStr;

pub fn wrap(s: &str, limit: usize) -> String {
    let mut w = Wrapper { limit, buf: String::new(), space: String::new(), word: String::new(), line_len: 0 };
    for c in s.chars() {
        w.write(c);
    }
    w.add_word();
    w.buf
}

struct Wrapper {
    limit: usize,
    buf: String,
    space: String,
    word: String,
    line_len: usize,
}

impl Wrapper {
    fn word_width(&self) -> usize {
        self.word.width()
    }

    fn add_space(&mut self) {
        self.line_len += self.space.len();
        self.buf.push_str(&self.space);
        self.space.clear();
    }

    fn add_word(&mut self) {
        if !self.word.is_empty() {
            self.add_space();
            self.line_len += self.word_width();
            self.buf.push_str(&self.word);
            self.word.clear();
        }
    }

    fn add_newline(&mut self) {
        self.buf.push('\n');
        self.line_len = 0;
        self.space.clear();
    }

    fn write(&mut self, c: char) {
        if c == '\n' {
            if self.word.is_empty() {
                if self.line_len + self.space.len() > self.limit {
                    self.line_len = 0;
                } else {
                    self.buf.push_str(&self.space);
                }
                self.space.clear();
            }
            self.add_word();
            self.add_newline();
        } else if c.is_whitespace() {
            self.add_word();
            self.space.push(c);
        } else if c == '-' {
            self.add_space();
            self.add_word();
            self.buf.push(c);
        } else {
            self.word.push(c);
            let ww = self.word_width();
            if self.line_len + self.space.len() + ww > self.limit && ww < self.limit {
                self.add_newline();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wraps_at_limit() {
        assert_eq!(wrap("aaa bbb ccc", 7), "aaa bbb\nccc");
    }

    #[test]
    fn keeps_newlines() {
        assert_eq!(wrap("a\nb", 10), "a\nb");
    }

    #[test]
    fn hyphen_is_breakpoint() {
        assert_eq!(wrap("foo-bar", 5), "foo-\nbar");
    }
}

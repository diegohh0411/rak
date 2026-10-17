//! The stdout protocol every generated solution follows: anything the user
//! prints for debugging, then a final `output: <value>` line (the C++ `main()`
//! writes `"\noutput: " << value`).

const MARKER: &str = "output:";

/// A solution's stdout split into the user's debug prints and the answer.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed<'a> {
    pub debug: &'a str,
    pub answer: &'a str,
}

/// Split stdout at the last line starting with `output:`. Returns `None` when
/// the program never printed an answer.
pub fn parse(stdout: &str) -> Option<Parsed<'_>> {
    let mut start = stdout.len();
    loop {
        let line_start = stdout[..start].rfind('\n').map_or(0, |i| i + 1);
        let line = &stdout[line_start..start];
        if let Some(rest) = line.strip_prefix(MARKER) {
            let rest = rest.trim();
            return Some(Parsed {
                debug: stdout[..line_start].trim_end_matches(['\n', '\r']),
                answer: rest,
            });
        }
        if line_start == 0 {
            return None;
        }
        start = line_start - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_only() {
        assert_eq!(
            parse("\noutput: [0,1]\n"),
            Some(Parsed {
                debug: "",
                answer: "[0,1]"
            })
        );
    }

    #[test]
    fn keeps_debug_prints() {
        assert_eq!(
            parse("i=0\ni=1\n\noutput: 42\n"),
            Some(Parsed {
                debug: "i=0\ni=1",
                answer: "42"
            })
        );
    }

    #[test]
    fn uses_last_marker() {
        let p = parse("output: fake\n\noutput: true\n").unwrap();
        assert_eq!(p.debug, "output: fake");
        assert_eq!(p.answer, "true");
    }

    #[test]
    fn marker_without_trailing_newline_or_with_crlf() {
        assert_eq!(parse("output: 1").unwrap().answer, "1");
        assert_eq!(parse("x\r\noutput: 1\r\n").unwrap().debug, "x");
    }

    #[test]
    fn missing_marker() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("hello\nworld\n"), None);
    }
}

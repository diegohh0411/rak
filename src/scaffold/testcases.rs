//! `testcases.txt` generation, matching leetgo's `generateTestCasesContent`,
//! and parsing it back for local testing.

use super::question::Question;

/// One `input:` / `output:` block of `testcases.txt`.
#[allow(dead_code)] // used by `rak test` once the command lands
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// One argument per line, newline-terminated, ready to feed to stdin.
    pub input: String,
    /// `None` when the block has no expected output.
    pub expected: Option<String>,
}

/// Parse `testcases.txt`. Blank lines are ignored; a block runs from its
/// `input:` line until the next `input:` line.
#[allow(dead_code)] // used by `rak test` once the command lands
pub fn parse(content: &str) -> Result<Vec<Case>, String> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Input,
        Output,
    }

    let mut cases = Vec::new();
    let mut input: Vec<&str> = Vec::new();
    let mut output: Vec<&str> = Vec::new();
    let mut section = Section::None;

    let mut flush = |input: &mut Vec<&str>, output: &mut Vec<&str>| {
        let expected = output.join("\n");
        cases.push(Case {
            input: input.iter().map(|l| format!("{l}\n")).collect(),
            expected: (!expected.is_empty()).then_some(expected),
        });
        input.clear();
        output.clear();
    };

    for (n, raw) in content.lines().enumerate() {
        let line = raw.trim();
        match line {
            "input:" => {
                if section != Section::None {
                    flush(&mut input, &mut output);
                }
                section = Section::Input;
            }
            "output:" => {
                if section != Section::Input {
                    return Err(format!(
                        "testcases.txt line {}: 'output:' without a preceding 'input:'",
                        n + 1
                    ));
                }
                section = Section::Output;
            }
            "" => {}
            _ => match section {
                Section::None => {
                    return Err(format!(
                        "testcases.txt line {}: expected 'input:', found {line:?}",
                        n + 1
                    ));
                }
                Section::Input => input.push(line),
                Section::Output => output.push(line),
            },
        }
    }
    if section != Section::None {
        flush(&mut input, &mut output);
    }
    Ok(cases)
}

pub fn generate(q: &Question) -> String {
    let cases = q.example_test_cases();
    let outputs = q.example_outputs();
    let n = q.meta.n_arg();
    if n == 0 {
        return String::new();
    }

    let mut blocks = Vec::new();
    let mut i = 0;
    while i < cases.len() && i / n < outputs.len() {
        let input = &cases[i..(i + n).min(cases.len())];
        let mut output = outputs[i / n].clone();
        if !output_is_valid(q, input, &output) {
            output.clear();
        }
        let mut block = String::from("input:\n");
        let joined = input.join("\n");
        block.push_str(&joined);
        if !joined.ends_with('\n') {
            block.push('\n');
        }
        block.push_str("output:\n");
        block.push_str(&output);
        block.push('\n');
        blocks.push(block);
        i += n;
    }
    blocks.join("\n")
}

/// leetgo `checkOutput`: drop example outputs that don't parse as the result type.
fn output_is_valid(q: &Question, input: &[String], output: &str) -> bool {
    if output.is_empty() {
        return false;
    }
    if q.meta.systemdesign {
        let Some(out) = split_array(output) else {
            return false;
        };
        let n_in = input
            .first()
            .and_then(|s| split_array(s))
            .map_or(0, |v| v.len());
        return out.len() == n_in;
    }
    is_valid_value(&q.meta.result_type(), output.trim())
}

fn split_array(raw: &str) -> Option<Vec<String>> {
    let raw = raw.trim();
    if raw.len() <= 1 || !raw.starts_with('[') || !raw.ends_with(']') {
        return None;
    }
    let items: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
    Some(items.iter().map(|v| v.to_string()).collect())
}

fn is_valid_value(ty: &str, raw: &str) -> bool {
    let raw = raw.trim();
    if let Some(elem) = ty.strip_suffix("[]") {
        return split_array(raw).is_some_and(|items| items.iter().all(|i| is_valid_value(elem, i)));
    }
    match ty {
        "integer" | "long" => raw.parse::<i64>().is_ok(),
        "double" => raw.parse::<f64>().is_ok(),
        "boolean" => raw == "true" || raw == "false",
        "character" => {
            let b = raw.as_bytes();
            b.len() == 3 && (b[0] == b'"' || b[0] == b'\'') && b[2] == b[0]
        }
        "string" => serde_json::from_str::<String>(raw).is_ok(),
        "TreeNode" | "ListNode" => split_array(raw).is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_values() {
        assert!(is_valid_value("integer[]", "[0,1]"));
        assert!(!is_valid_value("integer", "[0,1]"));
        assert!(is_valid_value("string", "\"abc\""));
        assert!(is_valid_value("integer[][]", "[[1,2],[3]]"));
        assert!(!is_valid_value("void", "1"));
    }

    #[test]
    fn parses_generated_format() {
        let content =
            "input:\n[2,7,11,15]\n9\noutput:\n[0,1]\n\ninput:\n[3,2,4]\n6\noutput:\n[1,2]\n";
        let cases = parse(content).unwrap();
        assert_eq!(
            cases,
            vec![
                Case {
                    input: "[2,7,11,15]\n9\n".into(),
                    expected: Some("[0,1]".into()),
                },
                Case {
                    input: "[3,2,4]\n6\n".into(),
                    expected: Some("[1,2]".into()),
                },
            ]
        );
    }

    #[test]
    fn parses_missing_output_and_crlf() {
        let content = "input:\r\n\"abc\"\r\noutput:\r\n\r\ninput:\r\n\"x\"\r\n";
        let cases = parse(content).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].input, "\"abc\"\n");
        assert_eq!(cases[0].expected, None);
        assert_eq!(cases[1].input, "\"x\"\n");
        assert_eq!(cases[1].expected, None);
    }

    #[test]
    fn parse_empty_file_has_no_cases() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("\n\n").unwrap().is_empty());
    }

    #[test]
    fn parse_rejects_stray_content() {
        assert!(parse("[1,2]\noutput:\n3\n").is_err());
        assert!(parse("output:\n3\n").is_err());
    }
}

//! `testcases.txt` generation, matching leetgo's `generateTestCasesContent`.

use super::question::Question;

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
        let Some(out) = split_array(output) else { return false };
        let n_in = input.first().and_then(|s| split_array(s)).map_or(0, |v| v.len());
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
}

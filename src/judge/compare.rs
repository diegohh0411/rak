//! Comparing an expected answer with a solution's answer. Both sides are
//! LeetCode-style JSON values; comparison ignores formatting and accepts
//! floating-point answers within LeetCode's 1e-5 tolerance.

use serde_json::{Number, Value};

const EPSILON: f64 = 1e-5;

pub fn outputs_match(expected: &str, actual: &str) -> bool {
    match (
        serde_json::from_str::<Value>(expected),
        serde_json::from_str::<Value>(actual),
    ) {
        (Ok(e), Ok(a)) => values_match(&e, &a),
        _ => strip_whitespace(expected) == strip_whitespace(actual),
    }
}

fn values_match(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => numbers_match(e, a),
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(e, a)| values_match(e, a))
        }
        (Value::Object(e), Value::Object(a)) => {
            e.len() == a.len()
                && e.iter()
                    .all(|(k, e)| a.get(k).is_some_and(|a| values_match(e, a)))
        }
        _ => expected == actual,
    }
}

fn numbers_match(expected: &Number, actual: &Number) -> bool {
    if !expected.is_f64() && !actual.is_f64() {
        return expected == actual;
    }
    match (expected.as_f64(), actual.as_f64()) {
        (Some(e), Some(a)) => (e - a).abs() <= EPSILON * e.abs().max(1.0),
        _ => false,
    }
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_formatting() {
        assert!(outputs_match("[0, 1]", "[0,1]"));
        assert!(outputs_match("[[1,2],\n[3]]", "[[1,2],[3]]"));
        assert!(outputs_match("\"a b\"", "\"a b\""));
    }

    #[test]
    fn detects_differences() {
        assert!(!outputs_match("[0,1]", "[1,0]"));
        assert!(!outputs_match("[0,1]", "[0,1,2]"));
        assert!(!outputs_match("\"a b\"", "\"ab\""));
        assert!(!outputs_match("true", "false"));
        assert!(!outputs_match("1", "\"1\""));
    }

    #[test]
    fn integers_are_exact() {
        assert!(outputs_match("9007199254740993", "9007199254740993"));
        assert!(!outputs_match("9007199254740993", "9007199254740992"));
        assert!(outputs_match("-3", "-3"));
    }

    #[test]
    fn doubles_use_tolerance() {
        assert!(outputs_match("2.50000", "2.5"));
        assert!(outputs_match("2.00000", "2"));
        assert!(outputs_match("0.1", "0.10000000000000001"));
        assert!(outputs_match(
            "[1.00000,0.33333]",
            "[1,0.33333333333333331]"
        ));
        assert!(outputs_match("123456.7", "123456.75"));
        assert!(!outputs_match("2.5", "2.6"));
        assert!(!outputs_match("0.00001", "0.00003"));
    }

    #[test]
    fn tree_and_system_design_nulls() {
        assert!(outputs_match("[1,null,2]", "[1,null,2]"));
        assert!(!outputs_match("[1,null,2]", "[1,2]"));
        assert!(outputs_match("[null, null, 1]", "[null,null,1]"));
    }

    #[test]
    fn non_json_falls_back_to_text() {
        assert!(outputs_match("nan", "nan"));
        assert!(outputs_match("[a, b]", "[a,b]"));
        assert!(!outputs_match("nan", "inf"));
    }
}

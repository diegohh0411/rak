//! `rak test`: build the solution and run it against `testcases.txt`
//! locally, like `leetgo test --local`.

use std::time::{Duration, Instant};

use crate::config;
use crate::judge::process;
use crate::judge::runtime::{self, BuildCtx};
use crate::judge::verdict::{self, CaseResult, Verdict};
use crate::scaffold::testcases::{self, Case};

pub fn run(
    id: String,
    case: Option<usize>,
    timeout_ms: Option<u64>,
    lang: Option<String>,
) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;
    let workspace_dir = problem_dir
        .parent()
        .ok_or("problem folder has no parent directory")?
        .to_path_buf();

    let testcases_path = problem_dir.join("testcases.txt");
    let content = std::fs::read_to_string(&testcases_path)
        .map_err(|e| format!("failed to read {}: {e}", testcases_path.display()))?;
    let cases = select_cases(testcases::parse(&content)?, case)?;

    let lang = match lang {
        Some(l) => l,
        None => runtime::detect(&problem_dir)?,
    };
    let rt = runtime::get(&lang, &cfg.test.language(&lang))?;

    eprint!("Building {}... ", rt.source_file());
    let started = Instant::now();
    let build_dir = runtime::default_build_dir(&problem_dir);
    let exe = rt
        .build(&BuildCtx {
            problem_dir: &problem_dir,
            workspace_dir: &workspace_dir,
            build_dir: &build_dir,
        })
        .inspect_err(|_| eprintln!("failed"))
        .map_err(|e| e.to_string())?;
    eprintln!("done ({})", format_duration(started.elapsed()));

    let timeout = Duration::from_millis(timeout_ms.unwrap_or(cfg.test.timeout_ms));
    let mut failed = 0;
    for (n, case) in &cases {
        let (exec, elapsed) = process::run(&exe, &problem_dir, &case.input, timeout)?;
        let result = verdict::judge(case.expected.as_deref(), &exec);
        if result.verdict.is_failure() {
            failed += 1;
        }
        print!("{}", format_case(*n, case, &result, elapsed));
    }

    let total = cases.len();
    println!();
    if failed == 0 {
        println!("✓ Passed {total}/{total} test cases");
        Ok(())
    } else {
        Err(format!("{failed} of {total} test cases failed"))
    }
}

/// Number the cases from 1 and keep only `only` when given.
fn select_cases(cases: Vec<Case>, only: Option<usize>) -> Result<Vec<(usize, Case)>, String> {
    if cases.is_empty() {
        return Err("testcases.txt has no test cases".to_string());
    }
    let total = cases.len();
    let numbered = cases.into_iter().enumerate().map(|(i, c)| (i + 1, c));
    match only {
        None => Ok(numbered.collect()),
        Some(n) if (1..=total).contains(&n) => Ok(numbered.filter(|(i, _)| *i == n).collect()),
        Some(n) => Err(format!(
            "no test case {n}; testcases.txt has {total} (numbered from 1)"
        )),
    }
}

fn format_case(n: usize, case: &Case, result: &CaseResult, elapsed: Duration) -> String {
    let (mark, label) = match &result.verdict {
        Verdict::Accepted => ("✓", "Accepted"),
        Verdict::NoExpected { .. } => ("?", "No expected output"),
        Verdict::WrongAnswer { .. } => ("✗", "Wrong Answer"),
        Verdict::RuntimeError { .. } => ("✗", "Runtime Error"),
        Verdict::InputError { .. } => ("✗", "Invalid Input"),
        Verdict::TimeLimitExceeded => ("✗", "Time Limit Exceeded"),
    };
    let time = match result.verdict {
        Verdict::TimeLimitExceeded => format!(">{}", format_duration(elapsed)),
        _ => format_duration(elapsed),
    };
    let mut out = format!("{mark} Case {n}: {label} ({time})\n");

    let mut field = |name: &str, value: &str| {
        let mut lines = value.lines();
        let first = lines.next().unwrap_or("");
        out.push_str(&format!("    {name:<9} {first}\n"));
        for line in lines {
            out.push_str(&format!("    {:<9} {line}\n", ""));
        }
    };
    if result.verdict.is_failure() {
        field("input:", case.input.trim_end());
    }
    match &result.verdict {
        Verdict::WrongAnswer { expected, actual } => {
            field("expected:", expected);
            field("actual:", actual);
        }
        Verdict::NoExpected { actual } => field("output:", actual),
        Verdict::RuntimeError { reason } | Verdict::InputError { reason } => {
            field("error:", reason)
        }
        Verdict::Accepted | Verdict::TimeLimitExceeded => {}
    }
    if !result.debug.trim().is_empty() {
        field("stdout:", &result.debug);
    }
    // An input error's stderr is already its reason.
    if !result.stderr.trim().is_empty() && !matches!(result.verdict, Verdict::InputError { .. }) {
        field("stderr:", result.stderr.trim_end());
    }
    out
}

fn format_duration(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{:.2} s", d.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(input: &str, expected: Option<&str>) -> Case {
        Case {
            input: input.to_string(),
            expected: expected.map(str::to_string),
        }
    }

    fn result(verdict: Verdict, debug: &str, stderr: &str) -> CaseResult {
        CaseResult {
            verdict,
            debug: debug.to_string(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn accepted_is_one_line() {
        let out = format_case(
            1,
            &case("[1]\n", Some("1")),
            &result(Verdict::Accepted, "", ""),
            Duration::from_millis(3),
        );
        assert_eq!(out, "✓ Case 1: Accepted (3 ms)\n");
    }

    #[test]
    fn wrong_answer_shows_details() {
        let out = format_case(
            2,
            &case("[3,2,4]\n6\n", Some("[1,2]")),
            &result(
                Verdict::WrongAnswer {
                    expected: "[1,2]".into(),
                    actual: "[2,1]".into(),
                },
                "i=0\ni=1",
                "",
            ),
            Duration::from_millis(12),
        );
        assert_eq!(
            out,
            "✗ Case 2: Wrong Answer (12 ms)\n\
             \x20   input:    [3,2,4]\n\
             \x20             6\n\
             \x20   expected: [1,2]\n\
             \x20   actual:   [2,1]\n\
             \x20   stdout:   i=0\n\
             \x20             i=1\n"
        );
    }

    #[test]
    fn timeout_and_no_expected() {
        let out = format_case(
            1,
            &case("1\n", Some("1")),
            &result(Verdict::TimeLimitExceeded, "", ""),
            Duration::from_millis(2000),
        );
        assert!(
            out.starts_with("✗ Case 1: Time Limit Exceeded (>2.00 s)\n"),
            "{out}"
        );

        let out = format_case(
            1,
            &case("1\n", None),
            &result(Verdict::NoExpected { actual: "7".into() }, "", ""),
            Duration::from_millis(1),
        );
        assert_eq!(
            out,
            "? Case 1: No expected output (1 ms)\n    output:   7\n"
        );
    }

    #[test]
    fn input_error_hides_duplicate_stderr() {
        let out = format_case(
            1,
            &case("[1\n", Some("1")),
            &result(
                Verdict::InputError {
                    reason: "LC_IO: expected ']'".into(),
                },
                "",
                "LC_IO: expected ']'\n",
            ),
            Duration::from_millis(1),
        );
        assert_eq!(out.matches("LC_IO").count(), 1, "{out}");
    }

    #[test]
    fn selects_cases() {
        let cases = vec![case("1\n", None), case("2\n", None)];
        assert_eq!(select_cases(cases.clone(), None).unwrap().len(), 2);
        let only = select_cases(cases.clone(), Some(2)).unwrap();
        assert_eq!(only, vec![(2, case("2\n", None))]);
        assert!(select_cases(cases, Some(3)).is_err());
        assert!(select_cases(vec![], None).is_err());
    }
}

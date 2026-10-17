//! Turning one test case's execution into a verdict.

use super::{compare, protocol};

/// What happened when a built solution ran on one test case. Produced by a
/// language runtime; judged here the same way for every language.
#[derive(Debug, Clone)]
pub struct Execution {
    pub stdout: String,
    pub stderr: String,
    pub termination: Termination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    Exited(i32),
    /// Killed by a signal (Unix); the number when known.
    Signaled(Option<i32>),
    TimedOut,
}

/// Exit code generated solutions use when `LC_IO` can't parse the input.
const INPUT_ERROR_EXIT: i32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    WrongAnswer {
        expected: String,
        actual: String,
    },
    /// The case has no expected output; the answer is reported as-is.
    NoExpected {
        actual: String,
    },
    RuntimeError {
        reason: String,
    },
    /// The test input itself couldn't be parsed by the solution's harness.
    InputError {
        reason: String,
    },
    TimeLimitExceeded,
}

impl Verdict {
    /// Whether the case counts as a failure in the overall result.
    pub fn is_failure(&self) -> bool {
        !matches!(self, Verdict::Accepted | Verdict::NoExpected { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseResult {
    pub verdict: Verdict,
    /// Whatever the solution printed before its answer.
    pub debug: String,
    pub stderr: String,
}

pub fn judge(expected: Option<&str>, exec: &Execution) -> CaseResult {
    let parsed = protocol::parse(&exec.stdout);
    let debug = match &parsed {
        Some(p) => p.debug.to_string(),
        None => exec.stdout.trim_end().to_string(),
    };
    let verdict = match (exec.termination, parsed) {
        (Termination::TimedOut, _) => Verdict::TimeLimitExceeded,
        (Termination::Signaled(sig), _) => Verdict::RuntimeError {
            reason: sanitizer_summary(&exec.stderr).unwrap_or_else(|| signal_reason(sig)),
        },
        (Termination::Exited(INPUT_ERROR_EXIT), _) if exec.stderr.contains("LC_IO:") => {
            Verdict::InputError {
                reason: exec.stderr.trim().to_string(),
            }
        }
        (Termination::Exited(code), _) if code != 0 => Verdict::RuntimeError {
            reason: sanitizer_summary(&exec.stderr)
                .unwrap_or_else(|| format!("exited with code {code}")),
        },
        (Termination::Exited(_), None) => Verdict::RuntimeError {
            reason: "no 'output:' line on stdout".to_string(),
        },
        (Termination::Exited(_), Some(p)) => {
            let actual = p.answer.to_string();
            match expected {
                None => Verdict::NoExpected { actual },
                Some(e) if compare::outputs_match(e, &actual) => Verdict::Accepted,
                Some(e) => Verdict::WrongAnswer {
                    expected: e.to_string(),
                    actual,
                },
            }
        }
    };
    CaseResult {
        verdict,
        debug,
        stderr: exec.stderr.clone(),
    }
}

/// The headline of an AddressSanitizer or UBSan report, if stderr has one.
fn sanitizer_summary(stderr: &str) -> Option<String> {
    stderr.lines().find_map(|line| {
        if let Some((_, rest)) = line.split_once("ERROR: AddressSanitizer: ") {
            let what = rest.split(" on ").next().unwrap_or(rest).trim();
            Some(format!("AddressSanitizer: {what}"))
        } else if line.contains(": runtime error: ") {
            Some(line.trim().to_string())
        } else {
            None
        }
    })
}

fn signal_reason(sig: Option<i32>) -> String {
    let name = match sig {
        Some(4) => "SIGILL",
        Some(6) => "SIGABRT (abort, failed assertion or uncaught exception)",
        Some(8) => "SIGFPE (arithmetic error, e.g. division by zero)",
        Some(9) => "SIGKILL",
        Some(7) if cfg!(target_os = "linux") => "SIGBUS (bad memory access)",
        Some(10) if cfg!(target_os = "macos") => "SIGBUS (bad memory access)",
        Some(11) => "SIGSEGV (segmentation fault)",
        Some(n) => return format!("killed by signal {n}"),
        None => return "killed by a signal".to_string(),
    };
    format!("killed by {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(stdout: &str, termination: Termination) -> Execution {
        Execution {
            stdout: stdout.to_string(),
            stderr: String::new(),
            termination,
        }
    }

    #[test]
    fn accepted() {
        let r = judge(
            Some("[0,1]"),
            &exec("\noutput: [0,1]\n", Termination::Exited(0)),
        );
        assert_eq!(r.verdict, Verdict::Accepted);
        assert_eq!(r.debug, "");
    }

    #[test]
    fn wrong_answer_keeps_debug() {
        let r = judge(
            Some("[0,1]"),
            &exec("dbg\n\noutput: [1,0]\n", Termination::Exited(0)),
        );
        assert_eq!(
            r.verdict,
            Verdict::WrongAnswer {
                expected: "[0,1]".into(),
                actual: "[1,0]".into()
            }
        );
        assert_eq!(r.debug, "dbg");
        assert!(r.verdict.is_failure());
    }

    #[test]
    fn no_expected() {
        let r = judge(None, &exec("\noutput: 3\n", Termination::Exited(0)));
        assert_eq!(r.verdict, Verdict::NoExpected { actual: "3".into() });
        assert!(!r.verdict.is_failure());
    }

    #[test]
    fn timeout_wins_over_output() {
        let r = judge(Some("1"), &exec("\noutput: 1\n", Termination::TimedOut));
        assert_eq!(r.verdict, Verdict::TimeLimitExceeded);
    }

    #[test]
    fn segfault() {
        let r = judge(Some("1"), &exec("partial", Termination::Signaled(Some(11))));
        assert_eq!(
            r.verdict,
            Verdict::RuntimeError {
                reason: "killed by SIGSEGV (segmentation fault)".into()
            }
        );
        assert_eq!(r.debug, "partial");
    }

    #[test]
    fn sanitizer_reports_become_the_reason() {
        let mut e = exec("", Termination::Signaled(Some(6)));
        e.stderr = "bal=0\n=====\n==1==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x60 at pc 0x1\nREAD of size 4\n".into();
        assert_eq!(
            judge(Some("1"), &e).verdict,
            Verdict::RuntimeError {
                reason: "AddressSanitizer: heap-buffer-overflow".into()
            }
        );

        let mut e = exec("", Termination::Exited(1));
        e.stderr = "solution.cpp:9:11: runtime error: signed integer overflow: 2147483647 + 1 cannot be represented in type 'int'\n".into();
        let Verdict::RuntimeError { reason } = judge(Some("1"), &e).verdict else {
            panic!("expected a runtime error");
        };
        assert!(reason.starts_with("solution.cpp:9:11: runtime error: signed integer overflow"));
    }

    #[test]
    fn lc_io_input_error() {
        let mut e = exec("", Termination::Exited(2));
        e.stderr = "LC_IO: expected ']'\n".into();
        assert_eq!(
            judge(Some("1"), &e).verdict,
            Verdict::InputError {
                reason: "LC_IO: expected ']'".into()
            }
        );
    }

    #[test]
    fn nonzero_exit_and_missing_output() {
        assert_eq!(
            judge(Some("1"), &exec("", Termination::Exited(2))).verdict,
            Verdict::RuntimeError {
                reason: "exited with code 2".into()
            }
        );
        assert_eq!(
            judge(Some("1"), &exec("hi\n", Termination::Exited(0))).verdict,
            Verdict::RuntimeError {
                reason: "no 'output:' line on stdout".into()
            }
        );
    }
}

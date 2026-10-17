//! Scaffold a real problem, then build, run and judge C++ solutions end to
//! end. Skipped when no C++ compiler is installed.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use super::process;
use super::runtime::{self, BuildCtx, BuildError, Executable, Profile};
use super::verdict::{self, Verdict};
use crate::leetcode::models::QuestionDetail;
use crate::scaffold::{self, testcases};

const TWO_SUM: &str = r#"{
    "questionFrontendId": "1",
    "titleSlug": "two-sum",
    "title": "Two Sum",
    "difficulty": "Easy",
    "topicTags": [],
    "status": null,
    "questionId": "1",
    "isPaidOnly": false,
    "content": "<p>Example 1:</p><pre><strong>Input:</strong> nums = [2,7,11,15], target = 9\n<strong>Output:</strong> [0,1]</pre><pre><strong>Input:</strong> nums = [3,2,4], target = 6\n<strong>Output:</strong> [1,2]</pre>",
    "codeSnippets": [{"langSlug": "cpp", "code": "class Solution {\npublic:\n    vector<int> twoSum(vector<int>& nums, int target) {\n        \n    }\n};"}],
    "exampleTestcaseList": ["[2,7,11,15]\n9", "[3,2,4]\n6"],
    "metaData": "{\"name\": \"twoSum\", \"params\": [{\"name\": \"nums\", \"type\": \"integer[]\"}, {\"name\": \"target\", \"type\": \"integer\"}], \"return\": {\"type\": \"integer[]\"}}"
}"#;

const CORRECT: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        unordered_map<int, int> seen;
        for (int i = 0; i < (int)nums.size(); ++i) {
            auto it = seen.find(target - nums[i]);
            if (it != seen.end()) return {it->second, i};
            seen[nums[i]] = i;
        }
        return {};
    }
};";

const WRONG: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        cout << \"size=\" << nums.size() << endl;
        return {1, 0};
    }
};";

const CRASH: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        volatile int *p = nullptr;
        *p = 1;
        return {};
    }
};";

const SPIN: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        volatile bool spin = true;
        while (spin) {}
        return {};
    }
};";

fn compiler_available() -> bool {
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "c++".into());
    Command::new(cxx)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Swap the code between the `@lc code` markers for `code`.
fn with_solution(generated: &str, code: &str) -> String {
    let begin = generated.find(scaffold::cpp::CODE_BEGIN_MARKER).unwrap();
    let begin = begin + generated[begin..].find('\n').unwrap() + 1;
    let end = generated.find(scaffold::cpp::CODE_END_MARKER).unwrap();
    let end = generated[..end].rfind('\n').unwrap() + 1;
    format!("{}\n{code}\n\n{}", &generated[..begin], &generated[end..])
}

struct Fixture {
    _tmp: tempfile::TempDir,
    workspace: std::path::PathBuf,
    problem: std::path::PathBuf,
    build: std::path::PathBuf,
}

fn scaffold_two_sum() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let workspace = tmp.path().join("leetcode");
    let problem = workspace.join("0001.two-sum");
    let detail: QuestionDetail = serde_json::from_str(TWO_SUM).unwrap();
    scaffold::write(&detail, &workspace, &problem).unwrap();
    let build = tmp.path().join("build");
    Fixture {
        _tmp: tmp,
        workspace,
        problem,
        build,
    }
}

fn build_profile(f: &Fixture, code: &str, profile: Profile) -> Result<Executable, BuildError> {
    let generated = std::fs::read_to_string(f.problem.join("solution.cpp")).unwrap();
    std::fs::write(
        f.problem.join("solution.cpp"),
        with_solution(&generated, code),
    )
    .unwrap();
    runtime::init_runtimes();
    let lang = runtime::detect(&f.problem).unwrap();
    let rt = runtime::get(
        &lang,
        &serde_json::json!({ "flags": ["-std=c++17", "-O0"] }),
    )
    .unwrap();
    rt.build(&BuildCtx {
        problem_dir: &f.problem,
        workspace_dir: &f.workspace,
        build_dir: &f.build,
        profile,
    })
}

fn build(f: &Fixture, code: &str) -> Result<Executable, BuildError> {
    build_profile(f, code, Profile::Release)
}

fn verdicts(f: &Fixture, exe: &Executable, timeout: Duration) -> Vec<Verdict> {
    let content = std::fs::read_to_string(f.problem.join("testcases.txt")).unwrap();
    let cases = testcases::parse(&content).unwrap();
    assert_eq!(cases.len(), 2, "{content}");
    cases
        .iter()
        .map(|c| {
            let (exec, _) = process::run(exe, &f.problem, &c.input, timeout).unwrap();
            verdict::judge(c.expected.as_deref(), &exec).verdict
        })
        .collect()
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

#[test]
fn cpp_end_to_end() {
    if !compiler_available() {
        eprintln!("skipping: no C++ compiler");
        return;
    }
    let f = scaffold_two_sum();
    let timeout = Duration::from_secs(10);

    let exe = build(&f, CORRECT).unwrap();
    assert_eq!(verdicts(&f, &exe, timeout), vec![Verdict::Accepted; 2]);

    // Unchanged source reuses the cached binary.
    let built_at = mtime(&exe.program);
    let again = build(&f, CORRECT).unwrap();
    assert_eq!(again, exe);
    assert_eq!(mtime(&again.program), built_at);

    let exe = build(&f, WRONG).unwrap();
    let v = verdicts(&f, &exe, timeout);
    assert_eq!(
        v[0],
        Verdict::WrongAnswer {
            expected: "[0,1]".into(),
            actual: "[1,0]".into()
        }
    );

    let exe = build(&f, CRASH).unwrap();
    assert!(
        verdicts(&f, &exe, timeout)
            .iter()
            .all(|v| matches!(v, Verdict::RuntimeError { .. })),
    );

    let exe = build(&f, SPIN).unwrap();
    assert_eq!(
        verdicts(&f, &exe, Duration::from_millis(300)),
        vec![Verdict::TimeLimitExceeded; 2]
    );

    match build(&f, "class Solution { oops };") {
        Err(BuildError::Compile(diag)) => assert!(diag.contains("solution.cpp"), "{diag}"),
        other => panic!("expected a compile error, got {other:?}"),
    }
}

const OUT_OF_BOUNDS: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        vector<int> v(2);
        return {v[nums.size() + 3], 0};
    }
};";

const OVERFLOW: &str = "class Solution {
public:
    vector<int> twoSum(vector<int>& nums, int target) {
        int x = INT_MAX - 1;
        x += (int)nums.size();
        return {x, 0};
    }
};";

/// Whether the compiler can build and link with ASan + UBSan here.
fn sanitizers_available() -> bool {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("t.cpp"), "int main() {}").unwrap();
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "c++".into());
    Command::new(cxx)
        .current_dir(dir.path())
        .args(["-fsanitize=address,undefined", "t.cpp", "-o", "t"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn first_reason(f: &Fixture, exe: &Executable) -> String {
    let content = std::fs::read_to_string(f.problem.join("testcases.txt")).unwrap();
    let case = &testcases::parse(&content).unwrap()[0];
    let (exec, _) = process::run(exe, &f.problem, &case.input, Duration::from_secs(20)).unwrap();
    match verdict::judge(case.expected.as_deref(), &exec).verdict {
        Verdict::RuntimeError { reason } => reason,
        other => panic!("expected a runtime error, got {other:?}\n{}", exec.stderr),
    }
}

#[test]
fn cpp_debug_profile_reports_sanitizer_errors() {
    if !compiler_available() || !sanitizers_available() {
        eprintln!("skipping: no C++ compiler with sanitizers");
        return;
    }
    let f = scaffold_two_sum();

    let exe = build_profile(&f, OUT_OF_BOUNDS, Profile::Debug).unwrap();
    assert!(exe.program.ends_with("solution-debug"));
    assert_eq!(
        first_reason(&f, &exe),
        "AddressSanitizer: heap-buffer-overflow"
    );

    let exe = build_profile(&f, OVERFLOW, Profile::Debug).unwrap();
    let reason = first_reason(&f, &exe);
    assert!(
        reason.contains("runtime error: signed integer overflow"),
        "{reason}"
    );
}

//! Problem scaffolding that produces the same files as `leetgo pick` (C++):
//! `question.md`, `solution.cpp` and `testcases.txt` in the problem folder,
//! plus the shared `LC_IO.h` and `bits/stdc++.h` headers in the workspace root.

pub mod cpp;
mod markdown;
mod question;
mod testcases;
mod wordwrap;

use std::path::Path;

use crate::leetcode::models::QuestionDetail;
use question::Question;

pub struct Files {
    pub question_md: String,
    pub solution_cpp: String,
    pub testcases_txt: String,
}

/// Render all problem files in memory.
pub fn render(detail: &QuestionDetail) -> Result<Files, String> {
    if detail.is_paid_only && detail.code_snippets.is_none() {
        return Err(format!(
            "question {:?} is paid only; you need to subscribe to LeetCode Premium",
            detail.summary.title_slug
        ));
    }
    let q = Question::new(detail);
    let snippet = q.code_snippet("cpp").ok_or_else(|| {
        format!(
            "question {:?} doesn't support language cpp",
            detail.summary.title_slug
        )
    })?;
    let question_md = format!(
        "# [{}. {}][link] ({})\n\n[link]: {}\n\n{}",
        detail.summary.frontend_id,
        detail.summary.title,
        detail.summary.difficulty,
        q.url(),
        q.formatted_content(),
    );
    Ok(Files {
        question_md,
        solution_cpp: cpp::generate(&q, snippet),
        testcases_txt: testcases::generate(&q),
    })
}

/// Write the problem files into `problem_dir` and make sure the C++ support
/// headers exist in `workspace_dir`.
pub fn write(
    detail: &QuestionDetail,
    workspace_dir: &Path,
    problem_dir: &Path,
) -> Result<(), String> {
    let files = render(detail)?;
    write_support_headers(workspace_dir)?;
    std::fs::create_dir_all(problem_dir).map_err(|e| e.to_string())?;
    for (name, content) in [
        ("solution.cpp", &files.solution_cpp),
        ("testcases.txt", &files.testcases_txt),
        ("question.md", &files.question_md),
    ] {
        std::fs::write(problem_dir.join(name), content)
            .map_err(|e| format!("failed to write {name}: {e}"))?;
    }
    Ok(())
}

/// leetgo always refreshes `LC_IO.h` and writes `bits/stdc++.h` when missing.
fn write_support_headers(workspace_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(workspace_dir.join("bits")).map_err(|e| e.to_string())?;
    std::fs::write(workspace_dir.join("LC_IO.h"), cpp::LC_IO_H)
        .map_err(|e| format!("failed to write LC_IO.h: {e}"))?;
    let stdcxx = workspace_dir.join("bits").join("stdc++.h");
    if !stdcxx.exists() {
        std::fs::write(&stdcxx, cpp::STDCXX_H)
            .map_err(|e| format!("failed to write bits/stdc++.h: {e}"))?;
    }
    Ok(())
}

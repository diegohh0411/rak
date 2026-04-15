use std::path::Path;

use crate::config;
use crate::leetcode::client::LeetcodeClient;
use crate::leetcode::credentials;

pub fn run(id: String) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;

    // Read the solution file
    let (code, lang) = read_solution_with_lang(&problem_dir)?;

    // Resolve credentials — required for submission
    let creds = credentials::resolve()?;
    let client = LeetcodeClient::new(Some(creds))?;

    // Derive the title slug from the folder name or fetch it
    let slug = slug_from_problem_dir(&problem_dir, &problem_id);

    // Fetch detail to get the internal questionId (required for submit body)
    eprint!("Fetching question detail... ");
    let detail = client.fetch_question_detail(&slug)?;
    eprintln!("done");

    let title_slug = &detail.summary.title_slug;
    let question_id = &detail.question_id;

    if question_id.is_empty() {
        return Err(format!(
            "could not determine internal question ID for '{}' — cannot submit",
            title_slug
        ));
    }

    eprintln!("Submitting {} ({})... ", title_slug, lang);
    let submission_id = client.submit(title_slug, question_id, &lang, &code)?;
    eprint!("Waiting for result");

    let result = client.poll_result(submission_id)?;
    eprintln!(); // newline after the dots

    display_result(&result);
    Ok(())
}

/// Read the latest solution file and detect its LeetCode language slug.
fn read_solution_with_lang(dir: &Path) -> Result<(String, String), String> {
    // We need the path to detect the extension, so find it ourselves.
    let extensions: &[(&str, &str)] = &[
        (".cpp", "cpp"),
        (".py", "python3"),
        (".rs", "rust"),
        (".go", "golang"),
        (".java", "java"),
        (".js", "javascript"),
        (".ts", "typescript"),
        (".cs", "csharp"),
        (".c", "c"),
    ];

    let mut best: Option<(std::time::SystemTime, std::path::PathBuf, &str)> = None;

    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = format!(".{}", ext.to_string_lossy());
                    if let Some((_, lang_slug)) =
                        extensions.iter().find(|(e, _)| *e == ext_str.as_str())
                    {
                        if let Ok(metadata) = entry.metadata() {
                            let mtime = metadata
                                .modified()
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            if best.is_none() || mtime > best.as_ref().unwrap().0 {
                                best = Some((mtime, path, lang_slug));
                            }
                        }
                    }
                }
            }
        }
    }

    let (_, path, lang_slug) =
        best.ok_or_else(|| "no solution file found in problem directory".to_string())?;
    let code =
        std::fs::read_to_string(&path).map_err(|e| format!("failed to read solution: {e}"))?;
    Ok((code, lang_slug.to_string()))
}

/// Derive the LeetCode title slug.
/// For standard numeric problems (e.g. folder "0268.missing-number") use the part after the dot.
/// For custom slugs (e.g. "c3ai-strings") use the id string directly.
fn slug_from_problem_dir(problem_dir: &Path, problem_id: &config::ProblemId) -> String {
    // First try: extract from folder name "NNNN.title-slug"
    if let Some(folder) = problem_dir.file_name().and_then(|n| n.to_str()) {
        if let Some(dot) = folder.find('.') {
            let after_dot = &folder[dot + 1..];
            if !after_dot.is_empty() {
                return after_dot.to_string();
            }
        }
    }
    // Fallback: use the raw id (works for slug-form inputs like "two-sum")
    match problem_id {
        config::ProblemId::Leetcode(n) => n.to_string(),
        config::ProblemId::Custom(s) => s.clone(),
    }
}

fn display_result(result: &crate::leetcode::models::CheckResult) {
    let ok = result.status_msg == "Accepted";
    let prefix = if ok { "✓" } else { "✗" };

    let mut line = format!("{prefix} {}", result.status_msg);

    if let (Some(correct), Some(total)) = (result.total_correct, result.total_testcases) {
        line.push_str(&format!(" [{correct}/{total} test cases]"));
    }

    if let (Some(rt), Some(mem)) = (&result.status_runtime, &result.status_memory) {
        if ok {
            line.push_str(&format!(" ({rt}, {mem})"));
        }
    }

    println!("{line}");

    if let Some(err) = result
        .full_compile_error
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(result.compile_error.as_deref().filter(|s| !s.is_empty()))
    {
        for l in err.lines().take(20) {
            println!("  {l}");
        }
    }
}

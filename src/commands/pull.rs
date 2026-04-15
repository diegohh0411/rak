use std::path::Path;

use chrono::Utc;

use crate::config;
use crate::leetcode::cache::{self};
use crate::leetcode::client::LeetcodeClient;
use crate::leetcode::credentials;
use crate::leetcode::models::{ProblemCache, QuestionSummary};

const CACHE_MAX_AGE_DAYS: i64 = 7;

pub fn run(qid: Option<String>, refresh: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let creds = credentials::resolve().ok();
    let client = LeetcodeClient::new(creds)?;

    let rak_toml_path = config::find_rak_toml(&config_dir)?;
    let rak_toml_dir = rak_toml_path
        .parent()
        .ok_or("rak.toml has no parent directory")?
        .to_path_buf();

    let slug: String = match qid.as_deref() {
        Some("today") => {
            eprint!("Fetching today's daily challenge... ");
            let s = client.fetch_daily_slug()?;
            eprintln!("{}", s);
            s
        }
        Some(id) => {
            if let Ok(n) = id.parse::<u32>() {
                let summary = resolve_by_id(n, &client, &rak_toml_dir, refresh)?;
                summary.title_slug
            } else {
                id.to_string()
            }
        }
        None => {
            let problems = load_or_refresh_cache(&client, &rak_toml_dir, refresh)?;
            match crate::commands::pull_tui::run_pull_tui(&problems)? {
                Some(s) => {
                    eprintln!("Selected: {}", s.title);
                    s.title_slug
                }
                None => {
                    eprintln!("Cancelled.");
                    return Ok(());
                }
            }
        }
    };

    eprint!("Fetching {}... ", slug);
    let detail = client.fetch_question_detail(&slug)?;
    eprintln!("done");

    let leetcode_dir = rak_toml_dir.join(&cfg.leetcode_dir);
    let folder_name = detail.summary.folder_name();
    let problem_dir = leetcode_dir.join(&folder_name);

    if problem_dir.is_dir() {
        eprintln!("Folder '{}' already exists — skipping scaffold", folder_name);
        return Ok(());
    }

    let html = detail.content.as_deref().unwrap_or("");
    let cpp = detail.cpp_snippet().unwrap_or("");

    scaffold_with_content(&problem_dir, html, cpp)?;
    eprintln!("Created {}/", folder_name);
    Ok(())
}

pub fn load_or_refresh_cache(
    client: &LeetcodeClient,
    rak_toml_dir: &Path,
    force_refresh: bool,
) -> Result<Vec<QuestionSummary>, String> {
    if !force_refresh {
        if let Some(cached) = cache::load(rak_toml_dir) {
            if !cache::is_stale(&cached, CACHE_MAX_AGE_DAYS) {
                return Ok(cached.problems);
            }
            eprintln!("Problem list cache is stale (>7 days). Refreshing...");
        } else {
            eprintln!("No problem list cache found. Fetching from LeetCode...");
        }
    } else {
        eprintln!("Refreshing problem list cache...");
    }

    let problems = client.fetch_all_problems()?;
    let pc = ProblemCache {
        fetched_at: Utc::now(),
        problems: problems.clone(),
    };
    cache::save(rak_toml_dir, &pc)?;
    eprintln!("Cached {} problems.", problems.len());
    Ok(problems)
}

fn resolve_by_id(
    id: u32,
    client: &LeetcodeClient,
    rak_toml_dir: &Path,
    refresh: bool,
) -> Result<QuestionSummary, String> {
    if !refresh {
        if let Some(cached) = cache::load(rak_toml_dir) {
            if let Some(s) = cached
                .problems
                .into_iter()
                .find(|p| p.frontend_id.parse::<u32>().unwrap_or(0) == id)
            {
                return Ok(s);
            }
        }
    }
    let problems = load_or_refresh_cache(client, rak_toml_dir, true)?;
    problems
        .into_iter()
        .find(|p| p.frontend_id.parse::<u32>().unwrap_or(0) == id)
        .ok_or_else(|| format!("problem #{id} not found in problem list"))
}

fn scaffold_with_content(
    problem_dir: &Path,
    question_html: &str,
    cpp_snippet: &str,
) -> Result<(), String> {
    std::fs::create_dir_all(problem_dir).map_err(|e| e.to_string())?;

    let markdown = if question_html.is_empty() {
        String::new()
    } else {
        htmd::convert(question_html)
            .map_err(|e| format!("markdown conversion failed: {e}"))?
    };

    std::fs::write(problem_dir.join("question.md"), markdown).map_err(|e| e.to_string())?;
    std::fs::write(problem_dir.join("solution.cpp"), cpp_snippet).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scaffold_with_content_writes_files() {
        let dir = tempfile::tempdir().unwrap();
        let pd = dir.path().join("0001.two-sum");
        scaffold_with_content(&pd, "<p>Find two numbers</p>", "class Solution {};").unwrap();
        let md = fs::read_to_string(pd.join("question.md")).unwrap();
        assert!(md.contains("Find two numbers"));
        let cpp = fs::read_to_string(pd.join("solution.cpp")).unwrap();
        assert_eq!(cpp, "class Solution {};");
    }

    #[test]
    fn scaffold_with_content_empty_html() {
        let dir = tempfile::tempdir().unwrap();
        let pd = dir.path().join("0001.two-sum");
        scaffold_with_content(&pd, "", "").unwrap();
        assert_eq!(fs::read_to_string(pd.join("question.md")).unwrap(), "");
        assert_eq!(fs::read_to_string(pd.join("solution.cpp")).unwrap(), "");
    }

    #[test]
    fn scaffold_with_content_idempotent_on_existing_dir() {
        let dir = tempfile::tempdir().unwrap();
        let pd = dir.path().join("0001.two-sum");
        fs::create_dir_all(&pd).unwrap();
        scaffold_with_content(&pd, "", "int main() {}").unwrap();
        let cpp = fs::read_to_string(pd.join("solution.cpp")).unwrap();
        assert_eq!(cpp, "int main() {}");
    }
}

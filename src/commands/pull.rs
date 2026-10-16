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

    crate::scaffold::write(&detail, &leetcode_dir, &problem_dir)?;
    eprintln!("Created {}/", folder_name);

    // Initialize in history.yaml
    let history_path = rak_toml_dir.join("history.yaml");
    let mut hist = crate::history::load(&history_path)?;
    let qid_str = detail.summary.frontend_id.clone();
    if !hist.problems.contains_key(&qid_str) {
        hist.problems.insert(
            qid_str,
            crate::history::Problem {
                title: Some(detail.summary.title.clone()),
                difficulty: Some(detail.summary.difficulty.clone()),
                box_num: 1,
                streak_perfect: 0,
                last_review: chrono::Local::now().date_naive(),
                attempts: vec![],
            },
        );
        crate::history::save(&history_path, &hist)?;
        eprintln!("Initialized in history.yaml");
    }

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

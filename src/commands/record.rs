use std::io::{self, BufRead, Write};

use crate::commands::add;
use crate::config;
use crate::credentials;
use crate::recorder::{self, tui};

pub fn run(id: String, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);

    // Auto-scaffold missing custom problem folders silently
    if let config::ProblemId::Custom(ref slug) = problem_id {
        let leetcode_dir = config_dir.join(&cfg.leetcode_dir);
        let candidate = leetcode_dir.join(slug);
        if !candidate.is_dir() {
            add::scaffold(&candidate)?;
        }
    }

    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;

    recorder::check_ffmpeg()?;

    let attempt = recorder::next_attempt_number(&problem_dir, force);
    let filename = format!("attempt-{attempt}.mp3");
    let output_path = problem_dir.join(&filename);
    let output_str = output_path.to_string_lossy().to_string();

    let saved = tui::run_recorder_tui(&output_str, &filename)?;

    if let Some(path) = saved {
        if !std::path::Path::new(&path).exists() {
            return Err("recording was not saved".to_string());
        }
        println!();
        let transcribed = match transcribe_attempt(&cfg, &problem_dir, &path, attempt) {
            Ok(done) => done,
            Err(e) => {
                eprintln!("Transcription error: {e}");
                false
            }
        };

        if transcribed && let Err(e) = crate::commands::analyze::run(id.clone(), None, true) {
            eprintln!("Analysis error: {e}");
        }

        println!();
        prompt_log(&id);
    }

    Ok(())
}

fn transcribe_attempt(
    cfg: &config::RakConfig,
    problem_dir: &std::path::Path,
    audio_path: &str,
    attempt: usize,
) -> Result<bool, String> {
    let provider_name = &cfg.transcribe.default_provider;
    let mut json_config = cfg
        .transcribe
        .providers
        .get(provider_name)
        .map(|pc| serde_json::json!({ "model": pc.model }))
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(creds) = credentials::get_provider(provider_name)
        && let (Some(obj), Some(creds_obj)) = (json_config.as_object_mut(), creds.as_object())
    {
        for (k, v) in creds_obj {
            obj.insert(k.clone(), v.clone());
        }
    }

    let provider = crate::stt::get(provider_name, &json_config)?;
    eprintln!("Transcribing...");
    let text = provider.transcribe(std::path::Path::new(audio_path))?;
    if text.is_empty() {
        eprintln!("Warning: transcript is empty, skipping");
        return Ok(false);
    }

    let md_name = format!("attempt-{attempt}.md");
    std::fs::write(problem_dir.join(&md_name), &text).map_err(|e| e.to_string())?;
    eprintln!("✓ Transcribed → {md_name}");
    Ok(true)
}

/// Ask for a self-assessed rating and log it. Enter skips.
fn prompt_log(id: &str) {
    println!("Rate this attempt (rak log):");
    println!("  5 — Solved perfectly, no issues");
    println!("  4 — Solved with minor hesitation");
    println!("  3 — Solved but had to consult external syntax reference");
    println!("  2 — Struggled significantly, needed major help");
    println!("  1 — Couldn't solve it without AI/Web giving me the answer");

    loop {
        print!("Rating [1-5, Enter to skip]: ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().lock().read_line(&mut input).unwrap_or(0) == 0 {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            eprintln!("Skipped — log it later with `rak log {id} <rating>`");
            break;
        }
        match input.parse::<u8>() {
            Ok(r) if (1..=5).contains(&r) => {
                if let Err(e) = crate::commands::log::run(id.to_string(), r, false, None) {
                    eprintln!("Log error: {e}");
                }
                break;
            }
            _ => eprintln!("Enter a number from 1 to 5."),
        }
    }
}

use std::io::{self, BufRead, Write};

use crate::config;
use crate::recorder::{self, tui};

pub fn run(id: String, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &id)?;

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
        if prompt_yes_no("Transcribe now?", true) {
            let provider_name = &cfg.transcribe.default_provider;
            let provider_config = cfg.transcribe.providers.get(provider_name);
            let json_config = provider_config
                .map(|pc| {
                    serde_json::json!({
                        "api_key": pc.api_key,
                        "model": pc.model,
                    })
                })
                .unwrap_or(serde_json::Value::Null);

            let provider = crate::stt::get(provider_name, &json_config)?;
            match provider.transcribe(std::path::Path::new(&path)) {
                Ok(text) if !text.is_empty() => {
                    let md_name = format!("attempt-{attempt}.md");
                    let md_path = problem_dir.join(&md_name);
                    std::fs::write(&md_path, &text).map_err(|e| e.to_string())?;
                    eprintln!("✓ Transcribed → {md_name}");
                }
                Ok(_) => eprintln!("Warning: transcript is empty, skipping"),
                Err(e) => eprintln!("Transcription error: {e}"),
            }
        }
    }

    Ok(())
}

fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
    let suffix = if default_yes { " [Y/n]" } else { " [y/N]" };
    print!("{question}{suffix} ");
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input).ok();
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        return default_yes;
    }
    input == "y" || input == "yes"
}

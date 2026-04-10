use std::path::Path;
use std::process::Command;

use regex::Regex;

use crate::config;
use crate::stt;

pub fn run(id: String, provider: Option<String>, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &id)?;

    let audio_files = if force {
        find_all_audio(&problem_dir)
    } else {
        find_untranscribed(&problem_dir)
    };

    if audio_files.is_empty() {
        println!("All transcripts up to date.");
        return Ok(());
    }

    let provider_name = provider
        .as_deref()
        .unwrap_or(&cfg.transcribe.default_provider);
    let provider_config = cfg.transcribe.providers.get(provider_name);
    let json_config = provider_config
        .map(|pc| {
            serde_json::json!({
                "api_key": pc.api_key,
                "model": pc.model,
            })
        })
        .unwrap_or(serde_json::Value::Null);

    let transcriber = stt::get(provider_name, &json_config)?;

    let re = Regex::new(r"^attempt-(\d+)\.mp3$").unwrap();
    let mut transcribed = 0;

    for audio_file in &audio_files {
        let audio_path = problem_dir.join(audio_file);
        println!("Transcribing {audio_file}...");

        match transcriber.transcribe(&audio_path) {
            Ok(text) if !text.is_empty() => {
                let mut content = text;
                if let Some(dur) = audio_duration(&audio_path)
                    && !dur.is_empty()
                {
                    content.push_str(&format!("\n\n---\nAudio duration: {dur}\n"));
                }

                let caps = re.captures(audio_file).unwrap();
                let md_name = format!("attempt-{}.md", &caps[1]);
                let md_path = problem_dir.join(&md_name);
                std::fs::write(&md_path, &content)
                    .map_err(|e| format!("failed to write {md_name}: {e}"))?;
                println!("  ✓ {md_name}");
                transcribed += 1;
            }
            Ok(_) => {
                println!("  Warning: transcript for {audio_file} is empty, skipping");
            }
            Err(e) => {
                println!("  Error: {e}");
            }
        }
    }

    println!("Transcribed {transcribed} file(s).");
    Ok(())
}

pub fn find_untranscribed(dir: &Path) -> Vec<String> {
    let re = Regex::new(r"^attempt-(\d+)\.mp3$").unwrap();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut transcribed = std::collections::HashSet::new();
    let mut audio_files = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = re.captures(&name) {
            let n = &caps[1];
            let md_name = format!("attempt-{n}.md");
            if dir.join(&md_name).exists() {
                transcribed.insert(n.to_string());
            }
            audio_files.push((n.parse::<usize>().unwrap_or(0), name));
        }
    }

    audio_files.sort_by_key(|(n, _)| *n);
    audio_files
        .into_iter()
        .filter(|(n, _)| !transcribed.contains(&n.to_string()))
        .map(|(_, name)| name)
        .collect()
}

pub fn find_all_audio(dir: &Path) -> Vec<String> {
    let re = Regex::new(r"^attempt-(\d+)\.mp3$").unwrap();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut audio_files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(caps) = re.captures(&name) {
            let n: usize = caps[1].parse().unwrap_or(0);
            audio_files.push((n, name));
        }
    }
    audio_files.sort_by_key(|(n, _)| *n);
    audio_files.into_iter().map(|(_, name)| name).collect()
}

pub fn audio_duration(audio_path: &Path) -> Option<String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(audio_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let secs: f64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    let mins = secs as u64 / 60;
    let secs = secs as u64 % 60;
    Some(format!("{mins:02}:{secs:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_untranscribed_skips_transcribed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-1.mp3"), "").unwrap();
        fs::write(dir.path().join("attempt-1.md"), "text").unwrap();
        fs::write(dir.path().join("attempt-2.mp3"), "").unwrap();
        let result = find_untranscribed(dir.path());
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("attempt-2"));
    }

    #[test]
    fn find_untranscribed_returns_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-3.mp3"), "").unwrap();
        fs::write(dir.path().join("attempt-1.mp3"), "").unwrap();
        let result = find_untranscribed(dir.path());
        assert_eq!(result.len(), 2);
        assert!(result[0].contains("attempt-1"));
        assert!(result[1].contains("attempt-3"));
    }

    #[test]
    fn find_all_audio_returns_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-5.mp3"), "").unwrap();
        fs::write(dir.path().join("attempt-1.mp3"), "").unwrap();
        let result = find_all_audio(dir.path());
        assert_eq!(result.len(), 2);
        assert!(result[0].contains("attempt-1"));
    }

    #[test]
    fn audio_duration_returns_empty_for_missing_file() {
        let result = audio_duration(Path::new("/nonexistent/file.mp3"));
        assert!(result.is_none());
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use crate::analyzer::{self, AnalysisContext};
use crate::config;

pub fn run(id: String, provider: Option<String>, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &id)?;

    let analysis_path = problem_dir.join("analysis.md");
    if analysis_path.exists() && !force {
        return Err("analysis.md already exists — use --force to overwrite".to_string());
    }

    let question = fs::read_to_string(problem_dir.join("question.md"))
        .unwrap_or_else(|_| "No question.md found".to_string());

    let solution = read_latest_solution(&problem_dir)?;
    let transcripts = read_transcripts(&problem_dir);

    if transcripts.is_empty() {
        return Err(format!(
            "no transcripts found in {}. run `rak transcribe {}` first.",
            id, id
        ));
    }

    let provider_name = provider
        .as_deref()
        .unwrap_or(&cfg.analyze.default_provider);
    let provider_config = cfg.analyze.providers.get(provider_name);
    let json_config = provider_config
        .map(|pc| serde_json::json!({ "model": pc.model }))
        .unwrap_or(serde_json::Value::Null);

    let analyzer = analyzer::get(provider_name, &json_config)?;

    println!("Analyzing {}...", id);
    let ctx = AnalysisContext {
        question,
        solution,
        transcripts,
    };

    let result = analyzer.analyze(&cfg.analyze.system_prompt, &ctx)?;

    fs::write(&analysis_path, result).map_err(|e| format!("failed to write analysis.md: {e}"))?;
    println!("✓ Saved {}", analysis_path.display());

    Ok(())
}

fn read_transcripts(dir: &Path) -> Vec<String> {
    let re = regex::Regex::new(r"^attempt-(\d+)\.md$").unwrap();
    let mut entries = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(caps) = re.captures(&name) {
                let n = caps[1].parse::<usize>().unwrap_or(0);
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    entries.push((n, content));
                }
            }
        }
    }
    entries.sort_by_key(|(n, _)| *n);
    entries.into_iter().map(|(_, content)| content).collect()
}

fn read_latest_solution(dir: &Path) -> Result<String, String> {
    let extensions = [
        ".cpp", ".py", ".go", ".java", ".rs", ".js", ".ts", ".c", ".cs",
    ];
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;

    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = format!(".{}", ext.to_string_lossy());
                    if extensions.contains(&ext_str.as_str()) {
                        if let Ok(metadata) = entry.metadata() {
                            let mod_time = metadata
                                .modified()
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            if best.is_none() || mod_time > best.as_ref().unwrap().0 {
                                best = Some((mod_time, path));
                            }
                        }
                    }
                }
            }
        }
    }

    let solution_path = best
        .map(|(_, p)| p)
        .ok_or_else(|| "no solution file found".to_string())?;
    fs::read_to_string(solution_path).map_err(|e| format!("failed to read solution file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_read_transcripts_sorting() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("attempt-2.md"), "second").unwrap();
        fs::write(dir.path().join("attempt-1.md"), "first").unwrap();
        fs::write(dir.path().join("attempt-10.md"), "tenth").unwrap();
        fs::write(dir.path().join("other.txt"), "other").unwrap();

        let transcripts = read_transcripts(dir.path());
        assert_eq!(transcripts.len(), 3);
        assert_eq!(transcripts[0], "first");
        assert_eq!(transcripts[1], "second");
        assert_eq!(transcripts[2], "tenth");
    }

    #[test]
    fn test_read_latest_solution() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("solution.py");
        let new_path = dir.path().join("solution.cpp");

        fs::write(&old_path, "old").unwrap();
        // Ensure new_path has a later modification time
        std::thread::sleep(std::time::Duration::from_millis(100));
        fs::write(&new_path, "new").unwrap();

        let content = read_latest_solution(dir.path()).unwrap();
        assert_eq!(content, "new");
    }

    #[test]
    fn test_read_latest_solution_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let res = read_latest_solution(dir.path());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("no solution file found"));
    }
}

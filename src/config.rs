use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RakConfig {
    pub leetcode_dir: String,
    #[serde(default)]
    pub transcribe: TranscribeConfig,
    #[serde(default)]
    pub analyze: AnalyzeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscribeConfig {
    #[serde(default = "default_transcribe_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

fn default_transcribe_provider() -> String {
    "elevenlabs".to_string()
}

impl Default for TranscribeConfig {
    fn default() -> Self {
        Self {
            default_provider: default_transcribe_provider(),
            providers: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeConfig {
    #[serde(default = "default_analyze_provider")]
    pub default_provider: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

fn default_analyze_provider() -> String {
    "claude".to_string()
}

fn default_system_prompt() -> String {
    r#"Analyze this Leetcode problem solution based on my voice notes. Keep it brief - 2-3 paragraphs max.

PROBLEM:
{question}

MY SOLUTION (latest attempt):
{solution}

MY VOICE NOTES:
{transcripts}

Provide:
1. Brief overview of how the problem went
2. What I did well
3. What I struggled with / areas to improve
4. Improvement guide: if the solution was unsolved, suboptimal, or inefficient, provide a concrete guide on how to solve or optimize it. Include the key algorithm/data structure to use, time/space complexity, and a brief pseudocode outline of the improved approach. If the solution is already optimal, skip this section.

Focus on identifying strengths, weaknesses, and actionable feedback for future practice."#.to_string()
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            default_provider: default_analyze_provider(),
            system_prompt: default_system_prompt(),
            providers: HashMap::new(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub api_key: String,
    pub model: Option<String>,
}

pub fn find_rak_toml(start: &Path) -> Result<PathBuf, String> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join("rak.toml");
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(
                "No rak.toml found. Run `rak init` to create one in the project root.".to_string(),
            );
        }
    }
}

pub fn load(config_dir: &Path) -> Result<RakConfig, String> {
    let path = find_rak_toml(config_dir)?;
    let contents = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    toml::from_str(&contents).map_err(|e| e.to_string())
}

pub fn resolve_api_key(config_key: &str, env_var: &str) -> Result<String, String> {
    if !config_key.is_empty() {
        return Ok(config_key.to_string());
    }
    std::env::var(env_var).map_err(|_| {
        format!(
            "API key not set. Provide it in rak.toml or set the {} environment variable.",
            env_var
        )
    })
}

pub fn resolve_problem_folder(
    config_dir: &Path,
    config: &RakConfig,
    id: &str,
) -> Result<PathBuf, String> {
    let padded = format!("{:0>4}", id);
    let leetcode_dir = config_dir.join(&config.leetcode_dir);
    let entries = std::fs::read_dir(&leetcode_dir).map_err(|e| {
        format!(
            "Failed to read leetcode_dir '{}': {}",
            config.leetcode_dir, e
        )
    })?;

    let re =
        regex::Regex::new(&format!("^{}\\.", regex::escape(&padded))).map_err(|e| e.to_string())?;

    let matches: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().is_some_and(|name| re.is_match(name)))
        .map(|e| e.path())
        .collect();

    match matches.len() {
        0 => Err(format!(
            "No problem folder matching '{}' found in {}",
            padded, config.leetcode_dir
        )),
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(format!(
            "Multiple problem folders matching '{}' found in {}: {:?}",
            padded, config.leetcode_dir, matches
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn find_rak_toml_walks_up() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        fs::write(tmp.path().join("rak.toml"), "[placeholder]\n").unwrap();

        let found = find_rak_toml(&deep).unwrap();
        assert_eq!(found, tmp.path().join("rak.toml"));
    }

    #[test]
    fn find_rak_toml_not_found_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = find_rak_toml(tmp.path()).unwrap_err();
        assert!(
            err.contains("rak init"),
            "error should hint at rak init: {err}"
        );
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
leetcode_dir = "leetcode"
"#;
        let config: RakConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.leetcode_dir, "leetcode");
        assert_eq!(config.transcribe.default_provider, "elevenlabs");
        assert!(config.transcribe.providers.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
leetcode_dir = "lc"
[transcribe]
default_provider = "openai"
[transcribe.providers.openai]
api_key = "sk-test"
model = "whisper-1"
[transcribe.providers.elevenlabs]
api_key = "elv-test"
"#;
        let config: RakConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.leetcode_dir, "lc");
        assert_eq!(config.transcribe.default_provider, "openai");
        assert_eq!(
            config.transcribe.providers.get("openai").unwrap().api_key,
            "sk-test"
        );
        assert_eq!(
            config.transcribe.providers.get("openai").unwrap().model,
            Some("whisper-1".to_string())
        );
        assert_eq!(
            config
                .transcribe
                .providers
                .get("elevenlabs")
                .unwrap()
                .api_key,
            "elv-test"
        );
        assert_eq!(
            config.transcribe.providers.get("elevenlabs").unwrap().model,
            None
        );
    }

    #[test]
    fn resolve_problem_folder_zero_pads() {
        let tmp = tempfile::tempdir().unwrap();
        let lc = tmp.path().join("leetcode");
        fs::create_dir_all(lc.join("0001.two-sum")).unwrap();
        fs::write(
            tmp.path().join("rak.toml"),
            format!("leetcode_dir = \"leetcode\""),
        )
        .unwrap();

        let config = RakConfig {
            leetcode_dir: "leetcode".to_string(),
            ..Default::default()
        };
        let result = resolve_problem_folder(tmp.path(), &config, "1").unwrap();
        assert!(result.ends_with("0001.two-sum"));
    }

    #[test]
    fn resolve_problem_folder_no_match_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let lc = tmp.path().join("leetcode");
        fs::create_dir_all(&lc).unwrap();

        let config = RakConfig {
            leetcode_dir: "leetcode".to_string(),
            ..Default::default()
        };
        let err = resolve_problem_folder(tmp.path(), &config, "9999").unwrap_err();
        assert!(err.contains("9999"), "error should mention id: {err}");
    }

    #[test]
    fn resolve_problem_folder_multiple_match_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let lc = tmp.path().join("leetcode");
        fs::create_dir_all(lc.join("0001.two-sum")).unwrap();
        fs::create_dir_all(lc.join("0001.two-sum-v2")).unwrap();

        let config = RakConfig {
            leetcode_dir: "leetcode".to_string(),
            ..Default::default()
        };
        let err = resolve_problem_folder(tmp.path(), &config, "1").unwrap_err();
        assert!(
            err.contains("Multiple"),
            "error should mention multiple: {err}"
        );
    }
}

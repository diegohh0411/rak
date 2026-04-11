use std::fs;

const ENV_KEYS: &[(&str, &str)] = &[("ELEVENLABS_API_KEY", ""), ("OPENROUTER_API_KEY", "")];

const RAK_TOML_TEMPLATE: &str = r#"leetcode_dir = "./cpp"

[transcribe]
default_provider = "elevenlabs"

[transcribe.providers.elevenlabs]
api_key = ""

[transcribe.providers.openrouter]
api_key = ""
model = "google/gemini-flash-2.5-lite"

[analyze]
default_provider = "claude"
system_prompt = """
Analyze this Leetcode problem solution based on my voice notes. Keep it brief - 2-3 paragraphs max.

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

Focus on identifying strengths, weaknesses, and actionable feedback for future practice.
"""

[analyze.providers.claude]
model = "sonnet"

[analyze.providers.gemini]
model = "gemini-2.5-flash"
"#;

pub fn run() -> Result<(), String> {
    init_rak_toml()?;
    init_env()?;
    init_gitignore()?;
    Ok(())
}

fn init_rak_toml() -> Result<(), String> {
    let path = "rak.toml";
    if std::fs::exists(path).map_err(|e| e.to_string())? {
        eprintln!("rak.toml already exists");
        return Ok(());
    }
    std::fs::write(path, RAK_TOML_TEMPLATE).map_err(|e| e.to_string())?;
    eprintln!("Created rak.toml");
    Ok(())
}

fn init_env() -> Result<(), String> {
    let path = ".env";

    if !fs::exists(path).map_err(|e| e.to_string())? {
        let content = ENV_KEYS
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(path, content).map_err(|e| e.to_string())?;
        eprintln!("Created .env");
        return Ok(());
    }

    let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut additions = Vec::new();

    for (key, placeholder) in ENV_KEYS {
        let has_key = existing
            .lines()
            .any(|line| line.starts_with(&format!("{key}=")));
        if !has_key {
            additions.push(format!("{key}={placeholder}"));
            eprintln!("Added {key} to .env");
        }
    }

    if !additions.is_empty() {
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&additions.join("\n"));
        content.push('\n');
        fs::write(path, content).map_err(|e| e.to_string())?;
    } else {
        eprintln!(".env already has all required keys");
    }

    Ok(())
}

fn init_gitignore() -> Result<(), String> {
    let path = ".gitignore";
    let entry = ".env";

    if !fs::exists(path).map_err(|e| e.to_string())? {
        fs::write(path, format!("{entry}\n")).map_err(|e| e.to_string())?;
        eprintln!("Created .gitignore");
        return Ok(());
    }

    let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let already_present = existing.lines().any(|line| line.trim() == entry);

    if already_present {
        eprintln!(".gitignore already contains .env");
        return Ok(());
    }

    let mut content = existing;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{entry}\n"));
    fs::write(path, content).map_err(|e| e.to_string())?;
    eprintln!("Added .env to .gitignore");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn init_creates_rak_toml() {
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = run();
        std::env::set_current_dir(orig).unwrap();
        result.unwrap();
        let content = fs::read_to_string(dir.path().join("rak.toml")).unwrap();
        assert!(content.contains("leetcode_dir"));
        assert!(content.contains("[transcribe]"));
        assert!(content.contains("elevenlabs"));
        assert!(content.contains("openrouter"));
    }

    #[test]
    fn init_creates_env_with_api_keys() {
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = run();
        std::env::set_current_dir(orig).unwrap();
        result.unwrap();
        let content = fs::read_to_string(dir.path().join(".env")).unwrap();
        assert!(content.contains("ELEVENLABS_API_KEY"));
        assert!(content.contains("OPENROUTER_API_KEY"));
    }
}

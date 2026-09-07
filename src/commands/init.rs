use std::fs;

const ENV_KEYS: &[(&str, &str)] = &[("LEETCODE_SESSION", "")];

const RAK_TOML_TEMPLATE: &str = r#"leetcode_dir = "./cpp"

[transcribe]
default_provider = "elevenlabs"

[transcribe.providers.elevenlabs]
# api_key: run `rak login elevenlabs` to set

[transcribe.providers.openrouter]
model = "x-ai/grok-stt-1.0"
# api_key: run `rak login openrouter` to set

[transcribe.providers.chirp]
# run `rak login chirp` to configure

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

[analyze.providers.grok]
model = "grok-4.6"

[analyze.providers.openrouter]
model = "~google/gemini-flash-latest"

[analyze.providers.codex]
# model omitted: uses Codex's own default

[leetcode]
# session = ""   # or set LEETCODE_SESSION env var
"#;

pub fn run() -> Result<(), String> {
    init_rak_toml()?;
    init_env()?;
    init_gitignore()?;
    init_completions()?;
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
    let entries = [".env", ".rak-cache/"];

    if !fs::exists(path).map_err(|e| e.to_string())? {
        let content = entries.iter().map(|e| format!("{e}\n")).collect::<String>();
        fs::write(path, content).map_err(|e| e.to_string())?;
        eprintln!("Created .gitignore");
        return Ok(());
    }

    let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut additions: Vec<&str> = Vec::new();

    for entry in &entries {
        if !existing.lines().any(|line| line.trim() == *entry) {
            additions.push(entry);
        }
    }

    if additions.is_empty() {
        eprintln!(".gitignore already has all required entries");
        return Ok(());
    }

    let mut content = existing;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    for entry in &additions {
        content.push_str(&format!("{entry}\n"));
        eprintln!("Added {entry} to .gitignore");
    }
    fs::write(path, content).map_err(|e| e.to_string())?;

    Ok(())
}

fn init_completions() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("cannot find home directory")?;
    let bashrc = home.join(".bashrc");

    let marker = "eval \"$(rak completions)\"";

    if bashrc.is_file() {
        let existing = fs::read_to_string(&bashrc).map_err(|e| e.to_string())?;
        if existing.lines().any(|line| line.trim() == marker) {
            eprintln!("~/.bashrc already has rak completions");
            return Ok(());
        }
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(marker);
        content.push('\n');
        fs::write(&bashrc, content).map_err(|e| e.to_string())?;
        eprintln!(
            "Added rak completions to ~/.bashrc (restart your shell or run `source ~/.bashrc`)"
        );
    } else {
        eprintln!("No ~/.bashrc found — to enable completions, add this to your shell config:");
        eprintln!("  {marker}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn init_creates_rak_toml() {
        let _guard = CWD_LOCK.lock().unwrap();
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
        assert!(content.contains("chirp"));
        assert!(
            content.contains("x-ai/grok-stt-1.0"),
            "openrouter default should be an STT model"
        );
        let transcribe_openrouter = content
            .split("[transcribe.providers.openrouter]")
            .nth(1)
            .unwrap()
            .split('[')
            .next()
            .unwrap();
        assert!(
            transcribe_openrouter.contains("x-ai/grok-stt-1.0"),
            "transcribe openrouter default must be an STT model"
        );
        assert!(
            !transcribe_openrouter.contains("gemini-flash"),
            "transcribe openrouter default must not be a chat model"
        );
        assert!(
            content.contains("[analyze.providers.openrouter]"),
            "analyze should list the openrouter provider"
        );
        assert!(
            content.contains("~google/gemini-flash-latest"),
            "analyze openrouter default should be the Gemini Flash latest alias"
        );
        assert!(
            content.contains("[analyze.providers.codex]"),
            "analyze should list the codex provider"
        );
        assert!(
            content.contains("default_provider = \"claude\""),
            "new installs should keep claude as the analyze default"
        );
        assert!(
            !content.contains("\napi_key"),
            "api_key should not appear as a TOML key in the template"
        );
    }

    #[test]
    fn init_creates_env() {
        let _guard = CWD_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let result = run();
        std::env::set_current_dir(orig).unwrap();
        result.unwrap();
        let content = fs::read_to_string(dir.path().join(".env")).unwrap();
        assert!(content.contains("LEETCODE_SESSION"));
    }
}

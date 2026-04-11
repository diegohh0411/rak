use std::io::Write;
use std::process::{Command, Stdio};

use crate::analyzer::{AnalysisContext, Analyzer};

pub struct ClaudeAnalyzer {
    model: String,
}

impl ClaudeAnalyzer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Analyzer for ClaudeAnalyzer {
    fn name(&self) -> &str {
        "claude"
    }

    fn analyze(&self, system_prompt: &str, ctx: &AnalysisContext) -> Result<String, String> {
        let prompt = ctx.build_prompt(system_prompt);

        let mut child = Command::new("claude")
            .args(["--model", &self.model, "--output-format", "text"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn claude: {e}"))?;

        let mut stdin = child.stdin.take().ok_or("failed to get stdin")?;
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| e.to_string())?;
        drop(stdin);

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("claude CLI failed: {err}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

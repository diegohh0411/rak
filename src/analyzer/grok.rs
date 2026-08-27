use std::fs;
use std::process::{Command, Stdio};

use crate::analyzer::{AnalysisContext, Analyzer};

pub struct GrokAnalyzer {
    model: String,
}

impl GrokAnalyzer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Analyzer for GrokAnalyzer {
    fn name(&self) -> &str {
        "grok"
    }

    fn analyze(&self, system_prompt: &str, ctx: &AnalysisContext) -> Result<String, String> {
        let prompt = ctx.build_prompt(system_prompt);

        let tmp = std::env::temp_dir().join(format!("rak-grok-prompt-{}.txt", std::process::id()));
        fs::write(&tmp, prompt.as_bytes())
            .map_err(|e| format!("failed to write grok prompt file: {e}"))?;

        let output = Command::new("grok")
            .args([
                "--model",
                &self.model,
                "--prompt-file",
                tmp.to_str().ok_or("grok prompt path is not valid UTF-8")?,
                "--output-format",
                "plain",
                "--no-plan",
                "--permission-mode",
                "dontAsk",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let _ = fs::remove_file(&tmp);

        let output = output.map_err(|e| format!("failed to spawn grok: {e}"))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("grok CLI failed: {err}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

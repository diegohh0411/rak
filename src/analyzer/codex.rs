use std::io::Write;
use std::process::{Command, Stdio};

use crate::analyzer::{AnalysisContext, Analyzer};

pub struct CodexAnalyzer {
    model: String,
}

impl CodexAnalyzer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

/// `codex exec` argv. Prompt is read from stdin (`-`). `--model` is omitted when
/// empty so Codex uses its own default. `--ask-for-approval never` is the
/// non-interactive equivalent of Grok's `dontAsk`, not workspace isolation.
fn exec_args(model: &str) -> Vec<String> {
    let mut args = vec![
        "exec".to_string(),
        "--ask-for-approval".to_string(),
        "never".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    if !model.is_empty() {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    args.push("-".to_string());
    args
}

impl Analyzer for CodexAnalyzer {
    fn name(&self) -> &str {
        "codex"
    }

    fn analyze(&self, system_prompt: &str, ctx: &AnalysisContext) -> Result<String, String> {
        let prompt = ctx.build_prompt(system_prompt);

        let mut child = Command::new("codex")
            .args(exec_args(&self.model))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn codex: {e}"))?;

        let mut stdin = child.stdin.take().ok_or("failed to get stdin")?;
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| e.to_string())?;
        drop(stdin);

        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("codex CLI failed: {err}"));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_args_omit_model_when_unset() {
        let args = exec_args("");
        assert_eq!(
            args,
            vec![
                "exec",
                "--ask-for-approval",
                "never",
                "--color",
                "never",
                "-"
            ]
        );
        assert!(
            !args.iter().any(|a| a == "--model"),
            "must not pin a model when rak.toml leaves it unset"
        );
    }

    #[test]
    fn exec_args_pass_model_when_set() {
        let args = exec_args("gpt-5.5");
        assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.5"]));
        assert_eq!(args.last().unwrap(), "-");
    }
}

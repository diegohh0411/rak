pub mod claude;
pub mod gemini;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

pub struct AnalysisContext {
    pub question: String,
    pub solution: String,
    pub transcripts: Vec<String>,
}

impl AnalysisContext {
    pub fn build_prompt(&self, system_prompt: &str) -> String {
        let transcripts_joined = self.transcripts.join("\n\n---\n\n");
        system_prompt
            .replace("{question}", &self.question)
            .replace("{solution}", &self.solution)
            .replace("{transcripts}", &transcripts_joined)
    }
}

pub trait Analyzer: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn analyze(&self, system_prompt: &str, ctx: &AnalysisContext) -> Result<String, String>;
}

type Factory = Box<dyn Fn(serde_json::Value) -> Box<dyn Analyzer> + Send + Sync>;

static REGISTRY: LazyLock<Mutex<HashMap<String, Factory>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register(
    name: &str,
    factory: impl Fn(serde_json::Value) -> Box<dyn Analyzer> + Send + Sync + 'static,
) {
    REGISTRY
        .lock()
        .unwrap()
        .insert(name.to_string(), Box::new(factory));
}

pub fn get(name: &str, config: &serde_json::Value) -> Result<Box<dyn Analyzer>, String> {
    let registry = REGISTRY.lock().unwrap();
    let factory = registry.get(name).ok_or_else(|| {
        let available: Vec<&str> = registry.keys().map(|s| s.as_str()).collect();
        format!(
            "unknown analysis provider {:?} (available: {})",
            name,
            available.join(", ")
        )
    })?;
    Ok(factory(config.clone()))
}

pub fn init_providers() {
    register("claude", |config| {
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("sonnet")
            .to_string();
        Box::new(claude::ClaudeAnalyzer::new(model))
    });
    register("gemini", |config| {
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("gemini-2.5-flash")
            .to_string();
        Box::new(gemini::GeminiAnalyzer::new(model))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt() {
        let ctx = AnalysisContext {
            question: "Q".into(),
            solution: "S".into(),
            transcripts: vec!["T1".into(), "T2".into()],
        };
        let system_prompt = "Q:{question} S:{solution} T:{transcripts}";
        let prompt = ctx.build_prompt(system_prompt);
        assert_eq!(prompt, "Q:Q S:S T:T1\n\n---\n\nT2");
    }
}

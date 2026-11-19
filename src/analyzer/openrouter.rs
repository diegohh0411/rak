use crate::analyzer::{AnalysisContext, Analyzer};

const DEFAULT_MODEL: &str = "~google/gemini-flash-latest";
const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterAnalyzer {
    api_key: String,
    model: String,
    reasoning_effort: Option<String>,
}

pub const REASONING_EFFORTS: &[&str] =
    &["max", "xhigh", "high", "medium", "low", "minimal", "none"];

impl OpenRouterAnalyzer {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model: if model.is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                model
            },
            reasoning_effort: None,
        }
    }

    pub fn with_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.reasoning_effort = effort.filter(|e| !e.trim().is_empty());
        self
    }
}

fn validate_effort(effort: &str) -> Result<(), String> {
    if REASONING_EFFORTS.contains(&effort) {
        Ok(())
    } else {
        Err(format!(
            "invalid reasoning_effort {effort:?} for openrouter (expected one of: {})",
            REASONING_EFFORTS.join(", ")
        ))
    }
}

fn chat_body(model: &str, prompt: &str, reasoning_effort: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }]
    });
    if let Some(effort) = reasoning_effort {
        body["reasoning"] = serde_json::json!({ "effort": effort, "exclude": true });
    }
    body
}

fn extract_completion(result: &serde_json::Value) -> Result<String, String> {
    match result["choices"][0]["message"]["content"].as_str() {
        Some(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Some(_) => Err("OpenRouter returned empty completion".to_string()),
        None => Err("OpenRouter returned no completion content".to_string()),
    }
}

fn format_api_error(status: reqwest::StatusCode, body: &str) -> String {
    format!("OpenRouter API error: {status} {body}")
}

impl Analyzer for OpenRouterAnalyzer {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn analyze(&self, system_prompt: &str, ctx: &AnalysisContext) -> Result<String, String> {
        let key = if self.api_key.is_empty() {
            std::env::var("OPENROUTER_API_KEY").map_err(|_| {
                "OpenRouter API key not set. Run 'rak login openrouter'.".to_string()
            })?
        } else {
            self.api_key.clone()
        };

        if let Some(effort) = &self.reasoning_effort {
            validate_effort(effort)?;
        }

        let prompt = ctx.build_prompt(system_prompt);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let resp = client
            .post(CHAT_URL)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&chat_body(
                &self.model,
                &prompt,
                self.reasoning_effort.as_deref(),
            ))
            .send()
            .map_err(|e| format!("OpenRouter API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().unwrap_or_default();
            return Err(format_api_error(status, &body_text));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("failed to parse OpenRouter response: {e}"))?;

        extract_completion(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_to_gemini_flash_latest_alias() {
        let a = OpenRouterAnalyzer::new("key".to_string(), "".to_string());
        assert_eq!(a.model, "~google/gemini-flash-latest");
    }

    #[test]
    fn new_keeps_explicit_model() {
        let a = OpenRouterAnalyzer::new("key".to_string(), "x-ai/grok-4.6".to_string());
        assert_eq!(a.model, "x-ai/grok-4.6");
    }

    #[test]
    fn analyze_without_api_key_tells_user_to_login() {
        unsafe { std::env::remove_var("OPENROUTER_API_KEY") };
        let a = OpenRouterAnalyzer::new("".to_string(), "".to_string());
        let ctx = AnalysisContext {
            question: "Q".into(),
            solution: "S".into(),
            transcripts: vec!["T".into()],
        };
        let err = a.analyze("prompt", &ctx).unwrap_err();
        assert!(
            err.contains("rak login openrouter"),
            "error should mention login: {err}"
        );
    }

    #[test]
    fn chat_body_is_a_single_user_message() {
        let body = chat_body("~google/gemini-flash-latest", "hello", None);
        assert_eq!(body["model"], "~google/gemini-flash-latest");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn chat_body_includes_reasoning_effort() {
        let body = chat_body("m", "hello", Some("low"));
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["reasoning"]["exclude"], true);
    }

    #[test]
    fn invalid_reasoning_effort_is_rejected_before_request() {
        let a = OpenRouterAnalyzer::new("key".into(), "".into())
            .with_reasoning_effort(Some("turbo".into()));
        let ctx = AnalysisContext {
            question: "Q".into(),
            solution: "S".into(),
            transcripts: vec!["T".into()],
        };
        let err = a.analyze("p", &ctx).unwrap_err();
        assert!(err.contains("invalid reasoning_effort"), "{err}");
    }

    #[test]
    fn blank_reasoning_effort_is_ignored() {
        let a =
            OpenRouterAnalyzer::new("k".into(), "".into()).with_reasoning_effort(Some(" ".into()));
        assert!(a.reasoning_effort.is_none());
    }

    #[test]
    fn extract_completion_reads_assistant_content() {
        let result = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "  analysis here  " } }]
        });
        assert_eq!(extract_completion(&result).unwrap(), "analysis here");
    }

    #[test]
    fn extract_completion_rejects_missing_content() {
        let result = serde_json::json!({ "choices": [] });
        let err = extract_completion(&result).unwrap_err();
        assert!(err.contains("no completion content"));
    }
}

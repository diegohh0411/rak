use crate::analyzer::{AnalysisContext, Analyzer};

const DEFAULT_MODEL: &str = "~google/gemini-flash-latest";
const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterAnalyzer {
    api_key: String,
    model: String,
}

impl OpenRouterAnalyzer {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model: if model.is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                model
            },
        }
    }
}

fn chat_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }]
    })
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

        let prompt = ctx.build_prompt(system_prompt);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let resp = client
            .post(CHAT_URL)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&chat_body(&self.model, &prompt))
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
        let body = chat_body("~google/gemini-flash-latest", "hello");
        assert_eq!(body["model"], "~google/gemini-flash-latest");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hello");
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

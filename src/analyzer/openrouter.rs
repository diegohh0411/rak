use crate::analyzer::{AnalysisContext, Analyzer};

const DEFAULT_MODEL: &str = "~google/gemini-flash-latest";
const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

pub struct OpenRouterAnalyzer {
    api_key: String,
    model: String,
    reasoning_effort: Option<String>,
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
            reasoning_effort: None,
        }
    }

    pub fn with_reasoning_effort(mut self, effort: Option<String>) -> Self {
        self.reasoning_effort = effort.filter(|e| !e.trim().is_empty());
        self
    }
}

/// Check `effort` against the model's `reasoning` metadata from
/// `GET /api/v1/models` (see OpenRouter's "Discovering per-model reasoning
/// options"). `models` is that endpoint's JSON response.
///
/// - model not listed: Ok (can't verify; let the chat request decide)
/// - `reasoning` absent: the model exposes no effort selection → Err
/// - `supported_efforts` null: every gateway effort is accepted → Ok
/// - `supported_efforts` list: effort must be in it
fn check_effort(models: &serde_json::Value, model: &str, effort: &str) -> Result<(), String> {
    // Variant suffixes (`:free`, `:nitro`, …) share the base model's metadata.
    let base = model.split(':').next().unwrap_or(model);
    let Some(entry) = models["data"].as_array().and_then(|list| {
        list.iter()
            .find(|m| m["id"] == model || m["id"] == base || m["canonical_slug"] == base)
    }) else {
        return Ok(());
    };

    let Some(reasoning) = entry.get("reasoning").filter(|r| !r.is_null()) else {
        return Err(format!(
            "model {model:?} does not support setting effort on OpenRouter; remove `effort` from rak.toml"
        ));
    };

    match reasoning
        .get("supported_efforts")
        .and_then(|v| v.as_array())
    {
        None => Ok(()),
        Some(allowed) if allowed.iter().any(|e| e == effort) => Ok(()),
        Some(allowed) => {
            let allowed: Vec<&str> = allowed.iter().filter_map(|e| e.as_str()).collect();
            Err(format!(
                "invalid effort {effort:?} for model {model:?} (supported: {})",
                allowed.join(", ")
            ))
        }
    }
}

/// Fetch the model catalogue. `None` on any failure: validation is
/// best-effort and must not block analysis when the catalogue is unreachable.
fn fetch_models(client: &reqwest::blocking::Client, key: &str) -> Option<serde_json::Value> {
    client
        .get(MODELS_URL)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .ok()
        .filter(|r| r.status().is_success())?
        .json()
        .ok()
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

        let prompt = ctx.build_prompt(system_prompt);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        if let Some(effort) = &self.reasoning_effort
            && let Some(models) = fetch_models(&client, &key)
        {
            check_effort(&models, &self.model, effort)?;
        }

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

    fn catalogue() -> serde_json::Value {
        serde_json::json!({ "data": [
            { "id": "~openai/gpt-sol-latest", "canonical_slug": "~openai/gpt-sol-latest",
              "reasoning": { "supported_efforts": ["max", "xhigh", "high", "medium", "low"] } },
            { "id": "acme/any-effort", "reasoning": { "supported_efforts": null } },
            { "id": "acme/no-reasoning" }
        ]})
    }

    #[test]
    fn check_effort_accepts_supported_value() {
        assert!(check_effort(&catalogue(), "~openai/gpt-sol-latest", "low").is_ok());
    }

    #[test]
    fn check_effort_rejects_unsupported_value_and_lists_allowed() {
        let err = check_effort(&catalogue(), "~openai/gpt-sol-latest", "none").unwrap_err();
        assert!(err.contains("invalid effort"), "{err}");
        assert!(err.contains("max, xhigh, high, medium, low"), "{err}");
    }

    #[test]
    fn check_effort_null_list_accepts_anything() {
        assert!(check_effort(&catalogue(), "acme/any-effort", "minimal").is_ok());
    }

    #[test]
    fn check_effort_rejects_model_without_reasoning() {
        let err = check_effort(&catalogue(), "acme/no-reasoning", "low").unwrap_err();
        assert!(err.contains("does not support setting effort"), "{err}");
    }

    #[test]
    fn check_effort_matches_variant_suffix() {
        assert!(check_effort(&catalogue(), "~openai/gpt-sol-latest:nitro", "none").is_err());
    }

    #[test]
    fn check_effort_unknown_model_is_not_blocked() {
        assert!(check_effort(&catalogue(), "acme/unlisted", "low").is_ok());
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

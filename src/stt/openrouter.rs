use std::path::Path;

use crate::config::resolve_api_key;
use crate::stt::Transcriber;

pub struct OpenRouterTranscriber {
    api_key: String,
    model: String,
}

impl OpenRouterTranscriber {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model: if model.is_empty() {
                "google/gemini-flash-2.5-lite".to_string()
            } else {
                model
            },
        }
    }
}

impl Transcriber for OpenRouterTranscriber {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn transcribe(&self, audio_path: &Path) -> Result<String, String> {
        let key = resolve_api_key(&self.api_key, "OPENROUTER_API_KEY")?;

        let audio_bytes =
            std::fs::read(audio_path).map_err(|e| format!("failed to read audio: {e}"))?;
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let body = serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "Provide a verbatim transcription of the following audio recording."},
                    {"type": "image_url", "image_url": {"url": format!("data:audio/mp3;base64,{b64}")}}
                ]
            }]
        });

        let resp = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("OpenRouter API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().unwrap_or_default();
            return Err(format!("OpenRouter API error: {status} {body_text}"));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("failed to parse OpenRouter response: {e}"))?;

        result["choices"][0]["message"]["content"]
            .as_str()
            .map(|s: &str| s.to_string())
            .ok_or_else(|| "OpenRouter returned no content".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_api_key_errors_on_transcribe() {
        let t =
            OpenRouterTranscriber::new("".to_string(), "google/gemini-flash-2.5-lite".to_string());
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(
            err.contains("OPENROUTER_API_KEY"),
            "error should mention env var: {err}"
        );
    }

    #[test]
    fn new_defaults_model_when_empty() {
        let t = OpenRouterTranscriber::new("key".to_string(), "".to_string());
        assert_eq!(t.model, "google/gemini-flash-2.5-lite");
    }
}

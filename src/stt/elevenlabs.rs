use std::path::Path;

use crate::config::resolve_api_key;
use crate::stt::Transcriber;

pub struct ElevenLabsTranscriber {
    api_key: String,
    model: String,
}

impl ElevenLabsTranscriber {
    pub fn new(api_key: String, model: String) -> Self {
        Self { api_key, model }
    }
}

impl Transcriber for ElevenLabsTranscriber {
    fn name(&self) -> &str {
        "elevenlabs"
    }

    fn transcribe(&self, audio_path: &Path) -> Result<String, String> {
        let key = resolve_api_key(&self.api_key, "ELEVENLABS_API_KEY")?;

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let form = reqwest::blocking::multipart::Form::new()
            .file("file", audio_path)
            .map_err(|e| format!("failed to attach file: {e}"))?
            .text("model_id", self.model.clone());

        let resp = client
            .post("https://api.elevenlabs.io/v1/speech-to-text")
            .header("xi-api-key", &key)
            .multipart(form)
            .send()
            .map_err(|e| format!("ElevenLabs API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("ElevenLabs API error: {status} {body}"));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("failed to parse ElevenLabs response: {e}"))?;

        result["text"]
            .as_str()
            .map(|s: &str| s.to_string())
            .ok_or_else(|| "ElevenLabs returned no text field".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_api_key_errors_on_transcribe() {
        let t = ElevenLabsTranscriber::new("".to_string(), "scribe_v1".to_string());
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(
            err.contains("ELEVENLABS_API_KEY"),
            "error should mention env var: {err}"
        );
    }

    #[test]
    fn new_with_explicit_key() {
        let t = ElevenLabsTranscriber::new("sk-test-key".to_string(), "scribe_v1".to_string());
        assert_eq!(t.name(), "elevenlabs");
    }
}

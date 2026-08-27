use std::path::Path;

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
                "x-ai/grok-stt-1.0".to_string()
            } else {
                model
            },
        }
    }
}

fn format_api_error(status: reqwest::StatusCode, body: &str) -> String {
    let err = format!("OpenRouter API error: {status} {body}");
    if status == reqwest::StatusCode::BAD_REQUEST {
        format!(
            "{err}\nOpenRouter transcribe: only STT models are allowed (e.g. x-ai/grok-stt-1.0). Chat and multimodal models will not work."
        )
    } else {
        err
    }
}

fn transcription_body(model: &str, b64: &str, format: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "input_audio": {
            "data": b64,
            "format": format
        }
    })
}

fn extract_transcript(result: &serde_json::Value) -> Result<String, String> {
    result["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "OpenRouter returned no text field".to_string())
}

fn audio_format(path: &Path) -> Result<&'static str, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mp3" => Ok("mp3"),
        "wav" => Ok("wav"),
        "flac" => Ok("flac"),
        "m4a" => Ok("m4a"),
        "ogg" => Ok("ogg"),
        "webm" => Ok("webm"),
        "aac" => Ok("aac"),
        _ => Err(format!(
            "unsupported audio format for {:?}; OpenRouter STT accepts mp3, wav, flac, m4a, ogg, webm, aac",
            path.file_name().unwrap_or_default()
        )),
    }
}

impl Transcriber for OpenRouterTranscriber {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn transcribe(&self, audio_path: &Path) -> Result<String, String> {
        let key = if self.api_key.is_empty() {
            std::env::var("OPENROUTER_API_KEY").map_err(|_| {
                "OpenRouter API key not set. Run 'rak login openrouter'.".to_string()
            })?
        } else {
            self.api_key.clone()
        };

        let format = audio_format(audio_path)?;
        let audio_bytes =
            std::fs::read(audio_path).map_err(|e| format!("failed to read audio: {e}"))?;
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let body = transcription_body(&self.model, &b64, format);

        let resp = client
            .post("https://openrouter.ai/api/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(&body)
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

        extract_transcript(&result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_api_key_errors_on_transcribe() {
        unsafe { std::env::remove_var("OPENROUTER_API_KEY") };
        let t = OpenRouterTranscriber::new("".to_string(), "x-ai/grok-stt-1.0".to_string());
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(
            err.contains("rak login openrouter"),
            "error should mention login: {err}"
        );
    }

    #[test]
    fn new_defaults_model_when_empty() {
        let t = OpenRouterTranscriber::new("key".to_string(), "".to_string());
        assert_eq!(t.model, "x-ai/grok-stt-1.0");
    }

    #[test]
    fn audio_format_from_known_extensions() {
        for (path, expected) in [
            ("attempt-1.mp3", "mp3"),
            ("note.wav", "wav"),
            ("clip.flac", "flac"),
            ("voice.m4a", "m4a"),
            ("rec.ogg", "ogg"),
            ("a.webm", "webm"),
            ("b.aac", "aac"),
            ("C.MP3", "mp3"),
        ] {
            assert_eq!(audio_format(Path::new(path)).unwrap(), expected, "{path}");
        }
    }

    #[test]
    fn audio_format_rejects_unknown_or_missing_extension() {
        let err = audio_format(Path::new("notes.txt")).unwrap_err();
        assert!(
            err.contains("unsupported audio format"),
            "error should mention unsupported format: {err}"
        );
        let err = audio_format(Path::new("attempt-1")).unwrap_err();
        assert!(
            err.contains("unsupported audio format"),
            "error should mention unsupported format: {err}"
        );
    }

    #[test]
    fn extract_transcript_reads_text_field() {
        let result = serde_json::json!({
            "text": "two sum brute force first then hash map",
            "usage": {
                "cost": 0.0005,
                "seconds": 9.2,
                "total_tokens": 113
            }
        });
        assert_eq!(
            extract_transcript(&result).unwrap(),
            "two sum brute force first then hash map"
        );
    }

    #[test]
    fn extract_transcript_errors_when_text_missing() {
        let result = serde_json::json!({
            "choices": [{
                "message": { "content": "chat-style payload" }
            }]
        });
        let err = extract_transcript(&result).unwrap_err();
        assert!(
            err.contains("no text"),
            "error should mention missing text: {err}"
        );
    }

    #[test]
    fn transcription_body_uses_input_audio_not_chat_messages() {
        let body = transcription_body("x-ai/grok-stt-1.0", "abc123", "mp3");
        assert_eq!(body["model"], "x-ai/grok-stt-1.0");
        assert_eq!(body["input_audio"]["data"], "abc123");
        assert_eq!(body["input_audio"]["format"], "mp3");
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn format_api_error_400_says_only_stt_models_allowed() {
        let body = r#"{"error":{"message":"google/gemini-flash-2.5-lite is not a transcription model","code":400}}"#;
        let err = format_api_error(reqwest::StatusCode::BAD_REQUEST, body);
        assert!(
            err.contains("only STT models are allowed"),
            "400 should say only STT models are allowed: {err}"
        );
        assert!(
            err.contains("not a transcription model"),
            "400 should keep OpenRouter's body: {err}"
        );
    }

    #[test]
    fn format_api_error_non_400_keeps_status_and_body() {
        let err = format_api_error(reqwest::StatusCode::UNAUTHORIZED, "invalid key");
        assert!(err.contains("401"), "error should include status: {err}");
        assert!(
            err.contains("invalid key"),
            "error should include body: {err}"
        );
        assert!(
            !err.contains("only STT models are allowed"),
            "non-400 should not claim the model type is wrong: {err}"
        );
    }
}

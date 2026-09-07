use std::path::{Path, PathBuf};
use std::process::Command;

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

const CHUNK_DURATION_SECS: f64 = 20.0 * 60.0;

fn chunk_ranges(total_secs: f64) -> Vec<(f64, f64)> {
    if total_secs <= 0.0 {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = 0.0;
    while start < total_secs {
        let len = (total_secs - start).min(CHUNK_DURATION_SECS);
        ranges.push((start, len));
        start += len;
    }
    ranges
}

fn join_transcripts(parts: &[String]) -> String {
    parts
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn audio_duration_secs(audio_path: &Path) -> Option<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(audio_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}

fn extract_chunk(
    input: &Path,
    start_secs: f64,
    duration_secs: f64,
    output: &Path,
) -> Result<(), String> {
    let result = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args([
            "-ss",
            &format!("{start_secs:.3}"),
            "-t",
            &format!("{duration_secs:.3}"),
            "-vn",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "64k",
        ])
        .arg(output)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "ffmpeg is not installed or not on PATH".to_string()
            } else {
                format!("failed to run ffmpeg: {e}")
            }
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let detail = stderr.trim();
        if detail.is_empty() {
            return Err(format!(
                "ffmpeg failed to extract audio chunk at {start_secs:.1}s"
            ));
        }
        return Err(format!(
            "ffmpeg failed to extract audio chunk at {start_secs:.1}s: {detail}"
        ));
    }
    if !output.exists() {
        return Err(format!(
            "ffmpeg did not write chunk at {}",
            output.display()
        ));
    }
    Ok(())
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn make_temp_dir() -> Result<TempDir, String> {
    let dir = std::env::temp_dir().join(format!(
        "rak-stt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create temp dir: {e}"))?;
    Ok(TempDir(dir))
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

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        if let Some(duration) = audio_duration_secs(audio_path) {
            let ranges = chunk_ranges(duration);
            if ranges.len() > 1 {
                return transcribe_chunked(&client, &key, &self.model, audio_path, &ranges);
            }
        }

        transcribe_path(&client, &key, &self.model, audio_path)
    }
}

fn transcribe_chunked(
    client: &reqwest::blocking::Client,
    key: &str,
    model: &str,
    audio_path: &Path,
    ranges: &[(f64, f64)],
) -> Result<String, String> {
    let tmp = make_temp_dir()?;
    let mut parts = Vec::new();
    let n = ranges.len();
    for (i, (start, duration)) in ranges.iter().enumerate() {
        println!("  chunk {}/{n}...", i + 1);
        let chunk_path = tmp.0.join(format!("chunk-{i:03}.mp3"));
        extract_chunk(audio_path, *start, *duration, &chunk_path)?;
        parts.push(transcribe_path(client, key, model, &chunk_path)?);
    }
    Ok(join_transcripts(&parts))
}

fn transcribe_path(
    client: &reqwest::blocking::Client,
    key: &str,
    model: &str,
    audio_path: &Path,
) -> Result<String, String> {
    let format = audio_format(audio_path)?;
    let audio_bytes =
        std::fs::read(audio_path).map_err(|e| format!("failed to read audio: {e}"))?;
    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &audio_bytes);

    let body = transcription_body(model, &b64, format);

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

    #[test]
    fn chunk_ranges_keeps_files_under_20_minutes_as_one_piece() {
        assert_eq!(chunk_ranges(19.0 * 60.0), vec![(0.0, 1140.0)]);
    }

    #[test]
    fn chunk_ranges_keeps_exactly_20_minutes_as_one_piece() {
        assert_eq!(chunk_ranges(20.0 * 60.0), vec![(0.0, 1200.0)]);
    }

    #[test]
    fn chunk_ranges_splits_50_minutes_into_20_minute_pieces() {
        assert_eq!(
            chunk_ranges(50.0 * 60.0),
            vec![(0.0, 1200.0), (1200.0, 1200.0), (2400.0, 600.0)]
        );
    }

    #[test]
    fn chunk_ranges_splits_the_netic_interview_length() {
        assert_eq!(
            chunk_ranges(3082.5),
            vec![(0.0, 1200.0), (1200.0, 1200.0), (2400.0, 682.5)]
        );
    }

    #[test]
    fn join_transcripts_concatenates_chunks_with_a_blank_line() {
        assert_eq!(
            join_transcripts(&[
                "first twenty minutes.".to_string(),
                "next twenty minutes.".to_string()
            ]),
            "first twenty minutes.\n\nnext twenty minutes."
        );
    }

    #[test]
    fn join_transcripts_skips_empty_chunks_and_trims() {
        assert_eq!(
            join_transcripts(&[
                "  hello there.  ".to_string(),
                "   ".to_string(),
                "goodbye.\n".to_string()
            ]),
            "hello there.\n\ngoodbye."
        );
    }

    #[test]
    fn extract_chunk_cuts_a_time_slice_with_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.mp3");
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=5",
                "-q:a",
                "9",
            ])
            .arg(&src)
            .status()
            .expect("ffmpeg should be installed");
        assert!(status.success(), "failed to generate test audio");

        let chunk = dir.path().join("chunk.mp3");
        extract_chunk(&src, 2.0, 2.0, &chunk).unwrap();
        let dur = audio_duration_secs(&chunk).unwrap();
        assert!((dur - 2.0).abs() < 0.35, "expected ~2s chunk, got {dur}s");
    }
}

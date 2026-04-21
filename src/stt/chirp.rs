use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::{Datelike, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;

use crate::stt::Transcriber;

pub struct ChirpTranscriber {
    api_key: String,
    project_id: String,
    monthly_cap_minutes: f64,
    service_account_path: String,
}

impl ChirpTranscriber {
    pub fn new(
        api_key: String,
        project_id: String,
        monthly_cap_minutes: f64,
        service_account_path: String,
    ) -> Self {
        Self {
            api_key,
            project_id,
            monthly_cap_minutes,
            service_account_path,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.api_key.is_empty() {
            return Err("Chirp api_key is not set. Run 'rak login chirp'.".to_string());
        }
        if self.project_id.is_empty() {
            return Err("Chirp project_id is not set. Run 'rak login chirp'.".to_string());
        }
        if self.service_account_path.is_empty() {
            return Err(
                "Chirp service_account_path is not set. Run 'rak login chirp'.".to_string(),
            );
        }
        Ok(())
    }
}

impl Transcriber for ChirpTranscriber {
    fn name(&self) -> &str {
        "chirp"
    }

    fn transcribe(&self, audio_path: &Path) -> Result<String, String> {
        self.validate()?;

        let duration_secs = audio_duration_secs(audio_path).unwrap_or(0.0);

        let access_token = get_access_token(&PathBuf::from(&self.service_account_path))?;

        let used_secs = query_usage_seconds(&access_token, &self.project_id)?;

        check_cap(used_secs, duration_secs, self.monthly_cap_minutes)?;

        let audio_bytes = std::fs::read(audio_path)
            .map_err(|e| format!("failed to read audio file: {e}"))?;
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let url = format!(
            "https://speech.googleapis.com/v2/projects/{}/locations/global/recognizers/_:recognize?key={}",
            self.project_id, self.api_key
        );

        let body = serde_json::json!({
            "config": {
                "model": "chirp_2",
                "auto_decoding_config": {}
            },
            "content": audio_b64
        });

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| format!("Chirp API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(format!("Chirp API error: {status} {body}"));
        }

        let result: serde_json::Value = resp
            .json()
            .map_err(|e| format!("failed to parse Chirp response: {e}"))?;

        parse_transcript(&result)
    }
}

fn audio_duration_secs(audio_path: &Path) -> Option<f64> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            audio_path.to_str()?,
        ])
        .output()
        .ok()?;

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.trim().parse::<f64>().ok()
}

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: i64,
    exp: i64,
}

fn get_access_token(sa_path: &Path) -> Result<String, String> {
    let sa_json = std::fs::read_to_string(sa_path)
        .map_err(|e| format!("failed to read service account file: {e}"))?;

    let sa: serde_json::Value = serde_json::from_str(&sa_json)
        .map_err(|e| format!("failed to parse service account JSON: {e}"))?;

    let client_email = sa["client_email"]
        .as_str()
        .ok_or_else(|| "service account missing client_email".to_string())?;

    let private_key = sa["private_key"]
        .as_str()
        .ok_or_else(|| "service account missing private_key".to_string())?;

    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        iss: client_email.to_string(),
        scope: "https://www.googleapis.com/auth/monitoring.read".to_string(),
        aud: "https://oauth2.googleapis.com/token".to_string(),
        iat: now,
        exp: now + 3600,
    };

    let encoding_key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| format!("failed to load RSA private key: {e}"))?;

    let header = Header::new(Algorithm::RS256);
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key)
        .map_err(|e| format!("failed to sign JWT: {e}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", &jwt),
    ];

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .map_err(|e| format!("OAuth2 token request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("OAuth2 token error: {status} {body}"));
    }

    let token_json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("failed to parse OAuth2 token response: {e}"))?;

    token_json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "OAuth2 response missing access_token".to_string())
}

fn query_usage_seconds(access_token: &str, project_id: &str) -> Result<f64, String> {
    let now = Utc::now();
    let start_of_month = now
        .date_naive()
        .with_day(1)
        .and_then(|d: chrono::NaiveDate| d.and_hms_opt(0, 0, 0))
        .map(|dt: chrono::NaiveDateTime| dt.and_utc())
        .ok_or_else(|| "failed to compute billing period start".to_string())?;

    let start_rfc3339 = start_of_month.to_rfc3339();
    let end_rfc3339 = now.to_rfc3339();

    let url = format!(
        "https://monitoring.googleapis.com/v3/projects/{}/timeSeries",
        project_id
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))?;

    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .query(&[
            (
                "filter",
                r#"metric.type="speech.googleapis.com/audio_seconds""#,
            ),
            ("interval.startTime", &start_rfc3339),
            ("interval.endTime", &end_rfc3339),
        ])
        .send()
        .map_err(|e| format!("Monitoring API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Monitoring API error: {status} {body}"));
    }

    let response: serde_json::Value = resp
        .json()
        .map_err(|e| format!("failed to parse Monitoring API response: {e}"))?;

    Ok(sum_audio_seconds(&response))
}

fn sum_audio_seconds(response: &serde_json::Value) -> f64 {
    let mut total = 0.0f64;

    if let Some(time_series) = response["timeSeries"].as_array() {
        for series in time_series {
            if let Some(points) = series["points"].as_array() {
                for point in points {
                    let value = &point["value"];
                    if let Some(s) = value["int64Value"].as_str() {
                        total += s.parse::<f64>().unwrap_or(0.0);
                    } else if let Some(d) = value["doubleValue"].as_f64() {
                        total += d;
                    }
                }
            }
        }
    }

    total
}

fn parse_transcript(response: &serde_json::Value) -> Result<String, String> {
    response["results"][0]["alternatives"][0]["transcript"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Chirp returned no transcript".to_string())
}

fn check_cap(used_secs: f64, file_secs: f64, cap_minutes: f64) -> Result<(), String> {
    if used_secs + file_secs >= cap_minutes * 60.0 {
        let used_min = used_secs / 60.0;
        let file_min = file_secs / 60.0;
        return Err(format!(
            "Chirp usage cap reached: {used_min:.1}/{cap_minutes} min (this file would add {file_min:.1} min)"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_transcriber(api_key: &str, project_id: &str) -> ChirpTranscriber {
        ChirpTranscriber::new(
            api_key.to_string(),
            project_id.to_string(),
            60.0,
            "/some/path/sa.json".to_string(),
        )
    }

    #[test]
    fn missing_api_key_errors_with_login_hint() {
        let t = make_transcriber("", "my-project");
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(
            err.contains("rak login chirp"),
            "error should mention 'rak login chirp': {err}"
        );
    }

    #[test]
    fn missing_project_id_errors_with_login_hint() {
        let t = make_transcriber("key", "");
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(
            err.contains("rak login chirp"),
            "error should mention 'rak login chirp': {err}"
        );
    }

    #[test]
    fn sum_audio_seconds_empty_response() {
        let resp = serde_json::json!({});
        assert_eq!(sum_audio_seconds(&resp), 0.0);
    }

    #[test]
    fn sum_audio_seconds_with_int64_values() {
        let resp = serde_json::json!({
            "timeSeries": [
                { "points": [{ "value": { "int64Value": "3600" } }] },
                { "points": [{ "value": { "int64Value": "1800" } }] }
            ]
        });
        assert_eq!(sum_audio_seconds(&resp), 5400.0);
    }

    #[test]
    fn sum_audio_seconds_with_double_values() {
        let resp = serde_json::json!({
            "timeSeries": [
                { "points": [{ "value": { "doubleValue": 120.5 } }] }
            ]
        });
        assert!((sum_audio_seconds(&resp) - 120.5).abs() < 0.001);
    }

    #[test]
    fn parse_transcript_success() {
        let resp = serde_json::json!({
            "results": [{
                "alternatives": [{ "transcript": "hello world", "confidence": 0.95 }]
            }]
        });
        assert_eq!(parse_transcript(&resp).unwrap(), "hello world");
    }

    #[test]
    fn parse_transcript_empty_results_errors() {
        let resp = serde_json::json!({ "results": [] });
        let err = parse_transcript(&resp).unwrap_err();
        assert!(err.contains("no transcript"), "error: {err}");
    }

    #[test]
    fn parse_transcript_missing_results_errors() {
        let resp = serde_json::json!({});
        let err = parse_transcript(&resp).unwrap_err();
        assert!(err.contains("no transcript"), "error: {err}");
    }

    #[test]
    fn cap_check_under_cap_passes() {
        let result = check_cap(600.0, 300.0, 60.0);
        assert!(result.is_ok());
    }

    #[test]
    fn cap_check_over_cap_errors() {
        let result = check_cap(3480.0, 300.0, 60.0);
        let err = result.unwrap_err();
        assert!(err.contains("cap reached"), "error: {err}");
        assert!(err.contains("58.0"), "should show used minutes: {err}");
        assert!(err.contains("60"), "should show cap: {err}");
    }
}

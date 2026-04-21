use std::io::{self, Write};
use std::path::PathBuf;

use serde_json::Map;

use crate::credentials;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct ProviderField {
    pub name: &'static str,
    pub prompt: &'static str,
    pub hidden: bool,
}

// ---------------------------------------------------------------------------
// Pure helper functions
// ---------------------------------------------------------------------------

pub fn provider_fields(provider: &str) -> Option<Vec<ProviderField>> {
    match provider {
        "elevenlabs" => Some(vec![ProviderField {
            name: "api_key",
            prompt: "API Key",
            hidden: true,
        }]),
        "openrouter" => Some(vec![ProviderField {
            name: "api_key",
            prompt: "API Key",
            hidden: true,
        }]),
        "chirp" => Some(vec![
            ProviderField {
                name: "api_key",
                prompt: "API Key",
                hidden: true,
            },
            ProviderField {
                name: "project_id",
                prompt: "Project ID",
                hidden: false,
            },
            ProviderField {
                name: "monthly_cap_minutes",
                prompt: "Monthly cap (minutes) [60]",
                hidden: false,
            },
            ProviderField {
                name: "service_account_path",
                prompt: "Service Account Path",
                hidden: false,
            },
        ]),
        _ => None,
    }
}

pub fn display_name(provider: &str) -> &str {
    match provider {
        "chirp" => "Chirp (Google Cloud Speech-to-Text)",
        "elevenlabs" => "ElevenLabs",
        "openrouter" => "OpenRouter",
        other => other,
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

// ---------------------------------------------------------------------------
// Service account copy
// ---------------------------------------------------------------------------

fn copy_service_account(source_path: &str) -> Result<String, String> {
    let src = expand_tilde(source_path);
    let content = std::fs::read_to_string(&src)
        .map_err(|e| format!("cannot read service account file {}: {e}", src.display()))?;

    // Validate required fields
    let parsed: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("service account is not valid JSON: {e}"))?;
    if parsed.get("client_email").map(|v| v.is_null()).unwrap_or(true) {
        return Err("service account JSON is missing 'client_email'".to_string());
    }
    if parsed.get("private_key").map(|v| v.is_null()).unwrap_or(true) {
        return Err("service account JSON is missing 'private_key'".to_string());
    }

    let dest = credentials::store_dir().join("gcp-service-account.json");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create dir: {e}"))?;
    }
    std::fs::write(&dest, &content)
        .map_err(|e| format!("cannot write service account to {}: {e}", dest.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot set permissions on service account file: {e}"))?;
    }

    println!("✓ Service account saved to {}", dest.display());
    Ok(dest.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// Interactive run
// ---------------------------------------------------------------------------

pub fn run(provider: Option<String>) -> Result<(), String> {
    let provider = match provider {
        Some(p) => p,
        None => {
            println!("Available providers: elevenlabs, openrouter, chirp");
            print!("Provider: ");
            io::stdout().flush().ok();
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|e| format!("failed to read input: {e}"))?;
            input.trim().to_string()
        }
    };

    let fields = provider_fields(&provider)
        .ok_or_else(|| format!("unknown provider: {provider}"))?;

    // Load existing store and check for overwrite
    let mut store = credentials::load()?;

    if store.contains_key(&provider) {
        print!("Credentials exist for {provider}. Overwrite? [y/N]: ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|e| format!("failed to read input: {e}"))?;
        let answer = answer.trim();
        if !answer.to_lowercase().starts_with('y') {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("{} setup\n", display_name(&provider));

    let mut creds: Map<String, serde_json::Value> = Map::new();

    for field in &fields {
        let raw = if field.hidden {
            rpassword::prompt_password(format!("{}: ", field.prompt))
                .map_err(|e| format!("failed to read password: {e}"))?
        } else {
            print!("{}: ", field.prompt);
            io::stdout().flush().ok();
            let mut line = String::new();
            io::stdin()
                .read_line(&mut line)
                .map_err(|e| format!("failed to read input: {e}"))?;
            line.trim().to_string()
        };

        match field.name {
            "monthly_cap_minutes" => {
                let raw = raw.trim();
                let val: f64 = if raw.is_empty() {
                    60.0
                } else {
                    raw.parse::<f64>()
                        .map_err(|_| format!("'{raw}' is not a valid number for monthly_cap_minutes"))?
                };
                creds.insert(field.name.to_string(), serde_json::json!(val));
            }
            "service_account_path" => {
                let dest_path = copy_service_account(raw.trim())?;
                creds.insert(field.name.to_string(), serde_json::json!(dest_path));
            }
            _ => {
                creds.insert(field.name.to_string(), serde_json::json!(raw));
            }
        }
    }

    store.insert(provider.clone(), serde_json::Value::Object(creds));
    credentials::save(&store)?;
    println!("\n✓ Credentials saved.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_fields_elevenlabs_has_hidden_api_key() {
        let fields = provider_fields("elevenlabs").expect("elevenlabs should be known");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "api_key");
        assert!(fields[0].hidden, "api_key should be hidden");
    }

    #[test]
    fn provider_fields_openrouter_has_hidden_api_key() {
        let fields = provider_fields("openrouter").expect("openrouter should be known");
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "api_key");
        assert!(fields[0].hidden);
    }

    #[test]
    fn provider_fields_chirp_has_four_fields() {
        let fields = provider_fields("chirp").expect("chirp should be known");
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].name, "api_key");
        assert!(fields[0].hidden);
        assert_eq!(fields[1].name, "project_id");
        assert!(!fields[1].hidden);
        assert_eq!(fields[2].name, "monthly_cap_minutes");
        assert!(!fields[2].hidden);
        assert_eq!(fields[3].name, "service_account_path");
        assert!(!fields[3].hidden);
    }

    #[test]
    fn provider_fields_unknown_returns_none() {
        assert!(provider_fields("unknown-provider").is_none());
    }

    #[test]
    fn expand_tilde_replaces_home() {
        let result = expand_tilde("~/Downloads/key.json");
        assert!(!result.to_string_lossy().starts_with('~'));
        assert!(result.to_string_lossy().ends_with("Downloads/key.json"));
    }

    #[test]
    fn expand_tilde_passes_through_absolute_path() {
        let result = expand_tilde("/absolute/path.json");
        assert_eq!(result, std::path::PathBuf::from("/absolute/path.json"));
    }

    #[test]
    fn display_name_known_providers() {
        assert_eq!(display_name("chirp"), "Chirp (Google Cloud Speech-to-Text)");
        assert_eq!(display_name("elevenlabs"), "ElevenLabs");
        assert_eq!(display_name("openrouter"), "OpenRouter");
    }
}

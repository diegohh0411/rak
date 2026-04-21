# Chirp STT Provider + Credential Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Google Cloud Chirp 2 STT provider with a credential store so all provider secrets live in `~/.local/share/rak/credentials.json` (not in git-trackable config files), plus a `rak login <provider>` command to manage those credentials.

**Architecture:** A new `src/credentials.rs` module owns the credential store (atomic JSON file, 0600 permissions). `src/commands/login.rs` provides the interactive setup flow. `src/stt/chirp.rs` implements the `Transcriber` trait, fetching an OAuth2 token from a service account to pre-check monthly usage against the Monitoring API before sending audio to Speech-to-Text v2. `src/commands/transcribe.rs` merges credential store values into the JSON config it passes to providers, so existing providers (elevenlabs, openrouter) keep working unchanged.

**Tech Stack:** `dirs = "6"` (data dir path), `jsonwebtoken = "9"` (JWT signing for GCP OAuth2), `rpassword = "7"` (hidden terminal input), `reqwest` blocking client (already present), `chrono` (billing period dates, already present), `base64` (audio encoding, already present).

---

## File Map

| Action | Path | Responsibility |
|--------|------|---------------|
| Create | `src/credentials.rs` | load/save credential store, get_provider, store_dir |
| Create | `src/commands/login.rs` | interactive per-provider credential prompts |
| Create | `src/stt/chirp.rs` | ChirpTranscriber: usage pre-check + transcription |
| Modify | `Cargo.toml` | add dirs, jsonwebtoken, rpassword |
| Modify | `src/main.rs` | add `mod credentials`, Login subcommand, dispatch |
| Modify | `src/commands/mod.rs` | add `pub mod login` |
| Modify | `src/stt/mod.rs` | register chirp provider |
| Modify | `src/commands/transcribe.rs` | merge credentials from store into json_config |
| Modify | `src/config.rs` | remove `api_key` field from `ProviderConfig` |
| Modify | `src/commands/init.rs` | remove api_key from rak.toml template; add GOOGLE_SPEECH_API_KEY to .env template |

---

## Task 1: Add Cargo.toml Dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add the three new dependencies**

In `Cargo.toml`, add these lines to `[dependencies]`:

```toml
dirs = "6"
jsonwebtoken = "9"
rpassword = "7"
```

- [ ] **Step 2: Verify it compiles**

```bash
cargo check
```

Expected: no errors (new deps compile cleanly).

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add dirs, jsonwebtoken, rpassword dependencies"
```

---

## Task 2: Credential Store (`src/credentials.rs`)

**Files:**
- Create: `src/credentials.rs`
- Modify: `src/main.rs` (add `mod credentials;`)

The store is a JSON object at `dirs::data_dir() / "rak" / "credentials.json"`. Atomic writes go through a `.tmp` file then `rename`. File permissions are set to 0600 on Unix.

- [ ] **Step 1: Write failing tests**

Create `src/credentials.rs` with just the test module (module body comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn store_dir_ends_with_rak() {
        let dir = store_dir();
        assert!(dir.ends_with("rak"), "store_dir should end with 'rak', got {:?}", dir);
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let result = load_from(std::path::Path::new("/nonexistent/credentials.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert("elevenlabs".to_string(), serde_json::json!({"api_key": "sk-test"}));
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded["elevenlabs"]["api_key"], "sk-test");
    }

    #[test]
    fn get_provider_returns_value_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert("chirp".to_string(), serde_json::json!({"project_id": "my-proj"}));
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        let val = loaded.get("chirp").cloned();
        assert!(val.is_some());
        assert_eq!(val.unwrap()["project_id"], "my-proj");
    }

    #[test]
    fn get_provider_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert!(loaded.get("nonexistent").is_none());
    }

    #[test]
    #[cfg(unix)]
    fn saved_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let meta = fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "credentials.json should be 0600, got {:o}", mode);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib credentials
```

Expected: compile error — `store_dir`, `load_from`, `save_to` not defined.

- [ ] **Step 3: Implement the module**

Replace the file with the full implementation:

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn store_dir() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine data directory")
        .join("rak")
}

fn credentials_path() -> PathBuf {
    store_dir().join("credentials.json")
}

pub fn load() -> Result<HashMap<String, serde_json::Value>, String> {
    load_from(&credentials_path())
}

pub fn load_from(path: &Path) -> Result<HashMap<String, serde_json::Value>, String> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| format!("malformed credentials file: {e}"))
}

pub fn save(data: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = credentials_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    save_to(data, &path)
}

pub fn save_to(data: &HashMap<String, serde_json::Value>, path: &Path) -> Result<(), String> {
    let content = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &content).map_err(|e| format!("failed to write credentials: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set permissions: {e}"))?;
    }

    std::fs::rename(&tmp, path).map_err(|e| format!("failed to save credentials: {e}"))
}

pub fn get_provider(name: &str) -> Option<serde_json::Value> {
    load().ok()?.remove(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn store_dir_ends_with_rak() {
        let dir = store_dir();
        assert!(dir.ends_with("rak"), "store_dir should end with 'rak', got {:?}", dir);
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let result = load_from(std::path::Path::new("/nonexistent/credentials.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert("elevenlabs".to_string(), serde_json::json!({"api_key": "sk-test"}));
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded["elevenlabs"]["api_key"], "sk-test");
    }

    #[test]
    fn get_provider_returns_value_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert("chirp".to_string(), serde_json::json!({"project_id": "my-proj"}));
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        let val = loaded.get("chirp").cloned();
        assert!(val.is_some());
        assert_eq!(val.unwrap()["project_id"], "my-proj");
    }

    #[test]
    fn get_provider_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert!(loaded.get("nonexistent").is_none());
    }

    #[test]
    #[cfg(unix)]
    fn saved_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let meta = fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "credentials.json should be 0600, got {:o}", mode);
    }
}
```

- [ ] **Step 4: Register module in `src/main.rs`**

Add `mod credentials;` after the existing module declarations in `src/main.rs`:

```rust
mod credentials;
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test --lib credentials
```

Expected: all 5 tests pass (6 on unix including permissions test).

- [ ] **Step 6: Commit**

```bash
git add src/credentials.rs src/main.rs
git commit -m "feat(credentials): add credential store with atomic save and 0600 permissions"
```

---

## Task 3: `rak login` Command (`src/commands/login.rs`)

**Files:**
- Create: `src/commands/login.rs`

Provides interactive per-provider credential prompts. Secret fields use `rpassword` for hidden input. For `chirp`, copies the service account JSON file into the store directory and stores the destination path.

- [ ] **Step 1: Write failing tests**

Create `src/commands/login.rs` with just tests first:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib commands::login
```

Expected: compile error — `provider_fields`, `expand_tilde`, `display_name` not defined.

- [ ] **Step 3: Implement the module**

Replace the file with the full implementation:

```rust
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::credentials;

pub struct ProviderField {
    pub name: &'static str,
    pub prompt: &'static str,
    pub hidden: bool,
}

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
            ProviderField { name: "api_key", prompt: "API Key", hidden: true },
            ProviderField { name: "project_id", prompt: "GCP Project ID", hidden: false },
            ProviderField {
                name: "monthly_cap_minutes",
                prompt: "Monthly cap (minutes) [60]",
                hidden: false,
            },
            ProviderField {
                name: "service_account_path",
                prompt: "Path to service account JSON",
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
        _ => provider,
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

pub fn run(provider: Option<String>) -> Result<(), String> {
    let provider = match provider {
        Some(p) => p,
        None => {
            println!("Available providers: elevenlabs, openrouter, chirp");
            print!("Provider: ");
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut s = String::new();
            io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
            s.trim().to_string()
        }
    };

    let fields = provider_fields(&provider).ok_or_else(|| {
        format!(
            "Unknown provider '{}'. Available: elevenlabs, openrouter, chirp",
            provider
        )
    })?;

    let mut store = credentials::load()?;

    if store.contains_key(&provider) {
        print!("Credentials exist for {provider}. Overwrite? [y/N]: ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut s = String::new();
        io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
        if !s.trim().to_lowercase().starts_with('y') {
            println!("Aborted.");
            return Ok(());
        }
    }

    println!("{} setup\n", display_name(&provider));

    let mut creds = serde_json::Map::new();
    for field in &fields {
        let raw = if field.hidden {
            rpassword::prompt_password(format!("{}: ", field.prompt))
                .map_err(|e| format!("failed to read input: {e}"))?
        } else {
            print!("{}: ", field.prompt);
            io::stdout().flush().map_err(|e| e.to_string())?;
            let mut s = String::new();
            io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
            s.trim().to_string()
        };

        if field.name == "monthly_cap_minutes" {
            let n: f64 = if raw.is_empty() {
                60.0
            } else {
                raw.parse()
                    .map_err(|_| "monthly_cap_minutes must be a number".to_string())?
            };
            creds.insert(field.name.to_string(), serde_json::Value::from(n));
        } else if field.name == "service_account_path" {
            copy_service_account(&raw)?;
            let dest = credentials::store_dir().join("gcp-service-account.json");
            creds.insert(
                field.name.to_string(),
                serde_json::Value::from(dest.to_string_lossy().to_string()),
            );
        } else {
            creds.insert(field.name.to_string(), serde_json::Value::from(raw));
        }
    }

    store.insert(provider.clone(), serde_json::Value::Object(creds));
    credentials::save(&store)?;
    println!("\n✓ Credentials saved.");
    Ok(())
}

fn copy_service_account(source_path: &str) -> Result<(), String> {
    let src = expand_tilde(source_path);
    let content = std::fs::read_to_string(&src)
        .map_err(|e| format!("failed to read service account file: {e}"))?;
    let sa: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("invalid service account JSON: {e}"))?;
    if sa["client_email"].is_null() || sa["private_key"].is_null() {
        return Err(
            "service account JSON is missing required fields: client_email, private_key"
                .to_string(),
        );
    }

    let dest = credentials::store_dir().join("gcp-service-account.json");
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&dest, &content)
        .map_err(|e| format!("failed to write service account: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set service account permissions: {e}"))?;
    }

    println!("✓ Service account saved to {}", dest.display());
    Ok(())
}

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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib commands::login
```

Expected: all 8 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/commands/login.rs
git commit -m "feat(login): add interactive rak login command for provider credential setup"
```

---

## Task 4: Wire `rak login` into the CLI

**Files:**
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add login to commands module**

In `src/commands/mod.rs`, add:

```rust
pub mod login;
```

- [ ] **Step 2: Add Login subcommand to `src/main.rs`**

In the imports at the top:

```rust
use commands::{add, analyze, init, log, login, next, pull, push, record, scrape, transcribe};
```

In the `Command` enum, add after `Push`:

```rust
    /// Configure credentials for a provider
    Login {
        /// Provider to configure (e.g. chirp, elevenlabs, openrouter)
        provider: Option<String>,
    },
```

In the `match cli.command` block, add:

```rust
        Command::Login { provider } => login::run(provider),
```

- [ ] **Step 3: Verify it compiles and help text appears**

```bash
cargo build && ./target/debug/rak login --help
```

Expected output includes:
```
Configure credentials for a provider

Usage: rak login [PROVIDER]

Arguments:
  [PROVIDER]  Provider to configure (e.g. chirp, elevenlabs, openrouter)
```

- [ ] **Step 4: Commit**

```bash
git add src/commands/mod.rs src/main.rs
git commit -m "feat: wire rak login subcommand into CLI"
```

---

## Task 5: Chirp STT Provider (`src/stt/chirp.rs`)

**Files:**
- Create: `src/stt/chirp.rs`

Implements `Transcriber`. Validates credentials at call time, gets an OAuth2 token via JWT grant, queries Monitoring API for current-month usage, checks against cap, then sends audio to Speech-to-Text v2.

- [ ] **Step 1: Write failing tests**

Create `src/stt/chirp.rs` with just the test module:

```rust
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
        assert!(err.contains("rak login chirp"), "error should mention 'rak login chirp': {err}");
    }

    #[test]
    fn missing_project_id_errors_with_login_hint() {
        let t = make_transcriber("key", "");
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(err.contains("rak login chirp"), "error should mention 'rak login chirp': {err}");
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
        // 10 minutes used, 5 minute file, 60 minute cap → ok
        let result = check_cap(600.0, 300.0, 60.0);
        assert!(result.is_ok());
    }

    #[test]
    fn cap_check_over_cap_errors() {
        // 58 minutes used, 5 minute file, 60 minute cap → error
        let result = check_cap(3480.0, 300.0, 60.0);
        let err = result.unwrap_err();
        assert!(err.contains("cap reached"), "error: {err}");
        assert!(err.contains("58.0"), "should show used minutes: {err}");
        assert!(err.contains("60"), "should show cap: {err}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib stt::chirp
```

Expected: compile error — `ChirpTranscriber`, `sum_audio_seconds`, `parse_transcript`, `check_cap` not defined.

- [ ] **Step 3: Implement the module**

Replace the file with the full implementation:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine;
use chrono::{Datelike, TimeZone, Utc};
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
            return Err("Chirp API key not set. Run 'rak login chirp'.".to_string());
        }
        if self.project_id.is_empty() {
            return Err("Chirp project_id not set. Run 'rak login chirp'.".to_string());
        }
        if self.service_account_path.is_empty() {
            return Err(
                "Chirp service_account_path not set. Run 'rak login chirp'.".to_string(),
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

        let duration_secs = audio_duration_secs(audio_path)
            .ok_or_else(|| format!("failed to get duration of {:?}", audio_path))?;

        let sa_path = PathBuf::from(&self.service_account_path);
        let access_token = get_access_token(&sa_path)?;
        let used_secs = query_usage_seconds(&access_token, &self.project_id)?;
        check_cap(used_secs, duration_secs, self.monthly_cap_minutes)?;

        let audio_bytes = std::fs::read(audio_path)
            .map_err(|e| format!("failed to read audio: {e}"))?;
        let b64 =
            base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

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
            "content": b64
        });

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("Chirp API request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(format!("Chirp API error: {status} {text}"));
        }

        let result: serde_json::Value =
            resp.json().map_err(|e| format!("failed to parse Chirp response: {e}"))?;

        parse_transcript(&result)
    }
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
        .parse()
        .ok()
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
    let content = std::fs::read_to_string(sa_path)
        .map_err(|e| format!("failed to read service account: {e}"))?;
    let sa: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("invalid service account JSON: {e}"))?;

    let client_email = sa["client_email"]
        .as_str()
        .ok_or("service account missing client_email")?;
    let private_key = sa["private_key"]
        .as_str()
        .ok_or("service account missing private_key")?;

    let now = Utc::now().timestamp();
    let claims = JwtClaims {
        iss: client_email.to_string(),
        scope: "https://www.googleapis.com/auth/monitoring.read".to_string(),
        aud: "https://oauth2.googleapis.com/token".to_string(),
        iat: now,
        exp: now + 3600,
    };

    let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| format!("invalid RSA key in service account: {e}"))?;
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let jwt = jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|e| format!("failed to sign JWT: {e}"))?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:jwt-bearer",
            ),
            ("assertion", jwt.as_str()),
        ])
        .send()
        .map_err(|e| format!("OAuth2 token request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("OAuth2 token error: {status} {body}"));
    }

    let token_resp: serde_json::Value =
        resp.json().map_err(|e| format!("failed to parse token response: {e}"))?;

    token_resp["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "OAuth2 response missing access_token".to_string())
}

fn query_usage_seconds(access_token: &str, project_id: &str) -> Result<f64, String> {
    let now = Utc::now();
    let billing_start = Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .ok_or("failed to compute billing period start")?;

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!(
            "https://monitoring.googleapis.com/v3/projects/{project_id}/timeSeries"
        ))
        .bearer_auth(access_token)
        .query(&[
            (
                "filter",
                r#"metric.type="speech.googleapis.com/audio_seconds""#,
            ),
            ("interval.startTime", billing_start.to_rfc3339().as_str()),
            ("interval.endTime", now.to_rfc3339().as_str()),
        ])
        .send()
        .map_err(|e| format!("Monitoring API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("Monitoring API error: {status} {body}"));
    }

    let result: serde_json::Value =
        resp.json().map_err(|e| format!("failed to parse Monitoring response: {e}"))?;

    Ok(sum_audio_seconds(&result))
}

fn sum_audio_seconds(response: &serde_json::Value) -> f64 {
    let Some(series) = response["timeSeries"].as_array() else {
        return 0.0;
    };
    let mut total = 0.0_f64;
    for ts in series {
        let Some(points) = ts["points"].as_array() else {
            continue;
        };
        for point in points {
            let v = &point["value"];
            if let Some(s) = v["int64Value"].as_str() {
                total += s.parse::<f64>().unwrap_or(0.0);
            } else if let Some(n) = v["doubleValue"].as_f64() {
                total += n;
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
    let cap_secs = cap_minutes * 60.0;
    if used_secs + file_secs >= cap_secs {
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
        assert!(err.contains("rak login chirp"), "error should mention 'rak login chirp': {err}");
    }

    #[test]
    fn missing_project_id_errors_with_login_hint() {
        let t = make_transcriber("key", "");
        let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
        assert!(err.contains("rak login chirp"), "error should mention 'rak login chirp': {err}");
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib stt::chirp
```

Expected: all 10 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stt/chirp.rs
git commit -m "feat(stt): add Chirp provider with JWT OAuth2 auth and monthly usage cap check"
```

---

## Task 6: Register Chirp in `src/stt/mod.rs`

**Files:**
- Modify: `src/stt/mod.rs`

- [ ] **Step 1: Add the chirp module and register the provider**

At the top of `src/stt/mod.rs`, add:

```rust
pub mod chirp;
```

In `init_providers()`, add after the `openrouter` registration:

```rust
    register("chirp", |config| {
        let api_key = config
            .get("api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let project_id = config
            .get("project_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let monthly_cap_minutes = config
            .get("monthly_cap_minutes")
            .and_then(|v| v.as_f64())
            .unwrap_or(60.0);
        let service_account_path = config
            .get("service_account_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Box::new(chirp::ChirpTranscriber::new(
            api_key,
            project_id,
            monthly_cap_minutes,
            service_account_path,
        ))
    });
```

- [ ] **Step 2: Run tests to verify nothing broke**

```bash
cargo test --lib stt
```

Expected: all existing stt tests pass.

- [ ] **Step 3: Verify chirp appears in the provider registry**

```bash
cargo build 2>&1 | head -5
```

Expected: builds cleanly (0 errors).

- [ ] **Step 4: Commit**

```bash
git add src/stt/mod.rs
git commit -m "feat(stt): register chirp provider in factory registry"
```

---

## Task 7: Update `transcribe.rs` to Use Credential Store + Remove `api_key` from `ProviderConfig`

**Files:**
- Modify: `src/commands/transcribe.rs`
- Modify: `src/config.rs`

The `ProviderConfig` struct no longer needs `api_key` since secrets come from the credential store. `transcribe.rs` builds the JSON config from `rak.toml` (non-secrets like `model`), then overlays credentials from the store.

- [ ] **Step 1: Remove `api_key` from `ProviderConfig` in `src/config.rs`**

In `src/config.rs`, replace the `ProviderConfig` struct:

```rust
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub model: Option<String>,
}
```

(Remove the `api_key` field entirely. Existing `rak.toml` files with `api_key = ""` will still parse fine — serde ignores unknown fields by default.)

Also remove `resolve_api_key` from `config.rs` since it's no longer needed:

```rust
// DELETE this entire function:
pub fn resolve_api_key(config_key: &str, env_var: &str) -> Result<String, String> { ... }
```

- [ ] **Step 2: Run tests to find all broken references**

```bash
cargo test 2>&1 | grep "error\[" | head -20
```

Expected: compile errors in `src/stt/elevenlabs.rs` and `src/stt/openrouter.rs` referencing `resolve_api_key`, and in `src/commands/transcribe.rs` referencing `pc.api_key`.

- [ ] **Step 3: Update `src/stt/elevenlabs.rs` to not use `resolve_api_key`**

`elevenlabs.rs` calls `resolve_api_key` to fall back to env var. Now that the credential store is the source of truth, the api_key is always injected via json_config. But for backward compatibility with any env var set by users, update `elevenlabs.rs` to check the env var directly if api_key is empty:

In `src/stt/elevenlabs.rs`, replace the `use crate::config::resolve_api_key;` import and the call:

```rust
// Remove this import:
use crate::config::resolve_api_key;

// Replace this in transcribe():
let key = resolve_api_key(&self.api_key, "ELEVENLABS_API_KEY")?;

// With:
let key = if self.api_key.is_empty() {
    std::env::var("ELEVENLABS_API_KEY").map_err(|_| {
        "ElevenLabs API key not set. Run 'rak login elevenlabs'.".to_string()
    })?
} else {
    self.api_key.clone()
};
```

Update the test in `elevenlabs.rs` to check the new error message:

```rust
#[test]
fn new_without_api_key_errors_on_transcribe() {
    std::env::remove_var("ELEVENLABS_API_KEY");
    let t = ElevenLabsTranscriber::new("".to_string(), "scribe_v1".to_string());
    let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
    assert!(
        err.contains("rak login elevenlabs"),
        "error should mention login: {err}"
    );
}
```

- [ ] **Step 4: Update `src/stt/openrouter.rs` to not use `resolve_api_key`**

In `src/stt/openrouter.rs`, replace the import and call the same way:

```rust
// Remove this import:
use crate::config::resolve_api_key;

// Replace this in transcribe():
let key = resolve_api_key(&self.api_key, "OPENROUTER_API_KEY")?;

// With:
let key = if self.api_key.is_empty() {
    std::env::var("OPENROUTER_API_KEY").map_err(|_| {
        "OpenRouter API key not set. Run 'rak login openrouter'.".to_string()
    })?
} else {
    self.api_key.clone()
};
```

Update the test in `openrouter.rs`:

```rust
#[test]
fn new_without_api_key_errors_on_transcribe() {
    std::env::remove_var("OPENROUTER_API_KEY");
    let t = OpenRouterTranscriber::new("".to_string(), "google/gemini-flash-2.5-lite".to_string());
    let err = t.transcribe(Path::new("test.mp3")).unwrap_err();
    assert!(
        err.contains("rak login openrouter"),
        "error should mention login: {err}"
    );
}
```

- [ ] **Step 5: Update `src/commands/transcribe.rs` to use credential store**

Add `use crate::credentials;` at the top of `transcribe.rs`.

Replace the `json_config` construction block (currently lines 29–37):

```rust
    let provider_name = provider
        .as_deref()
        .unwrap_or(&cfg.transcribe.default_provider);
    let provider_config = cfg.transcribe.providers.get(provider_name);

    // Build config from rak.toml (non-secret fields only), then overlay credentials.
    let mut json_config = provider_config
        .map(|pc| serde_json::json!({ "model": pc.model }))
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(creds) = credentials::get_provider(provider_name) {
        if let (Some(obj), Some(creds_obj)) = (json_config.as_object_mut(), creds.as_object()) {
            for (k, v) in creds_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
```

- [ ] **Step 6: Run all tests to verify they pass**

```bash
cargo test
```

Expected: all tests pass. (The elevenlabs and openrouter credential tests now check for the new error message.)

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/commands/transcribe.rs src/stt/elevenlabs.rs src/stt/openrouter.rs
git commit -m "feat: route provider credentials through credential store instead of rak.toml"
```

---

## Task 8: Update `src/commands/init.rs` Templates

**Files:**
- Modify: `src/commands/init.rs`

Remove `api_key` from rak.toml provider sections (secrets no longer belong there). Add `GOOGLE_SPEECH_API_KEY` to the `.env` template for discoverability.

- [ ] **Step 1: Update `RAK_TOML_TEMPLATE`**

In `src/commands/init.rs`, replace `RAK_TOML_TEMPLATE` with this version (api_key lines removed from provider sections):

```rust
const RAK_TOML_TEMPLATE: &str = r#"leetcode_dir = "./cpp"

[transcribe]
default_provider = "elevenlabs"

[transcribe.providers.elevenlabs]
# api_key: run `rak login elevenlabs` to set

[transcribe.providers.openrouter]
model = "google/gemini-flash-2.5-lite"
# api_key: run `rak login openrouter` to set

[transcribe.providers.chirp]
# run `rak login chirp` to configure

[analyze]
default_provider = "claude"
system_prompt = """
Analyze this Leetcode problem solution based on my voice notes. Keep it brief - 2-3 paragraphs max.

PROBLEM:
{question}

MY SOLUTION (latest attempt):
{solution}

MY VOICE NOTES:
{transcripts}

Provide:
1. Brief overview of how the problem went
2. What I did well
3. What I struggled with / areas to improve
4. Improvement guide: if the solution was unsolved, suboptimal, or inefficient, provide a concrete guide on how to solve or optimize it. Include the key algorithm/data structure to use, time/space complexity, and a brief pseudocode outline of the improved approach. If the solution is already optimal, skip this section.

Focus on identifying strengths, weaknesses, and actionable feedback for future practice.
"""

[analyze.providers.claude]
model = "sonnet"

[analyze.providers.gemini]
model = "gemini-2.5-flash"

[leetcode]
# session = ""   # or set LEETCODE_SESSION env var
"#;
```

- [ ] **Step 2: Add `GOOGLE_SPEECH_API_KEY` to `ENV_KEYS`**

In `src/commands/init.rs`, replace the `ENV_KEYS` constant:

```rust
const ENV_KEYS: &[(&str, &str)] = &[
    ("ELEVENLABS_API_KEY", ""),
    ("OPENROUTER_API_KEY", ""),
    ("GOOGLE_SPEECH_API_KEY", ""),
    ("LEETCODE_SESSION", ""),
];
```

- [ ] **Step 3: Update the init test to match new template**

The existing test `init_creates_rak_toml` checks `assert!(content.contains("api_key"))`. Since api_key is now only in comments, update the test:

In `src/commands/init.rs` tests, update:

```rust
#[test]
fn init_creates_rak_toml() {
    let dir = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = run();
    std::env::set_current_dir(orig).unwrap();
    result.unwrap();
    let content = fs::read_to_string(dir.path().join("rak.toml")).unwrap();
    assert!(content.contains("leetcode_dir"));
    assert!(content.contains("[transcribe]"));
    assert!(content.contains("elevenlabs"));
    assert!(content.contains("openrouter"));
    assert!(content.contains("chirp"));
    assert!(!content.contains("\napi_key"), "api_key should not appear as a TOML key in the template");
}
```

Update the `.env` test to check for the new key:

```rust
#[test]
fn init_creates_env_with_api_keys() {
    let dir = tempfile::tempdir().unwrap();
    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = run();
    std::env::set_current_dir(orig).unwrap();
    result.unwrap();
    let content = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(content.contains("ELEVENLABS_API_KEY"));
    assert!(content.contains("OPENROUTER_API_KEY"));
    assert!(content.contains("GOOGLE_SPEECH_API_KEY"));
}
```

- [ ] **Step 4: Run all tests**

```bash
cargo test
```

Expected: all tests pass including updated init tests.

- [ ] **Step 5: Commit**

```bash
git add src/commands/init.rs
git commit -m "feat(init): remove api_key from rak.toml template; add chirp section and GOOGLE_SPEECH_API_KEY to .env"
```

---

## Self-Review

**Spec coverage check:**

| Spec Requirement | Task |
|-----------------|------|
| `src/credentials.rs` with load/save/get_provider/store_dir | Task 2 |
| JSON file at `dirs::data_dir() / "rak" / "credentials.json"` | Task 2 |
| 0600 permissions | Task 2 |
| Atomic write via temp+rename | Task 2 |
| `rak login <provider>` interactive prompts | Task 3 |
| `rak login` (no arg) lists providers | Task 3 |
| Hidden input for api_key fields | Task 3 |
| Overwrite confirmation | Task 3 |
| Copy service account JSON → store_dir | Task 3 |
| Error → `run 'rak login <provider>'` | Tasks 3, 5 |
| Login subcommand in CLI | Task 4 |
| Chirp provider implements Transcriber | Task 5 |
| audio duration via ffprobe | Task 5 |
| JWT grant → OAuth2 access token | Task 5 |
| Monitoring API usage query | Task 5 |
| Usage cap check with error message | Task 5 |
| Speech-to-Text v2 POST with base64 audio | Task 5 |
| Parse `results[0].alternatives[0].transcript` | Task 5 |
| Register chirp in init_providers() | Task 6 |
| transcribe.rs reads credentials from store | Task 7 |
| Remove api_key from ProviderConfig | Task 7 |
| Remove api_key from rak.toml template | Task 8 |
| Add GOOGLE_SPEECH_API_KEY to .env template | Task 8 |
| `dirs = "6"` dependency | Task 1 |
| `jsonwebtoken` dependency | Task 1 |

**Placeholder scan:** No TBDs, TODOs, or "similar to Task N" references. All code blocks contain actual implementations.

**Type consistency check:**
- `ChirpTranscriber::new(api_key, project_id, monthly_cap_minutes, service_account_path)` — consistent across Task 5 (definition) and Task 6 (factory call).
- `credentials::load()` / `credentials::save()` / `credentials::get_provider()` — consistent across Task 2 (definition), Task 3 (login.rs), Task 7 (transcribe.rs).
- `ProviderConfig { model: Option<String> }` — `api_key` removed in Task 7; `pc.model` usage in Task 7 transcribe.rs is consistent with this definition.
- `sum_audio_seconds`, `parse_transcript`, `check_cap` — defined and tested within Task 5 only.

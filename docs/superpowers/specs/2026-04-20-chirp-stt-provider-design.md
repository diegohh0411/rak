# Chirp STT Provider + Credential Store

## Goal

Add a Google Cloud Chirp 2 speech-to-text provider to `rak` with stateless usage tracking against a configurable monthly cap, plus a credential store that all providers use instead of storing secrets in git-trackable config files.

## Background

`rak` is a Rust CLI for LeetCode practice workflows. It has an existing STT provider architecture:

- `Transcriber` trait in `src/stt/mod.rs` with a factory registry pattern
- Two providers: `elevenlabs` and `openrouter`
- Config via `rak.toml` with `[transcribe.providers.<name>]` sections
- API keys currently stored in `rak.toml` (git-trackable) or `.env` (ad-hoc)

Google Cloud Speech-to-Text offers 60 free minutes/month. The user wants a configurable cap (e.g. 16 min) with actual usage queried from Google's Cloud Monitoring API — no local state files.

## Design

### 1. Credential Store (`src/credentials.rs` — new)

A single JSON file at `dirs::data_dir() / "rak" / "credentials.json"` holds all provider secrets. File permissions set to 0600.

```json
{
  "elevenlabs": { "api_key": "..." },
  "openrouter": { "api_key": "..." },
  "chirp": {
    "api_key": "...",
    "project_id": "my-gcp-project",
    "monthly_cap_minutes": 16,
    "service_account_path": "~/.local/share/rak/gcp-service-account.json"
  }
}
```

Module provides:
- `load() -> HashMap<String, serde_json::Value>` — read the store
- `save(data: &HashMap) -> Result` — write atomically (write to temp file, rename)
- `get_provider(provider_name) -> Option<serde_json::Value>` — convenience getter
- `store_dir() -> PathBuf` — returns `dirs::data_dir() / "rak"`

No env var fallback. If a credential is missing, the error message says `run 'rak login <provider>'`.

### 2. `rak login` Command (`src/commands/login.rs` — new)

Interactive CLI command for saving provider credentials to the store.

```
$ rak login chirp
Chirp (Google Cloud Speech-to-Text) setup

API Key: ********
GCP Project ID: my-project-123
Monthly cap (minutes) [60]: 16
Path to service account JSON: ~/Downloads/my-project-abc123.json

✓ Service account saved to ~/.local/share/rak/gcp-service-account.json
✓ Credentials saved.

$ rak login elevenlabs
ElevenLabs setup

API Key: ********

✓ Credentials saved.
```

Behavior:
- `rak login <provider>` — interactive prompts for that provider's fields
- `rak login` (no provider) — list available providers and ask which one
- Each provider declares its required fields via a registry entry
- If credentials already exist for a provider, overwrite after confirmation
- For `chirp`: copies the service account JSON file into `dirs::data_dir() / "rak" / "gcp-service-account.json"`

Provider field declarations (registered alongside the factory in `init_providers`):
- `elevenlabs` → `api_key`
- `openrouter` → `api_key`, `model` (optional)
- `chirp` → `api_key`, `project_id`, `monthly_cap_minutes`, `service_account_path`

### 3. Chirp Provider (`src/stt/chirp.rs` — new)

Implements `Transcriber` trait. Two internal responsibilities:

#### 3a. Usage Pre-check

Before transcription, queries Google Cloud Monitoring API for actual Speech-to-Text usage this billing period.

1. Get audio file duration via `ffprobe` (same approach as `src/commands/transcribe.rs:audio_duration`)
2. Authenticate to Monitoring API using service account JSON → OAuth2 access token (JWT grant flow using `reqwest` + `base64` + `serde_json`, no Google SDK)
3. Query `GET https://monitoring.googleapis.com/v3/projects/{project}/timeSeries` with filter `metric.type = "speech.googleapis.com/audio_seconds"` for current billing period
4. Sum total seconds from response
5. Check: `current_usage_seconds + audio_duration_seconds < cap_minutes * 60`
6. If over cap → return error: `"Chirp usage cap reached: {used:.1}/{cap} min (this file would add {dur:.1} min)"`
7. If under cap → proceed to transcription

#### 3b. Transcription

Sends MP3 audio to Google Cloud Speech-to-Text v2.

- `POST https://speech.googleapis.com/v2/projects/{project}/locations/global/recognizers/_:recognize?key={api_key}`
- Body:
  ```json
  {
    "config": {
      "model": "chirp_2",
      "auto_decoding_config": {}
    },
    "content": "<base64-encoded MP3>"
  }
  ```
- Auth: API key in query parameter
- Parse `results[0].alternatives[0].transcript` from response

#### 3c. Credential Resolution

All credentials read from the credential store only:
- `api_key` — for Speech-to-Text API
- `project_id` — GCP project identifier
- `monthly_cap_minutes` — configurable cap, defaults to 60 if not set
- `service_account_path` — path to service account JSON for Monitoring API auth

If any credential is missing, error with `run 'rak login chirp'`.

### 4. Modified Files

#### `src/stt/mod.rs`

- Register `chirp` provider in `init_providers()`
- Add provider field declarations for `rak login` — a new struct/type that describes what fields each provider needs for the login flow

#### `src/commands/transcribe.rs`

- Resolve credentials from the credential store first
- Fall back to `rak.toml` provider config for non-secret fields like `model`
- Pass merged config to the provider factory

#### `src/main.rs`

- Add `Login` subcommand:
  ```rust
  Login {
      /// Provider to configure (e.g. chirp, elevenlabs)
      provider: Option<String>,
  }
  ```

#### `src/commands/init.rs`

- Add `GOOGLE_SPEECH_API_KEY` to `.env` template (for discoverability only; actual auth uses credential store)

#### `Cargo.toml`

- Add `dirs = "6"` dependency

### 5. What's NOT Changing

- The `Transcriber` trait — stays as-is with `name()` and `transcribe()`
- `rak.toml` config structure — still used for non-secret settings (`model`, `default_provider`)
- Existing `.env` keys remain in the template for discoverability but are not read by providers — all auth goes through the credential store
- `src/history.rs`, `src/recorder/`, `src/analyzer/`, etc. — no changes

### 6. JWT Grant Flow for Monitoring API (Implementation Detail)

To get an OAuth2 access token from a service account JSON without a Google SDK:

1. Read service account JSON → extract `client_email` and `private_key`
2. Build JWT header: `{"alg": "RS256", "typ": "JWT"}`
3. Build JWT claim set: `{"iss": client_email, "scope": "https://www.googleapis.com/auth/monitoring.read", "aud": "https://oauth2.googleapis.com/token", "iat": now, "exp": now + 3600}`
4. Sign JWT with RSA-SHA256 using the private key (requires `rsa` + `sha2` crates, or use the `jsonwebtoken` crate)
5. POST to `https://oauth2.googleapis.com/token` with `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer&assertion=<signed_jwt>`
6. Parse `access_token` from response

This adds `jsonwebtoken` as a dependency (lighter than the full Google SDK).

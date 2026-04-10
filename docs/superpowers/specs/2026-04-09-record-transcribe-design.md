# Design: `rak record` & `rak transcribe` + Command Flattening

**Date:** 2026-04-09  
**Status:** Approved

---

## Overview

Two new top-level commands — `rak record <id>` and `rak transcribe <id>` — bring voice-note recording and transcription into rak. Alongside this, the `leetcode` subcommand group is dissolved and all commands are flattened to the top level.

Reference implementation: the Go fork at `opensource/leetgo` (`cmd/record.go`, `cmd/recorder.go`, `cmd/recorder_tui.go`, `cmd/transcribe.go`, `stt_providers/`).

---

## 1. Command Surface

### Before → After

| Before | After |
|---|---|
| `rak leetcode log <id> <rating> [--force]` | `rak log <id> <rating> [--force]` |
| `rak leetcode next [--count N]` | `rak next [--count N]` |
| *(new)* | `rak record <id> [--force]` |
| *(new)* | `rak transcribe <id> [--provider=<name>] [--force]` |
| `rak init` | `rak init` (updated, see §2) |
| `rak scrape <url> [-o file]` | `rak scrape <url> [-o file]` |

The `leet` and `l` aliases on the old `leetcode` group are removed. Existing flags on `log` and `next` are preserved unchanged.

---

## 2. Config & Path Resolution

### Files created by `rak init`

**`rak.toml`** (git-tracked):
```toml
leetcode_dir = "leetcode/solutions/cpp"

[transcribe]
default_provider = "elevenlabs"

[transcribe.providers.elevenlabs]
api_key = ""  # falls back to ELEVENLABS_API_KEY env var if empty

[transcribe.providers.openrouter]
api_key = ""  # falls back to OPENROUTER_API_KEY env var if empty
model = "google/gemini-flash-2.5-lite"
```

**`.env`** (gitignored, unchanged behavior):
```
ELEVENLABS_API_KEY=
OPENROUTER_API_KEY=
```

### Config loading

At startup any command that needs config walks up from cwd to find `rak.toml` (same pattern as `dotenvy` finds `.env`). Returns a clear error if not found: `"rak.toml not found — run 'rak init' first"`.

API key resolution order per provider: `rak.toml` field (if non-empty) → env var fallback.

### Problem folder resolution

Given ID `"1"`:
1. Resolve `leetcode_dir` relative to the directory containing `rak.toml`
2. Zero-pad ID to 4 digits: `"0001"`
3. Scan `leetcode_dir` for entries matching `^0001\..*`
4. Exactly one match → use it. Zero matches → error. Multiple matches → error listing them.

---

## 3. `rak record <id>`

### Flags
- `--force` / `-f`: restart attempt numbering from 1 (overwrites existing attempt-1.mp3)

### Flow

1. Find problem folder (§2)
2. Check `ffmpeg` on PATH; if missing, print install instructions and exit
3. Scan folder for `attempt-N.mp3` → pick next N (or 1 if none; 1 if `--force`)
4. Output path: `<problem_dir>/attempt-N.mp3`
5. Spawn ffmpeg with dual-output: MP3 to file + raw PCM (s16le, mono, 44100 Hz) to stdout pipe
6. Launch ratatui TUI (blocks until user stops or cancels)
7. On save: prompt `Transcribe now? [Y/n]` — if yes, run transcribe inline with default provider

### Platform detection

| OS | ffmpeg input format | input device | pause/resume |
|---|---|---|---|
| macOS | `avfoundation` | `:0` | yes (SIGSTOP/SIGCONT) |
| Linux / WSL2 | `pulse` | `default` | yes |
| Windows | `dshow` | `audio` (auto-detect) | no (kill only) |

### TUI layout

```
⏺ 00:23  Recording attempt-2.mp3
▁ ▃ ▅ ▇ █ ▇ ▅ ▃ ▁ ▂ ▄ ▆ ▇ ▅ ▃ ▁   ← full terminal width, refreshes 10×/sec
[space] pause  [q] stop  [ctrl+c] cancel
```

States: Recording (red ⏺), Paused (yellow ⏸), Stopping (yellow "Saving…"), Done (green ✓), Cancelled, Error.

### EQ visualization

- Read PCM chunks (2048 samples) from ffmpeg stdout pipe
- Run Cooley-Tukey FFT (pad to next power of 2)
- Group magnitude bins into N logarithmic bands over 80–8000 Hz (N = terminal width / 2)
- Normalize per-frame to max band; map to Unicode block chars `▁▂▃▄▅▆▇█`
- Refresh every 100ms via ratatui tick

### Key bindings

| Key | Action |
|---|---|
| `space` | Pause / resume (Unix only; no-op on Windows) |
| `q` / `enter` | Stop & save (SIGINT to ffmpeg, wait for flush) |
| `ctrl+c` / `esc` | Cancel & discard (kill ffmpeg, delete partial file) |

### Stop without deadlock

Stopping drains the PCM pipe while waiting for ffmpeg to flush and exit (mirrors the Go fix for the stdout pipe deadlock).

---

## 4. `rak transcribe <id>`

### Flags
- `--provider=<name>` / `-p`: override default provider from `rak.toml`
- `--force` / `-f`: re-transcribe all attempts even if `.md` already exists

### Flow

1. Find problem folder (§2)
2. Scan for `attempt-N.mp3` files without a matching `attempt-N.md` (or all if `--force`)
3. Load provider from config (§2 key resolution)
4. For each file (ascending order):
   - Upload to provider → get transcript text
   - If text is empty, warn and skip
   - Append duration metadata via `ffprobe`: `\n\n---\nAudio duration: MM:SS\n`
   - Write to `attempt-N.md`
   - Print: `Transcribing attempt-N.mp3... ✓ attempt-N.md`
5. Print summary: `Transcribed N file(s).`

### Provider trait

```rust
trait Transcriber: Send + Sync {
    fn name(&self) -> &str;
    fn transcribe(&self, audio_path: &Path) -> Result<String>;
}
```

Providers registered in a global registry (similar to the Go `stt_providers` package). Two built-in implementations: `elevenlabs`, `openrouter`.

### ElevenLabs provider

- Endpoint: `POST https://api.elevenlabs.io/v1/speech-to-text`
- Multipart form: `file=<mp3>`, `model_id=scribe_v1`
- Header: `xi-api-key: <key>`
- Response: `{ "text": "..." }`
- Timeout: 5 minutes

### OpenRouter provider

- Endpoint: `POST https://openrouter.ai/api/v1/chat/completions`
- Model: configurable, default `google/gemini-flash-2.5-lite`
- Sends MP3 as base64-encoded audio content in the messages array
- Header: `Authorization: Bearer <key>`
- Prompt: requests verbatim transcription of the audio

---

## 5. Module Structure

```
src/
  main.rs                  # updated: flat Command enum, no leetcode group
  config.rs                # new: rak.toml loading, walk-up finder, RakConfig struct
  commands/
    mod.rs
    init.rs                # updated: writes rak.toml template + .env
    log.rs                 # moved up from commands/leetcode/log.rs
    next.rs                # moved up from commands/leetcode/next.rs
    record.rs              # new: attempt numbering, ffmpeg spawn, post-record prompt
    transcribe.rs          # new: scan untranscribed, drive provider, write .md
    scrape.rs              # unchanged
  recorder/
    mod.rs                 # new: platform detection, ffmpeg spawn/stop/pause/cancel
    tui.rs                 # new: ratatui model, EQ visualization, key handling
    fft.rs                 # new: Cooley-Tukey FFT, magnitude spectrum, band grouping
  stt/
    mod.rs                 # new: Transcriber trait, provider registry
    elevenlabs.rs          # new: ElevenLabs HTTP client
    openrouter.rs          # new: OpenRouter HTTP client
```

---

## 6. New Dependencies

| Crate | Purpose |
|---|---|
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend for ratatui |
| `reqwest` (with `multipart`, `blocking` or async) | HTTP client for STT providers |
| `toml` | Parse `rak.toml` |
| `base64` | Encode audio for OpenRouter |
| *(no extra crate)* | FFT implemented directly (Cooley-Tukey, ~40 lines, already proven in Go) |

Existing: `tokio`, `clap`, `serde`, `dotenvy` remain unchanged.

---

## 7. Error Handling

- `rak.toml` not found → clear message with `rak init` hint
- `ffmpeg` not on PATH → message with per-platform install command
- Problem folder not found → list `leetcode_dir` contents hint
- Provider API key missing → name the env var or toml key to set
- Empty transcript → warn, skip, continue (don't fail the whole batch)
- ffmpeg exits non-zero after SIGINT → treat as success (exit code 255 is normal)

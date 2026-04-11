# Custom Interview Problems Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend RAK to support custom (non-LeetCode) interview problems identified by non-numeric slugs, alongside all existing LeetCode functionality.

**Architecture:** Introduce a `ProblemId` enum parsed at every command entry point. `resolve_problem_folder` dispatches on the variant: numeric IDs use the existing zero-pad regex logic; slugs use exact directory name matching in `leetcode_dir`. A new `rak add` command scaffolds custom problem folders; `rak record` calls the same scaffold logic automatically when a custom folder is missing.

**Tech Stack:** Rust, Cargo, existing crates (clap, regex, tempfile for tests)

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `src/config.rs` | Add `ProblemId` enum + update `resolve_problem_folder` signature |
| Create | `src/commands/add.rs` | `run()` + `pub scaffold()` for custom folder creation |
| Modify | `src/commands/mod.rs` | Export `add` module |
| Modify | `src/commands/record.rs` | Parse `ProblemId`, auto-scaffold missing custom folders |
| Modify | `src/commands/transcribe.rs` | Parse `ProblemId`, pass to `resolve_problem_folder` |
| Modify | `src/commands/analyze.rs` | Parse `ProblemId`, pass to `resolve_problem_folder` |
| Modify | `src/main.rs` | Wire `Add` subcommand |

---

## Task 1: Add `ProblemId` to `config.rs`

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing tests for Custom resolution**

Add to the `#[cfg(test)]` block at the bottom of `src/config.rs`:

```rust
#[test]
fn resolve_problem_folder_custom_exact_match() {
    let tmp = tempfile::tempdir().unwrap();
    let lc = tmp.path().join("leetcode");
    fs::create_dir_all(lc.join("c3ai-strings")).unwrap();
    fs::write(
        tmp.path().join("rak.toml"),
        "leetcode_dir = \"leetcode\"",
    )
    .unwrap();

    let config = RakConfig {
        leetcode_dir: "leetcode".to_string(),
        ..Default::default()
    };
    let id = ProblemId::Custom("c3ai-strings".to_string());
    let result = resolve_problem_folder(tmp.path(), &config, &id).unwrap();
    assert!(result.ends_with("c3ai-strings"));
}

#[test]
fn resolve_problem_folder_custom_no_match_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let lc = tmp.path().join("leetcode");
    fs::create_dir_all(&lc).unwrap();

    let config = RakConfig {
        leetcode_dir: "leetcode".to_string(),
        ..Default::default()
    };
    let id = ProblemId::Custom("missing-slug".to_string());
    let err = resolve_problem_folder(tmp.path(), &config, &id).unwrap_err();
    assert!(err.contains("missing-slug"), "error should mention slug: {err}");
}

#[test]
fn resolve_problem_folder_custom_no_partial_match() {
    let tmp = tempfile::tempdir().unwrap();
    let lc = tmp.path().join("leetcode");
    fs::create_dir_all(lc.join("c3ai-strings-and-targets")).unwrap();

    let config = RakConfig {
        leetcode_dir: "leetcode".to_string(),
        ..Default::default()
    };
    // "c3ai" is NOT a match for "c3ai-strings-and-targets"
    let id = ProblemId::Custom("c3ai".to_string());
    let err = resolve_problem_folder(tmp.path(), &config, &id).unwrap_err();
    assert!(err.contains("c3ai"), "error should mention slug: {err}");
}

#[test]
fn problem_id_parse_numeric() {
    match ProblemId::parse("200") {
        ProblemId::Leetcode(n) => assert_eq!(n, 200),
        ProblemId::Custom(_) => panic!("expected Leetcode variant"),
    }
}

#[test]
fn problem_id_parse_slug() {
    match ProblemId::parse("c3ai-strings") {
        ProblemId::Custom(s) => assert_eq!(s, "c3ai-strings"),
        ProblemId::Leetcode(_) => panic!("expected Custom variant"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test resolve_problem_folder_custom 2>&1 | tail -20
```

Expected: compile errors — `ProblemId` not yet defined.

- [ ] **Step 3: Add `ProblemId` enum and update `resolve_problem_folder`**

In `src/config.rs`, add the enum and its `parse` method **before** the `find_rak_toml` function:

```rust
#[derive(Debug)]
pub enum ProblemId {
    Leetcode(u32),
    Custom(String),
}

impl ProblemId {
    pub fn parse(s: &str) -> Self {
        match s.parse::<u32>() {
            Ok(n) => ProblemId::Leetcode(n),
            Err(_) => ProblemId::Custom(s.to_owned()),
        }
    }
}
```

Replace the `resolve_problem_folder` function signature and body (currently takes `id: &str`):

```rust
pub fn resolve_problem_folder(
    config_dir: &Path,
    config: &RakConfig,
    id: &ProblemId,
) -> Result<PathBuf, String> {
    let leetcode_dir = config_dir.join(&config.leetcode_dir);

    match id {
        ProblemId::Leetcode(n) => {
            let padded = format!("{:0>4}", n);
            let entries = std::fs::read_dir(&leetcode_dir).map_err(|e| {
                format!(
                    "Failed to read leetcode_dir '{}': {}",
                    config.leetcode_dir, e
                )
            })?;

            let re = regex::Regex::new(&format!("^{}\\.", regex::escape(&padded)))
                .map_err(|e| e.to_string())?;

            let matches: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_str().is_some_and(|name| re.is_match(name)))
                .map(|e| e.path())
                .collect();

            match matches.len() {
                0 => Err(format!(
                    "No problem folder matching '{}' found in {}",
                    padded, config.leetcode_dir
                )),
                1 => Ok(matches.into_iter().next().unwrap()),
                _ => Err(format!(
                    "Multiple problem folders matching '{}' found in {}: {:?}",
                    padded, config.leetcode_dir, matches
                )),
            }
        }

        ProblemId::Custom(slug) => {
            let candidate = leetcode_dir.join(slug);
            if candidate.is_dir() {
                Ok(candidate)
            } else {
                Err(format!(
                    "No folder matching '{}' found in {}",
                    slug, config.leetcode_dir
                ))
            }
        }
    }
}
```

- [ ] **Step 4: Fix existing tests in `config.rs` that pass `&str` to `resolve_problem_folder`**

The three existing `resolve_problem_folder_*` tests pass `"1"`, `"9999"`, `"1"` as string IDs. Update each call to use the enum:

```rust
// resolve_problem_folder_zero_pads
let result = resolve_problem_folder(tmp.path(), &config, &ProblemId::Leetcode(1)).unwrap();

// resolve_problem_folder_no_match_errors
let err = resolve_problem_folder(tmp.path(), &config, &ProblemId::Leetcode(9999)).unwrap_err();

// resolve_problem_folder_multiple_match_errors
let err = resolve_problem_folder(tmp.path(), &config, &ProblemId::Leetcode(1)).unwrap_err();
```

- [ ] **Step 5: Run all config tests**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test --lib config 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/pi/pnyc/opensource/rak && git add src/config.rs && git commit -m "feat(config): add ProblemId enum with Custom slug resolution"
```

---

## Task 2: Update `transcribe.rs` and `analyze.rs`

**Files:**
- Modify: `src/commands/transcribe.rs`
- Modify: `src/commands/analyze.rs`

These two files are updated identically: parse `ProblemId` at entry and pass `&problem_id` to `resolve_problem_folder`. No new tests needed — existing tests don't call `run()` directly.

- [ ] **Step 1: Update `transcribe.rs`**

In `src/commands/transcribe.rs`, change the first three lines of `run()`:

```rust
pub fn run(id: String, provider: Option<String>, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;
    // rest of function unchanged
```

- [ ] **Step 2: Update `analyze.rs`**

In `src/commands/analyze.rs`, change the first three lines of `run()`:

```rust
pub fn run(id: String, provider: Option<String>, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);
    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;
    // rest of function unchanged
```

- [ ] **Step 3: Build to confirm no compile errors**

```bash
cd /home/pi/pnyc/opensource/rak && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] **Step 4: Run full test suite**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
cd /home/pi/pnyc/opensource/rak && git add src/commands/transcribe.rs src/commands/analyze.rs && git commit -m "feat(transcribe,analyze): use ProblemId for problem resolution"
```

---

## Task 3: Create `src/commands/add.rs`

**Files:**
- Create: `src/commands/add.rs`
- Modify: `src/commands/mod.rs`

- [ ] **Step 1: Export the new module in `mod.rs`**

Add `pub mod add;` to `src/commands/mod.rs`:

```rust
pub mod add;
pub mod analyze;
pub mod init;
pub mod log;
pub mod next;
pub mod record;
pub mod scrape;
pub mod transcribe;
```

- [ ] **Step 2: Write failing tests in the new file**

Create `src/commands/add.rs` with tests only:

```rust
use std::path::Path;

use crate::config;

pub fn run(slug: String) -> Result<(), String> {
    todo!()
}

pub fn scaffold(problem_dir: &Path) -> Result<(), String> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scaffold_creates_question_and_solution() {
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("my-slug");
        scaffold(&problem_dir).unwrap();
        assert!(problem_dir.join("question.md").exists());
        assert!(problem_dir.join("solution.cpp").exists());
    }

    #[test]
    fn scaffold_is_idempotent_on_existing_dir() {
        // scaffold uses create_dir_all, so it's safe to call on an existing directory.
        // Only run() (not unit-testable without cwd control) guards against this.
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("my-slug");
        fs::create_dir_all(&problem_dir).unwrap();
        // Should not error even though the directory already exists
        scaffold(&problem_dir).unwrap();
        assert!(problem_dir.join("question.md").exists());
    }

    #[test]
    fn scaffold_creates_empty_files() {
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("some-slug");
        scaffold(&problem_dir).unwrap();
        assert_eq!(fs::read_to_string(problem_dir.join("question.md")).unwrap(), "");
        assert_eq!(fs::read_to_string(problem_dir.join("solution.cpp")).unwrap(), "");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test commands::add 2>&1 | tail -20
```

Expected: tests fail with `not yet implemented` (todo! panics).

- [ ] **Step 4: Implement `scaffold` and `run`**

Replace the `todo!()` stubs in `src/commands/add.rs`:

```rust
use std::path::Path;

use crate::config;

pub fn run(slug: String) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let leetcode_dir = config_dir.join(&cfg.leetcode_dir);
    let problem_dir = leetcode_dir.join(&slug);

    if problem_dir.is_dir() {
        return Err(format!("Folder '{}' already exists", slug));
    }

    scaffold(&problem_dir)?;
    eprintln!("Created {}/", slug);
    Ok(())
}

pub fn scaffold(problem_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(problem_dir).map_err(|e| e.to_string())?;
    std::fs::write(problem_dir.join("question.md"), "").map_err(|e| e.to_string())?;
    std::fs::write(problem_dir.join("solution.cpp"), "").map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test commands::add 2>&1 | tail -20
```

Expected: all 3 tests pass.

- [ ] **Step 6: Run full test suite**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
cd /home/pi/pnyc/opensource/rak && git add src/commands/add.rs src/commands/mod.rs && git commit -m "feat(add): scaffold custom problem folders"
```

---

## Task 4: Update `record.rs` to auto-scaffold missing custom folders

**Files:**
- Modify: `src/commands/record.rs`

- [ ] **Step 1: Update `record.rs`**

In `src/commands/record.rs`, replace the existing `use crate::config;` line and the start of `run()` with:

```rust
use std::io::{self, BufRead, Write};

use crate::commands::add;
use crate::config;
use crate::recorder::{self, tui};

pub fn run(id: String, force: bool) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let problem_id = config::ProblemId::parse(&id);

    // Auto-scaffold missing custom problem folders silently
    if let config::ProblemId::Custom(ref slug) = problem_id {
        let leetcode_dir = config_dir.join(&cfg.leetcode_dir);
        let candidate = leetcode_dir.join(slug);
        if !candidate.is_dir() {
            add::scaffold(&candidate)?;
        }
    }

    let problem_dir = config::resolve_problem_folder(&config_dir, &cfg, &problem_id)?;
    // rest of function unchanged from here...
```

The rest of `run()` (from `recorder::check_ffmpeg()?;` onward) stays exactly as it was.

- [ ] **Step 2: Build to confirm no compile errors**

```bash
cd /home/pi/pnyc/opensource/rak && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

- [ ] **Step 3: Run full test suite**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
cd /home/pi/pnyc/opensource/rak && git add src/commands/record.rs && git commit -m "feat(record): auto-scaffold missing custom problem folders"
```

---

## Task 5: Wire `Add` subcommand in `main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add `Add` variant to `Command` enum and wire dispatch**

In `src/main.rs`, add `add` to the imports and add the `Add` variant:

```rust
use commands::{add, analyze, init, log, next, record, scrape, transcribe};
```

Add to the `Command` enum (e.g. after `Init`):

```rust
/// Add a custom interview problem folder
Add {
    /// Problem slug (e.g. c3ai-strings-and-targets)
    slug: String,
},
```

Add to the `match cli.command` block:

```rust
Command::Add { slug } => add::run(slug),
```

- [ ] **Step 2: Build and smoke-test**

```bash
cd /home/pi/pnyc/opensource/rak && cargo build 2>&1 | grep -E "^error" | head -20
```

Expected: no errors.

```bash
cd /home/pi/pnyc/opensource/rak && cargo run -- --help 2>&1 | grep -A1 "add"
```

Expected output includes:
```
  add   Add a custom interview problem folder
```

- [ ] **Step 3: Run full test suite**

```bash
cd /home/pi/pnyc/opensource/rak && cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
cd /home/pi/pnyc/opensource/rak && git add src/main.rs && git commit -m "feat(cli): wire rak add subcommand"
```

---

## Task 6: Migrate existing C3.ai problem (manual step)

This is a one-time filesystem operation, not a code change.

- [ ] **Step 1: Move the C3.ai second-interview folder into `leetcode_dir`**

```bash
mv /home/pi/pnyc/applications/roles/c3-ai/associates-program/second-interview \
   /home/pi/pnyc/leetcode/solutions/cpp/c3ai-second-interview
```

- [ ] **Step 2: Verify the folder has the expected files**

```bash
ls /home/pi/pnyc/leetcode/solutions/cpp/c3ai-second-interview/
```

Expected: `question.md  analysis.md  solution.cpp  attempt-1.mp3  attempt-1.md`

- [ ] **Step 3: Commit the migration in the pnyc repo**

```bash
cd /home/pi/pnyc && git add leetcode/solutions/cpp/c3ai-second-interview applications/roles/c3-ai/associates-program && git commit -m "chore: migrate c3ai second interview into leetcode_dir for rak"
```

---

## Post-Implementation Verification

- [ ] **Smoke-test `rak add`**

```bash
cd /home/pi/pnyc/leetcode/solutions && rak add test-custom-slug
ls cpp/test-custom-slug/
```

Expected: `question.md  solution.cpp`

- [ ] **Smoke-test `rak record` on new slug**

```bash
cd /home/pi/pnyc/leetcode/solutions && rak record test-custom-slug
```

Expected: auto-scaffolds if missing (already exists here), launches recorder TUI.

- [ ] **Smoke-test `rak record` on migrated C3.ai problem**

```bash
cd /home/pi/pnyc/leetcode/solutions && rak record c3ai-second-interview
```

Expected: finds `cpp/c3ai-second-interview/`, launches recorder TUI.

- [ ] **Smoke-test existing numeric problem still works**

```bash
cd /home/pi/pnyc/leetcode/solutions && rak next
```

Expected: due problems list renders with numeric IDs as before.

- [ ] **Clean up test slug**

```bash
rm -rf /home/pi/pnyc/leetcode/solutions/cpp/test-custom-slug
```

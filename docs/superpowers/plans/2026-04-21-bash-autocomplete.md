# Bash Autocomplete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `rak completions` subcommand that outputs a bash completion script with dynamic problem-slug completion from `leetcode_dir`.

**Architecture:** Use `clap_complete` to generate static bash completion (subcommands, flags), then append a custom bash function `_rak_problem_slugs()` that walks up from `$PWD` to find `rak.toml`, reads `leetcode_dir`, and lists folder names as candidates. The `rak completions` command prints the full script to stdout.

**Tech Stack:** Rust, clap 4, clap_complete 4, bash

---

### Task 1: Add `clap_complete` dependency and extract `Cli` builder

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

- [ ] **Step 1: Add `clap_complete` to `Cargo.toml`**

Add under `[dependencies]`:

```toml
clap_complete = "4"
```

- [ ] **Step 2: Extract the `Cli` struct and `Command` enum into a reusable function in `main.rs`**

The `Cli` struct and `Command` enum already exist in `main.rs`. We need to make the `Command` accessible from `commands::completions`. To do this, expose a function that returns the `clap::Command` without parsing:

In `src/main.rs`, add a function that builds and returns the `clap::Command`:

```rust
pub fn build_cli() -> clap::Command {
    Cli::command()
}
```

No changes to the existing `Cli` or `Command` definitions yet — that happens in Task 2.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat(completions): add clap_complete dependency and build_cli helper"
```

---

### Task 2: Add `Completions` variant to the CLI enum

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add the `Completions` variant to `Command`**

Add to the `Command` enum before the closing brace:

```rust
    /// Generate shell completion script
    Completions {
        /// Shell to generate completions for
        #[arg(short, long, default_value = "bash")]
        shell: String,
    },
```

- [ ] **Step 2: Add the match arm in `main()`**

Add to the `match cli.command` block, after the `Login` arm:

```rust
        Command::Completions { shell } => commands::completions::run(&shell),
```

Note: `completions::run` takes `&str` and returns `Result<(), String>`. We'll implement it in Task 3.

- [ ] **Step 3: Add the module to `src/commands/mod.rs`**

Add to the end of the file:

```rust
pub mod completions;
```

- [ ] **Step 4: Verify it compiles (will fail until Task 3, but check syntax)**

We'll skip this check and do it in Task 3 since the module doesn't exist yet.

---

### Task 3: Implement `src/commands/completions.rs`

**Files:**
- Create: `src/commands/completions.rs`

- [ ] **Step 1: Create `src/commands/completions.rs` with the completion generator**

```rust
use std::io;

use clap_complete::Shell;

pub fn run(shell_name: &str) -> Result<(), String> {
    let shell = match shell_name {
        "bash" => Shell::Bash,
        other => return Err(format!("unsupported shell: {other} (only bash is supported)")),
    };

    let mut cmd = crate::build_cli();
    let name = cmd.get_name().to_string();

    clap_complete::generate(shell, &mut cmd, name, &mut io::stdout());

    print!("{}", CUSTOM_BASH_SCRIPT);

    Ok(())
}

const CUSTOM_BASH_SCRIPT: &str = r#"

# ── rak: dynamic problem-slug completion ──────────────────────────────
__rak_find_rak_toml() {
    local dir="${1:-$PWD}"
    while [[ "$dir" != "/" ]]; do
        if [[ -f "$dir/rak.toml" ]]; then
            echo "$dir"
            return 0
        fi
        dir="$(cd "$dir/.." && pwd)"
    done
    return 1
}

__rak_problem_slugs() {
    local project_root
    project_root="$(__rak_find_rak_toml "$PWD")" || return

    local lc_dir
    lc_dir="$(grep -m1 '^leetcode_dir' "$project_root/rak.toml" 2>/dev/null \
        | sed 's/leetcode_dir[[:space:]]*=[[:space:]]*"\(.*\)"/\1/' \
        | tr -d "'")"
    [[ -n "$lc_dir" ]] || return

    local full_path="$project_root/$lc_dir"
    [[ -d "$full_path" ]] || return

    local cur="${COMP_WORDS[COMP_CWORD]}"
    local entries
    entries="$(ls -1 "$full_path" 2>/dev/null)" || return

    local candidates=()
    while IFS= read -r entry; do
        if [[ -d "$full_path/$entry" && "$entry" == "$cur"* ]]; then
            candidates+=("$entry")
        fi
    done <<< "$entries"

    if [[ ${#candidates[@]} -gt 0 ]]; then
        COMPREPLY=("${candidates[@]}")
    fi
}

__rak_known_providers="elevenlabs openrouter chirp"

# Override the positional-arg handler for subcommands that take problem IDs.
# The clap-generated completion already registered _rak() as the main handler.
# We hook into it by appending to the _rak() function's case body.
__rak_original_func="$(declare -f _rak | tail -n +3 | head -n -1)"
eval "_rak() {
$__rak_original_func
    # Provider completion for 'login <provider>'
    if [[ \${COMP_WORDS[1]} == \"login\" && \${COMP_CWORD} -eq 2 && \"\${COMP_WORDS[2]}\" != -* ]]; then
        COMPREPLY=(\$(compgen -W \"$__rak_known_providers\" -- \"\${COMP_WORDS[COMP_CWORD]}\"))
        return
    fi

    # Provider completion for '--provider <value>'
    local i
    for ((i=2; i<COMP_CWORD; i++)); do
        if [[ \"\${COMP_WORDS[i]}\" == \"--provider\" || \"\${COMP_WORDS[i]}\" == \"-p\" ]]; then
            COMPREPLY=(\$(compgen -W \"$__rak_known_providers\" -- \"\${COMP_WORDS[COMP_CWORD]}\"))
            return
        fi
    done

    # Dynamic slug completion for positional args on problem-taking subcommands
    if [[ \${COMP_WORDS[1]} == @(add|log|record|transcribe|analyze|push) ]]; then
        if [[ \${COMP_CWORD} -eq 2 && \"\${COMP_WORDS[2]}\" != -* ]]; then
            __rak_problem_slugs
            return
        fi
    fi
}"
"#;
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Test the output**

Run: `cargo run -- completions | head -20`
Expected: prints the beginning of a bash completion script

Run: `cargo run -- completions | tail -20`
Expected: prints the custom `_rak_problem_slugs` bash function

- [ ] **Step 4: Commit**

```bash
git add src/commands/completions.rs src/commands/mod.rs src/main.rs
git commit -m "feat(completions): add rak completions command with bash script generation"
```

---

### Task 4: Manual integration test

**Files:** None (testing only)

- [ ] **Step 1: Build the binary**

Run: `cargo build`
Expected: succeeds

- [ ] **Step 2: Source the completion script**

Run: `eval "$(cargo run -- completions 2>/dev/null)"`

Or if `rak` is already installed/aliased:
Run: `eval "$(rak completions)"`

- [ ] **Step 3: Test subcommand completion**

Type: `rak <TAB><TAB>`
Expected: shows all subcommands (add, analyze, completions, init, log, login, next, pull, push, record, scrape, transcribe)

- [ ] **Step 4: Test flag completion**

Type: `rak transcribe --<TAB><TAB>`
Expected: shows `--force`, `--provider`, `--help`

- [ ] **Step 5: Test slug completion**

From a directory with a `rak.toml` and populated `leetcode_dir`:

Type: `rak transcribe c3ai-<TAB>`
Expected: completes to matching folder name (e.g. `c3ai-second-interview`)

---

### Task 5: Ensure existing tests still pass

**Files:** None (testing only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test`
Expected: all tests pass (no regressions)

- [ ] **Step 2: Commit if any fixes were needed**

Only commit if changes were made to fix test failures.

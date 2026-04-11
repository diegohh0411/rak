# Custom Interview Problems — Design Spec

**Date:** 2026-04-11  
**Status:** Approved

## Overview

RAK currently supports official LeetCode problems identified by numeric ID (e.g. `200` → `0200.number-of-islands/`). This spec extends RAK to support custom interview problems — problems given during interviews or otherwise not in the official LeetCode catalog — using non-numeric slugs as identifiers. All three core commands (`record`, `transcribe`, `analyze`) work identically for both problem types. A new `rak add` command scaffolds custom problem folders.

---

## Problem ID Model

A new `ProblemId` enum is introduced, parsing the raw CLI string at the command boundary:

```rust
pub enum ProblemId {
    Leetcode(u32),   // "200"  → resolves to 0200.number-of-islands/
    Custom(String),  // "c3ai-strings" → resolves to c3ai-strings/
}

impl ProblemId {
    pub fn parse(s: &str) -> Self {
        match s.parse::<u32>() {
            Ok(n)  => ProblemId::Leetcode(n),
            Err(_) => ProblemId::Custom(s.to_owned()),
        }
    }
}
```

`ProblemId` lives in `config.rs` alongside `resolve_problem_folder`, which is updated to accept `&ProblemId` and dispatch:

- `Leetcode(n)` → existing zero-pad regex logic (no behaviour change)
- `Custom(slug)` → exact directory name match in `leetcode_dir`

All commands that currently accept `id: String` call `ProblemId::parse(&id)` at their entry point and pass `&ProblemId` to `resolve_problem_folder`. CLI signatures are unchanged — users type `rak record 200` or `rak record c3ai-strings` with no distinction.

---

## `rak add` Command

New command scaffolds a custom problem folder.

**CLI:**
```
rak add <slug>
```

**Implementation:** `src/commands/add.rs`

```rust
pub fn run(slug: String) -> Result<(), String>
pub fn scaffold(problem_dir: &Path) -> Result<(), String>
```

`run()`:
1. Loads `rak.toml` from cwd
2. Constructs `<leetcode_dir>/<slug>/`
3. Errors with `"Folder '<slug>' already exists"` if directory exists
4. Calls `scaffold()`

`scaffold()` (exported so `record.rs` can call it):
1. Creates the directory
2. Writes empty `question.md`
3. Writes empty `solution.cpp`
4. Prints `Created <slug>/`

---

## Auto-Scaffold in `rak record`

When `record` receives a `Custom(slug)` ID and `resolve_problem_folder` finds no match, instead of returning an error it calls `add::scaffold(&problem_dir)` and continues into recording. No prompt — silent creation.

Numeric IDs retain the existing behaviour: missing folder is an error.

---

## Error Handling

| Command | ID type | Folder missing | Folder exists |
|---------|---------|----------------|---------------|
| `add` | Custom slug | — | Error: `"Folder '<slug>' already exists"` |
| `record` | Leetcode | Error: `"No problem folder matching '0200' found in ..."` | Proceeds |
| `record` | Custom | Auto-scaffolds, then proceeds | Proceeds |
| `transcribe` | Custom | Error: `"No folder matching '<slug>' found in <leetcode_dir>"` | Proceeds |
| `analyze` | Custom | Error: `"No folder matching '<slug>' found in <leetcode_dir>"` | Proceeds |

---

## Testing

New unit tests added to `config.rs`:

- `resolve_problem_folder_custom_exact_match` — creates `c3ai-strings/`, resolves with slug `"c3ai-strings"`
- `resolve_problem_folder_custom_no_match` — errors when slug folder absent
- `resolve_problem_folder_custom_no_partial_match` — `"c3ai"` does not match `"c3ai-strings"`

New unit tests in `commands/add.rs`:

- `add_creates_scaffold` — verifies `question.md` and `solution.cpp` are created
- `add_errors_if_exists` — verifies error when folder already present

Existing numeric tests are unaffected.

---

## Migration: Existing C3.ai Problem

The folder `applications/roles/c3-ai/associates-program/second-interview/` already has the correct RAK layout (`question.md`, `solution.cpp`, `attempt-1.mp3`, `attempt-1.md`, `analysis.md`).

**One-time manual step:** move or copy it into `leetcode_dir` as `c3ai-second-interview/`:

```bash
mv applications/roles/c3-ai/associates-program/second-interview \
   leetcode/solutions/cpp/c3ai-second-interview
```

After migration: `rak record c3ai-second-interview` works as expected.

---

## Future

The `ProblemId` enum is the natural extension point for local test execution (LeetGo-style). A future `ProblemId::Custom` variant or additional metadata can carry test case info without touching the Leetcode resolution path.

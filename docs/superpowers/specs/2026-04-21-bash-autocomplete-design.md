# Bash Autocomplete for `rak`

## Goal

Add shell tab-completion so that typing `rak transcribe c3ai-` + Tab
auto-completes to `c3ai-second-interview` by scanning the configured
`leetcode_dir` for matching folder names. Full completion for
subcommands, flags, and flag values is included.

Target shell: **bash only**.

## Approach

Use `clap_complete` for static completion (subcommands, flags) and layer
a custom bash function for dynamic problem-slug completion from the
filesystem. Exposed via a `rak completions` subcommand that prints the
script to stdout.

## New Subcommand: `rak completions`

```
rak completions [--shell bash]
```

- Outputs a bash completion script to stdout
- `--shell` defaults to `bash`; flag exists for future extensibility
- User adds `eval "$(rak completions)"` to `~/.bashrc`

### Implementation

1. Add `clap_complete` dependency to `Cargo.toml`
2. Add `Completions` variant to the `Command` enum in `main.rs`
3. Create `src/commands/completions.rs` that:
   - Builds the clap `Cli` struct
   - Calls `clap_complete::generate()` to produce the standard bash
     completion (subcommands, flags, static values)
   - Appends a custom bash function for dynamic slug completion

## Dynamic Slug Completion

### Which arguments get slug completion

These positional arguments resolve to a problem folder and will
autocomplete from `leetcode_dir` directory names:

| Command    | Argument |
|------------|----------|
| `add`      | `slug`   |
| `log`      | `id`     |
| `record`   | `id`     |
| `transcribe` | `id`   |
| `analyze`  | `id`     |
| `push`     | `id`     |

Excluded:
- `pull` — `qid` is optional and accepts "today" as a special value
- `login` — `provider` uses a static list instead

### How it works (pure bash, no subprocess)

The appended bash function `_rak_problem_slugs()`:

1. Walks up from `$PWD` looking for `rak.toml` (mirrors
   `config::find_rak_toml` logic)
2. Parses the `leetcode_dir` value from the found `rak.toml` using a
   simple grep/sed (one-line value, no complex TOML parsing needed)
3. Lists directory names inside `<project_root>/<leetcode_dir>/`
4. Filters by the current word prefix being completed

No subprocess call back to `rak` — keeps Tab completion instant.

## Flag Value Completions

| Flag / Arg                          | Completion source                |
|-------------------------------------|----------------------------------|
| `--provider` on `transcribe`        | Hardcoded: `elevenlabs`, `openrouter`, `chirp` |
| `--provider` on `analyze`           | Hardcoded: `elevenlabs`, `openrouter`, `chirp` |
| `login <provider>`                  | Hardcoded: `elevenlabs`, `openrouter`, `chirp` |
| `--force`, `--date`, `--refresh`, etc. | Default clap behavior (no special completion) |

## Files Changed

| File                        | Change                                         |
|-----------------------------|------------------------------------------------|
| `Cargo.toml`                | Add `clap_complete` dependency                 |
| `src/main.rs`               | Add `Completions` variant, wire into dispatch  |
| `src/commands/mod.rs`       | Add `completions` module                       |
| `src/commands/completions.rs` | New file: generates completion script         |

## Dependencies

- `clap_complete = "4"` — clap's official shell completion generator

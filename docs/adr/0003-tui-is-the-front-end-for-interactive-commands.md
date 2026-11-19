# Interactive commands open one ratatui app; plain output stays for scripts

`rak` with no arguments, and `pull`, `record`, `test`, `analyze`, `push` and `next` on a terminal, open a single full-screen app (`src/tui`) built on ratatui, which `pull` and `record` already used. Each command is a problem plus a key press: `rak test 54` selects 54 and presses `t`. The commands keep their plain behavior when stdout is not a terminal or `--plain` is given, so scripts and CI see the same lines as before.

Commands that print progress as they work (`analyze`, `pull`, transcription, `submit`) are shared with the app. They report through the `say!`/`out!` macros, which the app silences with `quiet::set(true)`, and data-returning cores (`log::record`, `push::submit`, `pull::scaffold_slug`, `test::plan`) so the app never parses printed text. Long work runs on plain threads and reports with a `Msg`; it never touches the terminal.

We rejected rewriting in a TypeScript or Go renderer (OpenTUI, Ink, Bubble Tea): ratatui is already a dependency, is maintained, and diffs its buffer so it does not flicker.

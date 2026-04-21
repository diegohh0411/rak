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

eval "$(declare -f _rak | sed 's/^_rak /__rak_clap_original /')"
unset -f _rak

_rak() {
    __rak_clap_original

    if [[ ${#COMPREPLY[@]} -gt 0 ]]; then
        return
    fi

    if [[ ${COMP_WORDS[1]} == "login" && ${COMP_CWORD} -eq 2 && "${COMP_WORDS[2]}" != -* ]]; then
        COMPREPLY=($(compgen -W "$__rak_known_providers" -- "${COMP_WORDS[COMP_CWORD]}"))
        return
    fi

    local i
    for ((i=2; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" == "--provider" || "${COMP_WORDS[i]}" == "-p" ]]; then
            COMPREPLY=($(compgen -W "$__rak_known_providers" -- "${COMP_WORDS[COMP_CWORD]}"))
            return
        fi
    done

    if [[ ${COMP_WORDS[1]} == @(add|log|record|transcribe|analyze|push) ]]; then
        if [[ ${COMP_CWORD} -eq 2 && "${COMP_WORDS[2]}" != -* ]]; then
            __rak_problem_slugs
            return
        fi
    fi
}
"#;

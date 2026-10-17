//! C++: compile `solution.cpp` (whose generated `main()` reads stdin through
//! `LC_IO.h`) into a native binary.

use std::process::Command;

use super::{BuildCtx, BuildError, Executable, Profile, Runtime, stable_hash};
use crate::scaffold;

pub const SOURCE_FILE: &str = "solution.cpp";
const DEFAULT_COMPILER: &str = "c++";
const DEFAULT_FLAGS: &[&str] = &["-std=c++17", "-O2"];
/// Always passed, after the configured flags: falling off the end of a
/// non-void function (a forgotten `return`) is UB that otherwise shows up
/// as a garbage answer.
const REQUIRED_FLAGS: &[&str] = &["-Werror=return-type"];
/// AddressSanitizer + UBSan; UB aborts instead of printing and carrying on.
const DEFAULT_DEBUG_FLAGS: &[&str] = &[
    "-std=c++17",
    "-O1",
    "-g",
    "-fno-omit-frame-pointer",
    "-fsanitize=address,undefined",
    "-fno-sanitize-recover=undefined",
];
/// Generated harnesses never free their input trees and lists, so leak
/// reports would fail every such problem.
const DEBUG_ENV: &[(&str, &str)] = &[
    ("ASAN_OPTIONS", "detect_leaks=0"),
    ("UBSAN_OPTIONS", "print_stacktrace=1"),
];

pub struct CppRuntime {
    compiler: String,
    flags: Vec<String>,
    debug_flags: Vec<String>,
}

impl CppRuntime {
    /// Settings: `compiler` (falls back to `$CXX`, then `c++`), `flags` and
    /// `debug_flags` (used with `rak test --debug`).
    pub fn from_config(config: &serde_json::Value) -> Self {
        let compiler = config
            .get("compiler")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("CXX").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| DEFAULT_COMPILER.to_string());
        Self {
            compiler,
            flags: string_list(config, "flags", DEFAULT_FLAGS),
            debug_flags: string_list(config, "debug_flags", DEFAULT_DEBUG_FLAGS),
        }
    }

    fn flags(&self, profile: Profile) -> &[String] {
        match profile {
            Profile::Release => &self.flags,
            Profile::Debug => &self.debug_flags,
        }
    }

    fn compile_args(&self, ctx: &BuildCtx) -> Vec<String> {
        let mut args = self.flags(ctx.profile).to_vec();
        args.extend(REQUIRED_FLAGS.iter().map(|f| f.to_string()));
        args.push(format!("-I{}", ctx.workspace_dir.display()));
        // Relative, since the compiler runs in the problem folder: keeps
        // diagnostics short (`solution.cpp:20:20: error: ...`).
        args.push(SOURCE_FILE.to_string());
        args.push("-o".to_string());
        args.push(
            ctx.build_dir
                .join(binary_name(ctx.profile))
                .display()
                .to_string(),
        );
        args
    }

    /// Everything that affects the binary: compiler, flags, source and header.
    fn cache_key(&self, ctx: &BuildCtx, source: &[u8]) -> String {
        let header = std::fs::read(ctx.workspace_dir.join("LC_IO.h")).unwrap_or_default();
        let mut material = Vec::new();
        for part in [
            self.compiler.as_bytes(),
            self.flags(ctx.profile).join("\0").as_bytes(),
            REQUIRED_FLAGS.join("\0").as_bytes(),
            ctx.workspace_dir.to_string_lossy().as_bytes(),
            source,
            &header,
        ] {
            material.extend_from_slice(&(part.len() as u64).to_le_bytes());
            material.extend_from_slice(part);
        }
        format!("{:016x}", stable_hash(&material))
    }
}

impl Runtime for CppRuntime {
    fn source_file(&self) -> &str {
        SOURCE_FILE
    }

    fn build(&self, ctx: &BuildCtx) -> Result<Executable, BuildError> {
        let source_path = ctx.problem_dir.join(SOURCE_FILE);
        let source = std::fs::read(&source_path).map_err(|e| {
            BuildError::Other(format!("failed to read {}: {e}", source_path.display()))
        })?;
        if !ctx.workspace_dir.join("LC_IO.h").is_file() {
            scaffold::write_support_headers(ctx.workspace_dir).map_err(BuildError::Other)?;
        }
        std::fs::create_dir_all(ctx.build_dir).map_err(|e| {
            BuildError::Other(format!("failed to create {}: {e}", ctx.build_dir.display()))
        })?;

        let exe = Executable {
            program: ctx.build_dir.join(binary_name(ctx.profile)),
            args: vec![],
            env: match ctx.profile {
                Profile::Release => vec![],
                // Respect sanitizer options the user already exported.
                Profile::Debug => DEBUG_ENV
                    .iter()
                    .filter(|(k, _)| std::env::var_os(k).is_none())
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            },
        };
        let key = self.cache_key(ctx, &source);
        let key_path = ctx
            .build_dir
            .join(format!("{}.key", binary_name(ctx.profile)));
        if exe.program.is_file() && std::fs::read_to_string(&key_path).is_ok_and(|k| k == key) {
            return Ok(exe);
        }
        let _ = std::fs::remove_file(&key_path);

        let output = Command::new(&self.compiler)
            .args(self.compile_args(ctx))
            .current_dir(ctx.problem_dir)
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    BuildError::ToolchainMissing(missing_compiler_hint(&self.compiler))
                } else {
                    BuildError::Other(format!("failed to run {}: {e}", self.compiler))
                }
            })?;
        if !output.status.success() {
            let mut diagnostics = String::from_utf8_lossy(&output.stderr).into_owned();
            diagnostics.push_str(&String::from_utf8_lossy(&output.stdout));
            return Err(BuildError::Compile(diagnostics.trim_end().to_string()));
        }
        std::fs::write(&key_path, key)
            .map_err(|e| BuildError::Other(format!("failed to write build key: {e}")))?;
        Ok(exe)
    }
}

/// Each profile keeps its own binary so switching doesn't evict the cache.
fn binary_name(profile: Profile) -> &'static str {
    match profile {
        Profile::Release => "solution",
        Profile::Debug => "solution-debug",
    }
}

fn string_list(config: &serde_json::Value, key: &str, default: &[&str]) -> Vec<String> {
    config
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_else(|| default.iter().map(|s| s.to_string()).collect())
}

fn missing_compiler_hint(compiler: &str) -> String {
    let install = if cfg!(target_os = "macos") {
        "install the Xcode Command Line Tools (`xcode-select --install`)"
    } else {
        "install g++ or clang++"
    };
    format!(
        "C++ compiler {compiler:?} not found; {install}, or set `compiler` under [test.cpp] in rak.toml"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_overrides_defaults() {
        let rt = CppRuntime::from_config(&json!({
            "compiler": "g++-14",
            "flags": ["-std=c++20", "-O0"],
        }));
        assert_eq!(rt.compiler, "g++-14");
        assert_eq!(rt.flags, vec!["-std=c++20", "-O0"]);
    }

    #[test]
    fn default_flags() {
        let rt = CppRuntime::from_config(&serde_json::Value::Null);
        assert_eq!(rt.flags, vec!["-std=c++17", "-O2"]);
        assert!(rt.debug_flags.iter().any(|f| f.starts_with("-fsanitize=")));
    }

    #[test]
    fn missing_compiler_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SOURCE_FILE), "int main() {}").unwrap();
        let rt = CppRuntime::from_config(&json!({ "compiler": "rak-no-such-compiler" }));
        let build = dir.path().join("build");
        let err = rt
            .build(&BuildCtx {
                problem_dir: dir.path(),
                workspace_dir: dir.path(),
                build_dir: &build,
                profile: Profile::Release,
            })
            .unwrap_err();
        assert!(matches!(err, BuildError::ToolchainMissing(_)), "{err:?}");
    }
}

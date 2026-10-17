//! C++: compile `solution.cpp` (whose generated `main()` reads stdin through
//! `LC_IO.h`) into a native binary.

use std::process::Command;

use super::{BuildCtx, BuildError, Executable, Runtime, stable_hash};
use crate::scaffold;

pub const SOURCE_FILE: &str = "solution.cpp";
const BINARY: &str = "solution";
const KEY_FILE: &str = "solution.key";
const DEFAULT_COMPILER: &str = "c++";
const DEFAULT_FLAGS: &[&str] = &["-std=c++17", "-O2"];

pub struct CppRuntime {
    compiler: String,
    flags: Vec<String>,
}

impl CppRuntime {
    /// Settings: `compiler` (falls back to `$CXX`, then `c++`) and `flags`.
    pub fn from_config(config: &serde_json::Value) -> Self {
        let compiler = config
            .get("compiler")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("CXX").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| DEFAULT_COMPILER.to_string());
        let flags = config
            .get("flags")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_else(|| DEFAULT_FLAGS.iter().map(|s| s.to_string()).collect());
        Self { compiler, flags }
    }

    fn compile_args(&self, ctx: &BuildCtx) -> Vec<String> {
        let mut args = self.flags.clone();
        args.push(format!("-I{}", ctx.workspace_dir.display()));
        // Relative, since the compiler runs in the problem folder: keeps
        // diagnostics short (`solution.cpp:20:20: error: ...`).
        args.push(SOURCE_FILE.to_string());
        args.push("-o".to_string());
        args.push(ctx.build_dir.join(BINARY).display().to_string());
        args
    }

    /// Everything that affects the binary: compiler, flags, source and header.
    fn cache_key(&self, ctx: &BuildCtx, source: &[u8]) -> String {
        let header = std::fs::read(ctx.workspace_dir.join("LC_IO.h")).unwrap_or_default();
        let mut material = Vec::new();
        for part in [
            self.compiler.as_bytes(),
            self.flags.join("\0").as_bytes(),
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
            program: ctx.build_dir.join(BINARY),
            args: vec![],
        };
        let key = self.cache_key(ctx, &source);
        let key_path = ctx.build_dir.join(KEY_FILE);
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
            })
            .unwrap_err();
        assert!(matches!(err, BuildError::ToolchainMissing(_)), "{err:?}");
    }
}

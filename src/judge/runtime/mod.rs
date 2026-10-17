//! Language runtimes: how to turn a problem's solution file into something
//! runnable. Each runtime only builds; running and judging are shared (see
//! [`super::process`] and [`super::verdict`]).
//!
//! A runtime's program must follow the stdout protocol in
//! [`super::protocol`]: read the test input from stdin, one argument per
//! line, and finish with an `output: <value>` line.

pub mod cpp;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// A command that runs a built solution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Extra environment variables for each run.
    pub env: Vec<(String, String)>,
}

/// How to build: `Release` for normal runs, `Debug` to trade speed for
/// diagnostics (sanitizers, debug info) when chasing a crash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Profile {
    #[default]
    Release,
    Debug,
}

pub struct BuildCtx<'a> {
    /// Folder holding the solution file.
    pub problem_dir: &'a Path,
    /// The leetcode folder that holds shared support files (e.g. `LC_IO.h`).
    pub workspace_dir: &'a Path,
    /// Scratch space for build outputs; created by the runtime if missing.
    pub build_dir: &'a Path,
    pub profile: Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// The compiler or interpreter isn't installed; carries a hint to fix it.
    ToolchainMissing(String),
    /// The solution didn't compile; carries the compiler diagnostics.
    Compile(String),
    Other(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::ToolchainMissing(msg) | BuildError::Other(msg) => f.write_str(msg),
            BuildError::Compile(diagnostics) => write!(f, "compile error:\n{diagnostics}"),
        }
    }
}

pub trait Runtime: Send + Sync {
    /// Solution file name inside the problem folder, e.g. `solution.cpp`.
    fn source_file(&self) -> &str;
    /// Build the solution, reusing a previous build when nothing changed.
    fn build(&self, ctx: &BuildCtx) -> Result<Executable, BuildError>;
}

type Factory = Box<dyn Fn(serde_json::Value) -> Box<dyn Runtime> + Send + Sync>;

struct Entry {
    source_file: String,
    factory: Factory,
}

static REGISTRY: LazyLock<Mutex<HashMap<String, Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn register(
    lang: &str,
    source_file: &str,
    factory: impl Fn(serde_json::Value) -> Box<dyn Runtime> + Send + Sync + 'static,
) {
    REGISTRY.lock().unwrap().insert(
        lang.to_string(),
        Entry {
            source_file: source_file.to_string(),
            factory: Box::new(factory),
        },
    );
}

/// Build the runtime for `lang` from its `[test.<lang>]` settings.
pub fn get(lang: &str, config: &serde_json::Value) -> Result<Box<dyn Runtime>, String> {
    let registry = REGISTRY.lock().unwrap();
    let entry = registry.get(lang).ok_or_else(|| {
        let mut available: Vec<&str> = registry.keys().map(|s| s.as_str()).collect();
        available.sort();
        format!(
            "local testing doesn't support {lang:?} yet (supported: {})",
            available.join(", ")
        )
    })?;
    Ok((entry.factory)(config.clone()))
}

/// The language of the solution in `problem_dir`, picked by which registered
/// runtime's source file exists.
pub fn detect(problem_dir: &Path) -> Result<String, String> {
    let registry = REGISTRY.lock().unwrap();
    let mut found: Vec<&str> = registry
        .iter()
        .filter(|(_, e)| problem_dir.join(&e.source_file).is_file())
        .map(|(lang, _)| lang.as_str())
        .collect();
    found.sort();
    match found.as_slice() {
        [lang] => Ok(lang.to_string()),
        [] => {
            let mut files: Vec<&str> = registry.values().map(|e| e.source_file.as_str()).collect();
            files.sort();
            Err(format!(
                "no solution file to test in {} (looked for: {})",
                problem_dir.display(),
                files.join(", ")
            ))
        }
        many => Err(format!(
            "found solutions in several languages ({}); pick one with --lang",
            many.join(", ")
        )),
    }
}

/// Where builds for `problem_dir` go: the user cache dir, so nothing lands in
/// the practice repo.
pub fn default_build_dir(problem_dir: &Path) -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    let name = problem_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.join("rak").join("build").join(format!(
        "{name}-{:016x}",
        stable_hash(problem_dir.to_string_lossy().as_bytes())
    ))
}

/// FNV-1a: stable across runs and Rust versions, unlike `DefaultHasher`.
pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn init_runtimes() {
    register("cpp", cpp::SOURCE_FILE, |config| {
        Box::new(cpp::CppRuntime::from_config(&config))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cpp_solution() {
        init_runtimes();
        let dir = tempfile::tempdir().unwrap();
        assert!(detect(dir.path()).is_err());
        std::fs::write(dir.path().join("solution.cpp"), "").unwrap();
        assert_eq!(detect(dir.path()).unwrap(), "cpp");
    }

    #[test]
    fn unknown_language_errors() {
        init_runtimes();
        let err = get("brainfuck", &serde_json::Value::Null).err().unwrap();
        assert!(err.contains("cpp"), "{err}");
    }

    #[test]
    fn build_dir_is_per_problem() {
        let a = default_build_dir(Path::new("/x/0001.two-sum"));
        let b = default_build_dir(Path::new("/y/0001.two-sum"));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("0001.two-sum"));
    }
}

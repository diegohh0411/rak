use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};

use super::models::ProblemCache;

fn cache_dir(rak_toml_dir: &Path) -> PathBuf {
    rak_toml_dir.join(".rak-cache")
}

fn cache_path(rak_toml_dir: &Path) -> PathBuf {
    cache_dir(rak_toml_dir).join("problems.json")
}

/// Load cache from disk. Returns None if missing or unreadable.
pub fn load(rak_toml_dir: &Path) -> Option<ProblemCache> {
    let data = std::fs::read_to_string(cache_path(rak_toml_dir)).ok()?;
    serde_json::from_str(&data).ok()
}

/// Save cache to disk, creating the directory if needed.
pub fn save(rak_toml_dir: &Path, cache: &ProblemCache) -> Result<(), String> {
    let dir = cache_dir(rak_toml_dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create .rak-cache/: {e}"))?;
    let json = serde_json::to_string_pretty(cache)
        .map_err(|e| format!("serialization error: {e}"))?;
    std::fs::write(cache_path(rak_toml_dir), json)
        .map_err(|e| format!("failed to write cache: {e}"))
}

/// Returns true if `cache` is older than `max_age_days`.
pub fn is_stale(cache: &ProblemCache, max_age_days: i64) -> bool {
    let age = Utc::now().signed_duration_since(cache.fetched_at);
    age > Duration::days(max_age_days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn empty_cache(fetched_at: chrono::DateTime<Utc>) -> ProblemCache {
        ProblemCache {
            fetched_at,
            problems: vec![],
        }
    }

    #[test]
    fn round_trip_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = empty_cache(Utc::now());
        save(dir.path(), &cache).unwrap();
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded.problems.len(), 0);
    }

    #[test]
    fn load_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_none());
    }

    #[test]
    fn is_stale_old_cache() {
        let cache = empty_cache(Utc::now() - Duration::days(10));
        assert!(is_stale(&cache, 7));
    }

    #[test]
    fn is_stale_fresh_cache() {
        let cache = empty_cache(Utc::now() - Duration::days(3));
        assert!(!is_stale(&cache, 7));
    }
}

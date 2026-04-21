use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn store_dir() -> PathBuf {
    dirs::data_dir()
        .expect("cannot determine data directory")
        .join("rak")
}

fn credentials_path() -> PathBuf {
    store_dir().join("credentials.json")
}

pub fn load() -> Result<HashMap<String, serde_json::Value>, String> {
    load_from(&credentials_path())
}

pub fn load_from(path: &Path) -> Result<HashMap<String, serde_json::Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| format!("malformed credentials file: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save(data: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = credentials_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    save_to(data, &path)
}

pub fn save_to(data: &HashMap<String, serde_json::Value>, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &content).map_err(|e| format!("failed to write credentials: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set permissions: {e}"))?;
    }

    std::fs::rename(&tmp, path).map_err(|e| format!("failed to save credentials: {e}"))
}

pub fn get_provider(name: &str) -> Option<serde_json::Value> {
    load().ok()?.get(name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn store_dir_ends_with_rak() {
        let dir = store_dir();
        assert!(
            dir.ends_with("rak"),
            "store_dir should end with 'rak', got {:?}",
            dir
        );
    }

    #[test]
    fn load_returns_empty_when_file_missing() {
        let result = load_from(std::path::Path::new("/nonexistent/credentials.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert(
            "elevenlabs".to_string(),
            serde_json::json!({"api_key": "sk-test"}),
        );
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded["elevenlabs"]["api_key"], "sk-test");
    }

    #[test]
    fn get_provider_returns_value_when_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let mut data = std::collections::HashMap::new();
        data.insert(
            "chirp".to_string(),
            serde_json::json!({"project_id": "my-proj"}),
        );
        save_to(&data, &path).unwrap();
        let loaded = load_from(&path).unwrap();
        let val = loaded.get("chirp").cloned();
        assert!(val.is_some());
        assert_eq!(val.unwrap()["project_id"], "my-proj");
    }

    #[test]
    fn get_provider_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let loaded = load_from(&path).unwrap();
        assert!(loaded.get("nonexistent").is_none());
    }

    #[test]
    #[cfg(unix)]
    fn saved_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        save_to(&std::collections::HashMap::new(), &path).unwrap();
        let meta = fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "credentials.json should be 0600, got {:o}",
            mode
        );
    }
}

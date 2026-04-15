use std::fs;
use std::path::PathBuf;

/// Authenticated credentials for LeetCode API calls.
#[derive(Debug, Clone)]
pub struct LeetcodeCredentials {
    pub session: String,
    pub csrf_token: String,
    /// Human-readable origin, e.g. "env", "firefox"
    #[allow(dead_code)]
    pub source: String,
}

pub trait CredentialProvider {
    #[allow(dead_code)]
    fn name(&self) -> &str;
    fn load(&self) -> Option<LeetcodeCredentials>;
}

// ---------------------------------------------------------------------------
// EnvProvider
// ---------------------------------------------------------------------------

pub struct EnvProvider;

impl CredentialProvider for EnvProvider {
    fn name(&self) -> &str {
        "env"
    }

    fn load(&self) -> Option<LeetcodeCredentials> {
        let session = std::env::var("LEETCODE_SESSION").ok()?;
        if session.is_empty() {
            return None;
        }
        let csrf_token = std::env::var("LEETCODE_CSRFTOKEN").unwrap_or_default();
        Some(LeetcodeCredentials {
            session,
            csrf_token,
            source: "env".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// FirefoxProvider
// ---------------------------------------------------------------------------

pub struct FirefoxProvider;

impl CredentialProvider for FirefoxProvider {
    fn name(&self) -> &str {
        "firefox"
    }

    fn load(&self) -> Option<LeetcodeCredentials> {
        for db_path in find_firefox_dbs() {
            if let Some(creds) = read_firefox_cookies(&db_path) {
                return Some(creds);
            }
        }
        None
    }
}

/// Return all candidate `cookies.sqlite` paths across known Firefox profile locations.
fn find_firefox_dbs() -> Vec<PathBuf> {
    let mut dbs = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    // Candidate profile root directories
    let mut profile_roots: Vec<PathBuf> = vec![
        // Linux native Firefox
        PathBuf::from(&home).join(".mozilla/firefox"),
        // macOS Firefox
        PathBuf::from(&home).join("Library/Application Support/Firefox/Profiles"),
    ];

    // WSL2: enumerate Windows user directories under /mnt/c/Users
    let wsl_users = PathBuf::from("/mnt/c/Users");
    if wsl_users.is_dir() {
        if let Ok(entries) = fs::read_dir(&wsl_users) {
            for entry in entries.flatten() {
                profile_roots.push(
                    entry
                        .path()
                        .join("AppData/Roaming/Mozilla/Firefox/Profiles"),
                );
            }
        }
    }

    for root in &profile_roots {
        if !root.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let db = entry.path().join("cookies.sqlite");
                if db.exists() {
                    dbs.push(db);
                }
            }
        }
    }

    dbs
}

/// Open a Firefox `cookies.sqlite` (via a temp-file copy to avoid browser lock)
/// and extract LeetCode session + csrf cookies.
fn read_firefox_cookies(db_path: &PathBuf) -> Option<LeetcodeCredentials> {
    // Copy to a temp file so we can read it even when Firefox has it locked.
    let tmp = std::env::temp_dir().join("rak_firefox_cookies_tmp.sqlite");
    fs::copy(db_path, &tmp).ok()?;

    let conn = rusqlite::Connection::open(&tmp).ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT name, value FROM moz_cookies \
             WHERE host LIKE '%leetcode.com' \
               AND name IN ('LEETCODE_SESSION', 'csrftoken')",
        )
        .ok()?;

    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()?
        .filter_map(|r| r.ok())
        .collect();

    let session = rows
        .iter()
        .find(|(k, _)| k == "LEETCODE_SESSION")
        .map(|(_, v)| v.clone())?;

    if session.is_empty() {
        return None;
    }

    let csrf_token = rows
        .iter()
        .find(|(k, _)| k == "csrftoken")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    Some(LeetcodeCredentials {
        session,
        csrf_token,
        source: format!("firefox ({})", db_path.display()),
    })
}

// ---------------------------------------------------------------------------
// Public resolver
// ---------------------------------------------------------------------------

/// Try credential providers in order (env → Firefox) and return the first hit.
pub fn resolve() -> Result<LeetcodeCredentials, String> {
    let providers: &[&dyn CredentialProvider] = &[&EnvProvider, &FirefoxProvider];

    for provider in providers {
        if let Some(creds) = provider.load() {
            return Ok(creds);
        }
    }

    Err(
        "No LeetCode credentials found. \
         Set LEETCODE_SESSION in your .env / environment, \
         or ensure Firefox is installed with an active LeetCode session."
            .to_string(),
    )
}

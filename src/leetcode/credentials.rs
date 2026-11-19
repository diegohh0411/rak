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
    if wsl_users.is_dir()
        && let Ok(entries) = fs::read_dir(&wsl_users)
    {
        for entry in entries.flatten() {
            profile_roots.push(
                entry
                    .path()
                    .join("AppData/Roaming/Mozilla/Firefox/Profiles"),
            );
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
// SafariProvider (macOS)
// ---------------------------------------------------------------------------

pub struct SafariProvider;

impl CredentialProvider for SafariProvider {
    fn name(&self) -> &str {
        "safari"
    }

    fn load(&self) -> Option<LeetcodeCredentials> {
        for path in find_safari_cookie_files() {
            let Ok(data) = fs::read(&path) else { continue };
            let Some(cookies) = parse_binary_cookies(&data) else {
                continue;
            };
            let pick = |name: &str| {
                cookies
                    .iter()
                    .find(|c| c.domain.ends_with("leetcode.com") && c.name == name)
                    .map(|c| c.value.clone())
            };
            let Some(session) = pick("LEETCODE_SESSION").filter(|s| !s.is_empty()) else {
                continue;
            };
            return Some(LeetcodeCredentials {
                session,
                csrf_token: pick("csrftoken").unwrap_or_default(),
                source: format!("safari ({})", path.display()),
            });
        }
        None
    }
}

/// Candidate `Cookies.binarycookies` locations (sandboxed Safari first).
fn find_safari_cookie_files() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    [
        "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
        "Library/Cookies/Cookies.binarycookies",
    ]
    .iter()
    .map(|p| home.join(p))
    .filter(|p| p.exists())
    .collect()
}

#[derive(Debug)]
struct BinaryCookie {
    domain: String,
    name: String,
    value: String,
}

/// Parse Apple's `Cookies.binarycookies` format.
fn parse_binary_cookies(data: &[u8]) -> Option<Vec<BinaryCookie>> {
    let be = |o: usize| -> Option<u32> {
        Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?))
    };
    if data.get(0..4)? != b"cook" {
        return None;
    }
    let num_pages = be(4)? as usize;
    let mut page_start = 8 + num_pages * 4;
    let mut out = Vec::new();
    for i in 0..num_pages {
        let size = be(8 + i * 4)? as usize;
        let page = data.get(page_start..page_start + size)?;
        page_start += size;
        let le = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(page.get(o..o + 4)?.try_into().ok()?))
        };
        let count = le(4)? as usize;
        for c in 0..count {
            let off = le(8 + c * 4)? as usize;
            let cookie = page.get(off..off + le(off)? as usize)?;
            let cle = |o: usize| -> Option<usize> {
                Some(u32::from_le_bytes(cookie.get(o..o + 4)?.try_into().ok()?) as usize)
            };
            let cstr = |o: usize| -> Option<String> {
                let s = cookie.get(o..)?;
                let end = s.iter().position(|&b| b == 0)?;
                Some(String::from_utf8_lossy(&s[..end]).into_owned())
            };
            out.push(BinaryCookie {
                domain: cstr(cle(16)?)?,
                name: cstr(cle(20)?)?,
                value: cstr(cle(28)?)?,
            });
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// Public resolver
// ---------------------------------------------------------------------------

/// Try credential providers in order (env → Firefox → Safari) and return the first hit.
pub fn resolve() -> Result<LeetcodeCredentials, String> {
    let providers: &[&dyn CredentialProvider] = &[&EnvProvider, &FirefoxProvider, &SafariProvider];

    for provider in providers {
        if let Some(creds) = provider.load() {
            return Ok(creds);
        }
    }

    Err("No LeetCode credentials found. \
         Set LEETCODE_SESSION in your .env / environment, \
         or log in to LeetCode in Firefox or Safari \
         (Safari requires Full Disk Access for your terminal)."
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal `Cookies.binarycookies` file with the given cookies.
    fn build(cookies: &[(&str, &str, &str)]) -> Vec<u8> {
        let mut recs: Vec<Vec<u8>> = Vec::new();
        for (domain, name, value) in cookies {
            let mut strings = Vec::new();
            let base = 56u32;
            let mut offs = Vec::new();
            for s in [*domain, *name, "/", *value] {
                offs.push(base + strings.len() as u32);
                strings.extend_from_slice(s.as_bytes());
                strings.push(0);
            }
            let size = base + strings.len() as u32;
            let mut r = Vec::new();
            for v in [size, 0, 0, 0, offs[0], offs[1], offs[2], offs[3], 0, 0] {
                r.extend_from_slice(&v.to_le_bytes());
            }
            r.extend_from_slice(&0f64.to_le_bytes());
            r.extend_from_slice(&0f64.to_le_bytes());
            r.extend_from_slice(&strings);
            recs.push(r);
        }
        let mut page = vec![0, 0, 1, 0];
        page.extend_from_slice(&(recs.len() as u32).to_le_bytes());
        let mut off = 8 + 4 * recs.len() as u32 + 4;
        for r in &recs {
            page.extend_from_slice(&off.to_le_bytes());
            off += r.len() as u32;
        }
        page.extend_from_slice(&[0; 4]);
        for r in &recs {
            page.extend_from_slice(r);
        }
        let mut f = b"cook".to_vec();
        f.extend_from_slice(&1u32.to_be_bytes());
        f.extend_from_slice(&(page.len() as u32).to_be_bytes());
        f.extend_from_slice(&page);
        f
    }

    #[test]
    fn parses_binary_cookies() {
        let data = build(&[
            (".example.com", "x", "y"),
            (".leetcode.com", "LEETCODE_SESSION", "sess123"),
            ("leetcode.com", "csrftoken", "tok456"),
        ]);
        let c = parse_binary_cookies(&data).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[1].domain, ".leetcode.com");
        assert_eq!(c[1].name, "LEETCODE_SESSION");
        assert_eq!(c[1].value, "sess123");
        assert_eq!(c[2].value, "tok456");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_binary_cookies(b"nope").is_none());
        assert!(parse_binary_cookies(b"cook\0\0\0\x05").is_none());
    }
}

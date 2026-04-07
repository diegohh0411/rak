use std::fs;

const ENV_KEYS: &[(&str, &str)] = &[];

pub fn run() -> Result<(), String> {
    init_env()?;
    init_gitignore()?;
    Ok(())
}

fn init_env() -> Result<(), String> {
    let path = ".env";

    if !fs::exists(path).map_err(|e| e.to_string())? {
        let content = ENV_KEYS
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(path, content).map_err(|e| e.to_string())?;
        eprintln!("Created .env");
        return Ok(());
    }

    let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut additions = Vec::new();

    for (key, placeholder) in ENV_KEYS {
        let has_key = existing.lines().any(|line| {
            line.starts_with(&format!("{key}="))
        });
        if !has_key {
            additions.push(format!("{key}={placeholder}"));
            eprintln!("Added {key} to .env");
        }
    }

    if !additions.is_empty() {
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&additions.join("\n"));
        content.push('\n');
        fs::write(path, content).map_err(|e| e.to_string())?;
    } else {
        eprintln!(".env already has all required keys");
    }

    Ok(())
}

fn init_gitignore() -> Result<(), String> {
    let path = ".gitignore";
    let entry = ".env";

    if !fs::exists(path).map_err(|e| e.to_string())? {
        fs::write(path, format!("{entry}\n")).map_err(|e| e.to_string())?;
        eprintln!("Created .gitignore");
        return Ok(());
    }

    let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let already_present = existing.lines().any(|line| line.trim() == entry);

    if already_present {
        eprintln!(".gitignore already contains .env");
        return Ok(());
    }

    let mut content = existing;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{entry}\n"));
    fs::write(path, content).map_err(|e| e.to_string())?;
    eprintln!("Added .env to .gitignore");

    Ok(())
}

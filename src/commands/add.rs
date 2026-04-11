use std::path::Path;

use crate::config;

pub fn run(slug: String) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir)?;
    let leetcode_dir = config_dir.join(&cfg.leetcode_dir);
    let problem_dir = leetcode_dir.join(&slug);

    if problem_dir.is_dir() {
        return Err(format!("Folder '{}' already exists", slug));
    }

    scaffold(&problem_dir)?;
    eprintln!("Created {}/", slug);
    Ok(())
}

pub fn scaffold(problem_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(problem_dir).map_err(|e| e.to_string())?;
    std::fs::write(problem_dir.join("question.md"), "").map_err(|e| e.to_string())?;
    std::fs::write(problem_dir.join("solution.cpp"), "").map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scaffold_creates_question_and_solution() {
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("my-slug");
        scaffold(&problem_dir).unwrap();
        assert!(problem_dir.join("question.md").exists());
        assert!(problem_dir.join("solution.cpp").exists());
    }

    #[test]
    fn scaffold_is_idempotent_on_existing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("my-slug");
        fs::create_dir_all(&problem_dir).unwrap();
        // Should not error even though the directory already exists
        scaffold(&problem_dir).unwrap();
        assert!(problem_dir.join("question.md").exists());
    }

    #[test]
    fn scaffold_creates_empty_files() {
        let tmp = tempfile::tempdir().unwrap();
        let problem_dir = tmp.path().join("some-slug");
        scaffold(&problem_dir).unwrap();
        assert_eq!(fs::read_to_string(problem_dir.join("question.md")).unwrap(), "");
        assert_eq!(fs::read_to_string(problem_dir.join("solution.cpp")).unwrap(), "");
    }
}

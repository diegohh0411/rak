//! What the TUI shows, read from the workspace on disk: tracked problems
//! (history.yaml plus problem folders) and, lazily per problem, its
//! recordings, question and analysis.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{Local, NaiveDate};

use crate::history;
use crate::scaffold::testcases;

#[derive(Debug, Clone)]
pub struct Rating {
    pub date: NaiveDate,
    pub rating: u8,
}

#[derive(Debug, Clone)]
pub struct Problem {
    /// The key `rak log` uses: "54" for LeetCode problems, the folder name for custom ones.
    pub id: String,
    pub title: String,
    pub difficulty: String,
    pub folder: Option<PathBuf>,
    pub box_num: u8,
    pub streak: u8,
    pub attempts: Vec<Rating>,
    /// Days overdue (positive) or until due (negative); none before the first attempt.
    pub due: Option<i64>,
}

impl Problem {
    pub fn is_new(&self) -> bool {
        self.attempts.is_empty()
    }

    pub fn is_due(&self) -> bool {
        self.due.is_some_and(|d| d >= 0)
    }

    pub fn rated_today(&self, today: NaiveDate) -> Option<u8> {
        self.attempts
            .iter()
            .find(|a| a.date == today)
            .map(|a| a.rating)
    }
}

pub struct Workspace {
    pub problems: Vec<Problem>,
    pub today: NaiveDate,
    config_dir: PathBuf,
    leetcode_dir: PathBuf,
}

impl Workspace {
    pub fn load(config_dir: &Path, leetcode_dir: &Path) -> Result<Workspace, String> {
        let mut ws = Workspace {
            problems: Vec::new(),
            today: Local::now().date_naive(),
            config_dir: config_dir.to_path_buf(),
            leetcode_dir: leetcode_dir.to_path_buf(),
        };
        ws.reload()?;
        Ok(ws)
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.today = Local::now().date_naive();
        let hist = history::load(&self.config_dir.join("history.yaml"))?;
        let folders = scan_folders(&self.leetcode_dir);

        let mut problems: Vec<Problem> = Vec::new();
        for (id, p) in &hist.problems {
            let folder = folders
                .iter()
                .find(|(key, _)| key == id)
                .map(|(_, f)| f.clone());
            problems.push(Problem {
                id: id.clone(),
                title: p
                    .title
                    .clone()
                    .unwrap_or_else(|| folder_title(folder.as_deref(), id)),
                difficulty: p.difficulty.clone().unwrap_or_default(),
                folder,
                box_num: p.box_num,
                streak: p.streak_perfect,
                attempts: p
                    .attempts
                    .iter()
                    .map(|a| Rating {
                        date: a.date,
                        rating: a.rating,
                    })
                    .collect(),
                due: if p.attempts.is_empty() {
                    None
                } else {
                    Some(p.days_overdue(self.today))
                },
            });
        }
        // Folders nobody logged yet (custom problems).
        for (key, folder) in &folders {
            if !problems.iter().any(|p| &p.id == key) {
                problems.push(Problem {
                    id: key.clone(),
                    title: folder_title(Some(folder), key),
                    difficulty: String::new(),
                    folder: Some(folder.clone()),
                    box_num: 1,
                    streak: 0,
                    attempts: Vec::new(),
                    due: None,
                });
            }
        }

        // New problems first, then the most overdue, then the soonest due.
        problems.sort_by(|a, b| {
            let rank = |p: &Problem| match p.due {
                None => i64::MIN,
                Some(d) => -d,
            };
            rank(a)
                .cmp(&rank(b))
                .then_with(|| id_order(&a.id).cmp(&id_order(&b.id)))
        });
        self.problems = problems;
        Ok(())
    }

    pub fn find(&self, id: &str) -> Option<&Problem> {
        self.problems.iter().find(|p| p.id == id)
    }

    pub fn due_count(&self) -> usize {
        self.problems.iter().filter(|p| p.is_due()).count()
    }

    pub fn mastered_count(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| !p.is_new() && p.box_num == 5)
            .count()
    }
}

fn id_order(id: &str) -> (u32, String) {
    (id.parse().unwrap_or(u32::MAX), id.to_string())
}

/// Problem folders as (history key, path), skipping support folders like `bits`.
fn scan_folders(leetcode_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(leetcode_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !path.is_dir() || name.starts_with('.') {
            continue;
        }
        let looks_like_problem = ["question.md", "solution.cpp", "testcases.txt"]
            .iter()
            .any(|f| path.join(f).exists());
        if looks_like_problem {
            out.push((crate::commands::analyze::folder_to_history_key(&name), path));
        }
    }
    out
}

/// "0054.spiral-matrix" becomes "Spiral Matrix"; custom folders keep their name.
fn folder_title(folder: Option<&Path>, id: &str) -> String {
    let Some(name) = folder.and_then(|f| f.file_name()).and_then(|n| n.to_str()) else {
        return id.to_string();
    };
    match name.split_once('.') {
        Some((num, slug)) if num.chars().all(|c| c.is_ascii_digit()) => slug
            .split('-')
            .map(|w| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        _ => name.to_string(),
    }
}

/// One recording of a problem on disk.
#[derive(Debug, Clone)]
pub struct DiskAttempt {
    pub n: usize,
    pub secs: Option<u64>,
    pub date: Option<NaiveDate>,
    pub transcript: bool,
}

/// Files of one problem folder, read when the problem is opened.
#[derive(Debug, Clone, Default)]
pub struct Disk {
    pub attempts: Vec<DiskAttempt>,
    pub analysis: Option<String>,
    pub analysis_date: Option<NaiveDate>,
    pub question: Option<String>,
    pub case_count: usize,
    pub solution_file: Option<String>,
}

impl Disk {
    pub fn load(folder: &Path) -> Disk {
        let mut disk = Disk::default();
        let re = regex::Regex::new(r"^attempt-(\d+)\.mp3$").unwrap();
        if let Ok(rd) = std::fs::read_dir(folder) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(c) = re.captures(&name) {
                    let n: usize = c[1].parse().unwrap_or(0);
                    disk.attempts.push(DiskAttempt {
                        n,
                        secs: audio_seconds(&entry.path()),
                        date: file_date(&entry.path()),
                        transcript: folder.join(format!("attempt-{n}.md")).exists(),
                    });
                } else if name.starts_with("solution.") && disk.solution_file.is_none() {
                    disk.solution_file = Some(name);
                }
            }
        }
        disk.attempts.sort_by_key(|a| a.n);
        let analysis = folder.join("analysis.md");
        disk.analysis = std::fs::read_to_string(&analysis).ok();
        disk.analysis_date = file_date(&analysis);
        disk.question = std::fs::read_to_string(folder.join("question.md")).ok();
        disk.case_count = std::fs::read_to_string(folder.join("testcases.txt"))
            .ok()
            .and_then(|c| testcases::parse(&c).ok())
            .map_or(0, |c| c.len());
        disk
    }

    pub fn next_attempt(&self) -> usize {
        self.attempts.last().map_or(1, |a| a.n + 1)
    }
}

fn file_date(path: &Path) -> Option<NaiveDate> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(chrono::DateTime::<Local>::from(modified).date_naive())
}

fn audio_seconds(path: &Path) -> Option<u64> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .ok()
        .map(|s| s.round() as u64)
}

/// Where a `rating` lands a problem, computed the way `rak log` does.
pub fn outcome(box_num: u8, streak: u8, rating: u8, first_attempt: bool) -> (u8, u8) {
    let new_box = crate::leitner::next_box(box_num, rating, first_attempt);
    let new_streak = crate::leitner::next_streak(streak, rating);
    (
        crate::leitner::apply_mastery(new_box, new_streak),
        new_streak,
    )
}

/// Days until a problem in `box_num` comes back.
pub fn interval(box_num: u8) -> i64 {
    crate::leitner::interval_days(box_num.clamp(1, 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_titles_are_readable() {
        let p = PathBuf::from("/x/0054.spiral-matrix");
        assert_eq!(folder_title(Some(&p), "54"), "Spiral Matrix");
        let c = PathBuf::from("/x/c3ai-strings");
        assert_eq!(folder_title(Some(&c), "c3ai-strings"), "c3ai-strings");
        assert_eq!(folder_title(None, "9"), "9");
    }

    #[test]
    fn outcome_follows_leitner_rules() {
        assert_eq!(outcome(2, 0, 4, false), (3, 0));
        assert_eq!(outcome(2, 0, 3, false), (2, 0));
        assert_eq!(outcome(3, 0, 1, false), (1, 0));
        assert_eq!(outcome(3, 2, 5, false), (5, 3));
        assert_eq!(outcome(1, 0, 5, true), (1, 1));
    }

    #[test]
    fn new_problems_sort_first_then_most_overdue() {
        let mk = |id: &str, due: Option<i64>| Problem {
            id: id.into(),
            title: id.into(),
            difficulty: String::new(),
            folder: None,
            box_num: 1,
            streak: 0,
            attempts: due
                .map(|_| {
                    vec![Rating {
                        date: NaiveDate::MIN,
                        rating: 3,
                    }]
                })
                .unwrap_or_default(),
            due,
        };
        let mut ps = [
            mk("1", Some(-5)),
            mk("2", Some(3)),
            mk("3", None),
            mk("4", Some(0)),
        ];
        ps.sort_by_key(|p| match p.due {
            None => i64::MIN,
            Some(d) => -d,
        });
        let ids: Vec<&str> = ps.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["3", "2", "4", "1"]);
    }
}

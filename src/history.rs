use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::leitner;

#[derive(Debug, Serialize, Deserialize)]
pub struct History {
    pub problems: BTreeMap<String, Problem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Problem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    pub box_num: u8,
    pub streak_perfect: u8,
    pub last_review: NaiveDate,
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub date: NaiveDate,
    pub rating: u8,
}

impl History {
    pub fn new() -> Self {
        Self {
            problems: BTreeMap::new(),
        }
    }
}

impl Problem {
    /// Compute the date this problem is next due for review.
    pub fn due_date(&self) -> NaiveDate {
        self.last_review + chrono::Duration::days(leitner::interval_days(self.box_num))
    }

    /// Returns true if this problem is due for review on or before `today`.
    pub fn is_due(&self, today: NaiveDate) -> bool {
        self.due_date() <= today
    }

    /// How many days overdue (positive) or until due (negative).
    pub fn days_overdue(&self, today: NaiveDate) -> i64 {
        (today - self.due_date()).num_days()
    }
}

/// Load history from a YAML file. Returns empty history if file doesn't exist.
pub fn load(path: &Path) -> Result<History, String> {
    if !path.exists() {
        return Ok(History::new());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_yml::from_str(&content).map_err(|e| e.to_string())
}

/// Save history to a YAML file.
pub fn save(path: &Path, history: &History) -> Result<(), String> {
    let yaml = serde_yml::to_string(history).map_err(|e| e.to_string())?;
    fs::write(path, yaml).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        let mut history = History::new();
        history.problems.insert("532".to_string(), Problem {
            title: None,
            difficulty: None,
            box_num: 2,
            streak_perfect: 1,
            last_review: NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
            attempts: vec![
                Attempt {
                    date: NaiveDate::from_ymd_opt(2026, 4, 4).unwrap(),
                    rating: 3,
                },
                Attempt {
                    date: NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
                    rating: 5,
                },
            ],
        });

        save(&path, &history).unwrap();
        let loaded = load(&path).unwrap();

        assert_eq!(loaded.problems.len(), 1);
        let p = &loaded.problems["532"];
        assert_eq!(p.box_num, 2);
        assert_eq!(p.streak_perfect, 1);
        assert_eq!(p.attempts.len(), 2);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.yaml");
        let history = load(&path).unwrap();
        assert_eq!(history.problems.len(), 0);
    }

    #[test]
    fn due_date_box_1_is_tomorrow() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let problem = Problem {
            title: None,
            difficulty: None,
            box_num: 1,
            streak_perfect: 0,
            last_review: today,
            attempts: vec![Attempt { date: today, rating: 1 }],
        };
        // Box 1 interval = 1 day, so due tomorrow, NOT today
        assert!(!problem.is_due(today));
        let tomorrow = today + chrono::Duration::days(1);
        assert!(problem.is_due(tomorrow));
    }

    #[test]
    fn due_date_overdue_problem() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let problem = Problem {
            title: None,
            difficulty: None,
            box_num: 1,
            streak_perfect: 0,
            last_review: NaiveDate::from_ymd_opt(2026, 4, 5).unwrap(),
            attempts: vec![],
        };
        // due_date = Apr 5 + 1 = Apr 6, today is Apr 7 → overdue by 1 day
        assert!(problem.is_due(today));
        assert_eq!(problem.days_overdue(today), 1);
    }

    #[test]
    fn due_date_box_3_week_interval() {
        let reviewed = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let problem = Problem {
            title: None,
            difficulty: None,
            box_num: 3,
            streak_perfect: 0,
            last_review: reviewed,
            attempts: vec![],
        };
        // Box 3 = 7 days → due Apr 8
        let apr_7 = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let apr_8 = NaiveDate::from_ymd_opt(2026, 4, 8).unwrap();
        assert!(!problem.is_due(apr_7));
        assert!(problem.is_due(apr_8));
    }
}

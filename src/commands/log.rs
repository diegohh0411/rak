use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::config;
use crate::history::{self, Attempt, Problem};
use crate::leetcode::cache;
use crate::leitner;

const DEFAULT_HISTORY: &str = "history.yaml";

pub fn run(id: String, rating: u8, force: bool, date: Option<String>) -> Result<(), String> {
    let config_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let cfg = config::load(&config_dir).ok();

    let target_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|e| format!("invalid date format '{}': {}", d, e))?,
        None => Local::now().date_naive(),
    };

    let problem_id = config::ProblemId::parse(&id);
    let mut title = None;
    let mut difficulty = None;

    if cfg.is_some() {
        if let Some(rak_toml_path) = config::find_rak_toml(&config_dir).ok() {
            if let Some(rak_toml_dir) = rak_toml_path.parent() {
                if let Some(cached) = cache::load(rak_toml_dir) {
                    match &problem_id {
                        config::ProblemId::Leetcode(n) => {
                            if let Some(p) = cached
                                .problems
                                .into_iter()
                                .find(|p| p.frontend_id.parse::<u32>().unwrap_or(0) == *n)
                            {
                                title = Some(p.title);
                                difficulty = Some(p.difficulty);
                            }
                        }
                        config::ProblemId::Custom(_) => {
                            // For custom problems, we don't have a cache lookup yet
                            // Use the id as title if it's not numeric
                            title = Some(id.clone());
                        }
                    }
                }
            }
        }
    }

    let path = Path::new(DEFAULT_HISTORY);
    let (old_box, new_box, new_streak) =
        log_to_file(path, &id, rating, force, target_date, title, difficulty)?;
    if force {
        eprintln!(
            "Replaced {} on {} → rating {}, box {}→{}, streak {}/3",
            id, target_date, rating, old_box, new_box, new_streak
        );
    } else {
        eprintln!(
            "Logged {} on {} → rating {}, box {}→{}, streak {}/3",
            id, target_date, rating, old_box, new_box, new_streak
        );
    }
    Ok(())
}

/// Replay all attempts in order to derive the current box and streak.
/// Used after replacing an attempt to recompute state from scratch.
fn replay_attempts(attempts: &[Attempt]) -> (u8, u8) {
    let mut box_num = 1u8;
    let mut streak = 0u8;
    for (i, attempt) in attempts.iter().enumerate() {
        let new_box = leitner::next_box(box_num, attempt.rating, i == 0);
        let new_streak = leitner::next_streak(streak, attempt.rating);
        box_num = leitner::apply_mastery(new_box, new_streak);
        streak = new_streak;
    }
    (box_num, streak)
}

fn log_to_file(
    path: &Path,
    id: &str,
    rating: u8,
    force: bool,
    target_date: NaiveDate,
    title: Option<String>,
    difficulty: Option<String>,
) -> Result<(u8, u8, u8), String> {
    if !(1..=5).contains(&rating) {
        return Err("rating must be between 1 and 5".to_string());
    }

    let mut history = history::load(path)?;

    let (old_box, new_box, new_streak) = if let Some(problem) = history.problems.get_mut(id) {
        // Update title/difficulty if they are missing but provided now
        if problem.title.is_none() && title.is_some() {
            problem.title = title;
        }
        if problem.difficulty.is_none() && difficulty.is_some() {
            problem.difficulty = difficulty;
        }

        let existing_attempt_index = problem.attempts.iter().position(|a| a.date == target_date);

        if let Some(idx) = existing_attempt_index {
            if !force {
                return Err(format!(
                    "already logged {} on {} — use --force to replace",
                    id, target_date
                ));
            }
            // --force: replace attempt for this specific date and replay from scratch
            problem.attempts[idx].rating = rating;

            // Re-sort attempts just in case
            problem.attempts.sort_by_key(|a| a.date);

            let (new_box, new_streak) = replay_attempts(&problem.attempts);
            problem.box_num = new_box;
            problem.streak_perfect = new_streak;
            problem.last_review = problem.attempts.last().unwrap().date;

            // Compute old_box for display (it's less meaningful for past dates but let's just return what was there)
            (0, new_box, new_streak)
        } else {
            // New date, append attempt and sort
            let old_box = problem.box_num;

            problem.attempts.push(Attempt {
                date: target_date,
                rating,
            });
            problem.attempts.sort_by_key(|a| a.date);

            // Re-derive state after sorting
            let (new_box, new_streak) = replay_attempts(&problem.attempts);

            problem.box_num = new_box;
            problem.streak_perfect = new_streak;
            problem.last_review = problem.attempts.last().unwrap().date;

            (old_box, new_box, new_streak)
        }
    } else {
        // New problem
        let new_streak = leitner::next_streak(0, rating);
        let new_box = leitner::apply_mastery(1, new_streak);

        history.problems.insert(
            id.to_string(),
            Problem {
                title,
                difficulty,
                box_num: new_box,
                streak_perfect: new_streak,
                last_review: target_date,
                attempts: vec![Attempt {
                    date: target_date,
                    rating,
                }],
            },
        );

        (0, new_box, new_streak)
    };

    history::save(path, &history)?;
    Ok((old_box, new_box, new_streak))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APR1: fn() -> NaiveDate = || NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
    const APR2: fn() -> NaiveDate = || NaiveDate::from_ymd_opt(2026, 4, 2).unwrap();
    const APR3: fn() -> NaiveDate = || NaiveDate::from_ymd_opt(2026, 4, 3).unwrap();

    #[test]
    fn log_new_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "532", 4, false, APR1(), None, None).unwrap();

        let h = history::load(&path).unwrap();
        let p = &h.problems["532"];
        assert_eq!(p.box_num, 1);
        assert_eq!(p.streak_perfect, 0);
        assert_eq!(p.attempts.len(), 1);
        assert_eq!(p.attempts[0].rating, 4);
    }

    #[test]
    fn log_existing_problem_moves_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "532", 5, false, APR1(), None, None).unwrap(); // first attempt, box stays 1
        log_to_file(&path, "532", 5, false, APR2(), None, None).unwrap(); // second attempt, box 1→2

        let h = history::load(&path).unwrap();
        let p = &h.problems["532"];
        assert_eq!(p.box_num, 2);
        assert_eq!(p.streak_perfect, 2);
        assert_eq!(p.attempts.len(), 2);
    }

    #[test]
    fn log_invalid_rating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");
        let err = log_to_file(&path, "532", 6, false, APR1(), None, None).unwrap_err();
        assert!(err.contains("between 1 and 5"));
    }

    #[test]
    fn log_mastery_jumps_to_box_5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "1", 5, false, APR1(), None, None).unwrap(); // box 1, streak 1
        log_to_file(&path, "1", 5, false, APR2(), None, None).unwrap(); // box 2, streak 2
        log_to_file(&path, "1", 5, false, APR3(), None, None).unwrap(); // mastery → box 5

        let h = history::load(&path).unwrap();
        let p = &h.problems["1"];
        assert_eq!(p.box_num, 5);
        assert_eq!(p.streak_perfect, 3);
    }

    #[test]
    fn same_day_retry_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "238", 3, false, APR1(), None, None).unwrap();
        let err = log_to_file(&path, "238", 5, false, APR1(), None, None).unwrap_err();
        assert!(err.contains("already logged"));
        assert!(err.contains("--force"));
    }

    #[test]
    fn force_replaces_todays_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        // First day: log a bad rating
        log_to_file(&path, "238", 1, false, APR1(), None, None).unwrap();
        // Same day: --force replaces it with a better rating
        log_to_file(&path, "238", 5, true, APR1(), None, None).unwrap();

        let h = history::load(&path).unwrap();
        let p = &h.problems["238"];
        // Only one attempt (replaced, not appended)
        assert_eq!(p.attempts.len(), 1);
        assert_eq!(p.attempts[0].rating, 5);
        // Box computed from the replaced rating (first attempt stays box 1)
        assert_eq!(p.box_num, 1);
        assert_eq!(p.streak_perfect, 1);
    }

    #[test]
    fn force_replaces_then_next_day_advances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "238", 1, false, APR1(), None, None).unwrap(); // box 1, streak 0
        log_to_file(&path, "238", 5, true,  APR1(), None, None).unwrap(); // replace → box 1, streak 1
        log_to_file(&path, "238", 5, false, APR2(), None, None).unwrap(); // box 2, streak 2

        let h = history::load(&path).unwrap();
        let p = &h.problems["238"];
        assert_eq!(p.attempts.len(), 2);
        assert_eq!(p.box_num, 2);
        assert_eq!(p.streak_perfect, 2);
    }

    #[test]
    fn force_on_new_problem_works_normally() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        // --force on a brand-new problem should just work like a normal log
        log_to_file(&path, "999", 4, true, APR1(), None, None).unwrap();

        let h = history::load(&path).unwrap();
        let p = &h.problems["999"];
        assert_eq!(p.attempts.len(), 1);
        assert_eq!(p.box_num, 1);
    }

    #[test]
    fn log_past_date_sorts_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "1", 5, false, APR2(), None, None).unwrap(); // log Apr 2
        log_to_file(&path, "1", 5, false, APR1(), None, None).unwrap(); // log Apr 1 (past)

        let h = history::load(&path).unwrap();
        let p = &h.problems["1"];
        assert_eq!(p.attempts.len(), 2);
        assert_eq!(p.attempts[0].date, APR1());
        assert_eq!(p.attempts[1].date, APR2());
        // State should be:
        // Apr 1: first attempt, box 1, streak 1
        // Apr 2: second attempt, box 1->2, streak 2
        assert_eq!(p.box_num, 2);
        assert_eq!(p.streak_perfect, 2);
    }
}

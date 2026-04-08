use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::history::{self, Attempt, Problem};
use crate::leitner;

const DEFAULT_HISTORY: &str = "history.yaml";

pub fn run(id: String, rating: u8, force: bool) -> Result<(), String> {
    let path = Path::new(DEFAULT_HISTORY);
    let today = Local::now().date_naive();
    let (old_box, new_box, new_streak) = log_to_file(path, &id, rating, force, today)?;
    if force {
        eprintln!(
            "Replaced {} → rating {}, box {}→{}, streak {}/3",
            id, rating, old_box, new_box, new_streak
        );
    } else {
        eprintln!(
            "Logged {} → rating {}, box {}→{}, streak {}/3",
            id, rating, old_box, new_box, new_streak
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
    today: NaiveDate,
) -> Result<(u8, u8, u8), String> {
    if !(1..=5).contains(&rating) {
        return Err("rating must be between 1 and 5".to_string());
    }

    let mut history = history::load(path)?;

    let (old_box, new_box, new_streak) = if let Some(problem) = history.problems.get_mut(id) {
        if problem.last_review == today {
            if !force {
                return Err(format!(
                    "already logged {} today — use --force to replace",
                    id
                ));
            }
            // --force: replace today's attempt and replay from scratch
            let last = problem.attempts.last_mut()
                .expect("last_review == today implies at least one attempt");
            last.rating = rating;

            // old_box = state after all attempts except today's
            let (old_box, _) = if problem.attempts.len() > 1 {
                replay_attempts(&problem.attempts[..problem.attempts.len() - 1])
            } else {
                (0, 0)
            };

            let (new_box, new_streak) = replay_attempts(&problem.attempts);
            problem.box_num = new_box;
            problem.streak_perfect = new_streak;

            (old_box, new_box, new_streak)
        } else {
            // Normal: new day, append attempt
            let old_box = problem.box_num;
            let new_box = leitner::next_box(old_box, rating, false);
            let new_streak = leitner::next_streak(problem.streak_perfect, rating);
            let final_box = leitner::apply_mastery(new_box, new_streak);

            problem.box_num = final_box;
            problem.streak_perfect = new_streak;
            problem.last_review = today;
            problem.attempts.push(Attempt { date: today, rating });

            (old_box, final_box, new_streak)
        }
    } else {
        // New problem
        let new_streak = leitner::next_streak(0, rating);
        let new_box = leitner::apply_mastery(1, new_streak);

        history.problems.insert(id.to_string(), Problem {
            title: None,
            difficulty: None,
            box_num: new_box,
            streak_perfect: new_streak,
            last_review: today,
            attempts: vec![Attempt { date: today, rating }],
        });

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

        log_to_file(&path, "532", 4, false, APR1()).unwrap();

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

        log_to_file(&path, "532", 5, false, APR1()).unwrap(); // first attempt, box stays 1
        log_to_file(&path, "532", 5, false, APR2()).unwrap(); // second attempt, box 1→2

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
        let err = log_to_file(&path, "532", 6, false, APR1()).unwrap_err();
        assert!(err.contains("between 1 and 5"));
    }

    #[test]
    fn log_mastery_jumps_to_box_5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "1", 5, false, APR1()).unwrap(); // box 1, streak 1
        log_to_file(&path, "1", 5, false, APR2()).unwrap(); // box 2, streak 2
        log_to_file(&path, "1", 5, false, APR3()).unwrap(); // mastery → box 5

        let h = history::load(&path).unwrap();
        let p = &h.problems["1"];
        assert_eq!(p.box_num, 5);
        assert_eq!(p.streak_perfect, 3);
    }

    #[test]
    fn same_day_retry_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "238", 3, false, APR1()).unwrap();
        let err = log_to_file(&path, "238", 5, false, APR1()).unwrap_err();
        assert!(err.contains("already logged"));
        assert!(err.contains("--force"));
    }

    #[test]
    fn force_replaces_todays_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        // First day: log a bad rating
        log_to_file(&path, "238", 1, false, APR1()).unwrap();
        // Same day: --force replaces it with a better rating
        log_to_file(&path, "238", 5, true, APR1()).unwrap();

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

        log_to_file(&path, "238", 1, false, APR1()).unwrap(); // box 1, streak 0
        log_to_file(&path, "238", 5, true,  APR1()).unwrap(); // replace → box 1, streak 1
        log_to_file(&path, "238", 5, false, APR2()).unwrap(); // box 2, streak 2

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
        log_to_file(&path, "999", 4, true, APR1()).unwrap();

        let h = history::load(&path).unwrap();
        let p = &h.problems["999"];
        assert_eq!(p.attempts.len(), 1);
        assert_eq!(p.box_num, 1);
    }
}

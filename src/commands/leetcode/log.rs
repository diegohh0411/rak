use std::path::Path;

use chrono::Local;

use crate::history::{self, Attempt, Problem};
use crate::leitner;

const DEFAULT_HISTORY: &str = "history.yaml";

pub fn run(id: String, rating: u8) -> Result<(), String> {
    let path = Path::new(DEFAULT_HISTORY);
    let (old_box, new_box, new_streak) = log_to_file(path, &id, rating)?;
    eprintln!(
        "Logged {} → rating {}, box {}→{}, streak {}/3",
        id, rating, old_box, new_box, new_streak
    );
    Ok(())
}

fn log_to_file(path: &Path, id: &str, rating: u8) -> Result<(u8, u8, u8), String> {
    if !(1..=5).contains(&rating) {
        return Err("rating must be between 1 and 5".to_string());
    }

    let mut history = history::load(path)?;
    let today = Local::now().date_naive();

    let (old_box, new_box, new_streak) = if let Some(problem) = history.problems.get_mut(id) {
        let old_box = problem.box_num;
        let new_box = leitner::next_box(old_box, rating, false);
        let new_streak = leitner::next_streak(problem.streak_perfect, rating);
        let final_box = leitner::apply_mastery(new_box, new_streak);

        problem.box_num = final_box;
        problem.streak_perfect = new_streak;
        problem.last_review = today;
        problem.attempts.push(Attempt { date: today, rating });

        (old_box, final_box, new_streak)
    } else {
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

    #[test]
    fn log_new_problem() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "532", 4).unwrap();

        let h = history::load(&path).unwrap();
        let p = &h.problems["532"];
        assert_eq!(p.box_num, 1); // first attempt always box 1
        assert_eq!(p.streak_perfect, 0); // rating 4, not 5
        assert_eq!(p.attempts.len(), 1);
        assert_eq!(p.attempts[0].rating, 4);
    }

    #[test]
    fn log_existing_problem_moves_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "532", 5).unwrap(); // first attempt, box stays 1
        log_to_file(&path, "532", 5).unwrap(); // second attempt, box 1→2

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
        let err = log_to_file(&path, "532", 6).unwrap_err();
        assert!(err.contains("between 1 and 5"));
    }

    #[test]
    fn log_mastery_jumps_to_box_5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.yaml");

        log_to_file(&path, "1", 5).unwrap(); // box 1, streak 1
        log_to_file(&path, "1", 5).unwrap(); // box 2, streak 2
        log_to_file(&path, "1", 5).unwrap(); // box 3, streak 3 → mastery → box 5

        let h = history::load(&path).unwrap();
        let p = &h.problems["1"];
        assert_eq!(p.box_num, 5);
        assert_eq!(p.streak_perfect, 3);
    }
}

use std::path::Path;

use chrono::{Local, NaiveDate};

use crate::history::{self, History, Problem};

const DEFAULT_HISTORY: &str = "history.yaml";

pub fn run(count: usize) -> Result<(), String> {
    let path = Path::new(DEFAULT_HISTORY);
    let history = history::load(path)?;

    if history.problems.is_empty() {
        eprintln!("No problems logged yet. Use 'rak l log <id> <rating>' to start tracking.");
        return Ok(());
    }

    let today = Local::now().date_naive();
    let due = collect_due(&history, today, count);

    if due.is_empty() {
        if let Some((id, days)) = next_upcoming(&history, today) {
            eprintln!("Nothing to review today. Next up: {} in {} days.", id, days);
        } else {
            eprintln!("Nothing to review today.");
        }
        return Ok(());
    }

    println!(
        " {:<5} {:<30} {:<4} {:<12} {:<8} {:<8} {}",
        "#", "PROBLEM", "BOX", "LAST", "RATING", "DUE", "STREAK"
    );

    for item in &due {
        let title = item.title.as_deref().unwrap_or("");
        let display = if title.is_empty() {
            item.id.clone()
        } else {
            format!("{} {}", item.id, title)
        };
        println!(
            " {:<5} {:<30} {}/5  {:<12} {:<8} {:<8} {}/3",
            item.id,
            if display.len() > 30 { &display[..30] } else { &display },
            item.box_num,
            item.last_review.format("%Y-%m-%d"),
            format_stars(item.last_rating),
            format_due(item.days_overdue),
            item.streak,
        );
    }

    Ok(())
}

struct DueItem {
    id: String,
    title: Option<String>,
    box_num: u8,
    last_review: NaiveDate,
    last_rating: u8,
    days_overdue: i64,
    streak: u8,
}

fn collect_due(history: &History, today: NaiveDate, count: usize) -> Vec<DueItem> {
    let mut items: Vec<DueItem> = history
        .problems
        .iter()
        .filter(|(_, p)| p.is_due(today))
        .map(|(id, p)| {
            let last_rating = p.attempts.last().map(|a| a.rating).unwrap_or(0);
            DueItem {
                id: id.clone(),
                title: p.title.clone(),
                box_num: p.box_num,
                last_review: p.last_review,
                last_rating,
                days_overdue: p.days_overdue(today),
                streak: p.streak_perfect,
            }
        })
        .collect();

    items.sort_by(|a, b| b.days_overdue.cmp(&a.days_overdue));
    items.truncate(count);
    items
}

fn format_stars(rating: u8) -> String {
    let filled = rating as usize;
    let empty = 5 - filled;
    "★".repeat(filled) + &"☆".repeat(empty)
}

fn format_due(days_overdue: i64) -> String {
    match days_overdue {
        0 => "today".to_string(),
        n => format!("{}d ago", n),
    }
}

fn next_upcoming(history: &History, today: NaiveDate) -> Option<(String, i64)> {
    history
        .problems
        .iter()
        .filter(|(_, p)| !p.is_due(today))
        .map(|(id, p)| {
            let days_until = (p.due_date() - today).num_days();
            (id.clone(), days_until)
        })
        .min_by_key(|(_, days)| *days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Attempt, History};

    fn make_problem(box_num: u8, last_review: NaiveDate, rating: u8) -> Problem {
        Problem {
            title: None,
            difficulty: None,
            box_num,
            streak_perfect: 0,
            last_review,
            attempts: vec![Attempt { date: last_review, rating }],
        }
    }

    #[test]
    fn collect_due_filters_correctly() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let mut history = History::new();

        // Box 1, reviewed yesterday → due today (due=Apr6+1=Apr7)
        history.problems.insert("100".into(),
            make_problem(1, NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(), 2));

        // Box 1, reviewed today → due tomorrow, NOT due yet
        history.problems.insert("200".into(),
            make_problem(1, today, 3));

        // Box 2, reviewed Apr 3 → due Apr 6 (overdue 1 day)
        history.problems.insert("300".into(),
            make_problem(2, NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(), 4));

        let due = collect_due(&history, today, 10);
        assert_eq!(due.len(), 2);
        assert_eq!(due[0].id, "300"); // 1 day overdue
        assert_eq!(due[1].id, "100"); // 0 days overdue (due today)
    }

    #[test]
    fn reviewed_today_box1_not_due() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let mut history = History::new();
        history.problems.insert("238".into(), make_problem(1, today, 1));

        let due = collect_due(&history, today, 10);
        assert_eq!(due.len(), 0); // NOT due today — leetgo bug fix
    }

    #[test]
    fn format_stars_renders() {
        assert_eq!(format_stars(1), "★☆☆☆☆");
        assert_eq!(format_stars(3), "★★★☆☆");
        assert_eq!(format_stars(5), "★★★★★");
    }

    #[test]
    fn format_due_renders() {
        assert_eq!(format_due(0), "today");
        assert_eq!(format_due(1), "1d ago");
        assert_eq!(format_due(5), "5d ago");
    }

    #[test]
    fn next_upcoming_finds_soonest() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 7).unwrap();
        let mut history = History::new();

        // Box 1, reviewed today → due tomorrow (1 day away)
        history.problems.insert("100".into(), make_problem(1, today, 3));

        // Box 3, reviewed today → due in 7 days
        history.problems.insert("200".into(), make_problem(3, today, 3));

        let (id, days) = next_upcoming(&history, today).unwrap();
        assert_eq!(id, "100");
        assert_eq!(days, 1);
    }
}

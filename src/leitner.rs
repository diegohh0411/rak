/// Box intervals in days, indexed by box number (1-5).
/// Box 1 = 1 day, Box 2 = 3 days, Box 3 = 7 days, Box 4 = 14 days, Box 5 = 30 days.
const BOX_INTERVALS: [i64; 5] = [1, 3, 7, 14, 30];

/// Compute the new box after a review.
/// - Rating 4-5: move up one box (max 5)
/// - Rating 3: stay
/// - Rating 1-2: drop to box 1
///   First-time problems always start at box 1 regardless of rating.
pub fn next_box(current_box: u8, rating: u8, is_first_attempt: bool) -> u8 {
    if is_first_attempt {
        return 1;
    }
    match rating {
        4..=5 => (current_box + 1).min(5),
        3 => current_box,
        _ => 1,
    }
}

/// Update the perfect streak counter.
/// - Rating 5: increment (cap at 3)
/// - Rating <5: reset to 0
pub fn next_streak(current_streak: u8, rating: u8) -> u8 {
    if rating == 5 {
        current_streak.min(2) + 1
    } else {
        0
    }
}

/// If streak reaches 3, the box jumps to 5 (mastery).
/// Returns the final box after applying mastery bonus.
pub fn apply_mastery(new_box: u8, new_streak: u8) -> u8 {
    if new_streak >= 3 {
        5
    } else {
        new_box
    }
}

/// Get the review interval in days for a given box (1-5).
pub fn interval_days(box_num: u8) -> i64 {
    BOX_INTERVALS[(box_num - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- next_box --

    #[test]
    fn rating_5_moves_up() {
        assert_eq!(next_box(1, 5, false), 2);
        assert_eq!(next_box(4, 5, false), 5);
    }

    #[test]
    fn rating_4_moves_up() {
        assert_eq!(next_box(2, 4, false), 3);
    }

    #[test]
    fn rating_5_capped_at_5() {
        assert_eq!(next_box(5, 5, false), 5);
    }

    #[test]
    fn rating_3_stays() {
        assert_eq!(next_box(3, 3, false), 3);
    }

    #[test]
    fn rating_1_drops_to_1() {
        assert_eq!(next_box(4, 1, false), 1);
    }

    #[test]
    fn rating_2_drops_to_1() {
        assert_eq!(next_box(3, 2, false), 1);
    }

    #[test]
    fn first_attempt_always_box_1() {
        assert_eq!(next_box(1, 5, true), 1);
    }

    // -- next_streak --

    #[test]
    fn streak_increments_on_5() {
        assert_eq!(next_streak(0, 5), 1);
        assert_eq!(next_streak(2, 5), 3);
    }

    #[test]
    fn streak_caps_at_3() {
        assert_eq!(next_streak(3, 5), 3);
    }

    #[test]
    fn streak_resets_on_non_5() {
        assert_eq!(next_streak(2, 4), 0);
        assert_eq!(next_streak(1, 1), 0);
    }

    // -- apply_mastery --

    #[test]
    fn mastery_jumps_to_box_5() {
        assert_eq!(apply_mastery(2, 3), 5);
    }

    #[test]
    fn no_mastery_below_3_streak() {
        assert_eq!(apply_mastery(2, 2), 2);
    }

    // -- interval_days --

    #[test]
    fn box_intervals() {
        assert_eq!(interval_days(1), 1);
        assert_eq!(interval_days(2), 3);
        assert_eq!(interval_days(3), 7);
        assert_eq!(interval_days(4), 14);
        assert_eq!(interval_days(5), 30);
    }
}

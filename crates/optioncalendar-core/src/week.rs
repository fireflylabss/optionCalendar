//! Week-start preference for grid layout (Monday-first by default, ISO 8601).

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

/// First day of the week used by the month/week grids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WeekStart {
    Sunday,
    #[default]
    Monday,
}

impl WeekStart {
    /// Days from this week's start to `date`'s weekday (0..=6).
    pub fn days_since_start(&self, date: NaiveDate) -> i64 {
        let sunday_based = date.weekday().num_days_from_sunday() as i64;
        match self {
            WeekStart::Sunday => sunday_based,
            WeekStart::Monday => (sunday_based + 6) % 7,
        }
    }

    /// Date of the week-start for the week containing `date`.
    pub fn start_of(&self, date: NaiveDate) -> NaiveDate {
        date - Duration::days(self.days_since_start(date))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn monday_start_week_begins_on_monday() {
        // 2026-09-08 is a Tuesday.
        let tue = d(2026, 9, 8);
        assert_eq!(WeekStart::Monday.start_of(tue), d(2026, 9, 7));
        // Sunday belongs to the previous Monday-start week.
        let sun = d(2026, 9, 13);
        assert_eq!(WeekStart::Monday.start_of(sun), d(2026, 9, 7));
    }

    #[test]
    fn sunday_start_week_begins_on_sunday() {
        // 2026-09-08 (Tue) -> Sunday 2026-09-06.
        assert_eq!(WeekStart::Sunday.start_of(d(2026, 9, 8)), d(2026, 9, 6));
        // Sunday itself starts the week.
        assert_eq!(WeekStart::Sunday.start_of(d(2026, 9, 13)), d(2026, 9, 13));
    }

    #[test]
    fn days_since_start_is_zero_on_first_day() {
        assert_eq!(WeekStart::Monday.days_since_start(d(2026, 9, 7)), 0);
        assert_eq!(WeekStart::Sunday.days_since_start(d(2026, 9, 6)), 0);
    }
}

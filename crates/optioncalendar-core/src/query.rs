//! Day / week / month queries over events, plus the merged "today" view.

use chrono::{Datelike, Local, NaiveDate};

use crate::ics::Event;
use crate::tasks::TaskDue;

/// One line of the merged "today" view: a calendar event or a due task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayItem {
    Event(Event),
    Task(TaskDue),
}

impl DayItem {
    /// Sort key: timed entries first by time, then all-day items by title.
    fn sort_key(&self) -> (NaiveDate, String) {
        match self {
            DayItem::Event(event) => (
                event.start.date(),
                event.start.format("%H:%M ").to_string() + &event.summary,
            ),
            DayItem::Task(task) => (task.due, format!("zz {}", task.text)),
        }
    }
}

/// True when the event touches `date` (multi-day events included).
pub fn occurs_on(event: &Event, date: NaiveDate) -> bool {
    event.start.date() <= date && event.end_or_start().date() >= date
}

/// Events touching `date`, sorted by start time.
pub fn events_on<'a>(events: &'a [Event], date: NaiveDate) -> Vec<&'a Event> {
    let mut hits: Vec<&Event> = events.iter().filter(|e| occurs_on(e, date)).collect();
    hits.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.summary.cmp(&b.summary))
    });
    hits
}

/// Events touching any day in `[start, end]`, sorted by start time.
pub fn events_between<'a>(events: &'a [Event], start: NaiveDate, end: NaiveDate) -> Vec<&'a Event> {
    let mut hits: Vec<&Event> = events
        .iter()
        .filter(|e| e.start.date() <= end && e.end_or_start().date() >= start)
        .collect();
    hits.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.summary.cmp(&b.summary))
    });
    hits
}

/// Today's date in local time.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// First and last day of `date`'s month.
pub fn month_range(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month");
    let last = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
    .expect("valid month")
    .pred_opt()
    .expect("month has a last day");
    (first, last)
}

/// Merged "today" view: today's events plus tasks due today (or overdue).
/// Task failures are handled by the caller — use [`crate::tasks::due_tasks_or_empty`].
pub fn today_merged(events: &[Event], tasks: &[TaskDue], date: NaiveDate) -> Vec<DayItem> {
    let mut items: Vec<DayItem> = events_on(events, date)
        .into_iter()
        .cloned()
        .map(DayItem::Event)
        .collect();
    items.extend(
        tasks
            .iter()
            .filter(|task| task.due <= date)
            .cloned()
            .map(DayItem::Task),
    );
    items.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn dt(raw: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").unwrap()
    }

    fn event(summary: &str, start: &str, end: Option<&str>) -> Event {
        Event::new(summary, dt(start), end.map(dt))
    }

    #[test]
    fn multiday_event_occurs_on_middle_day() {
        let e = event("Conf", "2026-09-04T09:00", Some("2026-09-06T18:00"));
        let mid = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
        assert!(occurs_on(&e, mid));
        assert!(!occurs_on(&e, NaiveDate::from_ymd_opt(2026, 9, 7).unwrap()));
    }

    #[test]
    fn today_merged_includes_overdue_tasks() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let events = vec![event("Standup", "2026-09-04T10:00", None)];
        let tasks = vec![
            TaskDue {
                text: "pay bill".into(),
                due: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
                source: "tasks/a.md".into(),
            },
            TaskDue {
                text: "future".into(),
                due: NaiveDate::from_ymd_opt(2026, 9, 10).unwrap(),
                source: "tasks/a.md".into(),
            },
        ];
        let items = today_merged(&events, &tasks, date);
        assert_eq!(items.len(), 2);
        // Overdue tasks surface before today's timed events.
        assert!(matches!(items[0], DayItem::Task(_)));
        assert!(matches!(items[1], DayItem::Event(_)));
    }

    #[test]
    fn month_range_september() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (first, last) = month_range(date);
        assert_eq!(first, NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert_eq!(last, NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
    }
}

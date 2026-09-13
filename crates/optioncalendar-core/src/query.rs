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
    /// Day this item belongs to (event start day, or task due day).
    pub fn date(&self) -> NaiveDate {
        match self {
            DayItem::Event(event) => event.start.date(),
            DayItem::Task(task) => task.due,
        }
    }

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
pub fn events_on(events: &[Event], date: NaiveDate) -> Vec<&Event> {
    let mut hits: Vec<&Event> = events.iter().filter(|e| occurs_on(e, date)).collect();
    hits.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| a.summary.cmp(&b.summary))
    });
    hits
}

/// Events touching any day in `[start, end]`, sorted by start time.
pub fn events_between(events: &[Event], start: NaiveDate, end: NaiveDate) -> Vec<&Event> {
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
    items.sort_by_key(|item| item.sort_key());
    items
}

/// Merged range view: events touching `[start, end]` plus tasks due within
/// the same window, sorted by day then time (tasks sort after timed events).
pub fn merged_between(
    events: &[Event],
    tasks: &[TaskDue],
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<DayItem> {
    let mut items: Vec<DayItem> = events_between(events, start, end)
        .into_iter()
        .cloned()
        .map(DayItem::Event)
        .collect();
    items.extend(
        tasks
            .iter()
            .filter(|task| task.due >= start && task.due <= end)
            .cloned()
            .map(DayItem::Task),
    );
    items.sort_by_key(|item| item.sort_key());
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
    fn merged_between_groups_tasks_on_due_day_within_window() {
        let start = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap();
        let events = vec![
            event("Standup", "2026-09-08T10:00", None),
            event("Outside", "2026-09-20T10:00", None),
        ];
        let task = |text: &str, y: i32, m: u32, d: u32| TaskDue {
            text: text.into(),
            due: NaiveDate::from_ymd_opt(y, m, d).unwrap(),
            source: "tasks/a.md".into(),
        };
        let tasks = vec![
            task("before", 2026, 9, 6),
            task("same day", 2026, 9, 8),
            task("later", 2026, 9, 12),
            task("after", 2026, 9, 14),
        ];
        let items = merged_between(&events, &tasks, start, end);
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0], DayItem::Event(e) if e.summary == "Standup"));
        assert!(matches!(&items[1], DayItem::Task(t) if t.text == "same day"));
        assert_eq!(
            items[1].date(),
            NaiveDate::from_ymd_opt(2026, 9, 8).unwrap()
        );
        assert!(matches!(&items[2], DayItem::Task(t) if t.text == "later"));
    }

    #[test]
    fn month_range_september() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (first, last) = month_range(date);
        assert_eq!(first, NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert_eq!(last, NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
    }
}

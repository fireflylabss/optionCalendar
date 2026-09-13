//! Day / week / month queries over events, plus the merged "today" view.

use chrono::{Datelike, Duration, Local, Months, NaiveDate, NaiveDateTime};

use crate::ics::{Event, is_date_only, parse_dt};
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

/// Simple recurrence rule: `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY` with optional
/// `INTERVAL`, `COUNT` and `UNTIL`. Other parts (BYDAY…) are ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    freq: Freq,
    interval: u32,
    count: Option<u32>,
    /// Inclusive end of the recurrence; `until_date_only` compares by date.
    until: Option<NaiveDateTime>,
    until_date_only: bool,
}

/// Hard cap on occurrences examined per query, so a bad rule can't spin.
const MAX_OCCURRENCES: u32 = 10_000;

/// Index of the first occurrence that might still touch `from` (a lower bound,
/// so the scan can skip the bulk of an old series).
fn first_candidate(rule: &Rule, event: &Event, from: NaiveDate) -> u32 {
    let span = event.end_or_start().date() - event.start.date();
    let gap = from - event.start.date() - span;
    if gap <= Duration::zero() {
        return 0;
    }
    let days = gap.num_days();
    let steps = match rule.freq {
        Freq::Daily => days,
        Freq::Weekly => days / 7,
        // Month/year lengths vary; use the longest so we never overshoot.
        Freq::Monthly => days / 31,
        Freq::Yearly => days / 366,
    };
    u32::try_from(steps / i64::from(rule.interval)).unwrap_or(u32::MAX)
}

fn parse_rule(raw: &str) -> Option<Rule> {
    let mut rule = Rule {
        freq: Freq::Daily,
        interval: 1,
        count: None,
        until: None,
        until_date_only: false,
    };
    let mut has_freq = false;
    for part in raw.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                rule.freq = match value.to_ascii_uppercase().as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                };
                has_freq = true;
            }
            "INTERVAL" => rule.interval = value.parse().ok().filter(|i| *i > 0)?,
            "COUNT" => rule.count = Some(value.parse().ok().filter(|c| *c > 0)?),
            "UNTIL" => {
                rule.until = Some(parse_dt(value).ok()?);
                rule.until_date_only = is_date_only(value);
            }
            _ => {}
        }
    }
    has_freq.then_some(rule)
}

/// Start of the `n`-th occurrence (0 = the event itself), or `None` on overflow.
fn nth_start(rule: &Rule, base: NaiveDateTime, n: u32) -> Option<NaiveDateTime> {
    let steps = n.checked_mul(rule.interval)?;
    match rule.freq {
        Freq::Daily => base.checked_add_signed(Duration::days(i64::from(steps))),
        Freq::Weekly => base.checked_add_signed(Duration::weeks(i64::from(steps))),
        Freq::Monthly => base.checked_add_months(Months::new(steps)),
        Freq::Yearly => base.checked_add_months(Months::new(steps.checked_mul(12)?)),
    }
}

fn within_until(rule: &Rule, start: NaiveDateTime) -> bool {
    match rule.until {
        None => true,
        Some(until) if rule.until_date_only => start.date() <= until.date(),
        Some(until) => start <= until,
    }
}

/// Occurrences of `event` whose days touch `[from, to]`, in order. A
/// non-recurring event (or one with an unsupported rule) yields itself only.
pub fn occurrences_between(event: &Event, from: NaiveDate, to: NaiveDate) -> Vec<Event> {
    let touches = |e: &Event| e.start.date() <= to && e.end_or_start().date() >= from;
    let Some(rule) = event.rrule.as_deref().and_then(parse_rule) else {
        return if touches(event) {
            vec![event.clone()]
        } else {
            Vec::new()
        };
    };
    let mut hits = Vec::new();
    let first = first_candidate(&rule, event, from);
    let limit = rule
        .count
        .unwrap_or(u32::MAX)
        .min(first.saturating_add(MAX_OCCURRENCES));
    for n in first..limit {
        let Some(start) = nth_start(&rule, event.start, n) else {
            break;
        };
        if !within_until(&rule, start) || start.date() > to {
            break;
        }
        let mut occurrence = event.clone();
        if n > 0 {
            occurrence.end = event.end.map(|end| end + (start - event.start));
            occurrence.start = start;
            occurrence.start_raw = None;
            occurrence.end_raw = None;
        }
        if touches(&occurrence) {
            hits.push(occurrence);
        }
    }
    hits
}

/// True when the event (or one of its occurrences) touches `date`.
pub fn occurs_on(event: &Event, date: NaiveDate) -> bool {
    !occurrences_between(event, date, date).is_empty()
}

/// Event occurrences touching `date`, sorted by start time.
pub fn events_on(events: &[Event], date: NaiveDate) -> Vec<Event> {
    events_between(events, date, date)
}

/// Event occurrences touching any day in `[start, end]`, sorted by start time.
/// Recurring events are expanded; each occurrence is a clone with shifted
/// `start`/`end` and the original `uid`.
pub fn events_between(events: &[Event], start: NaiveDate, end: NaiveDate) -> Vec<Event> {
    let mut hits: Vec<Event> = events
        .iter()
        .flat_map(|e| occurrences_between(e, start, end))
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
        .map(DayItem::Event)
        .collect();
    items.extend(
        tasks
            .iter()
            .filter(|task| task.due <= date)
            .cloned()
            .map(DayItem::Task),
    );
    items.sort_by_key(DayItem::sort_key);
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

    fn day(raw: &str) -> NaiveDate {
        NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn all_day_event_with_exclusive_dtend_spans_one_day() {
        let e = event("Holiday", "2026-09-10T00:00", None);
        let mut e = e;
        e.all_day = true;
        e.end = Some(dt("2026-09-11T00:00"));
        assert!(occurs_on(&e, day("2026-09-10")));
        assert!(!occurs_on(&e, day("2026-09-11")));
        assert_eq!(
            events_between(&[e], day("2026-09-11"), day("2026-09-30")).len(),
            0
        );
    }

    #[test]
    fn rrule_weekly_count_expands_within_window() {
        let mut e = event("Standup", "2026-09-01T10:00", Some("2026-09-01T10:30"));
        e.rrule = Some("FREQ=WEEKLY;COUNT=3".into());
        let hits = events_between(&[e.clone()], day("2026-09-01"), day("2026-09-30"));
        let starts: Vec<String> = hits.iter().map(|h| h.start.to_string()).collect();
        assert_eq!(
            starts,
            vec![
                "2026-09-01 10:00:00",
                "2026-09-08 10:00:00",
                "2026-09-15 10:00:00"
            ]
        );
        assert_eq!(hits[1].end, Some(dt("2026-09-08T10:30")));
        assert_eq!(hits[1].uid, e.uid);
        assert!(occurs_on(&e, day("2026-09-15")));
        assert!(!occurs_on(&e, day("2026-09-22")));
        assert_eq!(events_on(&[e], day("2026-09-08")).len(), 1);
    }

    #[test]
    fn rrule_daily_interval_until_stops_inclusive() {
        let mut e = event("Meds", "2026-09-01T08:00", None);
        e.rrule = Some("FREQ=DAILY;INTERVAL=2;UNTIL=20260905".into());
        let hits = events_between(&[e], day("2026-08-01"), day("2026-12-31"));
        let days: Vec<NaiveDate> = hits.iter().map(|h| h.start.date()).collect();
        assert_eq!(
            days,
            vec![day("2026-09-01"), day("2026-09-03"), day("2026-09-05")]
        );

        let mut timed = event("Meds", "2026-09-01T08:00", None);
        timed.rrule = Some("FREQ=DAILY;UNTIL=20260903T070000Z".into());
        let hits = events_between(&[timed], day("2026-08-01"), day("2026-12-31"));
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn rrule_monthly_and_yearly() {
        let mut monthly = event("Rent", "2026-01-31T09:00", None);
        monthly.rrule = Some("FREQ=MONTHLY;COUNT=3".into());
        let days: Vec<NaiveDate> = events_between(&[monthly], day("2026-01-01"), day("2026-12-31"))
            .iter()
            .map(|h| h.start.date())
            .collect();
        assert_eq!(
            days,
            vec![day("2026-01-31"), day("2026-02-28"), day("2026-03-31")]
        );

        let mut yearly = event("Birthday", "2026-09-10T00:00", None);
        yearly.rrule = Some("FREQ=YEARLY".into());
        assert!(occurs_on(&yearly, day("2030-09-10")));
        assert!(!occurs_on(&yearly, day("2025-09-10")));
    }

    #[test]
    fn unsupported_rrule_yields_first_occurrence_only() {
        let mut e = event("Odd", "2026-09-01T10:00", None);
        e.rrule = Some("FREQ=HOURLY;COUNT=5".into());
        assert_eq!(
            events_between(&[e.clone()], day("2026-09-01"), day("2026-09-30")).len(),
            1
        );
        e.rrule = Some("garbage".into());
        assert!(occurs_on(&e, day("2026-09-01")));
        assert!(!occurs_on(&e, day("2026-09-02")));
        e.rrule = Some("FREQ=DAILY;COUNT=0".into());
        assert!(occurs_on(&e, day("2026-09-01")));
        assert!(!occurs_on(&e, day("2026-09-02")));
    }

    #[test]
    fn old_series_still_reaches_far_windows() {
        let mut daily = event("Old", "1990-01-01T07:00", Some("1990-01-03T07:00"));
        daily.rrule = Some("FREQ=DAILY".into());
        let hits = events_between(&[daily.clone()], day("2026-09-10"), day("2026-09-10"));
        // Three-day span: occurrences starting 09-08, 09-09 and 09-10 all touch the day.
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].start.date(), day("2026-09-08"));
        assert!(hits[0].start_raw.is_none());
        daily.rrule = Some("FREQ=DAILY;COUNT=100".into());
        assert!(!occurs_on(&daily, day("2026-09-10")));

        let mut monthly = event("Rent", "1990-01-31T09:00", None);
        monthly.rrule = Some("FREQ=MONTHLY;INTERVAL=2".into());
        assert!(occurs_on(&monthly, day("2026-09-30")));
        assert!(!occurs_on(&monthly, day("2026-08-31")));
    }

    #[test]
    fn month_range_september() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let (first, last) = month_range(date);
        assert_eq!(first, NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert_eq!(last, NaiveDate::from_ymd_opt(2026, 9, 30).unwrap());
    }
}

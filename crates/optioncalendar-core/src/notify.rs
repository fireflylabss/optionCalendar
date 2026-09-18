//! One-shot reminder support: which event occurrences are due, and what was
//! already sent (`~/.option/cal/notified`).
//!
//! No daemon — a systemd user timer or cron entry is expected to run
//! `oca notify --send` periodically. Dedup keys live in a plain text file so
//! frequent runs don't re-fire the same occurrence.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use chrono::{Duration, NaiveDateTime};

use crate::Result;
use crate::ics::{Event, format_dt, parse_dt};
use crate::query::events_between;

/// How far back an occurrence may have started and still count as due —
/// covers events that began while the machine was suspended or a scheduled
/// run was missed.
pub const DEFAULT_LATE: Duration = Duration::minutes(10);

/// Occurrences starting within `(now - late, now + window]`, sorted by start.
/// All-day events never produce reminders (a midnight notification is noise).
pub fn due_events(
    events: &[Event],
    now: NaiveDateTime,
    window: Duration,
    late: Duration,
) -> Vec<Event> {
    events_between(events, (now - late).date(), (now + window).date())
        .into_iter()
        .filter(|event| !event.all_day && event.start > now - late && event.start <= now + window)
        .collect()
}

/// Stable dedup key: event UID plus the occurrence's start. Recurring
/// occurrences share the UID, so the shifted start distinguishes them.
pub fn notification_key(event: &Event) -> String {
    format!("{}\t{}", event.uid, format_dt(&event.start))
}

/// Load already-sent keys from `path` (missing file → empty set).
/// Malformed lines are tolerated and kept verbatim — they simply never match.
pub fn load_notified(path: &Path) -> HashSet<String> {
    fs::read_to_string(path)
        .map(|raw| {
            raw.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Persist sent keys to `path`, one `uid<TAB>start` per line, sorted for
/// stable output. Keys whose start is older than 2 days — or that no longer
/// parse — are pruned so the file stays small.
pub fn save_notified(path: &Path, keys: &HashSet<String>, now: NaiveDateTime) -> Result<()> {
    let cutoff = now - Duration::days(2);
    let mut kept: Vec<&String> = keys
        .iter()
        .filter(|key| {
            key.rsplit('\t')
                .next()
                .and_then(|raw| parse_dt(raw).ok())
                .is_some_and(|start| start >= cutoff)
        })
        .collect();
    kept.sort();
    let mut out = String::new();
    for key in kept {
        out.push_str(key);
        out.push('\n');
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    option_sdk::atomic_write(path, out.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(raw: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").unwrap()
    }

    fn day(raw: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn due_events_respects_window_and_late() {
        let now = dt("2026-09-16T10:00");
        let window = Duration::minutes(15);
        let events = vec![
            Event::new("Started", dt("2026-09-16T09:55"), None), // within late
            Event::new("Soon", dt("2026-09-16T10:10"), None),
            Event::new("Edge", dt("2026-09-16T10:15"), None), // boundary is due
            Event::new("Late", dt("2026-09-16T10:16"), None),
            Event::new("Old", dt("2026-09-16T09:00"), None), // beyond late
        ];
        let due = due_events(&events, now, window, DEFAULT_LATE);
        let names: Vec<&str> = due.iter().map(|e| e.summary.as_str()).collect();
        assert_eq!(names, ["Started", "Soon", "Edge"]);
    }

    #[test]
    fn due_events_skips_all_day() {
        let now = dt("2026-09-16T10:00");
        let mut holiday = Event::new("Holiday", dt("2026-09-16T10:05"), None);
        holiday.all_day = true;
        assert!(due_events(&[holiday], now, Duration::minutes(15), DEFAULT_LATE).is_empty());
    }

    #[test]
    fn recurring_occurrence_is_due_with_shifted_start() {
        let mut meds = Event::new("Meds", dt("2026-09-14T08:00"), None);
        meds.rrule = Some("FREQ=DAILY".into());
        let uid = meds.uid.clone();
        let due = due_events(
            &[meds],
            dt("2026-09-16T07:55"),
            Duration::minutes(10),
            DEFAULT_LATE,
        );
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].start, dt("2026-09-16T08:00"));
        assert_eq!(due[0].uid, uid);
    }

    #[test]
    fn notification_key_distinguishes_occurrences() {
        let mut meds = Event::new("Meds", dt("2026-09-14T08:00"), None);
        meds.rrule = Some("FREQ=DAILY".into());
        let occurrences = events_between(&[meds], day("2026-09-14"), day("2026-09-16"));
        let keys: HashSet<String> = occurrences.iter().map(notification_key).collect();
        assert_eq!(occurrences.len(), 3);
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&format!("{}\t20260915T080000", occurrences[0].uid)));
    }

    #[test]
    fn notified_state_roundtrips_prunes_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal").join("notified");
        assert!(load_notified(&path).is_empty());

        let keys = HashSet::from([
            "a\t20260916T100000".to_string(),
            "old\t20260910T100000".to_string(),
            "garbage".to_string(),
        ]);
        save_notified(&path, &keys, dt("2026-09-16T12:00")).unwrap();

        let loaded = load_notified(&path);
        assert!(loaded.contains("a\t20260916T100000"));
        assert!(!loaded.iter().any(|k| k.starts_with("old\t")));
        assert!(!loaded.contains("garbage"));

        let raw = fs::read_to_string(&path).unwrap();
        let mut lines: Vec<&str> = raw.lines().collect();
        lines.sort_unstable();
        assert_eq!(raw.lines().collect::<Vec<_>>(), lines); // written sorted
    }
}

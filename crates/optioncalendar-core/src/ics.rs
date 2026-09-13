//! Minimal ICS (RFC 5545 subset): VEVENT with UID/DTSTART/DTEND/SUMMARY/DESCRIPTION/RRULE.
//!
//! Only the properties optionCalendar interprets are parsed into typed fields;
//! every other line inside a VEVENT (including nested components such as
//! VALARM) is kept verbatim in [`Event::extra`] and written back by [`to_ics`],
//! so files from other calendars survive a load/save cycle.

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::Serialize;

use crate::{Error, Result};

/// One calendar event.
///
/// Serializes with ISO 8601 `YYYY-MM-DDTHH:MM:SS` datetimes (`end` is `null` when unset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub description: String,
    #[serde(serialize_with = "serialize_iso")]
    pub start: NaiveDateTime,
    /// End of the event. For `all_day` events this follows ICS semantics and is
    /// **exclusive** (the day after the last day); see [`Event::end_or_start`].
    #[serde(serialize_with = "serialize_iso_opt")]
    pub end: Option<NaiveDateTime>,
    /// Date-only event (`DTSTART;VALUE=DATE`).
    pub all_day: bool,
    /// Raw `RRULE` value (e.g. `FREQ=WEEKLY;COUNT=4`), expanded by `query`.
    pub rrule: Option<String>,
    /// Uninterpreted VEVENT lines as `(name-with-params, value)`, in file order.
    /// Nested components are kept line by line (`("BEGIN", "VALARM")` … `("END", "VALARM")`).
    #[serde(skip)]
    pub extra: Vec<(String, String)>,
    /// Original `DTSTART` line as `(name-with-params, raw value)`, re-emitted
    /// verbatim while it still matches `start`.
    #[serde(skip)]
    pub start_raw: Option<(String, String)>,
    /// Original `DTEND` line, same rules as `start_raw`.
    #[serde(skip)]
    pub end_raw: Option<(String, String)>,
}

fn serialize_iso<S: serde::Serializer>(
    value: &NaiveDateTime,
    s: S,
) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&value.format("%Y-%m-%dT%H:%M:%S").to_string())
}

fn serialize_iso_opt<S: serde::Serializer>(
    value: &Option<NaiveDateTime>,
    s: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(value) => serialize_iso(value, s),
        None => s.serialize_none(),
    }
}

impl Event {
    /// Build a new timed event (see [`Event::new_all_day`] for date-only ones).
    pub fn new(
        summary: impl Into<String>,
        start: NaiveDateTime,
        end: Option<NaiveDateTime>,
    ) -> Self {
        let summary = summary.into();
        let uid = format!(
            "{:x}@optioncalendar",
            generate_uid(&summary, &start, end.as_ref())
        );
        Self {
            uid,
            summary,
            description: String::new(),
            start,
            end,
            all_day: false,
            rrule: None,
            extra: Vec::new(),
            start_raw: None,
            end_raw: None,
        }
    }

    /// All-day event from `start` through `last_day` (inclusive); the stored
    /// `end` is the exclusive ICS `DTEND`.
    pub fn new_all_day(
        summary: impl Into<String>,
        start: NaiveDate,
        last_day: Option<NaiveDate>,
    ) -> Self {
        let end = last_day.map(|d| (d + Duration::days(1)).and_time(NaiveTime::MIN));
        let mut event = Self::new(summary, start.and_time(NaiveTime::MIN), end);
        event.all_day = true;
        event
    }

    /// Inclusive end: `end` if set (minus one day for all-day events, whose
    /// ICS `DTEND` is exclusive), otherwise `start`.
    pub fn end_or_start(&self) -> NaiveDateTime {
        match self.end {
            Some(end) if self.all_day => {
                let last = end - Duration::days(1);
                if last < self.start { self.start } else { last }
            }
            Some(end) => end,
            None => self.start,
        }
    }
}

/// FNV-1a hash of summary/start/end mixed with wall-clock nanos for generated
/// UIDs (no extra dependency).
fn generate_uid(summary: &str, start: &NaiveDateTime, end: Option<&NaiveDateTime>) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    let text = format!(
        "{summary}|{start}|{}",
        end.map(|e| e.to_string()).unwrap_or_default()
    );
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Mix in wall-clock nanos so two identical adds don't collide.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    hash ^ nanos.rotate_left(17)
}

/// Parse a datetime in any accepted shape:
/// `YYYYMMDDTHHMMSS`, `YYYYMMDDTHHMM`, `YYYY-MM-DDTHH:MM`,
/// `YYYY-MM-DD HH:MM`, `YYYYMMDD`, `YYYY-MM-DD` (midnight).
pub fn parse_dt(raw: &str) -> Result<NaiveDateTime> {
    let value = raw.trim();
    // Strip a trailing `Z` (UTC designator); optionCalendar keeps wall-clock time.
    let value = value.strip_suffix('Z').unwrap_or(value);
    for format in [
        "%Y%m%dT%H%M%S",
        "%Y%m%dT%H%M",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%Y%m%d",
        "%Y-%m-%d",
    ] {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(value, format)
            && (format == "%Y%m%d" || format == "%Y-%m-%d")
        {
            return Ok(date.and_hms_opt(0, 0, 0).expect("midnight is valid"));
        }
        if let Ok(date_time) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(date_time);
        }
    }
    Err(Error::InvalidDate(raw.to_owned()))
}

/// True when `raw` is a date-only value (`YYYYMMDD` / `YYYY-MM-DD`).
pub fn is_date_only(raw: &str) -> bool {
    let value = raw.trim();
    let value = value.strip_suffix('Z').unwrap_or(value);
    !value.contains('T') && !value.contains(' ') && !value.contains(':')
}

/// Serialize back to compact ICS local time: `YYYYMMDDTHHMMSS`.
pub fn format_dt(value: &NaiveDateTime) -> String {
    value.format("%Y%m%dT%H%M%S").to_string()
}

/// Serialize as an ICS date: `YYYYMMDD`.
pub fn format_date(value: &NaiveDateTime) -> String {
    value.format("%Y%m%d").to_string()
}

fn unescape(value: &str) -> String {
    value
        .replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

/// Parse all VEVENT blocks in an ICS document. Malformed events are skipped.
pub fn parse_ics(text: &str) -> Vec<Event> {
    // Unfold continuation lines (a space or tab prefix appends to previous).
    let mut logical: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(first) = line.chars().next()
            && (first == ' ' || first == '\t')
            && let Some(previous) = logical.last_mut()
        {
            previous.push_str(line[1..].as_ref());
            continue;
        }
        logical.push(line.to_owned());
    }

    let mut events = Vec::new();
    let mut current: Option<Vec<(String, String)>> = None;
    // Depth of nested components (VALARM…) inside the current VEVENT.
    let mut depth = 0usize;
    for line in logical {
        match line.as_str() {
            "BEGIN:VEVENT" if current.is_none() => current = Some(Vec::new()),
            "END:VEVENT" if depth == 0 => {
                if let Some(props) = current.take()
                    && let Some(event) = build_event(&props)
                {
                    events.push(event);
                }
            }
            _ => {
                if let Some(props) = current.as_mut()
                    && let Some((left, value)) = split_prop(&line)
                {
                    match left.to_ascii_uppercase().as_str() {
                        "BEGIN" => depth += 1,
                        "END" => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                    props.push((left, value));
                }
            }
        }
    }
    events
}

/// Split `KEY;PARAM=X:value` into (`KEY;PARAM=X`, `value`).
fn split_prop(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let (left, value) = line.split_at(colon);
    Some((left.trim().to_owned(), value[1..].to_owned()))
}

/// Property name (before any `;PARAM`), upper-cased.
fn prop_name(left: &str) -> String {
    left.split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase()
}

fn has_param(left: &str, param: &str) -> bool {
    left.split(';')
        .skip(1)
        .any(|p| p.trim().eq_ignore_ascii_case(param))
}

const KNOWN: [&str; 6] = ["UID", "DTSTART", "DTEND", "SUMMARY", "DESCRIPTION", "RRULE"];

/// First top-level property named `name` (lines inside nested components are skipped).
fn find_prop<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a (String, String)> {
    let mut depth = 0usize;
    for line in props {
        let current = prop_name(&line.0);
        match current.as_str() {
            "BEGIN" => depth += 1,
            "END" => depth = depth.saturating_sub(1),
            _ if depth == 0 && current == name => return Some(line),
            _ => {}
        }
    }
    None
}

fn prop<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a str> {
    find_prop(props, name).map(|(_, value)| value.as_str())
}

fn build_event(props: &[(String, String)]) -> Option<Event> {
    let start_line = find_prop(props, "DTSTART")?;
    let start = parse_dt(&start_line.1).ok()?;
    let all_day = has_param(&start_line.0, "VALUE=DATE") || is_date_only(&start_line.1);
    let end_line = find_prop(props, "DTEND");
    let end = end_line.and_then(|(_, raw)| parse_dt(raw).ok());
    let end_raw = end_line.filter(|_| end.is_some()).cloned();
    // A nested component's lines are always preserved, even when they reuse a
    // known property name (e.g. DESCRIPTION inside VALARM).
    let mut depth = 0usize;
    let mut extra = Vec::new();
    for (left, value) in props {
        let name = prop_name(left);
        let unparsed_end = name == "DTEND" && end.is_none();
        if depth > 0 || unparsed_end || !KNOWN.contains(&name.as_str()) {
            extra.push((left.clone(), value.clone()));
        }
        match name.as_str() {
            "BEGIN" => depth += 1,
            "END" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Some(Event {
        uid: prop(props, "UID").unwrap_or("").to_owned(),
        summary: prop(props, "SUMMARY").map(unescape).unwrap_or_default(),
        description: prop(props, "DESCRIPTION").map(unescape).unwrap_or_default(),
        start,
        end,
        all_day,
        rrule: prop(props, "RRULE").map(str::to_owned),
        extra,
        start_raw: Some(start_line.clone()),
        end_raw,
    })
}

/// Emit a DTSTART/DTEND line, reusing the original params+value when unchanged.
fn push_dt(
    out: &mut String,
    name: &str,
    value: &NaiveDateTime,
    raw: Option<&(String, String)>,
    all_day: bool,
) {
    if let Some((left, raw_value)) = raw
        && parse_dt(raw_value).ok().as_ref() == Some(value)
        && (has_param(left, "VALUE=DATE") || is_date_only(raw_value)) == all_day
    {
        out.push_str(&format!("{left}:{raw_value}\r\n"));
    } else if all_day {
        out.push_str(&format!("{name};VALUE=DATE:{}\r\n", format_date(value)));
    } else {
        out.push_str(&format!("{name}:{}\r\n", format_dt(value)));
    }
}

/// Serialize events as a VCALENDAR document.
pub fn to_ics(events: &[Event]) -> String {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//optionCalendar//optionCalendar//EN\r\n",
    );
    for event in events {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{}\r\n", event.uid));
        push_dt(
            &mut out,
            "DTSTART",
            &event.start,
            event.start_raw.as_ref(),
            event.all_day,
        );
        if let Some(end) = event.end {
            push_dt(
                &mut out,
                "DTEND",
                &end,
                event.end_raw.as_ref(),
                event.all_day,
            );
        }
        out.push_str(&format!("SUMMARY:{}\r\n", escape(&event.summary)));
        if !event.description.is_empty() {
            out.push_str(&format!("DESCRIPTION:{}\r\n", escape(&event.description)));
        }
        if let Some(rrule) = &event.rrule {
            out.push_str(&format!("RRULE:{rrule}\r\n"));
        }
        for (left, value) in &event.extra {
            out.push_str(&format!("{left}:{value}\r\n"));
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:abc-1\r\nDTSTART:20260904T100000\r\nDTEND:20260904T110000\r\nSUMMARY:Standup\r\nDESCRIPTION:team\\, daily\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:abc-2\r\nDTSTART:20260905\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    const RICH: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:rich-1\r\nDTSTART;TZID=America/Sao_Paulo:20260904T100000\r\nDTEND;TZID=America/Sao_Paulo:20260904T110000\r\nSUMMARY:Standup\r\nLOCATION:Room 1\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nX-FOO;X-BAR=1:baz\r\nBEGIN:VALARM\r\nTRIGGER:-PT10M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn dt(raw: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn parses_uid_start_end_summary_description() {
        let events = parse_ics(SAMPLE);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].uid, "abc-1");
        assert_eq!(events[0].summary, "Standup");
        assert_eq!(events[0].description, "team, daily");
        assert_eq!(events[0].start, dt("2026-09-04T10:00:00"));
        assert_eq!(events[0].end, Some(dt("2026-09-04T11:00:00")));
        assert!(!events[0].all_day);
        // Date-only DTSTART becomes midnight and all-day.
        assert_eq!(events[1].start, dt("2026-09-05T00:00:00"));
        assert_eq!(events[1].end, None);
        assert!(events[1].all_day);
    }

    #[test]
    fn roundtrip_serialize_parse() {
        let events = parse_ics(SAMPLE);
        let reparsed = parse_ics(&to_ics(&events));
        assert_eq!(events, reparsed);
    }

    #[test]
    fn parse_dt_accepts_cli_shapes() {
        let midnight = parse_dt("2026-09-04").unwrap();
        assert_eq!(midnight.format("%H:%M").to_string(), "00:00");
        assert_eq!(
            parse_dt("2026-09-04T10:30").unwrap(),
            parse_dt("20260904T103000").unwrap()
        );
    }

    #[test]
    fn skips_events_without_dtstart() {
        let text = "BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:no date\r\nEND:VEVENT\r\n";
        assert!(parse_ics(text).is_empty());
    }

    #[test]
    fn preserves_unknown_props_nested_components_and_params() {
        let events = parse_ics(RICH);
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.rrule.as_deref(), Some("FREQ=WEEKLY;COUNT=4"));
        assert_eq!(event.description, "");
        assert_eq!(
            event.extra,
            vec![
                ("LOCATION".to_string(), "Room 1".to_string()),
                ("X-FOO;X-BAR=1".to_string(), "baz".to_string()),
                ("BEGIN".to_string(), "VALARM".to_string()),
                ("TRIGGER".to_string(), "-PT10M".to_string()),
                ("ACTION".to_string(), "DISPLAY".to_string()),
                ("DESCRIPTION".to_string(), "Reminder".to_string()),
                ("END".to_string(), "VALARM".to_string()),
            ]
        );

        let text = to_ics(&events);
        for line in [
            "DTSTART;TZID=America/Sao_Paulo:20260904T100000\r\n",
            "DTEND;TZID=America/Sao_Paulo:20260904T110000\r\n",
            "LOCATION:Room 1\r\n",
            "RRULE:FREQ=WEEKLY;COUNT=4\r\n",
            "X-FOO;X-BAR=1:baz\r\n",
            "BEGIN:VALARM\r\nTRIGGER:-PT10M\r\nACTION:DISPLAY\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\n",
        ] {
            assert!(text.contains(line), "missing {line:?} in {text}");
        }
        assert_eq!(parse_ics(&text), events);
    }

    #[test]
    fn modified_start_drops_stale_raw_params() {
        let mut events = parse_ics(RICH);
        events[0].start = dt("2026-09-05T10:00:00");
        let text = to_ics(&events);
        assert!(text.contains("DTSTART:20260905T100000\r\n"));
        assert!(text.contains("DTEND;TZID=America/Sao_Paulo:20260904T110000\r\n"));
    }

    #[test]
    fn all_day_dtend_is_exclusive_and_roundtrips() {
        let text = "BEGIN:VEVENT\r\nUID:d\r\nDTSTART;VALUE=DATE:20260910\r\nDTEND;VALUE=DATE:20260911\r\nSUMMARY:Day off\r\nEND:VEVENT\r\n";
        let events = parse_ics(text);
        let event = &events[0];
        assert!(event.all_day);
        assert_eq!(event.end_or_start().date(), event.start.date());
        let out = to_ics(&events);
        assert!(out.contains("DTSTART;VALUE=DATE:20260910\r\n"));
        assert!(out.contains("DTEND;VALUE=DATE:20260911\r\n"));
        assert_eq!(parse_ics(&out), events);
    }

    #[test]
    fn new_all_day_event_serializes_as_dates() {
        let event = Event::new_all_day(
            "Trip",
            dt("2026-09-10T00:00:00").date(),
            Some(dt("2026-09-12T00:00:00").date()),
        );
        assert!(event.all_day);
        // Inclusive CLI end becomes exclusive ICS DTEND.
        assert_eq!(event.end, Some(dt("2026-09-13T00:00:00")));
        assert_eq!(event.end_or_start(), dt("2026-09-12T00:00:00"));
        let out = to_ics(std::slice::from_ref(&event));
        assert!(out.contains("DTSTART;VALUE=DATE:20260910\r\n"));
        assert!(out.contains("DTEND;VALUE=DATE:20260913\r\n"));
        let reparsed = parse_ics(&out);
        assert!(reparsed[0].all_day);
        assert_eq!(reparsed[0].end, event.end);

        let timed = Event::new("Call", dt("2026-09-10T09:00:00"), None);
        assert!(!timed.all_day);
        assert!(to_ics(&[timed]).contains("DTSTART:20260910T090000\r\n"));

        let midnight = Event::new(
            "Maintenance",
            dt("2026-09-10T00:00:00"),
            Some(dt("2026-09-11T00:00:00")),
        );
        assert!(!midnight.all_day);
        assert_eq!(midnight.end, Some(dt("2026-09-11T00:00:00")));
        assert!(to_ics(&[midnight]).contains("DTEND:20260911T000000\r\n"));
    }

    #[test]
    fn unparseable_dtend_is_kept_verbatim() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nDTSTART:20260910T100000\r\nDTEND;TZID=Mars/Olympus:garbage\r\nSUMMARY:S\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let events = parse_ics(ics);
        assert_eq!(events[0].end, None);
        assert!(to_ics(&events).contains("DTEND;TZID=Mars/Olympus:garbage\r\n"));
    }
}

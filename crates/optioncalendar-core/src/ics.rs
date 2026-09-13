//! Minimal ICS (RFC 5545 subset): VEVENT with UID/DTSTART/DTEND/SUMMARY/DESCRIPTION.
//!
//! Only the properties optionCalendar needs are parsed; unknown lines inside a
//! VEVENT are ignored so files from other calendars keep loading.

use chrono::NaiveDateTime;
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
    #[serde(serialize_with = "serialize_iso_opt")]
    pub end: Option<NaiveDateTime>,
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
    pub fn new(
        summary: impl Into<String>,
        start: NaiveDateTime,
        end: Option<NaiveDateTime>,
    ) -> Self {
        let summary = summary.into();
        let uid = format!(
            "{:x}@optioncalendar",
            blake_like(&summary, &start, end.as_ref())
        );
        Self {
            uid,
            summary,
            description: String::new(),
            start,
            end,
        }
    }

    /// End if set, otherwise start (all-day / instant events occupy one day).
    pub fn end_or_start(&self) -> NaiveDateTime {
        self.end.unwrap_or(self.start)
    }
}

/// Tiny deterministic hash for generated UIDs (no extra dependency).
fn blake_like(summary: &str, start: &NaiveDateTime, end: Option<&NaiveDateTime>) -> u64 {
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

/// Serialize back to compact ICS local time: `YYYYMMDDTHHMMSS`.
pub fn format_dt(value: &NaiveDateTime) -> String {
    value.format("%Y%m%dT%H%M%S").to_string()
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
    for line in logical {
        match line.as_str() {
            "BEGIN:VEVENT" => current = Some(Vec::new()),
            "END:VEVENT" => {
                if let Some(props) = current.take()
                    && let Some(event) = build_event(&props)
                {
                    events.push(event);
                }
            }
            _ => {
                if let Some(props) = current.as_mut()
                    && let Some((key, value)) = split_prop(&line)
                {
                    props.push((key, value));
                }
            }
        }
    }
    events
}

/// Split `KEY;PARAM=X:value` into (`KEY`, `value`).
fn split_prop(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let (left, value) = line.split_at(colon);
    let key = left.split(';').next()?.trim().to_ascii_uppercase();
    Some((key, value[1..].to_owned()))
}

fn prop<'a>(props: &'a [(String, String)], name: &str) -> Option<&'a str> {
    props
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn build_event(props: &[(String, String)]) -> Option<Event> {
    let start = parse_dt(prop(props, "DTSTART")?).ok()?;
    let end = prop(props, "DTEND").and_then(|raw| parse_dt(raw).ok());
    Some(Event {
        uid: prop(props, "UID").unwrap_or("").to_owned(),
        summary: prop(props, "SUMMARY").map(unescape).unwrap_or_default(),
        description: prop(props, "DESCRIPTION").map(unescape).unwrap_or_default(),
        start,
        end,
    })
}

/// Serialize events as a VCALENDAR document.
pub fn to_ics(events: &[Event]) -> String {
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//optionCalendar//optionCalendar//EN\r\n",
    );
    for event in events {
        out.push_str("BEGIN:VEVENT\r\n");
        out.push_str(&format!("UID:{}\r\n", event.uid));
        out.push_str(&format!("DTSTART:{}\r\n", format_dt(&event.start)));
        if let Some(end) = event.end {
            out.push_str(&format!("DTEND:{}\r\n", format_dt(&end)));
        }
        out.push_str(&format!("SUMMARY:{}\r\n", escape(&event.summary)));
        if !event.description.is_empty() {
            out.push_str(&format!("DESCRIPTION:{}\r\n", escape(&event.description)));
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

    #[test]
    fn parses_uid_start_end_summary_description() {
        let events = parse_ics(SAMPLE);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].uid, "abc-1");
        assert_eq!(events[0].summary, "Standup");
        assert_eq!(events[0].description, "team, daily");
        assert_eq!(
            events[0].start,
            NaiveDateTime::parse_from_str("2026-09-04T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap()
        );
        assert_eq!(
            events[0].end,
            NaiveDateTime::parse_from_str("2026-09-04T11:00:00", "%Y-%m-%dT%H:%M:%S").ok()
        );
        // Date-only DTSTART becomes midnight.
        assert_eq!(
            events[1].start,
            NaiveDateTime::parse_from_str("2026-09-05T00:00:00", "%Y-%m-%dT%H:%M:%S").unwrap()
        );
        assert_eq!(events[1].end, None);
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
}

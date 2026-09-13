//! File store: one ICS file on disk plus `~/.option/cal/config.toml` settings.

use std::fs;
use std::path::PathBuf;

use option_sdk::App;
use serde::{Deserialize, Serialize};

use crate::ics::{Event, parse_ics, to_ics};
use crate::week::WeekStart;
use crate::{Error, Result};

/// User settings for optionCalendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Absolute path to the ICS calendar file.
    pub ics_path: PathBuf,
    /// Launch the TUI when `oca` is invoked with no subcommand.
    /// Opt-in, default off; missing in old configs means false.
    #[serde(default)]
    pub launch_tui_on_no_args: bool,
    /// First day of the week in the grids (Monday by default, ISO 8601).
    #[serde(default)]
    pub week_start: WeekStart,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ics_path: default_ics_path(),
            launch_tui_on_no_args: false,
            week_start: WeekStart::default(),
        }
    }
}

/// Default calendar file: `~/.option/cal/calendar.ics`.
pub fn default_ics_path() -> PathBuf {
    App::CAL.dir().join("calendar.ics")
}

/// Load settings from [`App::CAL`] config, creating defaults when missing.
pub fn load_settings() -> Result<Settings> {
    let app = App::CAL;
    app.ensure().map_err(Error::Io)?;
    let path = app.config_toml();
    if !path.exists() {
        let settings = Settings::default();
        save_settings(&settings)?;
        return Ok(settings);
    }
    let raw = fs::read_to_string(&path)?;
    let settings: Settings =
        toml::from_str(&raw).map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    Ok(settings)
}

/// Persist settings to [`App::CAL`] config.toml.
pub fn save_settings(settings: &Settings) -> Result<()> {
    let app = App::CAL;
    app.ensure().map_err(Error::Io)?;
    let path = app.config_toml();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(settings).map_err(|e| Error::Config(e.to_string()))?;
    option_sdk::atomic_write(&path, raw.as_bytes()).map_err(Error::Io)?;
    Ok(())
}

/// Events backed by a single ICS file.
#[derive(Debug, Clone, Default)]
pub struct CalStore {
    pub path: PathBuf,
    pub events: Vec<Event>,
}

impl CalStore {
    /// Open the store, starting empty when the file does not exist yet.
    pub fn open(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path,
                events: Vec::new(),
            });
        }
        let raw = fs::read_to_string(&path)?;
        Ok(Self {
            path,
            events: parse_ics(&raw),
        })
    }

    /// Persist all events to disk (parents created as needed).
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        option_sdk::atomic_write(&self.path, to_ics(&self.events).as_bytes()).map_err(Error::Io)?;
        Ok(())
    }

    /// Insert an event and persist.
    pub fn add(&mut self, event: Event) -> Result<()> {
        self.events.push(event);
        self.sort();
        self.save()
    }

    /// Remove the event with `uid` and persist.
    /// Returns `true` when an event was actually removed.
    pub fn remove(&mut self, uid: &str) -> Result<bool> {
        let before = self.events.len();
        self.events.retain(|event| event.uid != uid);
        if self.events.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    /// Apply `f` to the event with `uid`, re-sort and persist.
    /// Returns `false` (without saving) when no event has that UID.
    /// The UID itself is left untouched even if `f` changes it.
    pub fn update(&mut self, uid: &str, f: impl FnOnce(&mut Event)) -> Result<bool> {
        let Some(event) = self.events.iter_mut().find(|event| event.uid == uid) else {
            return Ok(false);
        };
        f(event);
        event.uid = uid.to_owned();
        self.sort();
        self.save()?;
        Ok(true)
    }

    /// Merge ICS text, skipping events whose UID already exists.
    /// Returns the number of newly added events.
    pub fn import(&mut self, text: &str) -> Result<usize> {
        let mut added = 0usize;
        for event in parse_ics(text) {
            if self
                .events
                .iter()
                .any(|e| e.uid == event.uid && !e.uid.is_empty())
            {
                continue;
            }
            self.events.push(event);
            added += 1;
        }
        self.sort();
        self.save()?;
        Ok(added)
    }

    fn sort(&mut self) {
        self.events.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| a.summary.cmp(&b.summary))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_missing_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CalStore::open(dir.path().join("cal.ics")).unwrap();
        assert!(store.events.is_empty());
    }

    #[test]
    fn add_persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal.ics");
        let mut store = CalStore::open(path.clone()).unwrap();
        let event = Event::new("Dentist", parse_dt_for_test("2026-09-04T10:00"), None);
        store.add(event.clone()).unwrap();
        let reloaded = CalStore::open(path).unwrap();
        assert_eq!(reloaded.events.len(), 1);
        assert_eq!(reloaded.events[0].summary, "Dentist");
        assert_eq!(reloaded.events[0].start, event.start);
    }

    #[test]
    fn import_skips_duplicate_uids() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CalStore::open(dir.path().join("cal.ics")).unwrap();
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:dup\r\nDTSTART:20260904T100000\r\nSUMMARY:A\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(store.import(text).unwrap(), 1);
        assert_eq!(store.import(text).unwrap(), 0);
        assert_eq!(store.events.len(), 1);
    }

    fn parse_dt_for_test(raw: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").unwrap()
    }

    #[test]
    fn remove_deletes_by_uid_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal.ics");
        let mut store = CalStore::open(path.clone()).unwrap();
        let event = Event::new("Dentist", parse_dt_for_test("2026-09-04T10:00"), None);
        let uid = event.uid.clone();
        store.add(event).unwrap();
        assert!(!store.remove("missing-uid").unwrap());
        assert!(store.remove(&uid).unwrap());
        assert!(store.events.is_empty());
        assert!(CalStore::open(path).unwrap().events.is_empty());
    }

    #[test]
    fn update_edits_resorts_persists_and_keeps_uid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal.ics");
        let mut store = CalStore::open(path.clone()).unwrap();
        let first = Event::new("First", parse_dt_for_test("2026-09-04T10:00"), None);
        let second = Event::new("Second", parse_dt_for_test("2026-09-05T10:00"), None);
        let uid = first.uid.clone();
        store.add(first).unwrap();
        store.add(second).unwrap();

        assert!(!store.update("missing-uid", |_| {}).unwrap());
        let moved = parse_dt_for_test("2026-09-06T09:00");
        assert!(
            store
                .update(&uid, |event| {
                    event.summary = "Renamed".into();
                    event.start = moved;
                    event.uid = "tampered".into();
                })
                .unwrap()
        );

        let reloaded = CalStore::open(path).unwrap();
        assert_eq!(reloaded.events.len(), 2);
        assert_eq!(reloaded.events[0].summary, "Second");
        assert_eq!(reloaded.events[1].summary, "Renamed");
        assert_eq!(reloaded.events[1].start, moved);
        assert_eq!(reloaded.events[1].uid, uid);
    }

    #[test]
    fn event_serializes_iso_dates() {
        let event = Event {
            uid: "u1".into(),
            summary: "S".into(),
            description: String::new(),
            start: parse_dt_for_test("2026-09-04T10:00"),
            end: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"uid":"u1","summary":"S","description":"","start":"2026-09-04T10:00:00","end":null}"#
        );
    }

    /// Old configs without the opt-in flag load with it defaulting to false.
    #[test]
    fn launch_flag_defaults_false_for_old_configs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("option-home");
        // SAFETY: no other test in this crate touches OPTION_HOME.
        unsafe {
            std::env::set_var("OPTION_HOME", &root);
        }
        let settings = load_settings().unwrap();
        assert!(!settings.launch_tui_on_no_args);

        // Persisting `true` round-trips through config.toml.
        let mut updated = settings.clone();
        updated.launch_tui_on_no_args = true;
        save_settings(&updated).unwrap();
        assert!(load_settings().unwrap().launch_tui_on_no_args);

        // A hand-written old config (no flag field) still loads as false.
        std::fs::write(
            App::CAL.config_toml(),
            format!("ics_path = \"{}\"\n", settings.ics_path.display()),
        )
        .unwrap();
        assert!(!load_settings().unwrap().launch_tui_on_no_args);

        unsafe {
            std::env::remove_var("OPTION_HOME");
        }
    }
}

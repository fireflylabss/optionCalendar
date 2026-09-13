//! File store: one ICS file on disk plus `~/.option/cal/config.toml` settings.

use std::fs;
use std::path::{Path, PathBuf};

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
    load_settings_from(&app.config_toml())
}

/// Persist settings to [`App::CAL`] config.toml.
pub fn save_settings(settings: &Settings) -> Result<()> {
    let app = App::CAL;
    app.ensure().map_err(Error::Io)?;
    save_settings_to(&app.config_toml(), settings)
}

/// Load settings from `path`, writing defaults there when the file is missing.
pub fn load_settings_from(path: &Path) -> Result<Settings> {
    if !path.exists() {
        let settings = Settings::default();
        save_settings_to(path, &settings)?;
        return Ok(settings);
    }
    let raw = fs::read_to_string(path)?;
    let settings: Settings =
        toml::from_str(&raw).map_err(|e| Error::Config(format!("{}: {e}", path.display())))?;
    Ok(settings)
}

/// Persist settings to `path` (parents created as needed).
pub fn save_settings_to(path: &Path, settings: &Settings) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let raw = toml::to_string_pretty(settings).map_err(|e| Error::Config(e.to_string()))?;
    option_sdk::atomic_write(path, raw.as_bytes()).map_err(Error::Io)?;
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

    #[test]
    fn import_then_add_keeps_foreign_props() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal.ics");
        let mut store = CalStore::open(path.clone()).unwrap();
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:rich\r\nDTSTART;TZID=America/Sao_Paulo:20260904T100000\r\nDTEND;TZID=America/Sao_Paulo:20260904T110000\r\nSUMMARY:Standup\r\nLOCATION:Room 1\r\nRRULE:FREQ=WEEKLY;COUNT=4\r\nX-FOO:bar\r\nBEGIN:VALARM\r\nTRIGGER:-PT10M\r\nACTION:DISPLAY\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(store.import(text).unwrap(), 1);
        store
            .add(Event::new(
                "Dentist",
                parse_dt_for_test("2026-09-05T10:00"),
                None,
            ))
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        for line in [
            "DTSTART;TZID=America/Sao_Paulo:20260904T100000\r\n",
            "DTEND;TZID=America/Sao_Paulo:20260904T110000\r\n",
            "LOCATION:Room 1\r\n",
            "RRULE:FREQ=WEEKLY;COUNT=4\r\n",
            "X-FOO:bar\r\n",
            "BEGIN:VALARM\r\nTRIGGER:-PT10M\r\nACTION:DISPLAY\r\nEND:VALARM\r\n",
        ] {
            assert!(raw.contains(line), "missing {line:?} in {raw}");
        }
        let reloaded = CalStore::open(path).unwrap();
        assert_eq!(reloaded.events.len(), 2);
        let rich = reloaded.events.iter().find(|e| e.uid == "rich").unwrap();
        assert_eq!(rich, &store.events[0]);
        assert_eq!(rich.rrule.as_deref(), Some("FREQ=WEEKLY;COUNT=4"));
        assert_eq!(rich.extra.len(), 6);
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

    /// Old configs without the opt-in flag load with it defaulting to false.
    #[test]
    fn launch_flag_defaults_false_for_old_configs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cal").join("config.toml");
        let settings = load_settings_from(&path).unwrap();
        assert!(!settings.launch_tui_on_no_args);
        assert!(path.exists());

        // Persisting `true` round-trips through config.toml.
        let mut updated = settings.clone();
        updated.launch_tui_on_no_args = true;
        save_settings_to(&path, &updated).unwrap();
        assert!(load_settings_from(&path).unwrap().launch_tui_on_no_args);

        // A hand-written old config (no flag field) still loads as false.
        std::fs::write(
            &path,
            format!("ics_path = \"{}\"\n", settings.ics_path.display()),
        )
        .unwrap();
        assert!(!load_settings_from(&path).unwrap().launch_tui_on_no_args);
    }

    #[test]
    fn load_settings_from_rejects_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "ics_path = 42\n").unwrap();
        assert!(matches!(load_settings_from(&path), Err(Error::Config(_))));
    }
}

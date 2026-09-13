//! Integration tests for the `oca` binary. Each test gets its own `OPTION_HOME`
//! (which optionSDK prefers over `HOME`), so nothing touches the real config.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

struct Sandbox {
    home: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn path(&self) -> &Path {
        self.home.path()
    }

    fn oca(&self) -> Command {
        let mut cmd = Command::cargo_bin("oca").expect("oca binary");
        cmd.env("OPTION_HOME", self.path())
            .env("NO_COLOR", "1")
            .env_remove("HOME");
        cmd
    }

    fn add(&self, summary: &str, at: &str) {
        self.oca()
            .args(["add", summary, "--at", at])
            .assert()
            .success()
            .stdout(contains("added").and(contains(summary)));
    }

    fn ls_uid_lines(&self) -> Vec<String> {
        let out = self.oca().args(["ls", "--uid"]).output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .skip(1)
            .map(str::to_string)
            .collect()
    }

    /// Nth (1-based) UID from `oca ls --uid`.
    fn uid_at(&self, index: usize) -> String {
        let line = self.ls_uid_lines()[index - 1].clone();
        let open = line.rfind('[').unwrap();
        let close = line.rfind(']').unwrap();
        line[open + 1..close].to_string()
    }

    fn event_count(&self) -> usize {
        self.ls_uid_lines().len()
    }
}

/// Today's date plus `days`, formatted `YYYY-MM-DDTHH:MM`.
fn at(days: i64, hhmm: &str) -> String {
    let day = chrono::Local::now().date_naive() + chrono::Duration::days(days);
    format!("{}T{hhmm}", day.format("%Y-%m-%d"))
}

#[test]
fn bare_invocation_prints_help() {
    let sb = Sandbox::new();
    sb.oca()
        .assert()
        .success()
        .stdout(contains("Usage:").and(contains("add")).and(contains("tui")));
}

#[test]
fn add_then_ls_with_and_without_uid() {
    let sb = Sandbox::new();
    sb.add("Dentist", "2026-09-10T10:00");
    sb.oca().arg("ls").assert().success().stdout(
        contains("1 event")
            .and(contains("2026-09-10 10:00  Dentist"))
            .and(contains("@optioncalendar").not()),
    );
    sb.oca()
        .args(["ls", "--uid"])
        .assert()
        .success()
        .stdout(contains("Dentist  [").and(contains("@optioncalendar]")));
}

#[test]
fn ls_on_empty_store_says_no_events() {
    let sb = Sandbox::new();
    sb.oca()
        .arg("ls")
        .assert()
        .success()
        .stdout(contains("no events"));
}

#[test]
fn add_with_end_before_start_fails() {
    let sb = Sandbox::new();
    sb.oca()
        .args([
            "add",
            "Backwards",
            "--at",
            "2026-09-10T10:00",
            "--end",
            "2026-09-10T09:00",
        ])
        .assert()
        .failure()
        .stderr(contains("is before start"));
    assert_eq!(sb.event_count(), 0);
}

#[test]
fn add_with_invalid_date_fails() {
    let sb = Sandbox::new();
    sb.oca()
        .args(["add", "Bad", "--at", "not-a-date"])
        .assert()
        .failure()
        .stderr(contains("invalid date"));
}

#[test]
fn today_lists_todays_events() {
    let sb = Sandbox::new();
    sb.add("Standup", &at(0, "09:30"));
    sb.add("Far away", &at(40, "09:30"));
    sb.oca().arg("today").assert().success().stdout(
        contains("today ·")
            .and(contains("Standup"))
            .and(contains("Far away").not()),
    );
}

#[test]
fn today_empty_says_nothing_today() {
    let sb = Sandbox::new();
    sb.oca()
        .arg("today")
        .assert()
        .success()
        .stdout(contains("nothing today"));
}

#[test]
fn week_lists_next_seven_days_grouped() {
    let sb = Sandbox::new();
    sb.add("Soon", &at(2, "14:00"));
    sb.add("Far away", &at(40, "14:00"));
    sb.oca().arg("week").assert().success().stdout(
        contains("week ·")
            .and(contains("14:00  Soon"))
            .and(contains("Far away").not()),
    );
}

#[test]
fn month_counts_events_in_current_month() {
    let sb = Sandbox::new();
    sb.add("This month", &at(0, "08:00"));
    sb.add("Next year", &at(400, "08:00"));
    sb.oca().arg("month").assert().success().stdout(
        contains("· 1 event")
            .and(contains("08:00  This month"))
            .and(contains("Next year").not()),
    );
}

#[test]
fn next_shows_nearest_upcoming_event() {
    let sb = Sandbox::new();
    sb.add("Later", &at(5, "10:00"));
    sb.add("Sooner", &at(1, "10:00"));
    sb.add("Past", "2000-01-01T10:00");
    sb.oca().arg("next").assert().success().stdout(
        contains("next ·")
            .and(contains("Sooner"))
            .and(contains("tomorrow"))
            .and(contains("Later").not()),
    );
}

#[test]
fn next_without_upcoming_events() {
    let sb = Sandbox::new();
    sb.add("Past", "2000-01-01T10:00");
    sb.oca()
        .arg("next")
        .assert()
        .success()
        .stdout(contains("no upcoming events"));
}

#[test]
fn search_is_case_insensitive_over_summary_and_description() {
    let sb = Sandbox::new();
    sb.add("Dentist", "2026-09-10T10:00");
    sb.oca()
        .args([
            "add",
            "Meeting",
            "--at",
            "2026-09-11T10:00",
            "--description",
            "Quarterly Budget review",
        ])
        .assert()
        .success();
    sb.oca()
        .args(["search", "DENTIST"])
        .assert()
        .success()
        .stdout(contains("1 event").and(contains("Dentist")));
    sb.oca()
        .args(["search", "budget"])
        .assert()
        .success()
        .stdout(contains("Meeting"));
    sb.oca()
        .args(["search", "nothing-here"])
        .assert()
        .success()
        .stdout(contains("no matches"));
}

#[test]
fn rm_by_index_removes_that_event() {
    let sb = Sandbox::new();
    sb.add("First", "2026-09-10T10:00");
    sb.add("Second", "2026-09-11T10:00");
    sb.oca()
        .args(["rm", "1"])
        .assert()
        .success()
        .stdout(contains("removed ["));
    assert_eq!(sb.event_count(), 1);
    sb.oca()
        .arg("ls")
        .assert()
        .stdout(contains("Second").and(contains("First").not()));
}

#[test]
fn rm_by_uid_removes_that_event() {
    let sb = Sandbox::new();
    sb.add("First", "2026-09-10T10:00");
    sb.add("Second", "2026-09-11T10:00");
    let uid = sb.uid_at(2);
    sb.oca()
        .args(["rm", &uid])
        .assert()
        .success()
        .stdout(contains(format!("removed [{uid}]")));
    assert_eq!(sb.event_count(), 1);
    sb.oca()
        .arg("ls")
        .assert()
        .stdout(contains("First").and(contains("Second").not()));
}

#[test]
fn rm_index_zero_fails() {
    let sb = Sandbox::new();
    sb.add("Only", "2026-09-10T10:00");
    sb.oca()
        .args(["rm", "0"])
        .assert()
        .failure()
        .stderr(contains("1-based"));
    assert_eq!(sb.event_count(), 1);
}

#[test]
fn rm_out_of_range_index_fails() {
    let sb = Sandbox::new();
    sb.add("Only", "2026-09-10T10:00");
    sb.oca()
        .args(["rm", "7"])
        .assert()
        .failure()
        .stderr(contains("no event at index 7"));
    assert_eq!(sb.event_count(), 1);
}

#[test]
fn rm_unknown_uid_fails() {
    let sb = Sandbox::new();
    sb.add("Only", "2026-09-10T10:00");
    sb.oca()
        .args(["rm", "missing@optioncalendar"])
        .assert()
        .failure()
        .stderr(contains("no event with id"));
    assert_eq!(sb.event_count(), 1);
}

#[test]
fn import_then_export_roundtrip() {
    let sb = Sandbox::new();
    let ics = sb.path().join("in.ics");
    fs::write(
        &ics,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
         BEGIN:VEVENT\r\nUID:a@test\r\nDTSTART:20260910T100000\r\nSUMMARY:Alpha\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:b@test\r\nDTSTART:20260911T100000\r\nSUMMARY:Beta\r\nEND:VEVENT\r\n\
         END:VCALENDAR\r\n",
    )
    .unwrap();
    sb.oca()
        .args(["import", ics.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("imported 2 events"));
    // Re-importing the same UIDs adds nothing.
    sb.oca()
        .args(["import", ics.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("imported 0 events"));
    assert_eq!(sb.event_count(), 2);

    let out = sb.path().join("out.ics");
    sb.oca()
        .args(["export", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("exported 2 events"));
    let exported = fs::read_to_string(&out).unwrap();
    assert!(exported.contains("UID:a@test"));
    assert!(exported.contains("SUMMARY:Beta"));

    let fresh = Sandbox::new();
    fresh
        .oca()
        .args(["import", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(contains("imported 2 events"));
    assert_eq!(fresh.event_count(), 2);
}

#[test]
fn import_missing_file_fails() {
    let sb = Sandbox::new();
    sb.oca()
        .args(["import", "/nonexistent/file.ics"])
        .assert()
        .failure()
        .stderr(contains("cannot read"));
}

#[test]
fn config_shows_defaults_and_persists_launch_flag() {
    let sb = Sandbox::new();
    sb.oca()
        .arg("config")
        .assert()
        .success()
        .stdout(contains("ics_path =").and(contains("launch_tui_on_no_args = false")));

    sb.oca()
        .args(["config", "--launch-tui-on-no-args", "true"])
        .assert()
        .success()
        .stdout(contains("launch_tui_on_no_args set to true"));
    let config = fs::read_to_string(sb.path().join("cal").join("config.toml")).unwrap();
    assert!(config.contains("launch_tui_on_no_args = true"));
    sb.oca()
        .arg("config")
        .assert()
        .success()
        .stdout(contains("launch_tui_on_no_args = true"));

    sb.oca()
        .args(["config", "--launch-tui-on-no-args", "false"])
        .assert()
        .success()
        .stdout(contains("launch_tui_on_no_args set to false"));
    sb.oca()
        .arg("config")
        .assert()
        .success()
        .stdout(contains("launch_tui_on_no_args = false"));
}

#[test]
fn config_rejects_invalid_bool() {
    let sb = Sandbox::new();
    sb.oca()
        .args(["config", "--launch-tui-on-no-args", "maybe"])
        .assert()
        .failure()
        .stderr(contains("invalid BOOL 'maybe'"));
    sb.oca()
        .arg("config")
        .assert()
        .success()
        .stdout(contains("launch_tui_on_no_args = false"));
}

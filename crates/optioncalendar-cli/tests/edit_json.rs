//! Integration tests for `oca edit` and the global `--json` flag.
//!
//! Each test gets its own `OPTION_HOME` tempdir so nothing touches `~/.option`.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn oca(home: &tempfile::TempDir) -> Command {
    let mut cmd = Command::cargo_bin("oca").expect("oca binary");
    cmd.env("OPTION_HOME", home.path()).env("NO_COLOR", "1");
    cmd
}

fn ls_json(home: &tempfile::TempDir) -> Vec<Value> {
    let out = oca(home).args(["--json", "ls"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice::<Value>(&out.stdout)
        .expect("stdout is valid JSON")
        .as_array()
        .expect("array")
        .clone()
}

#[test]
fn json_ls_is_stable_array_of_events() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(ls_json(&home), Vec::<Value>::new());

    oca(&home)
        .args([
            "add",
            "Dentist",
            "--at",
            "2030-09-10T10:00",
            "--description",
            "cleaning",
        ])
        .assert()
        .success();
    oca(&home)
        .args(["add", "Trip", "--at", "2030-09-01", "--end", "2030-09-03"])
        .assert()
        .success();

    let events = ls_json(&home);
    assert_eq!(events.len(), 2);
    // Sorted by start; stable key set; ISO 8601 datetimes.
    let trip = &events[0];
    assert_eq!(trip["summary"], "Trip");
    assert_eq!(trip["start"], "2030-09-01T00:00:00");
    assert_eq!(trip["end"], "2030-09-03T00:00:00");
    assert_eq!(trip["description"], "");
    assert!(trip["uid"].as_str().is_some_and(|u| !u.is_empty()));
    let dentist = &events[1];
    assert_eq!(dentist["start"], "2030-09-10T10:00:00");
    assert_eq!(dentist["end"], Value::Null);
    assert_eq!(dentist["description"], "cleaning");
    let mut keys: Vec<&String> = dentist.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["description", "end", "start", "summary", "uid"]);
}

#[test]
fn json_search_next_today_week_month_emit_arrays() {
    let home = tempfile::tempdir().unwrap();
    oca(&home)
        .args(["add", "Far future", "--at", "2099-01-01T09:00"])
        .assert()
        .success();

    let out = oca(&home)
        .args(["--json", "search", "future"])
        .output()
        .unwrap();
    let hits: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["summary"], "Far future");

    let out = oca(&home)
        .args(["--json", "search", "nomatch"])
        .output()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        Value::Array(vec![])
    );

    let out = oca(&home).args(["next", "--json"]).output().unwrap();
    let next: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(next[0]["start"], "2099-01-01T09:00:00");

    for sub in ["today", "week", "month"] {
        let out = oca(&home).args(["--json", sub]).output().unwrap();
        assert!(out.status.success(), "{sub}");
        let value: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{sub}: invalid JSON: {e}"));
        assert!(value.is_array(), "{sub} must emit an array");
        // No mark / colour in JSON mode.
        assert!(!String::from_utf8_lossy(&out.stdout).contains('◷'));
    }
}

#[test]
fn edit_updates_fields_and_keeps_uid() {
    let home = tempfile::tempdir().unwrap();
    oca(&home)
        .args([
            "add",
            "Dentist",
            "--at",
            "2030-09-10T10:00",
            "--description",
            "cleaning",
        ])
        .assert()
        .success();
    let before = ls_json(&home);
    let uid = before[0]["uid"].as_str().unwrap().to_owned();

    oca(&home)
        .args([
            "edit",
            "1",
            "--summary",
            "Dentist (moved)",
            "--at",
            "2030-09-11T11:00",
            "--end",
            "2030-09-11T12:00",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "edited 2030-09-11 11:00  Dentist (moved)",
        ));

    let after = ls_json(&home);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0]["uid"], uid.as_str());
    assert_eq!(after[0]["summary"], "Dentist (moved)");
    assert_eq!(after[0]["start"], "2030-09-11T11:00:00");
    assert_eq!(after[0]["end"], "2030-09-11T12:00:00");
    assert_eq!(after[0]["description"], "cleaning");

    // Edit by UID with the clear flags.
    oca(&home)
        .args(["edit", &uid, "--clear-end", "--clear-description"])
        .assert()
        .success();
    let cleared = ls_json(&home);
    assert_eq!(cleared[0]["end"], Value::Null);
    assert_eq!(cleared[0]["description"], "");
}

#[test]
fn edit_rejects_no_flags_bad_range_and_unknown_id() {
    let home = tempfile::tempdir().unwrap();
    oca(&home)
        .args(["add", "Dentist", "--at", "2030-09-10T10:00"])
        .assert()
        .success();

    oca(&home)
        .args(["edit", "1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing to change"));

    oca(&home)
        .args(["edit", "1", "--end", "2030-09-09T10:00"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is before start"));

    oca(&home)
        .args(["edit", "does-not-exist", "--summary", "X"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no event with id 'does-not-exist'",
        ));

    oca(&home)
        .args(["edit", "5", "--summary", "X"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no event at index 5"));

    // Nothing changed.
    assert_eq!(ls_json(&home)[0]["summary"], "Dentist");
}

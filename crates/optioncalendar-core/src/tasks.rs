//! Optional bridge to optionNotes tasks.
//!
//! Reads `~/Documents/Notes/tasks/*.md` looking for open checklist items with
//! a due date (`- [ ] text due:YYYY-MM-DD`). Never fails: a missing vault,
//! missing directory, or unreadable file simply yields no tasks.

use std::path::PathBuf;

use chrono::NaiveDate;
use serde::Serialize;

/// A task with a due date, sourced from a Markdown checklist.
///
/// Serializes `due` as ISO 8601 `YYYY-MM-DD`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskDue {
    pub text: String,
    #[serde(serialize_with = "serialize_date")]
    pub due: NaiveDate,
    pub source: PathBuf,
}

fn serialize_date<S: serde::Serializer>(value: &NaiveDate, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&value.format("%Y-%m-%d").to_string())
}

/// Default tasks directory: `~/Documents/Notes/tasks`.
pub fn default_tasks_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| option_sdk::home_dir().join("Documents"))
        .join("Notes")
        .join("tasks")
}

/// Collect due tasks, returning an empty vec on any failure.
pub fn due_tasks_or_empty() -> Vec<TaskDue> {
    due_tasks(&default_tasks_dir()).unwrap_or_default()
}

/// Collect due tasks from `dir` (`*.md`, non-recursive).
pub fn due_tasks(dir: &std::path::Path) -> crate::Result<Vec<TaskDue>> {
    let mut tasks = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(Vec::new()),
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    for file in files {
        let content = match std::fs::read_to_string(&file) {
            Ok(content) => content,
            Err(_) => continue,
        };
        tasks.extend(parse_due_tasks(&content, &file));
    }
    tasks.sort_by(|a, b| a.due.cmp(&b.due).then_with(|| a.text.cmp(&b.text)));
    Ok(tasks)
}

/// Parse `- [ ] text due:YYYY-MM-DD` lines (also accepts `[x]`? no — open only).
fn parse_due_tasks(content: &str, source: &std::path::Path) -> Vec<TaskDue> {
    let mut tasks = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let rest = match trimmed
            .strip_prefix("- [ ]")
            .or_else(|| trimmed.strip_prefix("* [ ]"))
        {
            Some(rest) => rest.trim(),
            None => continue,
        };
        let Some(due_pos) = rest.find("due:") else {
            continue;
        };
        let date_raw = rest[due_pos + 4..]
            .trim()
            .chars()
            .take(10)
            .collect::<String>();
        let Ok(due) = NaiveDate::parse_from_str(&date_raw, "%Y-%m-%d") else {
            continue;
        };
        let text = rest[..due_pos]
            .trim()
            .trim_end_matches(',')
            .trim()
            .to_owned();
        if text.is_empty() {
            continue;
        }
        tasks.push(TaskDue {
            text,
            due,
            source: source.to_path_buf(),
        });
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_tasks_with_due() {
        let content = "# Tasks\n\n- [ ] pay bill due:2026-09-03\n- [x] done due:2026-09-01\n- [ ] no date\n- [ ] bad date due:yesterday\n";
        let tasks = parse_due_tasks(content, std::path::Path::new("tasks/a.md"));
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "pay bill");
        assert_eq!(tasks[0].due, NaiveDate::from_ymd_opt(2026, 9, 3).unwrap());
    }

    #[test]
    fn missing_dir_yields_empty() {
        let tasks = due_tasks(std::path::Path::new(
            "/nonexistent-optioncalendar-tasks-dir",
        ))
        .unwrap();
        assert!(tasks.is_empty());
        // The helper wraps any failure and never panics.
        let _ = due_tasks_or_empty();
    }

    #[test]
    fn reads_md_files_sorted_by_due() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("b.md"), "- [ ] late due:2026-09-10\n").unwrap();
        std::fs::write(dir.path().join("a.md"), "- [ ] early due:2026-09-01\n").unwrap();
        std::fs::write(dir.path().join("ignore.txt"), "- [ ] nope due:2026-01-01\n").unwrap();
        let tasks = due_tasks(dir.path()).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].text, "early");
        assert_eq!(tasks[1].text, "late");
    }
}

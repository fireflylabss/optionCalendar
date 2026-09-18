//! optioncalendar-core — ICS event store, day/week/month queries, and tasks bridge.
//!
//! Local-first minimal calendar for the Option family. No network. No daemon.

mod error;
pub mod ics;
pub mod notify;
pub mod query;
pub mod store;
pub mod tasks;
pub mod week;

pub use error::{Error, Result};
pub use ics::{Event, format_date, format_dt, is_date_only, parse_dt, parse_ics, to_ics};
pub use notify::{DEFAULT_LATE, due_events, load_notified, notification_key, save_notified};
pub use query::{
    DayItem, events_between, events_on, merged_between, month_range, occurrences_between,
    occurs_on, today, today_merged,
};
pub use store::{
    CalStore, Settings, default_ics_path, load_settings, load_settings_from, save_settings,
    save_settings_to,
};
pub use tasks::{TaskDue, default_tasks_dir, due_tasks, due_tasks_or_empty};
pub use week::WeekStart;

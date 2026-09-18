//! `oca` / `optioncalendar` — CLI for optionCalendar.

mod tui;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use clap::{
    CommandFactory, Parser, Subcommand,
    builder::styling::{AnsiColor, Effects, Styles},
};
use option_sdk::App;
use optioncalendar_core::{
    CalStore, DEFAULT_LATE, DayItem, Event, TaskDue, WeekStart, due_events, due_tasks_or_empty,
    is_date_only, load_notified, load_settings, merged_between, month_range, notification_key,
    parse_dt, save_notified, to_ics, today, today_merged,
};
use serde::Serialize;

fn cli_styles() -> Styles {
    if !option_sdk::color_enabled() {
        return Styles::plain();
    }
    Styles::styled()
        .header(AnsiColor::White.on_default() | Effects::BOLD)
        .usage(AnsiColor::White.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightWhite.on_default())
        .placeholder(AnsiColor::BrightBlack.on_default())
        .error(AnsiColor::BrightRed.on_default() | Effects::BOLD)
        .valid(AnsiColor::BrightWhite.on_default())
        .invalid(AnsiColor::BrightRed.on_default())
}

/// ◷ optionCalendar — minimal local calendar
#[derive(Debug, Parser)]
#[command(
    name = "oca",
    version,
    about = "◷ optionCalendar",
    long_about = "optionCalendar — minimal local calendar over a single ICS file.\n\
\n\
  binaries  optioncalendar · oca (same entrypoint)\n\
  config    ~/.option/cal/config.toml\n\
  calendar  ~/.option/cal/calendar.ics\n\
 \n\
  tui       `oca tui` opens the month + agenda view\n\
  notify    `oca notify --send` fires desktop reminders (run it from a timer)\n\
 \n\
 Bare `oca` prints this help unless `launch_tui_on_no_args`\n\
 is enabled via `oca config` (then bare `oca` opens the TUI).",
    styles = cli_styles(),
    propagate_version = true,
    disable_help_subcommand = true,
)]
struct Cli {
    /// Print machine-readable JSON (ls, today, week, month, next, search)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Add an event
    Add {
        /// Event summary
        summary: String,
        /// Start: YYYY-MM-DD or YYYY-MM-DDTHH:MM
        #[arg(long, value_name = "AT")]
        at: String,
        /// End: YYYY-MM-DD or YYYY-MM-DDTHH:MM
        #[arg(long, value_name = "END")]
        end: Option<String>,
        /// Event description / notes
        #[arg(long, value_name = "DESC")]
        description: Option<String>,
    },
    /// List all events
    Ls {
        /// Show the event UID alongside each line
        #[arg(long)]
        uid: bool,
    },
    /// Show a day's events plus due tasks (today by default)
    Today {
        /// Day to show: YYYY-MM-DD (default: today)
        #[arg(value_name = "DATE")]
        date: Option<String>,
    },
    /// Show 7 days of events and due tasks (starting today by default)
    Week {
        /// First day of the window: YYYY-MM-DD (default: today)
        #[arg(value_name = "DATE")]
        date: Option<String>,
    },
    /// Show a month's events and due tasks (current month by default)
    Month {
        /// Month to show: YYYY-MM or YYYY-MM-DD (default: current month)
        #[arg(value_name = "DATE")]
        date: Option<String>,
    },
    /// Show the next upcoming event
    Next,
    /// Show events starting soon; --send fires desktop notifications
    Notify {
        /// Lookahead window in minutes (default: notify_window_minutes, 15)
        #[arg(long, value_name = "MIN")]
        window: Option<u32>,
        /// Run the notifier for each due occurrence and remember what was sent
        #[arg(long)]
        send: bool,
    },
    /// Search events by summary or description
    Search {
        /// Text to search for (case-insensitive)
        query: String,
    },
    /// Edit an event by UID or 1-based list index (the UID never changes)
    Edit {
        /// UID or 1-based index from `oca ls`
        id: String,
        /// New summary
        #[arg(long, value_name = "SUMMARY")]
        summary: Option<String>,
        /// New start: YYYY-MM-DD or YYYY-MM-DDTHH:MM
        #[arg(long, value_name = "AT")]
        at: Option<String>,
        /// New end: YYYY-MM-DD or YYYY-MM-DDTHH:MM
        #[arg(long, value_name = "END", conflicts_with = "clear_end")]
        end: Option<String>,
        /// New description / notes
        #[arg(long, value_name = "DESC", conflicts_with = "clear_description")]
        description: Option<String>,
        /// Remove the end time
        #[arg(long)]
        clear_end: bool,
        /// Remove the description
        #[arg(long)]
        clear_description: bool,
    },
    /// Remove an event by UID or the 1-based index printed by `oca ls`
    Rm {
        /// UID (see `oca ls --uid`) or the index shown by `oca ls`
        id: String,
    },
    /// Import events from an .ics file (skips duplicate UIDs)
    Import {
        /// Path to the .ics file
        file: PathBuf,
    },
    /// Export all events to an .ics file
    Export {
        /// Path to write the .ics file
        file: PathBuf,
    },
    /// Open the interactive month + agenda view (read-only)
    Tui,
    /// Show settings, or persist `--launch-tui-on-no-args` / `--week-start` / `--notify-window-minutes`
    Config {
        /// Launch the TUI when `oca` is invoked with no subcommand
        #[arg(long, value_name = "BOOL")]
        launch_tui_on_no_args: Option<String>,
        /// First day of the week: monday (default) or sunday
        #[arg(long, value_name = "DAY")]
        week_start: Option<String>,
        /// Reminder lookahead for `oca notify`, in minutes
        #[arg(long, value_name = "MIN")]
        notify_window_minutes: Option<String>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        for cause in error.chain().skip(1) {
            eprintln!("  ↳ {cause}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let json = cli.json;
    App::CAL
        .ensure()
        .context("failed to ensure ~/.option/cal")?;

    let Some(command) = cli.command else {
        // No subcommand: open the TUI only when opted in, else show help.
        let settings = load_settings().context("failed to load settings")?;
        if settings.launch_tui_on_no_args {
            return tui::launch();
        }
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };
    match command {
        Commands::Add {
            summary,
            at,
            end,
            description,
        } => cmd_add(&summary, &at, end.as_deref(), description.as_deref())?,
        Commands::Ls { uid } => cmd_ls(uid, json)?,
        Commands::Today { date } => cmd_today(date.as_deref(), json)?,
        Commands::Week { date } => cmd_week(date.as_deref(), json)?,
        Commands::Month { date } => cmd_month(date.as_deref(), json)?,
        Commands::Next => cmd_next(json)?,
        Commands::Notify { window, send } => cmd_notify(window, send, json)?,
        Commands::Search { query } => cmd_search(&query, json)?,
        Commands::Edit {
            id,
            summary,
            at,
            end,
            description,
            clear_end,
            clear_description,
        } => cmd_edit(
            &id,
            EditArgs {
                summary: summary.as_deref(),
                at: at.as_deref(),
                end: end.as_deref(),
                description: description.as_deref(),
                clear_end,
                clear_description,
            },
        )?,
        Commands::Rm { id } => cmd_rm(&id)?,
        Commands::Import { file } => cmd_import(&file)?,
        Commands::Export { file } => cmd_export(&file)?,
        Commands::Tui => tui::launch()?,
        Commands::Config {
            launch_tui_on_no_args,
            week_start,
            notify_window_minutes,
        } => cmd_config(
            launch_tui_on_no_args.as_deref(),
            week_start.as_deref(),
            notify_window_minutes.as_deref(),
        )?,
    }
    Ok(())
}

fn open_store() -> Result<CalStore> {
    let settings = load_settings().context("failed to load settings")?;
    CalStore::open(settings.ics_path).context("failed to open calendar")
}

/// Accept `YYYY-MM-DD[THH:MM]` (also `YYYY-MM-DD HH:MM`).
fn parse_cli_dt(raw: &str) -> Result<NaiveDateTime> {
    parse_dt(raw)
        .with_context(|| format!("invalid date '{raw}': use YYYY-MM-DD or YYYY-MM-DDTHH:MM"))
}

fn cmd_add(summary: &str, at: &str, end: Option<&str>, description: Option<&str>) -> Result<()> {
    if summary.trim().is_empty() {
        bail!("summary cannot be empty");
    }
    let start = parse_cli_dt(at)?;
    let all_day = is_date_only(at) && end.is_none_or(is_date_only);
    let end = end.map(parse_cli_dt).transpose()?;
    if let Some(end) = end
        && end < start
    {
        bail!(
            "end ({}) is before start ({})",
            fmt_dt(&end),
            fmt_dt(&start)
        );
    }
    let mut store = open_store()?;
    let mut event = if all_day {
        Event::new_all_day(summary.trim(), start.date(), end.map(|e| e.date()))
    } else {
        Event::new(summary.trim(), start, end)
    };
    if let Some(desc) = description {
        event.description = desc.trim().to_string();
    }
    store.add(event.clone()).context("failed to save event")?;
    println!(
        "{} added {}  {}",
        App::CAL.mark(),
        fmt_dt(&event.start),
        event.summary
    );
    Ok(())
}

/// Optional positional day for `today` / `week`: `YYYY-MM-DD`, default today.
fn parse_cli_day(raw: Option<&str>) -> Result<NaiveDate> {
    match raw {
        None => Ok(today()),
        Some(raw) => NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
            .with_context(|| format!("invalid date '{raw}': use YYYY-MM-DD")),
    }
}

/// Optional positional month for `month`: `YYYY-MM` or `YYYY-MM-DD`, default current month.
fn parse_cli_month(raw: Option<&str>) -> Result<NaiveDate> {
    let Some(raw) = raw else {
        return Ok(today());
    };
    let trimmed = raw.trim();
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{trimmed}-01"), "%Y-%m-%d"))
        .with_context(|| format!("invalid month '{raw}': use YYYY-MM or YYYY-MM-DD"))
}

/// JSON line of `today`: an event or a due task, tagged by `kind`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum DayItemJson<'a> {
    Event(&'a Event),
    Task(&'a TaskDue),
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("failed to serialize JSON")?
    );
    Ok(())
}

/// Map merged day items to their JSON form.
fn day_items_json(items: &[DayItem]) -> Vec<DayItemJson<'_>> {
    items
        .iter()
        .map(|item| match item {
            DayItem::Event(event) => DayItemJson::Event(event),
            DayItem::Task(task) => DayItemJson::Task(task),
        })
        .collect()
}

fn cmd_ls(show_uid: bool, json: bool) -> Result<()> {
    let store = open_store()?;
    if json {
        return print_json(&store.events);
    }
    print_events(
        &store.events.iter().collect::<Vec<_>>(),
        "no events",
        show_uid,
        true,
    );
    Ok(())
}

fn cmd_today(date: Option<&str>, json: bool) -> Result<()> {
    let date = parse_cli_day(date)?;
    let store = open_store()?;
    // The tasks bridge never fails: missing vault means "no tasks".
    let tasks = due_tasks_or_empty();
    let items = today_merged(&store.events, &tasks, date);
    if json {
        return print_json(&day_items_json(&items));
    }
    let mark = App::CAL.mark();
    let label = if date == today() { "today" } else { "day" };
    if items.is_empty() {
        println!("{mark} nothing {label} ({})", date.format("%Y-%m-%d"));
        return Ok(());
    }
    println!("{mark} {label} · {}", date.format("%Y-%m-%d"));
    for item in items {
        match item {
            DayItem::Event(event) => println!("  {}  {}", fmt_dt(&event.start), event.summary),
            DayItem::Task(task) => println!(
                "  [ ] {}  (due {}, {})",
                task.text,
                task.due.format("%Y-%m-%d"),
                task.source.display()
            ),
        }
    }
    Ok(())
}

fn cmd_week(date: Option<&str>, json: bool) -> Result<()> {
    let start = parse_cli_day(date)?;
    let store = open_store()?;
    let end = start + chrono::Duration::days(6);
    let tasks = due_tasks_or_empty();
    let items = merged_between(&store.events, &tasks, start, end);
    if json {
        return print_json(&day_items_json(&items));
    }
    println!(
        "{} week · {} → {}",
        App::CAL.mark(),
        start.format("%m-%d"),
        end.format("%m-%d")
    );
    print_grouped_by_day(&items);
    Ok(())
}

fn cmd_month(date: Option<&str>, json: bool) -> Result<()> {
    let (first, last) = month_range(parse_cli_month(date)?);
    let store = open_store()?;
    let tasks = due_tasks_or_empty();
    let items = merged_between(&store.events, &tasks, first, last);
    if json {
        return print_json(&day_items_json(&items));
    }
    let events = items
        .iter()
        .filter(|item| matches!(item, DayItem::Event(_)))
        .count();
    let tasks_count = items.len() - events;
    let mut header = format!(
        "{} {} · {} event{}",
        App::CAL.mark(),
        first.format("%Y-%m"),
        events,
        if events == 1 { "" } else { "s" }
    );
    if tasks_count > 0 {
        header.push_str(&format!(
            ", {tasks_count} task{}",
            if tasks_count == 1 { "" } else { "s" }
        ));
    }
    println!("{header}");
    print_grouped_by_day(&items);
    Ok(())
}

fn cmd_import(file: &PathBuf) -> Result<()> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let mut store = open_store()?;
    let added = store.import(&text).context("failed to import")?;
    println!(
        "{} imported {added} event{} from {}",
        App::CAL.mark(),
        if added == 1 { "" } else { "s" },
        file.display()
    );
    Ok(())
}

fn cmd_export(file: &PathBuf) -> Result<()> {
    let store = open_store()?;
    let text = to_ics(&store.events);
    std::fs::write(file, text).with_context(|| format!("cannot write {}", file.display()))?;
    println!(
        "{} exported {} event{} to {}",
        App::CAL.mark(),
        store.events.len(),
        if store.events.len() == 1 { "" } else { "s" },
        file.display()
    );
    Ok(())
}

/// Show the next event strictly after now.
fn cmd_next(json: bool) -> Result<()> {
    let store = open_store()?;
    let now = Local::now().naive_local();
    let event = store
        .events
        .iter()
        .filter(|event| event.start > now)
        .min_by_key(|event| event.start);
    if json {
        // Always an array: empty or a single upcoming event.
        return print_json(&event.into_iter().collect::<Vec<_>>());
    }
    let Some(event) = event else {
        println!("{} no upcoming events", App::CAL.mark());
        return Ok(());
    };
    let days = (event.start.date() - now.date()).num_days();
    let when = match days {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        n => format!("in {n} days"),
    };
    println!(
        "{} next · {}  {}",
        App::CAL.mark(),
        fmt_dt(&event.start),
        event.summary
    );
    println!("  {when}");
    Ok(())
}

/// List occurrences starting soon; `--send` runs the notifier once per
/// occurrence and remembers what was sent (dedup state lives in
/// `~/.option/cal/notified`). No daemon: a systemd user timer or cron entry
/// is expected to run this periodically. `OCA_NOTIFY_CMD` overrides the
/// notifier binary (default `notify-send`) — used by tests and by anyone with
/// a custom notification script.
fn cmd_notify(window: Option<u32>, send: bool, json: bool) -> Result<()> {
    let settings = load_settings().context("failed to load settings")?;
    let minutes = window.unwrap_or(settings.notify_window_minutes);
    if minutes == 0 {
        bail!("invalid MIN '0': use a positive number of minutes");
    }
    let store = open_store()?;
    let now = Local::now().naive_local();
    let due = due_events(
        &store.events,
        now,
        Duration::minutes(i64::from(minutes)),
        DEFAULT_LATE,
    );
    let mark = App::CAL.mark();
    if !send {
        if json {
            return print_json(&due);
        }
        if due.is_empty() {
            println!("{mark} nothing due within {minutes}m");
            return Ok(());
        }
        println!("{mark} {} due within {minutes}m", due.len());
        for event in &due {
            println!("  {}  {}", fmt_time(&event.start), event.summary);
        }
        return Ok(());
    }
    let state_path = App::CAL.path("notified");
    let mut sent = load_notified(&state_path);
    let notifier = std::env::var("OCA_NOTIFY_CMD").unwrap_or_else(|_| "notify-send".to_string());
    let mut fired = 0usize;
    for event in &due {
        let key = notification_key(event);
        if sent.contains(&key) {
            continue;
        }
        let body = if event.description.is_empty() {
            fmt_dt(&event.start)
        } else {
            format!("{}  {}", fmt_dt(&event.start), event.description)
        };
        std::process::Command::new(&notifier)
            .arg(format!("{mark} {}", event.summary))
            .arg(body)
            .status()
            .with_context(|| format!("cannot run '{notifier}'"))?;
        sent.insert(key);
        fired += 1;
    }
    if fired == 0 {
        println!("{mark} nothing to send");
        return Ok(());
    }
    save_notified(&state_path, &sent, now).context("failed to save notify state")?;
    println!(
        "{mark} sent {fired} reminder{}",
        if fired == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Case-insensitive (Unicode) search over summary and description.
fn cmd_search(query: &str, json: bool) -> Result<()> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        bail!("search query cannot be empty");
    }
    let store = open_store()?;
    let hits: Vec<&Event> = store
        .events
        .iter()
        .filter(|event| event_matches(event, &needle))
        .collect();
    if json {
        return print_json(&hits);
    }
    print_events(&hits, "no matches", false, false);
    Ok(())
}

/// True when `needle` (already lowercased) occurs in the summary or description.
fn event_matches(event: &Event, needle: &str) -> bool {
    event.summary.to_lowercase().contains(needle)
        || event.description.to_lowercase().contains(needle)
}

struct EditArgs<'a> {
    summary: Option<&'a str>,
    at: Option<&'a str>,
    end: Option<&'a str>,
    description: Option<&'a str>,
    clear_end: bool,
    clear_description: bool,
}

impl EditArgs<'_> {
    fn is_empty(&self) -> bool {
        self.summary.is_none()
            && self.at.is_none()
            && self.end.is_none()
            && self.description.is_none()
            && !self.clear_end
            && !self.clear_description
    }
}

/// Edit an event by UID or 1-based list index; the UID is preserved.
fn cmd_edit(id: &str, args: EditArgs<'_>) -> Result<()> {
    if args.is_empty() {
        bail!(
            "nothing to change: pass at least one of --summary/--at/--end/--description/--clear-end/--clear-description"
        );
    }
    if let Some(summary) = args.summary
        && summary.trim().is_empty()
    {
        bail!("summary cannot be empty");
    }
    let start = args.at.map(parse_cli_dt).transpose()?;
    let end = args.end.map(parse_cli_dt).transpose()?;

    let mut store = open_store()?;
    let uid = resolve_id(&store, id)?;
    let current = store
        .events
        .iter()
        .find(|event| event.uid == uid)
        .with_context(|| format!("no event with id '{id}'"))?;
    let new_start = start.unwrap_or(current.start);
    let new_end = if args.clear_end {
        None
    } else {
        end.or(current.end)
    };
    if let Some(new_end) = new_end
        && new_end < new_start
    {
        bail!(
            "end ({}) is before start ({})",
            fmt_dt(&new_end),
            fmt_dt(&new_start)
        );
    }

    let updated = store
        .update(&uid, |event| {
            if let Some(summary) = args.summary {
                event.summary = summary.trim().to_string();
            }
            if let Some(at) = args.at {
                event.all_day = is_date_only(at);
            }
            event.start = new_start;
            event.end = new_end;
            if args.clear_description {
                event.description.clear();
            } else if let Some(description) = args.description {
                event.description = description.trim().to_string();
            }
        })
        .context("failed to save event")?;
    if !updated {
        bail!("no event with id '{id}'");
    }
    let event = store
        .events
        .iter()
        .find(|event| event.uid == uid)
        .expect("updated event is in the store");
    println!(
        "{} edited {}  {}  [{}]",
        App::CAL.mark(),
        fmt_dt(&event.start),
        event.summary,
        event.uid
    );
    Ok(())
}

/// Remove an event by UID or 1-based list index.
fn cmd_rm(id: &str) -> Result<()> {
    let mut store = open_store()?;
    let uid = resolve_id(&store, id)?;
    let removed = store.remove(&uid).context("failed to remove event")?;
    if removed {
        println!("{} removed [{}]", App::CAL.mark(), uid);
    } else {
        bail!("no event with id '{id}'");
    }
    Ok(())
}

/// Resolve an event id: a 1-based index into the sorted store, or a raw UID.
fn resolve_id(store: &CalStore, id: &str) -> Result<String> {
    if let Ok(index) = id.parse::<usize>() {
        if index == 0 {
            bail!("list index is 1-based, not 0");
        }
        let event = store
            .events
            .get(index - 1)
            .with_context(|| format!("no event at index {index} (have {})", store.events.len()))?;
        return Ok(event.uid.clone());
    }
    Ok(id.to_string())
}

fn cmd_config(
    launch_tui_on_no_args: Option<&str>,
    week_start: Option<&str>,
    notify_window_minutes: Option<&str>,
) -> Result<()> {
    let mut settings = load_settings().context("failed to load settings")?;
    let mark = App::CAL.mark();
    let mut changed = false;
    if let Some(raw) = launch_tui_on_no_args {
        settings.launch_tui_on_no_args = parse_bool(raw)?;
        changed = true;
    }
    if let Some(raw) = week_start {
        settings.week_start = parse_week_start(raw)?;
        changed = true;
    }
    if let Some(raw) = notify_window_minutes {
        settings.notify_window_minutes = parse_minutes(raw)?;
        changed = true;
    }
    if changed {
        optioncalendar_core::save_settings(&settings).context("failed to save settings")?;
        if launch_tui_on_no_args.is_some() {
            println!(
                "{mark} launch_tui_on_no_args set to {}",
                settings.launch_tui_on_no_args
            );
        }
        if week_start.is_some() {
            println!(
                "{mark} week_start set to {}",
                week_start_name(settings.week_start)
            );
        }
        if notify_window_minutes.is_some() {
            println!(
                "{mark} notify_window_minutes set to {}",
                settings.notify_window_minutes
            );
        }
        return Ok(());
    }
    println!("{mark} config {}", App::CAL.config_toml().display());
    println!("  ics_path = {}", settings.ics_path.display());
    println!(
        "  launch_tui_on_no_args = {}",
        settings.launch_tui_on_no_args
    );
    println!("  week_start = {}", week_start_name(settings.week_start));
    println!(
        "  notify_window_minutes = {}",
        settings.notify_window_minutes
    );
    Ok(())
}

fn parse_week_start(raw: &str) -> Result<WeekStart> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "monday" | "mon" => Ok(WeekStart::Monday),
        "sunday" | "sun" => Ok(WeekStart::Sunday),
        _ => bail!("invalid DAY '{raw}': use monday or sunday"),
    }
}

fn week_start_name(value: WeekStart) -> &'static str {
    match value {
        WeekStart::Monday => "monday",
        WeekStart::Sunday => "sunday",
    }
}

fn parse_minutes(raw: &str) -> Result<u32> {
    let value: u32 = raw
        .trim()
        .parse()
        .with_context(|| format!("invalid MIN '{raw}': use a positive number of minutes"))?;
    if value == 0 {
        bail!("invalid MIN '{raw}': use a positive number of minutes");
    }
    Ok(value)
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("invalid BOOL '{raw}': use true or false"),
    }
}

/// Print events one per line; `numbered` prefixes the 1-based index `rm` accepts.
fn print_events(events: &[&Event], empty_msg: &str, show_uid: bool, numbered: bool) {
    let mark = App::CAL.mark();
    if events.is_empty() {
        println!("{mark} {empty_msg}");
        return;
    }
    println!(
        "{mark} {} event{}",
        events.len(),
        if events.len() == 1 { "" } else { "s" }
    );
    for (index, event) in events.iter().enumerate() {
        println!("  {}", event_line(event, index + 1, show_uid, numbered));
    }
}

/// One `ls`/`search` line: `[index  ]date  summary[  [uid]]`.
fn event_line(event: &Event, index: usize, show_uid: bool, numbered: bool) -> String {
    let mut line = String::new();
    if numbered {
        line.push_str(&format!("{index:>3}  "));
    }
    line.push_str(&fmt_dt(&event.start));
    line.push_str("  ");
    line.push_str(&event.summary);
    if show_uid {
        line.push_str(&format!("  [{}]", event.uid));
    }
    line
}

fn print_grouped_by_day(items: &[DayItem]) {
    let mut current: Option<NaiveDate> = None;
    for item in items {
        let day = item.date();
        if current != Some(day) {
            current = Some(day);
            println!("  {}", day.format("%a %Y-%m-%d"));
        }
        println!("    {}", day_item_line(item));
    }
    if items.is_empty() {
        println!("  (empty)");
    }
}

/// One line inside a day group: `HH:MM  summary`, `all-day  summary`, or `[ ] task`.
fn day_item_line(item: &DayItem) -> String {
    match item {
        DayItem::Event(event) => format!("{}  {}", fmt_time(&event.start), event.summary),
        DayItem::Task(task) => format!("[ ] {}", task.text),
    }
}

/// `HH:MM`, or `all-day` when the time is midnight, like [`fmt_dt`].
fn fmt_time(value: &NaiveDateTime) -> String {
    if value.format("%H:%M:%S").to_string() == "00:00:00" {
        "all-day".to_string()
    } else {
        value.format("%H:%M").to_string()
    }
}

fn fmt_dt(value: &NaiveDateTime) -> String {
    if value.format("%H:%M:%S").to_string() == "00:00:00" {
        value.format("%Y-%m-%d").to_string()
    } else {
        value.format("%Y-%m-%d %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn search_is_unicode_case_insensitive() {
        let event = Event::new(
            "Reunião de equipe",
            parse_dt("2026-09-10T10:00").unwrap(),
            None,
        );
        assert!(event_matches(&event, &"REUNIÃO".to_lowercase()));
        assert!(event_matches(&event, &"equipe".to_lowercase()));
        assert!(!event_matches(&event, &"dentista".to_lowercase()));
    }

    #[test]
    fn ls_lines_are_numbered_with_and_without_uid() {
        let mut event = Event::new("Dentist", parse_dt("2026-09-10T10:00").unwrap(), None);
        event.uid = "abc".into();
        assert_eq!(
            event_line(&event, 1, false, true),
            "  1  2026-09-10 10:00  Dentist"
        );
        assert_eq!(
            event_line(&event, 12, true, true),
            " 12  2026-09-10 10:00  Dentist  [abc]"
        );
        assert_eq!(
            event_line(&event, 1, false, false),
            "2026-09-10 10:00  Dentist"
        );
    }

    #[test]
    fn positional_dates_parse_or_fail_clearly() {
        assert_eq!(parse_cli_day(Some("2026-10-01")).unwrap(), d(2026, 10, 1));
        assert!(parse_cli_day(Some("2026-10")).is_err());
        assert!(parse_cli_day(Some("bogus")).is_err());
        assert_eq!(parse_cli_day(None).unwrap(), today());

        assert_eq!(parse_cli_month(Some("2026-10")).unwrap(), d(2026, 10, 1));
        assert_eq!(
            parse_cli_month(Some("2026-10-15")).unwrap(),
            d(2026, 10, 15)
        );
        let err = parse_cli_month(Some("10/2026")).unwrap_err().to_string();
        assert!(err.contains("use YYYY-MM or YYYY-MM-DD"), "{err}");
    }

    #[test]
    fn week_start_parses_and_prints() {
        assert_eq!(parse_week_start("Sunday").unwrap(), WeekStart::Sunday);
        assert_eq!(parse_week_start("mon").unwrap(), WeekStart::Monday);
        assert!(parse_week_start("friday").is_err());
        assert_eq!(week_start_name(WeekStart::Sunday), "sunday");
    }

    #[test]
    fn grouped_lines_show_all_day_and_tasks() {
        let timed = DayItem::Event(Event::new(
            "Standup",
            parse_dt("2026-09-10T09:30").unwrap(),
            None,
        ));
        let all_day = DayItem::Event(Event::new("Holiday", parse_dt("2026-09-10").unwrap(), None));
        let task = DayItem::Task(optioncalendar_core::TaskDue {
            text: "pay bill".into(),
            due: d(2026, 9, 10),
            source: "tasks/a.md".into(),
        });
        assert_eq!(day_item_line(&timed), "09:30  Standup");
        assert_eq!(day_item_line(&all_day), "all-day  Holiday");
        assert_eq!(day_item_line(&task), "[ ] pay bill");
    }
}

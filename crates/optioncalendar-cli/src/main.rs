//! `oca` / `optioncalendar` — CLI for optionCalendar.

mod tui;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{Local, NaiveDate, NaiveDateTime};
use clap::{
    CommandFactory, Parser, Subcommand,
    builder::styling::{AnsiColor, Effects, Styles},
};
use option_sdk::App;
use optioncalendar_core::{
    CalStore, DayItem, Event, TaskDue, due_tasks_or_empty, events_between, load_settings,
    month_range, parse_dt, to_ics, today, today_merged,
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
    /// Show today's events plus due tasks
    Today,
    /// Show the next 7 days
    Week,
    /// Show the current month
    Month,
    /// Show the next upcoming event
    Next,
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
    /// Remove an event by UID or 1-based list index (see `oca ls --uid`)
    Rm {
        /// UID or 1-based index from `oca ls`
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
    /// Show settings, or persist `--launch-tui-on-no-args true|false`
    Config {
        /// Launch the TUI when `oca` is invoked with no subcommand
        #[arg(long, value_name = "BOOL")]
        launch_tui_on_no_args: Option<String>,
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
        Commands::Today => cmd_today(json)?,
        Commands::Week => cmd_week(json)?,
        Commands::Month => cmd_month(json)?,
        Commands::Next => cmd_next(json)?,
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
        } => cmd_config(launch_tui_on_no_args.as_deref())?,
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
    let mut event = Event::new(summary.trim(), start, end);
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

fn cmd_ls(show_uid: bool, json: bool) -> Result<()> {
    let store = open_store()?;
    if json {
        return print_json(&store.events);
    }
    print_events(
        &store.events.iter().collect::<Vec<_>>(),
        "no events",
        show_uid,
    );
    Ok(())
}

fn cmd_today(json: bool) -> Result<()> {
    let store = open_store()?;
    let date = today();
    // The tasks bridge never fails: missing vault means "no tasks".
    let tasks = due_tasks_or_empty();
    let items = today_merged(&store.events, &tasks, date);
    if json {
        let items: Vec<DayItemJson> = items
            .iter()
            .map(|item| match item {
                DayItem::Event(event) => DayItemJson::Event(event),
                DayItem::Task(task) => DayItemJson::Task(task),
            })
            .collect();
        return print_json(&items);
    }
    let mark = App::CAL.mark();
    if items.is_empty() {
        println!("{mark} nothing today ({})", date.format("%Y-%m-%d"));
        return Ok(());
    }
    println!("{mark} today · {}", date.format("%Y-%m-%d"));
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

fn cmd_week(json: bool) -> Result<()> {
    let store = open_store()?;
    let start = today();
    let end = start + chrono::Duration::days(6);
    let hits = events_between(&store.events, start, end);
    if json {
        return print_json(&hits);
    }
    println!(
        "{} week · {} → {}",
        App::CAL.mark(),
        start.format("%m-%d"),
        end.format("%m-%d")
    );
    print_grouped_by_day(&hits);
    Ok(())
}

fn cmd_month(json: bool) -> Result<()> {
    let store = open_store()?;
    let (first, last) = month_range(today());
    let hits = events_between(&store.events, first, last);
    if json {
        return print_json(&hits);
    }
    println!(
        "{} {} · {} event{}",
        App::CAL.mark(),
        first.format("%Y-%m"),
        hits.len(),
        if hits.len() == 1 { "" } else { "s" }
    );
    print_grouped_by_day(&hits);
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

/// Case-insensitive search over summary and description.
fn cmd_search(query: &str, json: bool) -> Result<()> {
    let needle = query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        bail!("search query cannot be empty");
    }
    let store = open_store()?;
    let hits: Vec<&Event> = store
        .events
        .iter()
        .filter(|event| {
            event.summary.to_ascii_lowercase().contains(&needle)
                || event.description.to_ascii_lowercase().contains(&needle)
        })
        .collect();
    if json {
        return print_json(&hits);
    }
    print_events(&hits, "no matches", false);
    Ok(())
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

fn cmd_config(launch_tui_on_no_args: Option<&str>) -> Result<()> {
    let mut settings = load_settings().context("failed to load settings")?;
    if let Some(raw) = launch_tui_on_no_args {
        settings.launch_tui_on_no_args =
            parse_bool(raw).with_context(|| format!("invalid BOOL '{raw}': use true or false"))?;
        optioncalendar_core::save_settings(&settings).context("failed to save settings")?;
        println!(
            "{} launch_tui_on_no_args set to {}",
            App::CAL.mark(),
            settings.launch_tui_on_no_args
        );
        return Ok(());
    }
    println!(
        "{} config {}",
        App::CAL.mark(),
        App::CAL.config_toml().display()
    );
    println!("  ics_path = {}", settings.ics_path.display());
    println!(
        "  launch_tui_on_no_args = {}",
        settings.launch_tui_on_no_args
    );
    Ok(())
}

fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("invalid BOOL '{raw}': use true or false"),
    }
}

fn print_events(events: &[&Event], empty_msg: &str, show_uid: bool) {
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
    for event in events {
        if show_uid {
            println!(
                "  {}  {}  [{}]",
                fmt_dt(&event.start),
                event.summary,
                event.uid
            );
        } else {
            println!("  {}  {}", fmt_dt(&event.start), event.summary);
        }
    }
}

fn print_grouped_by_day(events: &[&Event]) {
    let mut current: Option<NaiveDate> = None;
    for event in events {
        let day = event.start.date();
        if current != Some(day) {
            current = Some(day);
            println!("  {}", day.format("%a %Y-%m-%d"));
        }
        println!("    {}  {}", event.start.format("%H:%M"), event.summary);
    }
    if events.is_empty() {
        println!("  (empty)");
    }
}

fn fmt_dt(value: &NaiveDateTime) -> String {
    if value.format("%H:%M:%S").to_string() == "00:00:00" {
        value.format("%Y-%m-%d").to_string()
    } else {
        value.format("%Y-%m-%d %H:%M").to_string()
    }
}

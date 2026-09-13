//! Interactive month + agenda view for `oca`.
//!
//! Read-only v1: month grid on the left, selected-day agenda on the right.
//! Compact monochrome, keyboard-first — same feel as `fls` / `msc`.
//! No editing here; use `oca add` / `oca import` for changes.

use std::io::{self, Write};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use crossterm::{
    cursor,
    event::{self, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use optioncalendar_core::{
    DayItem, Event as CalEvent, TaskDue, WeekStart, due_tasks_or_empty, events_on, load_settings,
    occurs_on, save_settings,
};

// ── Palette (flat B&W, no gradients) ────────────────────────────
const DIM: Color = Color::DarkGrey;
const WHITE: Color = Color::White;
const BRIGHT: Color = Color::Rgb {
    r: 245,
    g: 245,
    b: 245,
};
const RULE: Color = Color::Rgb {
    r: 42,
    g: 42,
    b: 42,
};

/// Which side gets arrow-key scrolling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pane {
    #[default]
    Month,
    Agenda,
}

/// Launch the TUI. Returns when the user quits (`q` / `Esc`).
pub fn run(events: Vec<CalEvent>, tasks: Vec<TaskDue>) -> Result<()> {
    let mut app = TuiApp::new(events, tasks);
    let mut ui = TerminalGuard::enter().context("failed to open terminal")?;
    app.draw(&mut ui.out)?;
    loop {
        if event::poll(Duration::from_millis(150)).context("failed to poll input")? {
            match event::read().context("failed to read input")? {
                event::Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    if app.handle_key(key.code, key.modifiers) {
                        break;
                    }
                    app.draw(&mut ui.out)?;
                }
                event::Event::Resize(_, _) => {
                    app.draw(&mut ui.out)?;
                }
                _ => {}
            }
        } else {
            // Re-draw on a slow tick so a resized terminal recovers
            // even without keypresses.
            app.draw(&mut ui.out)?;
        }
    }
    Ok(())
}

/// Load store + tasks and run the TUI (thin entry for `main.rs`).
pub fn launch() -> Result<()> {
    let store = load_store()?;
    // The tasks bridge never fails: missing vault means "no tasks".
    let tasks = due_tasks_or_empty();
    run(store.events, tasks)
}

fn load_store() -> Result<optioncalendar_core::CalStore> {
    let settings = optioncalendar_core::load_settings().context("failed to load settings")?;
    optioncalendar_core::CalStore::open(settings.ics_path).context("failed to open calendar")
}

/// Owns raw mode + alternate screen; restores on drop.
struct TerminalGuard {
    out: io::Stdout,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            cursor::Hide,
            Clear(ClearType::All)
        )?;
        Ok(Self { out })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            self.out,
            cursor::Show,
            LeaveAlternateScreen,
            ResetColor,
            SetAttribute(Attribute::Reset)
        );
        let _ = terminal::disable_raw_mode();
    }
}

struct TuiApp {
    events: Vec<CalEvent>,
    tasks: Vec<TaskDue>,
    today: NaiveDate,
    selected: NaiveDate,
    pane: Pane,
    agenda_scroll: usize,
    show_help: bool,
    /// Mirrors `launch_tui_on_no_args` in config.toml (toggled with `T`).
    launch_on_no_args: bool,
    /// First day of the week (from config.toml, Monday by default).
    week_start: WeekStart,
    /// One-shot notice (e.g. config save failure) shown in the status bar.
    status_note: Option<String>,
}

impl TuiApp {
    fn new(events: Vec<CalEvent>, tasks: Vec<TaskDue>) -> Self {
        let today = optioncalendar_core::today();
        // The TUI stays usable when config is missing or unreadable.
        let settings = load_settings().unwrap_or_default();
        Self {
            events,
            tasks,
            today,
            selected: today,
            pane: Pane::Month,
            agenda_scroll: 0,
            show_help: false,
            launch_on_no_args: settings.launch_tui_on_no_args,
            week_start: settings.week_start,
            status_note: None,
        }
    }

    /// Returns true when the user asked to quit.
    fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        if self.show_help {
            // Any of these closes help; q/Esc also quit from help.
            match code {
                KeyCode::Esc | KeyCode::Char('q') => return true,
                _ => {
                    self.show_help = false;
                    return false;
                }
            }
        }
        match code {
            KeyCode::Esc => return true,
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') => {
                self.show_help = true;
                return false;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.pane = match self.pane {
                    Pane::Month => Pane::Agenda,
                    Pane::Agenda => Pane::Month,
                };
                self.clamp_scroll();
                return false;
            }
            KeyCode::Char('t') if !mods.contains(KeyModifiers::CONTROL) => {
                self.selected = self.today;
                self.agenda_scroll = 0;
                return false;
            }
            KeyCode::Char('T') if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.toggle_launch_on_no_args();
                return false;
            }
            KeyCode::Char('n') | KeyCode::PageDown | KeyCode::Char(']') => {
                self.selected = shift_month(self.selected, 1);
                self.agenda_scroll = 0;
                return false;
            }
            KeyCode::Char('p') | KeyCode::PageUp | KeyCode::Char('[') => {
                self.selected = shift_month(self.selected, -1);
                self.agenda_scroll = 0;
                return false;
            }
            KeyCode::Home => {
                self.selected = self.today;
                self.agenda_scroll = 0;
                return false;
            }
            _ => {}
        }

        match self.pane {
            Pane::Month => match code {
                KeyCode::Left | KeyCode::Char('h') => self.move_days(-1),
                KeyCode::Right | KeyCode::Char('l') => self.move_days(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_days(-7),
                KeyCode::Down | KeyCode::Char('j') => self.move_days(7),
                KeyCode::Enter => {}
                _ => {}
            },
            Pane::Agenda => match code {
                KeyCode::Up | KeyCode::Char('k') => self.scroll_agenda(-1),
                KeyCode::Down | KeyCode::Char('j') => self.scroll_agenda(1),
                KeyCode::Left | KeyCode::Char('h') => self.move_days(-1),
                KeyCode::Right | KeyCode::Char('l') => self.move_days(1),
                KeyCode::Char('g') => {
                    self.agenda_scroll = 0;
                }
                KeyCode::Char('G') => {
                    self.agenda_scroll = usize::MAX;
                    self.clamp_scroll();
                }
                _ => {}
            },
        }
        false
    }

    /// Flip `launch_tui_on_no_args` and persist it; revert + notify on failure.
    fn toggle_launch_on_no_args(&mut self) {
        let next = !self.launch_on_no_args;
        let mut settings = load_settings().unwrap_or_default();
        settings.launch_tui_on_no_args = next;
        if save_settings(&settings).is_err() {
            self.status_note = Some("config save failed".to_string());
            return;
        }
        self.launch_on_no_args = next;
        self.status_note = None;
    }

    fn move_days(&mut self, delta: i64) {
        self.selected = self
            .selected
            .checked_add_signed(chrono::Duration::days(delta))
            .unwrap_or(self.selected);
        self.agenda_scroll = 0;
    }

    fn scroll_agenda(&mut self, delta: i32) {
        let max = self.agenda_max_scroll();
        if delta < 0 {
            self.agenda_scroll = self.agenda_scroll.saturating_sub((-delta) as usize);
        } else {
            self.agenda_scroll = (self.agenda_scroll + delta as usize).min(max);
        }
    }

    fn clamp_scroll(&mut self) {
        let max = self.agenda_max_scroll();
        self.agenda_scroll = self.agenda_scroll.min(max);
    }

    fn agenda_max_scroll(&self) -> usize {
        let rows = agenda_visible_rows();
        let total = self.agenda().len();
        total.saturating_sub(rows.max(1))
    }

    /// Items for the selected day: events on that day plus due tasks.
    ///
    /// On today, overdue tasks are included (same rule as `oca today`).
    /// On other days, only tasks due exactly that day are shown.
    fn agenda(&self) -> Vec<DayItem> {
        agenda_for_day(&self.events, &self.tasks, self.selected, self.today)
    }

    fn view_month(&self) -> (i32, u32) {
        (self.selected.year(), self.selected.month())
    }

    fn draw(&mut self, out: &mut io::Stdout) -> Result<()> {
        let (w, h) = terminal::size().context("failed to read terminal size")?;
        queue!(
            out,
            crossterm::terminal::BeginSynchronizedUpdate,
            cursor::MoveTo(0, 0),
            Clear(ClearType::All)
        )?;
        if w < 70 || h < 16 {
            print_at(out, 2, 2, BRIGHT, true, "◷ optionCalendar")?;
            print_at(
                out,
                2,
                4,
                DIM,
                false,
                "terminal too small · resize to 70×16",
            )?;
            queue!(out, crossterm::terminal::EndSynchronizedUpdate)?;
            out.flush()?;
            return Ok(());
        }

        let margin = 2u16;
        let title = format!("{} optionCalendar", option_sdk::App::CAL.mark());
        print_at(out, margin, 1, BRIGHT, true, &title)?;
        print_right(out, w - margin, 1, DIM, "read-only")?;

        let (vy, vm) = self.view_month();
        let month_name = NaiveDate::from_ymd_opt(vy, vm, 1)
            .map(|d| d.format("%B %Y").to_string())
            .unwrap_or_default();
        let sel = self.selected.format("%a %Y-%m-%d").to_string();
        print_at(out, margin, 2, WHITE, false, &month_name)?;
        print_right(out, w - margin, 2, DIM, &sel)?;
        line(out, margin, 3, w - margin * 2)?;

        // Panes: month grid left, agenda right.
        let month_w: u16 = 30;
        let div_x = margin + month_w + 1;
        let agenda_x = div_x + 2;
        let agenda_w = w.saturating_sub(agenda_x + margin);

        let month_focused = self.pane == Pane::Month;
        let agenda_focused = self.pane == Pane::Agenda;
        let month_head = if month_focused {
            "› month"
        } else {
            "  month"
        };
        let agenda_head = if agenda_focused {
            "› agenda"
        } else {
            "  agenda"
        };
        print_at(
            out,
            margin,
            4,
            if month_focused { BRIGHT } else { DIM },
            month_focused,
            month_head,
        )?;
        print_at(
            out,
            agenda_x,
            4,
            if agenda_focused { BRIGHT } else { DIM },
            agenda_focused,
            agenda_head,
        )?;

        self.draw_month(out, margin, 5)?;
        for y in 4..h.saturating_sub(4) {
            print_at(out, div_x, y, RULE, false, "│")?;
        }
        self.draw_agenda(out, agenda_x, 5, agenda_w, h)?;

        let footer_y = h - 4;
        line(out, margin, footer_y - 1, w - margin * 2)?;
        let mut counts = agenda_summary(&self.events, &self.tasks, self.selected);
        if let Some(note) = &self.status_note {
            counts.push_str(" · ");
            counts.push_str(note);
        }
        print_at(
            out,
            margin,
            footer_y,
            DIM,
            false,
            &truncate(&counts, (w - margin * 2) as usize),
        )?;
        let bare = if self.launch_on_no_args { "on" } else { "off" };
        print_at(
            out,
            margin,
            footer_y + 2,
            DIM,
            false,
            &truncate(
                &format!(
                    "↑↓←→/hjkl move · tab pane · t today · T tui-bare {bare} · n/p month · ? help · q quit"
                ),
                (w - margin * 2) as usize,
            ),
        )?;

        if self.show_help {
            overlay_help(out, w, h, self.launch_on_no_args)?;
        }

        queue!(out, crossterm::terminal::EndSynchronizedUpdate)?;
        out.flush()?;
        Ok(())
    }

    fn draw_month(&self, out: &mut io::Stdout, x: u16, y: u16) -> Result<()> {
        print_at(out, x, y, DIM, false, weekday_header(self.week_start))?;
        let (vy, vm) = self.view_month();
        let weeks = month_weeks(vy, vm, self.week_start);
        for (ri, week) in weeks.iter().enumerate() {
            let row_y = y + 1 + ri as u16;
            let mut col_x = x;
            for day in week {
                let in_month = day.month() == vm;
                let is_selected = *day == self.selected;
                let is_today = *day == self.today;
                let marker = if day_has_items(&self.events, &self.tasks, *day) {
                    "•"
                } else {
                    " "
                };
                let label = format!("{:>2}{}", day.day(), marker);
                if is_selected {
                    selected_cell(out, col_x, row_y, &label)?;
                } else if !in_month {
                    print_at(out, col_x, row_y, RULE, false, &label)?;
                } else if is_today {
                    print_at(out, col_x, row_y, BRIGHT, true, &label)?;
                } else {
                    print_at(out, col_x, row_y, WHITE, false, &label)?;
                }
                col_x += 3;
            }
        }
        Ok(())
    }

    fn draw_agenda(
        &mut self,
        out: &mut io::Stdout,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<()> {
        let items = self.agenda();
        let rows = (height.saturating_sub(y + 5)) as usize;
        let rows = rows.max(1);
        let max = items.len().saturating_sub(rows);
        self.agenda_scroll = self.agenda_scroll.min(max);

        let head = self.selected.format("%a %Y-%m-%d").to_string();
        print_at(out, x, y, WHITE, false, &head)?;

        if items.is_empty() {
            print_at(out, x, y + 2, DIM, false, "nothing on this day")?;
            print_at(
                out,
                x,
                y + 3,
                DIM,
                false,
                &truncate("use `oca add` to add an event", width as usize),
            )?;
            return Ok(());
        }

        for (i, item) in items.iter().skip(self.agenda_scroll).take(rows).enumerate() {
            let row_y = y + 2 + i as u16;
            match item {
                DayItem::Event(event) => {
                    let when = if event.start.date() == self.selected {
                        event.start.format("%H:%M").to_string()
                    } else {
                        event.start.format("%m-%d %H:%M").to_string()
                    };
                    // Timed events show time; midnight means all-day.
                    let when = if when == "00:00" {
                        "all-day".to_string()
                    } else {
                        when
                    };
                    let text = format!("{when}  {}", event.summary);
                    print_at(
                        out,
                        x,
                        row_y,
                        WHITE,
                        false,
                        &truncate(&text, width as usize),
                    )?;
                }
                DayItem::Task(task) => {
                    let overdue = task.due < self.selected;
                    let text = if overdue {
                        format!("[ ] {}  (due {})", task.text, task.due.format("%Y-%m-%d"))
                    } else {
                        format!("[ ] {}", task.text)
                    };
                    print_at(
                        out,
                        x,
                        row_y,
                        WHITE,
                        false,
                        &truncate(&text, width as usize),
                    )?;
                }
            }
        }

        if items.len() > rows {
            let pos = format!("{}/{}", self.agenda_scroll + 1, items.len());
            print_right(out, x + width, y, DIM, &pos)?;
        }
        Ok(())
    }
}

/// Two-letter weekday header for the month grid, ordered by `week_start`.
fn weekday_header(week_start: WeekStart) -> &'static str {
    match week_start {
        WeekStart::Monday => "Mo Tu We Th Fr Sa Su",
        WeekStart::Sunday => "Su Mo Tu We Th Fr Sa",
    }
}

/// Weeks of a month, ordered by `week_start`. Each week has exactly 7 days;
/// edge weeks include days from the next/previous month.
fn month_weeks(year: i32, month: u32, week_start: WeekStart) -> Vec<Vec<NaiveDate>> {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let offset = week_start.days_since_start(first);
    let mut start = first
        .checked_add_signed(chrono::Duration::days(-offset))
        .expect("week start valid");
    let mut weeks = Vec::new();
    loop {
        let week: Vec<NaiveDate> = (0..7)
            .map(|i| {
                start
                    .checked_add_signed(chrono::Duration::days(i))
                    .expect("week day valid")
            })
            .collect();
        weeks.push(week.clone());
        let last = week[6];
        start = last
            .checked_add_signed(chrono::Duration::days(1))
            .expect("next week valid");
        // Stop after the week that contains the last day of the month.
        if last.month() != month && last > first {
            break;
        }
        if weeks.len() >= 6 {
            break;
        }
    }
    weeks
}

/// Same month, same day number clamped to the target month length.
fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let mut y = date.year();
    let mut m = date.month() as i32 + delta;
    while m < 1 {
        m += 12;
        y -= 1;
    }
    while m > 12 {
        m -= 12;
        y += 1;
    }
    let last = last_day_of_month(y, m as u32);
    let d = date.day().min(last);
    NaiveDate::from_ymd_opt(y, m as u32, d).unwrap_or(date)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .expect("valid month")
        .pred_opt()
        .expect("month has a last day")
        .day()
}

fn day_has_items(events: &[CalEvent], tasks: &[TaskDue], day: NaiveDate) -> bool {
    events.iter().any(|e| occurs_on(e, day)) || tasks.iter().any(|t| t.due == day)
}

fn agenda_for_day(
    events: &[CalEvent],
    tasks: &[TaskDue],
    selected: NaiveDate,
    today: NaiveDate,
) -> Vec<DayItem> {
    if selected == today {
        return optioncalendar_core::today_merged(events, tasks, selected);
    }
    let mut items: Vec<DayItem> = events_on(events, selected)
        .into_iter()
        .map(DayItem::Event)
        .collect();
    items.extend(
        tasks
            .iter()
            .filter(|t| t.due == selected)
            .cloned()
            .map(DayItem::Task),
    );
    items.sort_by_key(day_sort_key);
    items
}

fn day_sort_key(item: &DayItem) -> (String, String) {
    match item {
        DayItem::Event(e) => (e.start.format("%H:%M ").to_string(), e.summary.clone()),
        DayItem::Task(t) => ("zz ".to_string(), t.text.clone()),
    }
}

fn agenda_summary(events: &[CalEvent], tasks: &[TaskDue], selected: NaiveDate) -> String {
    let n_events = events.iter().filter(|e| occurs_on(e, selected)).count();
    let n_tasks = tasks.iter().filter(|t| t.due == selected).count();
    let day = selected.format("%Y-%m-%d").to_string();
    match (n_events, n_tasks) {
        (0, 0) => format!("{day} · nothing"),
        (e, 0) => format!("{day} · {e} event{}", plural(e)),
        (0, t) => format!("{day} · {t} task{}", plural(t)),
        (e, t) => format!("{day} · {e} event{} + {t} task{}", plural(e), plural(t)),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn agenda_visible_rows() -> usize {
    terminal::size()
        .map(|(_, h)| h.saturating_sub(12) as usize)
        .unwrap_or(8)
        .max(1)
}

// ── Drawing helpers (same monochrome style as `fls`) ────────────

fn print_at(
    out: &mut impl Write,
    x: u16,
    y: u16,
    color: Color,
    bold: bool,
    text: &str,
) -> Result<()> {
    queue!(
        out,
        cursor::MoveTo(x, y),
        SetForegroundColor(color),
        SetAttribute(if bold {
            Attribute::Bold
        } else {
            Attribute::NormalIntensity
        }),
        Print(text),
        ResetColor,
        SetAttribute(Attribute::Reset)
    )?;
    Ok(())
}

fn print_right(out: &mut impl Write, right: u16, y: u16, color: Color, text: &str) -> Result<()> {
    let w = text.chars().count() as u16;
    let x = right.saturating_sub(w);
    print_at(out, x, y, color, false, text)
}

fn line(out: &mut impl Write, x: u16, y: u16, width: u16) -> Result<()> {
    print_at(out, x, y, RULE, false, &"─".repeat(width as usize))
}

fn selected_cell(out: &mut impl Write, x: u16, y: u16, text: &str) -> Result<()> {
    queue!(
        out,
        cursor::MoveTo(x, y),
        SetForegroundColor(Color::Black),
        SetBackgroundColor(BRIGHT),
        SetAttribute(Attribute::Bold),
        Print(text),
        ResetColor,
        SetAttribute(Attribute::Reset)
    )?;
    Ok(())
}

fn overlay_help(out: &mut impl Write, w: u16, h: u16, launch_on_no_args: bool) -> Result<()> {
    let bare = if launch_on_no_args { "on" } else { "off" };
    let rows = [
        "hjkl / arrows  move selection".to_string(),
        "tab            switch pane".to_string(),
        "t              go to today".to_string(),
        format!("T              bare `oca` opens TUI ({bare})"),
        "n / p          next / prev month".to_string(),
        "pgup / pgdn    prev / next month".to_string(),
        "g / G          top / bottom (agenda)".to_string(),
        "?              close this help".to_string(),
        "q / esc        quit".to_string(),
        "".to_string(),
        "read-only: add events with".to_string(),
        "`oca add \"Title\" --at …`".to_string(),
    ];
    let width = 36.min(w - 4);
    let height = (rows.len() as u16 + 4).min(h - 2);
    let x = (w - width) / 2;
    let y = (h - height) / 2;
    for row in 0..height {
        queue!(
            out,
            cursor::MoveTo(x, y + row),
            SetForegroundColor(Color::Rgb {
                r: 18,
                g: 18,
                b: 18
            }),
            Print(" ".repeat(width as usize))
        )?;
    }
    queue!(out, ResetColor)?;
    print_at(out, x + 2, y + 1, BRIGHT, true, "keyboard")?;
    for (i, row) in rows.iter().enumerate() {
        print_at(out, x + 2, y + 3 + i as u16, DIM, false, row)?;
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    fn dt(raw: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M").unwrap()
    }

    #[test]
    fn month_weeks_cover_september_monday_first() {
        let weeks = month_weeks(2026, 9, WeekStart::Monday);
        // Sep 1 2026 is a Tuesday: first week starts Mon Aug 31.
        assert_eq!(weeks[0][0], NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
        assert!(weeks.iter().all(|w| w.len() == 7));
        assert!(
            weeks
                .concat()
                .contains(&NaiveDate::from_ymd_opt(2026, 9, 30).unwrap())
        );
    }

    #[test]
    fn shift_month_clamps_short_months() {
        let jan31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(
            shift_month(jan31, 1),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
        let mar31 = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        assert_eq!(
            shift_month(mar31, -1),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
    }

    #[test]
    fn agenda_today_includes_overdue_other_days_do_not() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let events = vec![CalEvent::new("Standup", dt("2026-09-04T10:00"), None)];
        let tasks = vec![TaskDue {
            text: "pay bill".into(),
            due: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            source: "tasks/a.md".into(),
        }];
        let items = agenda_for_day(&events, &tasks, today, today);
        assert_eq!(items.len(), 2);
        let other = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
        let items = agenda_for_day(&events, &tasks, other, today);
        assert!(items.is_empty());
    }
}

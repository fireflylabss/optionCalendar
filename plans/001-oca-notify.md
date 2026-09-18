# Plan 001: Add `oca notify` — one-shot desktop reminders driven by systemd/cron

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat ce676b6..HEAD -- crates/ packaging/ README.md CHANGELOG.md AGENTS.md Cargo.toml`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `ce676b6`, 2026-09-16

## Why this matters

optionCalendar stores events but can never *surface* them — a calendar without
reminders misses its core job. The family rule is "no daemon", so the design is
a **one-shot command** (`oca notify`) that a `systemd --user` timer or cron
entry runs periodically; systemd is the supervisor, not a custom daemon. The
command lists event occurrences starting within a lookahead window and, with
`--send`, fires `notify-send` per occurrence while recording what was already
sent so a timer firing every 5 minutes doesn't spam.

## Current state

Repo: Rust workspace, edition 2024, rust-version 1.85. Two crates:

- `crates/optioncalendar-core/` — `ics.rs` (VEVENT parse/serialize), `store.rs`
  (`CalStore`, `Settings`), `query.rs` (day/week/month queries, recurrence
  expansion), `tasks.rs` (optionNotes bridge), `week.rs` (`WeekStart`),
  `error.rs`, `lib.rs` (re-exports).
- `crates/optioncalendar-cli/` — `src/main.rs` (clap CLI, all `cmd_*` handlers,
  private `fmt_dt`/`fmt_time` helpers, `#[cfg(test)] mod tests`), `src/tui.rs`,
  `tests/cli.rs` (assert_cmd integration tests sandboxed by `OPTION_HOME`).

Key facts the executor must work with:

**`Event`** (`crates/optioncalendar-core/src/ics.rs:16-42`) already carries
everything needed — no `ics.rs` changes:

```rust
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,   // exclusive for all_day
    pub all_day: bool,
    pub rrule: Option<String>,
    pub extra: Vec<(String, String)>, // uninterpreted lines (VALARM preserved here)
    pub start_raw: Option<(String, String)>,
    pub end_raw: Option<(String, String)>,
}
```

**Recurrence expansion exists** (`query.rs:185`): `events_between(events,
start: NaiveDate, end: NaiveDate) -> Vec<Event>` returns occurrences sorted by
start; recurring events are cloned with shifted `start`/`end` and the original
`uid`. A dedup key must therefore be `uid + occurrence.start`, not `uid` alone.

**`Settings`** (`store.rs:14-35`) — TOML-backed, old configs must keep loading,
so new fields need `#[serde(default ...)]`:

```rust
pub struct Settings {
    pub ics_path: PathBuf,
    #[serde(default)]
    pub launch_tui_on_no_args: bool,
    #[serde(default)]
    pub week_start: WeekStart,
}
```

**CLI shape** (`main.rs:62-158`): `enum Commands` derives `clap::Subcommand`;
global `--json` flag on `Cli` (`main.rs:56`); dispatch in `run()`
(`main.rs:187-227`). `open_store()` (`main.rs:231`) loads `Settings` then opens
`CalStore`. `App::CAL` (optionSDK) gives `~/.option/cal` paths —
`App::CAL.path("notified")` yields `~/.option/cal/notified`, and
`option_sdk::atomic_write(path, bytes)` persists safely (see `store.rs:77`).

**Output conventions**: lines start with `App::CAL.mark()` (`◷`), e.g.
`println!("{} next · {}  {}", mark, fmt_dt(&event.start), event.summary)`.
Errors go through `anyhow` with `.context(...)` / `bail!`. `fmt_dt` prints
`YYYY-MM-DD HH:MM` (or just the date at midnight); `fmt_time` prints `HH:MM` or
`all-day` (`main.rs:741-756`). `edit_json.rs`/`cli.rs` tests sandbox with
`OPTION_HOME=<tempdir>` + `NO_COLOR=1` + `env_remove("HOME")`.

**Config command** (`main.rs:625-661`): `cmd_config` prints current values and
persists `--launch-tui-on-no-args` / `--week-start` via `save_settings`.

## Commands you will need

Run from the repo root (`optionCalendar/`). `optionSDK` is a path dependency at
`../optionSDK` — it must exist as a sibling checkout.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Build | `export CARGO_TARGET_DIR="$(pwd)/target" && cargo build -p optioncalendar-cli` | exit 0 |
| Core tests | `cargo test -p optioncalendar-core` | all pass |
| CLI tests | `cargo test -p optioncalendar-cli` | all pass |
| Lint | `cargo clippy --all-targets -- -D warnings` | exit 0, no warnings |
| Format | `cargo fmt --all` then `cargo fmt --check` | exit 0 |
| Smoke | `OPTION_HOME=/tmp/oca-smoke ./target/debug/oca add "Test" --at 2026-09-10T10:00 && OPTION_HOME=/tmp/oca-smoke ./target/debug/oca ls` | prints `added`/`1 event` |

## Scope

**In scope** (the only files you should modify or create):

- `crates/optioncalendar-core/src/notify.rs` (create)
- `crates/optioncalendar-core/src/lib.rs` (add `pub mod notify` + re-exports)
- `crates/optioncalendar-core/src/store.rs` (one new `Settings` field)
- `crates/optioncalendar-cli/src/main.rs` (new subcommand, handler, config flag)
- `crates/optioncalendar-cli/tests/cli.rs` (new integration tests)
- `packaging/systemd/optioncalendar-notify.service` (create)
- `packaging/systemd/optioncalendar-notify.timer` (create)
- `README.md` (usage docs)
- `CHANGELOG.md` (new `v0.1.2-beta` entry)
- `Cargo.toml` (workspace version bump to `0.1.2`)
- `AGENTS.md` (short architecture/config notes — this file is in Portuguese;
  match its language)

**Out of scope** (do NOT touch, even though they look related):

- `crates/optioncalendar-core/src/ics.rs` — VALARM `TRIGGER` offsets stay
  preserved-but-uninterpreted in `Event::extra`; honoring per-event alarms is a
  separate follow-up, not this plan.
- `crates/optioncalendar-core/src/tasks.rs` — notes tasks have due *dates*, not
  times; surfacing them in `notify` is a follow-up.
- `src/tui.rs`, `packaging/aur/*`, `.github/workflows/*` — unchanged.
- No new crates/dependencies. The notifier is an external command
  (`notify-send`), spawned per event.

## Git workflow

- Branch: `advisor/001-oca-notify`
- Commit style observed in `git log`: short imperative messages, optionally
  scoped (`packaging/aur: bump to 0.1.1`, `feat: doctor req/opc`). One or two
  commits is fine.
- Do NOT push, tag, or open a PR unless the operator instructed it. Do NOT run
  `packaging/aur/bump.sh` or `publish.sh`.

## Steps

### Step 1: `notify` module in optioncalendar-core

Create `crates/optioncalendar-core/src/notify.rs`:

- `pub fn due_events(events: &[Event], now: NaiveDateTime, window: Duration, late: Duration) -> Vec<Event>`
  - Expand occurrences with `events_between(events, (now - late).date(), (now + window).date())`.
  - Keep occurrences where `!e.all_day` and `start > now - late && start <= now + window`.
    (`late` covers events that just started while the machine was asleep or the
    timer drifted; a `10-minute` constant is fine — name it
    `pub const DEFAULT_LATE: Duration`.)
- `pub fn notification_key(event: &Event) -> String` →
  `format!("{}\t{}", event.uid, crate::ics::format_dt(&event.start))`
  (uses occurrence `start`, so recurring instances dedup independently).
- `pub fn load_notified(path: &Path) -> HashSet<String>` — read `path` if it
  exists (missing → empty set), split lines, keep non-empty lines verbatim as
  keys. Tolerant: never fails on malformed content.
- `pub fn save_notified(path: &Path, keys: &HashSet<String>, now: NaiveDateTime) -> Result<()>`
  — prune keys whose `start` part (the `\tYYYYMMDDTHHMMSS` half, parsed with
  `parse_dt`) is older than `now - Duration::days(2)`, sort remaining keys for
  stable output, write one per line via `option_sdk::atomic_write` mapped
  through `Error::Io` (mirror `store.rs:77`).

In `lib.rs`: add `pub mod notify;` to the module block and re-export
`due_events, notification_key, load_notified, save_notified` (and
`DEFAULT_LATE` if declared `pub`) in the `pub use` section, matching the
existing grouped style.

In `store.rs` `Settings`, add:

```rust
/// Lookahead window for `oca notify`, in minutes.
#[serde(default = "default_notify_window_minutes")]
pub notify_window_minutes: u32,
```

plus `fn default_notify_window_minutes() -> u32 { 15 }` and the same field in
`impl Default for Settings`. Old configs without the field must still load —
that is what `#[serde(default = ...)]` is for; add a regression test mirroring
`launch_flag_defaults_false_for_old_configs` (`store.rs:317`).

**Verify**: `cargo test -p optioncalendar-core` → all existing + new tests pass.

### Step 2: `oca notify` subcommand

In `crates/optioncalendar-cli/src/main.rs`:

- Add to `enum Commands` (clap derive, after `Next` reads naturally):

```rust
/// Show events starting soon; --send fires desktop notifications
Notify {
    /// Lookahead window in minutes (default: config notify_window_minutes, 15)
    #[arg(long, value_name = "MIN")]
    window: Option<u32>,
    /// Run the notifier for each due occurrence and remember what was sent
    #[arg(long)]
    send: bool,
},
```

- Add `notify_window_minutes: Option<String>` to `Commands::Config` and persist
  it in `cmd_config` (parse as `u32`, `bail!("invalid MIN '{raw}': use a positive number of minutes")`
  on failure or `0`), printed as `notify_window_minutes = N` — same pattern as
  `week_start`.
- New handler `cmd_notify(window: Option<u32>, send: bool, json: bool)`:
  1. `let settings = load_settings()?; let store = open_store()?;`
  2. `let window = Duration::minutes(window.or(settings.notify_window_minutes.into()) ... )`
     — i.e. CLI flag wins, else config, else 15.
  3. `let now = Local::now().naive_local();`
  4. `let due = due_events(&store.events, now, window, DEFAULT_LATE);`
  5. Without `--send`: `json` → `print_json(&due)`; else print
     `{mark} {n} due within {mins}m` then one `  HH:MM  summary` line each
     (reuse `fmt_time`), or `{mark} nothing due within {mins}m` when empty.
     **No state is written.**
  6. With `--send`: load `load_notified(&App::CAL.path("notified"))`, filter
     `due` to keys not in the set; for each remaining event run
     `Command::new(notifier)` where `notifier =
     std::env::var("OCA_NOTIFY_CMD").unwrap_or_else(|_| "notify-send".into())`
     with args `format!("◷ {}", event.summary)` as title and
     `format!("{}", fmt_dt(&event.start))` (append `"  " + &event.description`
     when non-empty) as body. `.status()` it; on spawn failure
     `.with_context(|| format!("cannot run '{notifier}'"))?`. Insert each sent
     `notification_key` into the set; after the loop `save_notified` and print
     `{mark} sent {n} reminder{s}` (or `{mark} nothing to send` when the
     filtered list is empty — still exit 0).
- Wire the match arm in `run()` and a one-line mention in the `long_about`
  help block next to the `tui` line.

`OCA_NOTIFY_CMD` exists so tests (and users with a different notifier, e.g. a
script) can substitute the binary; document it in the code comment and README.

**Verify**: `cargo build -p optioncalendar-cli && OPTION_HOME=/tmp/oca-n ./target/debug/oca notify` → prints `nothing due within 15m` (or due items), exit 0.

### Step 3: systemd user units + docs

Create `packaging/systemd/optioncalendar-notify.service`:

```ini
[Unit]
Description=optionCalendar reminders (oca notify --send)

[Service]
Type=oneshot
ExecStart=/usr/bin/oca notify --send
```

and `packaging/systemd/optioncalendar-notify.timer`:

```ini
[Unit]
Description=Fire optionCalendar reminders every 5 minutes

[Timer]
OnCalendar=*:0/5
Persistent=true

[Install]
WantedBy=timers.target
```

`Persistent=true` catches up runs missed while suspended/off (the `late`
window covers short gaps). Note in the service file comment that non-AUR
installs should adjust `ExecStart` to where `oca` lives (e.g.
`%h/.cargo/bin/oca`).

README.md: in the quick-start command list add `./target/debug/oca notify` and
`./target/debug/oca notify --send`, plus a short "Reminders" section showing
`systemctl --user enable --now` after copying the units to
`~/.config/systemd/user/`, and documenting `--window`, the
`notify_window_minutes` config key, and `OCA_NOTIFY_CMD`.

**Verify**: `systemd-analyze --user verify packaging/systemd/optioncalendar-notify.{service,timer}` (if available) → no errors; otherwise skip and note it.

### Step 4: tests

Core (`notify.rs` `#[cfg(test)]`, modeled on `query.rs` tests — build events
with `Event::new`, fixed `NaiveDateTime` "now"):

- timed event inside the window is due; outside (earlier than `-late` or later
  than `+window`) is not; boundary `start == now + window` is due.
- `all_day` events are never due.
- an `RRULE` daily event yields an occurrence due on a later day with shifted
  `start` and same `uid`; `notification_key` differs between occurrences.
- `load_notified` on a missing file → empty; garbage lines tolerated.
- `save_notified` round-trips through `load_notified`, sorts output, and prunes
  keys older than 2 days.
- `store.rs`: old config without `notify_window_minutes` loads with `15`.

CLI (`tests/cli.rs`, existing `Sandbox` pattern — `OPTION_HOME` tempdir,
`NO_COLOR=1`, `env_remove("HOME")`):

- `notify` lists an event added at `now + 5m` (`at(0, ...)` helper builds
  `YYYY-MM-DDTHH:MM` — you may need a helper that offsets *minutes*, not days;
  add one) and omits one at `now + 2h` under `--window 15`.
- `--json notify` emits a JSON array containing the summary.
- `--send` with `OCA_NOTIFY_CMD` pointing at a test script (`#!/bin/sh` writing
  `"$@"` to a file inside the sandbox, `chmod +x` via `std::fs` +
  `PermissionsExt`) sends once; a second `notify --send` prints
  `nothing to send` and the file still has exactly one line.
- `notify --send` twice on a fresh event with `--window 0`... skip this — use
  the dedup test above instead.

**Verify**: `cargo test --workspace` → all pass.

### Step 5: changelog, version, AGENTS.md

- `Cargo.toml` `[workspace.package] version`: `0.1.1` → `0.1.2`.
- `CHANGELOG.md`: prepend `## v0.1.2-beta · DD/MM/YYYY` (today's date,
  DD/MM/YYYY) with a one-line summary ending in the conventional sentence
  ("This version was made for CLI with a beta release channel on …
  (v0.1.2-beta).") and bullets covering: `oca notify [--window MIN] [--send]`,
  dedup state at `~/.option/cal/notified`, `notify_window_minutes` config,
  systemd user units under `packaging/systemd/`, `OCA_NOTIFY_CMD`.
- `AGENTS.md` (Portuguese — match it): add `notify.rs` to the architecture
  list, `notify_window_minutes` + `~/.option/cal/notified` to the config/state
  notes, and a line under "Detalhes que já morderam" style notes: dedup key is
  `uid + occurrence start`, state is only written by `--send`.

**Verify**: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace` → all green.

## Test plan

Covered in Step 4. Structural patterns: core tests model after
`crates/optioncalendar-core/src/query.rs` `mod tests`; CLI tests model after
`crates/optioncalendar-cli/tests/cli.rs` (`Sandbox`, `at()` helper,
`assert_cmd` + `predicates::str::contains`).

Verification: `cargo test --workspace` → all pass, including the new tests.

## Done criteria

- [ ] `cargo fmt --check` exits 0
- [ ] `cargo clippy --all-targets -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0; new notify tests exist and pass
- [ ] `OPTION_HOME=/tmp/oca-n ./target/debug/oca notify --send` with
      `OCA_NOTIFY_CMD` set to a script sends once and a repeat run prints
      `nothing to send`
- [ ] `~/.option/cal/notified` (or `$OPTION_HOME/cal/notified`) contains
      `uid<TAB>YYYYMMDDTHHMMSS` lines only after `--send`
- [ ] `oca config` prints `notify_window_minutes = 15` on a fresh config;
      `oca config --notify-window-minutes 30` persists
- [ ] `packaging/systemd/` contains the service + timer; README documents them
- [ ] `CHANGELOG.md` has the `v0.1.2-beta` entry; `Cargo.toml` is `0.1.2`
- [ ] No files outside the in-scope list are modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The code at the locations in "Current state" doesn't match the excerpts
  (the codebase has drifted since `ce676b6`).
- A step's verification fails twice after a reasonable fix attempt.
- The work appears to require touching an out-of-scope file (e.g. `ics.rs` —
  `Event` already has every field this feature needs).
- `events_between` turns out not to expand recurring occurrences with shifted
  `start` (it does per `query.rs:136-170`; if that changes, the dedup key
  design must be revisited — STOP).

## Maintenance notes

- **VALARM**: ICS `VALARM`/`TRIGGER` blocks already survive the load/save cycle
  via `Event::extra`. A natural follow-up is honoring `TRIGGER:-PTnM` as a
  per-event override of the global window — that changes `due_events`'s
  signature/semantics, so anyone touching `notify.rs` should check whether
  VALARM support has landed first.
- **Tasks**: `tasks.rs` due dates (date granularity) are intentionally not
  notified. If they ever are, decide whether "due today" fires once at a
  configurable morning hour — don't just fold them into the minute window.
- **All-day events** are skipped in v1 (a 00:00 notification is noise). If
  requested later, a `notify_all_day_at` config (e.g. `08:00`) is the shape to
  prefer over widening the window.
- **AUR**: the units are user-installed, not packaged. A PKGBUILD follow-up
  could `install -Dm644` them into `/usr/lib/systemd/user/`; coordinate with
  the `option-aur-packaging` skill conventions before doing so.
- Reviewer scrutiny: that `--send` is the *only* writer of the `notified`
  state file, and that no code path shells out except the single
  `notify-send`/`OCA_NOTIFY_CMD` spawn (never through a shell — args are passed
  directly to `Command`).

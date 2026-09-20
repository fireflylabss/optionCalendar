# optionCalendar

Minimal local calendar for the **Option** family — one ICS file on disk,
day/week/month views, and an optional bridge to Markdown task lists
(`~/Documents/Notes/tasks/*.md`) — via the `oca` CLI.

```text
◷ optionCalendar
```

| Binary | Role |
|--------|------|
| `optioncalendar` | CLI (canonical) |
| `oca` | CLI (short, same entrypoint) |

Config: `~/.option/cal/config.toml`. Calendar: `~/.option/cal/calendar.ics`.

## Product

Quiet local calendar — events as ICS, no accounts, no sync. See [`PRODUCT.md`](PRODUCT.md).

## Workspace

```text
crates/
  optioncalendar-core/   # ICS parse/serialize, file store, queries, tasks bridge
  optioncalendar-cli/    # optioncalendar · oca
packaging/               # AUR (packaging/aur)
```

## Prerequisites

- Rust stable (1.85+)

## Quick start

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo build -p optioncalendar-cli

./target/debug/oca add "Dentist" --at 2026-09-10T10:00 --description "cleaning"
./target/debug/oca ls                 # numbered: `  1  2026-09-10 10:00  Dentist`
./target/debug/oca ls --uid
./target/debug/oca today              # or: oca today 2026-10-01
./target/debug/oca week               # or: oca week 2026-10-01 (7 days from that date)
./target/debug/oca month              # or: oca month 2026-10 / 2026-10-01
./target/debug/oca next
./target/debug/oca search dentist
./target/debug/oca edit 1 --summary "Dentist (moved)" --at 2026-09-11T11:00 --end 2026-09-11T12:00
./target/debug/oca --json ls          # also today/week/month/next/search
./target/debug/oca import backup.ics
./target/debug/oca export backup.ics
./target/debug/oca rm 1               # index from `oca ls`, or a UID
./target/debug/oca notify             # events starting within the next 15m
./target/debug/oca notify --send      # fire notify-send once per due event
./target/debug/oca tui
./target/debug/oca config
./target/debug/oca config --week-start sunday --launch-tui-on-no-args true
```

`oca add "X" --at 2026-09-10` (date only) creates an all-day event; `--end`
is the inclusive last day. Imported ICS files keep everything optionCalendar
does not interpret (LOCATION, VALARM, TZID params, X-props…) across saves, and
simple `RRULE`s (`FREQ=DAILY|WEEKLY|MONTHLY|YEARLY` with `INTERVAL`, `COUNT`,
`UNTIL`) are expanded in `today`/`week`/`month`/`tui`.

`oca edit <id>` takes a UID or a 1-based index from `oca ls --uid` and any of
`--summary`, `--at`, `--end`, `--description`, `--clear-end`,
`--clear-description`; the UID never changes and `end >= start` is enforced.

The global `--json` flag makes `ls`, `today`, `week`, `month`, `next` and
`search` print a JSON array of `{uid, summary, description, start, end, all_day, rrule}`
(ISO 8601 `YYYY-MM-DDTHH:MM:SS`, `end` may be `null`) with no mark or colour.
`today`, `week` and `month` with `--json` tag each item with `kind`: `"event"` as
above, or `"task"` with `{text, due, source}`.

`week_start` (`monday` default, ISO 8601, or `sunday`) in
`~/.option/cal/config.toml` controls the first day of the week in the TUI grid;
set it with `oca config --week-start monday|sunday`.

`search` is case-insensitive, including non-ASCII (`REUNIÃO` matches `reunião`).

Tasks from `~/Documents/Notes/tasks/*.md` with `- [ ] text due:YYYY-MM-DD`
appear in `today` (due or overdue) and in `week`/`month` (due inside the
window, grouped on the due day); a missing vault simply means no tasks.
All-day events show as `all-day` in the `week`/`month` day groups.

`oca tui` opens the month + agenda view (arrows/hjkl move, Tab switches
pane, `T` toggles TUI-on-bare-invocation, q quits; read-only).

## Reminders

`oca notify` is a one-shot reminder pass — no daemon. Without flags it lists
timed event occurrences starting within the lookahead window (15 minutes by
default, or `--window MIN`); with `--send` it runs `notify-send` once per due
occurrence and records what it sent in `~/.option/cal/notified`, so frequent
runs never repeat. A short lookback (10 min) still catches events that began
while the machine was suspended. All-day events are not notified.

Run it on a schedule with the bundled systemd user units:

```bash
cp packaging/systemd/optioncalendar-notify.{service,timer} ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now optioncalendar-notify.timer
```

The service calls `/usr/bin/oca` (the AUR path); adjust `ExecStart` if `oca`
lives elsewhere (e.g. `%h/.cargo/bin/oca`). The timer fires every 5 minutes
and is `Persistent`, so runs missed while suspended or off catch up. Set
`notify_window_minutes` with `oca config --notify-window-minutes N`, and
`OCA_NOTIFY_CMD` to swap `notify-send` for another notifier.

Bare `oca` prints help. Opt in to TUI-on-bare via
`oca config --launch-tui-on-no-args true` (stored as
`launch_tui_on_no_args` in `~/.option/cal/config.toml`, default off);
`oca config` prints the current settings.

## After every code change

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p optioncalendar-cli
```

## CI / Releases

- `ci.yml` runs fmt, clippy (`-D warnings`), tests and a release build on every
  push to `main` and every PR. `optionSDK` is cloned into `../optionSDK` at the
  ref set by `OPTIONSDK_REF` in the workflow.
- `release.yml` runs on `v*` tags: publishes a GitHub Release with a Linux
  x86_64 tarball (`optioncalendar` + `oca`) and, once `OPTIONSDK_REF` points at
  an optionSDK tag, runs `packaging/aur/bump.sh`.
  Pushing to the AUR requires the `AUR_SSH_KEY` repository secret (private SSH
  key registered on aur.archlinux.org); without it that step is skipped and
  `packaging/aur/publish.sh` can be run locally.

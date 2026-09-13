# optionCalendar

Minimal local calendar for the **Option** family — one ICS file on disk,
day/week/month views, and an optional bridge to optionNotes tasks — via the
`oca` CLI.

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
./target/debug/oca import backup.ics
./target/debug/oca export backup.ics
./target/debug/oca rm 1               # index from `oca ls`, or a UID
./target/debug/oca tui
./target/debug/oca config
./target/debug/oca config --week-start sunday --launch-tui-on-no-args true
```

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

Bare `oca` prints help. Opt in to TUI-on-bare via
`oca config --launch-tui-on-no-args true` (stored as
`launch_tui_on_no_args` in `~/.option/cal/config.toml`, default off);
`oca config` prints the current settings.

## After every code change

```bash
export CARGO_TARGET_DIR="$(pwd)/target"
cargo fmt --check
cargo test -p optioncalendar-core
cargo build -p optioncalendar-cli
```

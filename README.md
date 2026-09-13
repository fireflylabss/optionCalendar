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
./target/debug/oca ls --uid
./target/debug/oca today
./target/debug/oca week
./target/debug/oca month
./target/debug/oca next
./target/debug/oca search dentist
./target/debug/oca import backup.ics
./target/debug/oca export backup.ics
./target/debug/oca rm 1
./target/debug/oca tui
./target/debug/oca config
```

`week_start` (`monday` default, ISO 8601, or `sunday`) in
`~/.option/cal/config.toml` controls the first day of the week in the TUI grid.

Tasks from `~/Documents/Notes/tasks/*.md` with `- [ ] text due:YYYY-MM-DD`
appear in `today` automatically; a missing vault simply means no tasks.

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
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build -p optioncalendar-cli
```

## CI / Releases

- `ci.yml` runs fmt, clippy (`-D warnings`), tests and a release build on every
  push to `main` and every PR. `optionSDK` is cloned into `../optionSDK` at the
  ref set by `OPTIONSDK_REF` in the workflow.
- `release.yml` runs on `v*` tags: publishes a GitHub Release with a Linux
  x86_64 tarball (`optioncalendar` + `oca`) and runs `packaging/aur/bump.sh`.
  Pushing to the AUR requires the `AUR_SSH_KEY` repository secret (private SSH
  key registered on aur.archlinux.org); without it that step is skipped and
  `packaging/aur/publish.sh` can be run locally.

# Changelog

We follow [Semantic Versioning](https://semver.org/) and [Keep a Changelog](https://keepachangelog.com/). optionCalendar is a single CLI surface.

<details>
<summary>To see more about versioning, expand this.</summary>

Every version string starts with `v` (required), e.g. `v0.1.0-stable`.

Here the installable surface is **CLI** (`optioncalendar` / `oca`) in the terminal. Other Option apps swap in their own names the same way — e.g. **GNOME**, **Desktop**, **Web** — whatever you actually ship.

| Part | What you install | Example |
| --- | --- | --- |
| **CLI** | `optioncalendar` / `oca` in the terminal | `v0.1.0-stable` |

With one surface there is no `m` in the tag and no per-surface sections — just the version notes.

Each release heading is the version and date (`## v0.1.0-stable · 08/09/2026`); under it, a short summary ends with a plain sentence like: "This version was made for CLI with a stable release channel on 08/09/2026 (v0.1.0-stable)."

### What the channel suffix means

| Suffix | In plain words |
| --- | --- |
| **-alpha** | Very early. Expect missing pieces and lots of bugs. |
| **-beta** | Mostly there, but still rough. Fine to try; not the "official" install. |
| **-stable** | Ready for daily use. This is what we put on GitHub Releases and the AUR. |

We only call something **stable** when we mean it.

</details>

## Unreleased

- CI workflow (`.github/workflows/ci.yml`): fmt, clippy `-D warnings`, tests and release build on pushes to `main` and PRs; `optionSDK` cloned at `OPTIONSDK_REF`.
- Release workflow (`.github/workflows/release.yml`): on `v*` tags builds the Linux x86_64 tarball, publishes a GitHub Release, runs `packaging/aur/bump.sh`, and pushes to the AUR when the `AUR_SSH_KEY` secret exists.
- `edit <id> [--summary S] [--at START] [--end END] [--description D] [--clear-end] [--clear-description]` edits an event by UID or 1-based index; the UID never changes, `end >= start` is validated, and no flags is an error.
- Global `--json` flag for `ls`, `today`, `week`, `month`, `next`, `search`: stable JSON array of `{uid, summary, description, start, end}` (ISO 8601); `today` items carry `kind: "event" | "task"`.
- `CalStore::update(uid, f)` in core; `Event` and `TaskDue` implement `Serialize`.
- CLI integration tests (`tests/edit_json.rs`) for `edit` and `--json`.
- CLI integration tests (`crates/optioncalendar-cli/tests/cli.rs`, `assert_cmd` + `predicates`) covering help, add/ls, today/week/month/next/search, rm by index and UID, import/export roundtrip and `config --launch-tui-on-no-args`.
- `load_settings_from(path)` / `save_settings_to(path, &Settings)` in core; `load_settings`/`save_settings` are thin wrappers. The settings unit test no longer mutates `OPTION_HOME`.
- Renamed internal `blake_like` to `generate_uid` (it is FNV-1a + wall-clock nanos).
- Fixed `cargo test --workspace` (stale `month_weeks` test call) and clippy `-D warnings` lints on recent toolchains.
- `ls` numbers each line with the 1-based index `rm` accepts (with and without `--uid`).
- `config` prints `week_start` and accepts `--week-start monday|sunday`; flags can be combined in one call.
- `search` is Unicode case-insensitive (`REUNIÃO` matches `reunião`).
- `today`, `week` and `month` take an optional positional date (`oca today 2026-10-01`, `oca week 2026-10-01`, `oca month 2026-10`); invalid input is a clear error.
- Tasks due inside the window now appear in `week` and `month`, grouped on their due day as `[ ] text` (core: `merged_between`).
- `week`/`month` show `all-day` instead of `00:00` for midnight/all-day events.

## v0.1.0-stable · 08/09/2026

First stable cut: ICS calendar CLI with day/week/month views, tasks bridge, and a read-only TUI. This version was made for CLI with a stable release channel on 08/09/2026 (v0.1.0-stable).

- Minimal ICS (RFC 5545 subset): VEVENT UID/DTSTART/DTEND/SUMMARY/DESCRIPTION parse + serialize over `~/.option/cal/calendar.ics`.
- Commands: `add <summary> --at … [--end …] [--description …]`, `ls [--uid]`, `today`, `week`, `month`, `next`, `search <text>`, `rm <id>`, `import <file.ics>`, `export <file.ics>`, `tui`, `config`.
- `rm` accepts a UID or a 1-based index from `oca ls --uid`.
- `next` shows the next upcoming event with days-until.
- `search` is case-insensitive over summary and description.
- `export` writes the calendar to an ICS file (inverse of `import`).
- Optional tasks bridge: `- [ ] … due:YYYY-MM-DD` from `~/Documents/Notes/tasks/*.md` merged into `today`, never failing when missing.
- Read-only `tui` month + agenda view (arrows/hjkl, Tab pane, `t` today, `n/p` month, `?` help, `q` quit).
- `week_start` setting (`monday` default, ISO 8601, or `sunday`) in `~/.option/cal/config.toml` controls the TUI grid order.
- Opt-in `launch_tui_on_no_args` (default off): bare `oca` prints help unless enabled via `oca config`.
- AUR package `optioncalendar` with `bump.sh` and `publish.sh` helpers.
- Various other small tweaks

## v0.1.0-alpha · 04/09/2026

First scaffold of optionCalendar: minimal ICS calendar CLI. This version was made for CLI with an alpha release channel on 04/09/2026 (v0.1.0-alpha).

- ICS minimal: VEVENT UID/DTSTART/DTEND/SUMMARY/DESCRIPTION parse + serialize over `~/.option/cal/calendar.ics`.
- Commands: `add <summary> --at … [--end …]`, `ls`, `today`, `week`, `month`, `import <file.ics>`.
- Optional tasks bridge: `- [ ] … due:YYYY-MM-DD` from `~/Documents/Notes/tasks/*.md` merged into `today`, never failing when missing.

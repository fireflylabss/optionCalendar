# Versioning

This project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with an explicit **release channel** suffix, and [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## Surface

optionCalendar is a single surface (CLI). Changelog headings use the channel suffix, e.g. `## v0.1.0-stable · DD/MM/YYYY`.

`Cargo.toml` / git tags keep the numeric version (`0.1.0`, `v0.1.0`) — the channel lives in the changelog (and packaging notes), matching the Option family convention.

## Release channels (`x.y.z-<channel>`)

| Channel | Tag example | Meaning |
|---------|-------------|---------|
| **alpha** | `0.1.0-alpha` | Extremely early. Features incomplete; bugs are expected and common. |
| **beta** | `0.2.0-beta` | Feature set nearly complete, but still rough — bugs and hard edges remain. |
| **stable** | `0.1.0-stable` | Production-ready: finished for that version, few or no known bugs. |

Do **not** label something `stable` unless it is actually release-ready. Prefer **beta** while a large rewrite is settling; use **alpha** only for brand-new / half-built surfaces.

Alpha/beta cuts are normally changelog + local/dev artifacts — not GitHub Release / AUR — unless explicitly promoted to **stable**.

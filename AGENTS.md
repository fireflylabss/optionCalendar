# optionCalendar — notas para agentes

## Build
- Rust stable (1.85+). Sem dependências de sistema além da toolchain.
- `cargo build --release` ou `cargo build -p optioncalendar-cli`.
- `CARGO_TARGET_DIR` deve apontar para `$(pwd)/target` nos scripts.
- `optionSDK` é path dep (`../optionSDK`); o PKGBUILD resolve via download separado.
- Smoke test: `OPTION_HOME=/tmp/oca-smoke ./target/debug/oca add "Test" --at 2026-09-10T10:00 && ./target/debug/oca ls --uid`.
- Testes: `cargo test --workspace` (sem rede) — 19 unitários no core, 3 no TUI e
  21 de integração do CLI em `crates/optioncalendar-cli/tests/cli.rs` (+4 em `tests/edit_json.rs` para `edit`/`--json`)
  (`assert_cmd` + `predicates`; cada teste roda `oca` com `OPTION_HOME` num tempdir).
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`.

## Arquitetura
- `crates/optioncalendar-core/` — ICS parse/serialize, file store, queries, tasks bridge, `WeekStart`.
  - `ics.rs` — VEVENT minimal (UID/DTSTART/DTEND/SUMMARY/DESCRIPTION). Unknown props ignorados.
  - `store.rs` — `CalStore` (um ICS file), `Settings` (ics_path, launch_tui_on_no_args, week_start).
    `load_settings`/`save_settings` são wrappers de `load_settings_from(path)`/`save_settings_to(path, &Settings)`;
    testes usam as versões com path (nunca mexa em `OPTION_HOME` via `std::env` em teste unitário).
  - `query.rs` — day/week/month queries, `today_merged` (events + tasks).
  - `tasks.rs` — bridge optionNotes: `- [ ] text due:YYYY-MM-DD` de `~/Documents/Notes/tasks/*.md`.
  - `week.rs` — `WeekStart` (Monday default ISO 8601, ou Sunday).
- `crates/optioncalendar-cli/` — `oca` / `optioncalendar` (mesmo entrypoint).
  - `main.rs` — clap CLI: add, ls, today, week, month, next, search, edit, rm, import, export, tui, config.
    Flag global `--json` (ls/today/week/month/next/search) imprime array JSON estável em stdout.
  - `tests/edit_json.rs` — testes de integração de `edit` e `--json`.
  - `tui.rs` — month + agenda view read-only (crossterm). Grid segue `week_start`.

## Config
- `~/.option/cal/config.toml` — ics_path, launch_tui_on_no_args, week_start.
- `~/.option/cal/calendar.ics` — único arquivo de calendário.
- `OPTION_HOME` tem precedência sobre `HOME` (optionSDK).

## Empacotamento / AUR
- `packaging/aur/PKGBUILD` — `optioncalendar`, builda `-p optioncalendar-cli`.
- `packaging/aur/bump.sh` — bump pkgver/pkgrel + hashes (CI-friendly, sem makepkg).
- `packaging/aur/publish.sh` — push local pro AUR (fallback sem workflow).
- `.SRCINFO` é gerado por `bump.sh`; não editar à mão.

## Release / Versioning
- Ver [VERSIONING.md](VERSIONING.md): changelog usa `x.y.z-stable` (ou alpha/beta);
  `Cargo.toml` / tags git ficam numéricos (`0.1.0`, `v0.1.0`).
- Não marque `stable` no changelog sem estar pronto pra release/AUR.
- Fluxo: bump Cargo.toml → CHANGELOG.md → commit → `git tag -a vX.Y.Z` → push tag.
- `bump.sh` espera o tag existir no GitHub antes de hashear o tarball.

## Detalhes que já morderam
- `UID` é variável reservada do bash; use outro nome em scripts de teste.
- `generate_uid` (FNV-1a, não BLAKE) mistura nanos do wall-clock pra evitar colisão de UID em adds idênticos.
- `parse_dt` aceita `YYYYMMDDTHHMMSS`, `YYYY-MM-DDTHH:MM`, `YYYY-MM-DD HH:MM`, `YYYYMMDD`, `YYYY-MM-DD`.
- Strip de `Z` suffix (UTC designator) — optionCalendar mantém wall-clock time.
- `CalStore::remove` e `CalStore::update` retornam `bool` (encontrou ou não); `rm`/`edit` por UID ou índice 1-based (`resolve_id`).
- `edit` nunca muda o UID (`CalStore::update` restaura o UID após o closure); valida `end >= start`; sem flags = erro "nothing to change".
- `Event`/`TaskDue` derivam `Serialize` com datas ISO 8601 (`YYYY-MM-DDTHH:MM:SS` / `YYYY-MM-DD`).
- Tasks bridge nunca falha: dir/vault ausente = lista vazia.

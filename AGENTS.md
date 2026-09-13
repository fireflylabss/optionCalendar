# optionCalendar — notas para agentes

## Build
- Rust stable (1.85+). Sem dependências de sistema além da toolchain.
- `cargo build --release` ou `cargo build -p optioncalendar-cli`.
- `CARGO_TARGET_DIR` deve apontar para `$(pwd)/target` nos scripts.
- `optionSDK` é path dep (`../optionSDK`); o PKGBUILD resolve via download separado.
- Smoke test: `OPTION_HOME=/tmp/oca-smoke ./target/debug/oca add "Test" --at 2026-09-10T10:00 && ./target/debug/oca ls --uid`.
- Testes: `cargo test --workspace` (sem rede) — 29 unitários no core, 3 no TUI e
  21 de integração do CLI em `crates/optioncalendar-cli/tests/cli.rs`
  (`assert_cmd` + `predicates`; cada teste roda `oca` com `OPTION_HOME` num tempdir).
- Lint: `cargo clippy --workspace --all-targets -- -D warnings`.

## Arquitetura
- `crates/optioncalendar-core/` — ICS parse/serialize, file store, queries, tasks bridge, `WeekStart`.
  - `ics.rs` — VEVENT: UID/DTSTART/DTEND/SUMMARY/DESCRIPTION/RRULE interpretados; todo o resto
    (LOCATION, X-props, blocos aninhados como VALARM) vai cru em `Event::extra` e volta em `to_ics`.
    `start_raw`/`end_raw` guardam a linha original de DTSTART/DTEND (com params, ex. `TZID=`) e são
    re-emitidas enquanto ainda batem com `start`/`end`. `all_day` = `VALUE=DATE` ou valor `YYYYMMDD`.
  - `store.rs` — `CalStore` (um ICS file), `Settings` (ics_path, launch_tui_on_no_args, week_start).
    `load_settings`/`save_settings` são wrappers de `load_settings_from(path)`/`save_settings_to(path, &Settings)`;
    testes usam as versões com path (nunca mexa em `OPTION_HOME` via `std::env` em teste unitário).
  - `query.rs` — day/week/month queries, `today_merged` (events + tasks). `events_on`/`events_between`
    retornam `Vec<Event>` (clones) e expandem RRULE `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY` com
    INTERVAL/COUNT/UNTIL (`occurrences_between`); regra não suportada = só a primeira ocorrência.
  - `tasks.rs` — bridge optionNotes: `- [ ] text due:YYYY-MM-DD` de `~/Documents/Notes/tasks/*.md`.
  - `week.rs` — `WeekStart` (Monday default ISO 8601, ou Sunday).
- `crates/optioncalendar-cli/` — `oca` / `optioncalendar` (mesmo entrypoint).
  - `main.rs` — clap CLI: add, ls, today, week, month, next, search, rm, import, export, tui, config.
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
- `CalStore::remove` retorna `bool` (encontrou ou não); `rm` por UID ou índice 1-based.
- Tasks bridge nunca falha: dir/vault ausente = lista vazia.
- `DTEND;VALUE=DATE` é EXCLUSIVO no ICS: `Event.end` de um all-day guarda o dia seguinte;
  use `end_or_start()` (inclusivo) nas queries. `Event::new` é sempre timed (meia-noite explícita
  continua timed); `Event::new_all_day(start, last_day)` recebe o último dia inclusivo e guarda
  exclusivo. O CLI decide por `is_date_only(--at/--end)`.
- Expansão de RRULE começa em `first_candidate` (estimativa pela janela) e examina no máximo
  10k ocorrências a partir dali — séries antigas (1990…) ainda aparecem hoje.
- Props conhecidas dentro de sub-componentes (ex. `DESCRIPTION` de um VALARM) não pertencem ao
  evento: `find_prop` só olha depth 0.
- Ocorrências expandidas de RRULE mantêm o mesmo `uid`; `rm` opera em `store.events`, não nas queries.

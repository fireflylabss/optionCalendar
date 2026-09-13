# optionCalendar — notas para agentes

## Build
- Rust stable (1.85+). Sem dependências de sistema além da toolchain.
- `cargo build --release` ou `cargo build -p optioncalendar-cli`.
- `CARGO_TARGET_DIR` deve apontar para `$(pwd)/target` nos scripts.
- `optionSDK` é path dep (`../optionSDK`); o PKGBUILD resolve via download separado.
- Smoke test: `OPTION_HOME=/tmp/oca-smoke ./target/debug/oca add "Test" --at 2026-09-10T10:00 && ./target/debug/oca ls --uid`.
- Testes: `cargo test --workspace` (sem rede) — 19 unitários no core, 3 no TUI e
  21 de integração do CLI em `crates/optioncalendar-cli/tests/cli.rs`
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

## CI
- `.github/workflows/ci.yml` — push em `main` e PRs: clona `optionSDK` em `../optionSDK`,
  `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `cargo build --release -p optioncalendar-cli`. Cache via `Swatinem/rust-cache`.
- `OPTIONSDK_REF` (env no topo de `ci.yml` e `release.yml`) diz qual ref do optionSDK clonar.
  Hoje é `pull/1/head` (PR que adiciona `App::CAL`); trocar pra tag (`vX.Y.Z`) quando
  ele for mergeado/taggeado, junto com `_optionsdk_ver` no PKGBUILD.
- `.github/workflows/release.yml` — push de tag `v*`: verifica tag == versão do workspace,
  testa, builda release, publica GitHub Release com
  `optioncalendar-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` (`optioncalendar` + `oca`),
  roda `packaging/aur/bump.sh` e anexa PKGBUILD/.SRCINFO como artifact.
- Push pro AUR só roda se o secret `AUR_SSH_KEY` (chave SSH privada cadastrada no AUR)
  existir no repo; sem ele o step é pulado e o publish é manual via `publish.sh`.
- Rode localmente antes de abrir PR: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.

## Release / Versioning
- Ver [VERSIONING.md](VERSIONING.md): changelog usa `x.y.z-stable` (ou alpha/beta);
  `Cargo.toml` / tags git ficam numéricos (`0.1.0`, `v0.1.0`).
- Não marque `stable` no changelog sem estar pronto pra release/AUR.
- Fluxo: bump Cargo.toml → CHANGELOG.md → commit → `git tag -a vX.Y.Z` → push tag.
- `bump.sh` espera o tag existir no GitHub antes de hashear o tarball.
- O push da tag dispara `release.yml` (ver seção CI).

## Detalhes que já morderam
- `UID` é variável reservada do bash; use outro nome em scripts de teste.
- `generate_uid` (FNV-1a, não BLAKE) mistura nanos do wall-clock pra evitar colisão de UID em adds idênticos.
- `parse_dt` aceita `YYYYMMDDTHHMMSS`, `YYYY-MM-DDTHH:MM`, `YYYY-MM-DD HH:MM`, `YYYYMMDD`, `YYYY-MM-DD`.
- Strip de `Z` suffix (UTC designator) — optionCalendar mantém wall-clock time.
- `CalStore::remove` retorna `bool` (encontrou ou não); `rm` por UID ou índice 1-based.
- Tasks bridge nunca falha: dir/vault ausente = lista vazia.

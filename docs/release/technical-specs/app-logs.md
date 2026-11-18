# App logs — technical reference

**Since v0.0.11.** Requirements: FR-26 (application logging), UI-53 (Settings *Log* section); AC-154, AC-155, AC-156. User guide: [app-logs](../features/app-logs.md). Design authority: SPEC-v0.0.11.md §3.2/§3.3 (frozen); this document describes the code **as released and verified**.

## Modules

**`src-tauri/src/applog/mod.rs`** — a plain library module (R1.1), `cargo test`-able without UI. A synchronous writer behind process-wide state; no `log` facade as the product API, no async queue — a line is a few hundred bytes appended to an OS file, and the volume is bounded by design.

Public surface:

| Function | Role |
|---|---|
| `init(data_dir)` | Resolve `<data_dir>/logs/app.log` once at startup (platform-resolved data directory — never a hard-coded path). |
| `set_level(value)` | Validate `error \| info` (the settings-store grammar `log_level`) and update the cached level flag. |
| `error(subsystem, message)` / `info(subsystem, message)` | Write one line when the level allows it. Line format: `RFC3339-UTC <spaced> LEVEL <spaced> subsystem <spaced> single-line message` — messages are collapsed to one line. |
| `command_error(cmd, &AppError)` | The command-boundary hook: logs the command name and the error class as the user would see it. |
| `file_path()` / `ensure_file()` | Path for the Settings display; `ensure_file` creates an empty log on a clean install so *Open log file* never fails. |
| `install_log_bridge()` | Startup-only bridge onto the `log` crate facade (`log::set_logger`, `LevelFilter::Error`): forwards foreign error records — plugin internals like `tauri_plugin_updater` — into the log file. Additive: a second call or a pre-installed logger is ignored. |

**Cap:** on write, if `app.log` exceeds **2 MiB**, it is renamed to `app.log.1` (replacing any existing `.1`) and a fresh file starts — two files maximum, no compression, no timestamps in names.

## Hook points (as shipped)

- **Command boundary** (the single choke point, M5): every `Err(AppError)` returned to a frontend call — `commands.rs` (the `db.with` wrapper + the three log commands' own error paths).
- **Import subsystem:** per-file pass failures — `library/service.rs`.
- **Cover queue:** render failures — `covers/mod.rs`.
- **Lifecycle:** startup/exit and window open/close lines at Info — `lib.rs`.
- **Foreign error records:** the `log` facade bridge (`install_log_bridge`).
- Hot paths (page serving, search, cover decode) are **not** instrumented — logging sits at boundaries, not inside loops.

## Commands (R1.2 thin wrappers) and the frontend

`log_level_set(level)` (validates through the settings grammar; writes through the settings store key `log_level`, which joins `SETTINGS_KEYS`; refreshes the cached flag), `log_path()` (returns the resolved path string), `log_open()` (resolves the path, `ensure_file()`, opens via the opener plugin's Rust API — the frontend never opens OS paths itself, the DR-191 shape). All three are on the R1.3 whitelist and in the typed client (`logLevelSet`, `logPath`, `logOpen` in `ui/src/api.ts`). A settings write of key `log_level` through the *existing* `set_setting` path also refreshes the level — the effect is live in every window through the released settings broadcast (AC-156).

**UI-53:** a new section in `SettingsDialog.svelte` on the existing section pattern — a `role=radiogroup` level selector (Error / Info) wired through `settings.setLogLevel`, a read-only monospace path row (`log_path`), and an icon action *Open log file* (`log_open`). i18n keys: `settings.log`, `settings.logLevel`, `settings.logLevel.error`, `settings.logLevel.info`, `settings.openLog`. The reader windows' *Settings…* opens the same component (DR-338), so the section behaves identically there.

## Invariants

- A failed log write never fails a product operation — the log cannot create the failures it records (swallowed I/O).
- Privacy line: paths and application events only; never document content, never file contents, never secrets (no password-prompt values, no update-URL query parts).
- Level default is `Error` when no key exists (the released settings default pattern).
- The log bridge is an addition, never a replacement the app depends on.

## Test mapping

- **Rust units** (`src-tauri/src/applog/mod.rs::tests`, AC-154/AC-155): `error_level_writes_and_info_is_suppressed_by_default`, `info_level_adds_info_lines_and_keeps_error_lines`, `line_format_is_timestamp_level_subsystem_message`, `messages_are_collapsed_to_single_lines`, `cap_rotates_to_a_single_backup_and_keeps_two_files_maximum`, `a_directory_that_cannot_be_written_never_fails_a_product_call`, `ensure_file_creates_the_log_on_a_clean_install`, `level_set_accepts_only_the_two_grammar_values`, `timestamps_match_the_utc_calendar_at_known_epochs`, `the_bridge_forwards_plugin_error_records_into_the_log_file`.
- **UI units** (`ui/src/lib/state/settings.test.ts`): `setLogLevel` writes through `logLevelSet` and mirrors state; the settings-store round trip keeps the level after "restart".
- **Feature lane** (`flowLogs`, area `settings-and-appearance` in `scripts/ui-smoke.mjs`, AC-156): the Log section in the Settings dialog — level selector persists and takes effect without restart, the path row shows the real path, *Open log file* opens it; exercised from the library window's Settings. The reader-window parity rides the shared component (DR-338).
- i18n coverage test pins the five new message keys.

## Change history

- **2026-11-18 (v0.0.11):** Feature shipped (`Since v0.0.11`). The `applog` module, the three commands, the Settings *Log* section, and the log bridge all land in this release; verified by the ten Rust units, the UI units, and the `flowLogs` lane (green in the full lane run 62/62, 2026-11-18). The bridge's first real-world capture: the updater endpoint 404 of the pre-flight period appears in the log as `tauri_plugin_updater::updater` error records — expected behavior while no published release existed (AC-160's feed arrives with this release's tag).

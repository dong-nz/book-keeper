//! Application logging (FR-26, SPEC-v0.0.11 §3.2). A small synchronous
//! writer behind a process-wide handle: `error`/`info` each append one line
//! `RFC3339 UTC timestamp <spaced> LEVEL <spaced> subsystem <spaced>
//! single-line message` to `<app_data_dir>/logs/app.log`. No `log` facade
//! for the app's own calls and no async queue: the write is a few hundred
//! bytes appended to an OS file, and the volume is bounded by design —
//! errors are rare by definition, Info adds tens of lines per session.
//!
//! What logging must never do (§3.2): fail a product path (every write
//! error is swallowed — the log cannot create the failures it records), log
//! document content, or log secrets. The level is a cached flag refreshed
//! on the settings-write path (M6); the default is `error`.
//!
//! The `log`-facade bridge (DR-352) forwards the updater plugin's own
//! `log::error!` records — the only visible signal for update-check
//! failures, startup and manual — into the same file.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::AppError;

/// §3.2: on write, a file past 2 MiB is renamed to `app.log.1` (replacing
/// any existing `.1`) and a fresh file starts. Two files maximum — a safety
/// valve, not an archive.
const CAP_BYTES: u64 = 2 * 1024 * 1024;
const LOGS_DIR: &str = "logs";
const FILE_NAME: &str = "app.log";
const ROTATED_NAME: &str = "app.log.1";

/// The level values are the settings-store grammar (`log_level`): exactly
/// these two strings.
pub const LEVELS: [&str; 2] = ["error", "info"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Error,
    Info,
}

struct State {
    dir: PathBuf,
    level: Level,
}

/// The process-wide handle, owned here (the same singleton pattern as
/// `pdf::RenderService::shared()`): `init` installs it at startup, product
/// paths call the free functions. Before `init` — or in unit tests that
/// never init — every call is a no-op, which is the §3.2 swallow rule in
/// its strictest form.
static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Install the log location: `<data_dir>/logs`. Directory creation is
/// attempted here but a failure is not fatal — every later write would
/// fail and be swallowed, exactly like a directory that cannot be written.
pub fn init(data_dir: &Path) {
    let dir = data_dir.join(LOGS_DIR);
    let _ = fs::create_dir_all(&dir);
    *lock() = Some(State {
        dir,
        level: Level::Error,
    });
}

/// Validate and cache the level. Validation is the settings grammar: only
/// the two §3.2 values are accepted; anything else is the typed rejection
/// the settings store would also reject (belt and braces, AC-154).
pub fn set_level(value: &str) -> Result<(), AppError> {
    let level = match value {
        "error" => Level::Error,
        "info" => Level::Info,
        other => {
            return Err(AppError::Validation(format!(
                "Unknown log level: {other} (expected {})",
                LEVELS.join(" or ")
            )));
        }
    };
    if let Some(state) = lock().as_mut() {
        state.level = level;
    }
    Ok(())
}

/// The Error level: every hook point in §3.2 (command boundary, import
/// per-file failures, cover render failures, update-check failures).
pub fn error(subsystem: &str, message: &str) {
    write_line(Level::Error, subsystem, message);
}

/// The Info level: lifecycle only (OQ-67 answer A) — startup/exit, import
/// runs with counts, reader window opens/closes.
pub fn info(subsystem: &str, message: &str) {
    write_line(Level::Info, subsystem, message);
}

/// The command-boundary hook (M5, DR-354): every `Err(AppError)` returned
/// to a frontend call, with its command name and error text as the user
/// would see it. One format, both call sites (`Db::with` and the non-DB
/// wrappers in commands.rs).
pub fn command_error(cmd: &str, app_error: &AppError) {
    error("command", &format!("{cmd}: {app_error}"));
}

/// The path the Settings *Log* section displays (the `log_path` command).
pub fn file_path() -> Option<PathBuf> {
    lock().as_ref().map(|state| state.dir.join(FILE_NAME))
}

/// Resolve the log file and make sure it exists — `log_open` creates the
/// empty file on a clean install so the action never fails there (§3.2).
pub fn ensure_file() -> Result<PathBuf, AppError> {
    let path = file_path().ok_or_else(|| AppError::Internal("logging not initialized".into()))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|io_error| AppError::Io {
            path: dir.display().to_string(),
            message: io_error.to_string(),
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|io_error| AppError::Io {
            path: path.display().to_string(),
            message: io_error.to_string(),
        })?;
    Ok(path)
}

fn write_line(line_level: Level, subsystem: &str, message: &str) {
    let mut guard = lock();
    let Some(state) = guard.as_mut() else { return };
    if line_level == Level::Info && state.level != Level::Info {
        return;
    }
    let level_token = match line_level {
        Level::Error => "ERROR",
        Level::Info => "INFO",
    };
    let line = format!(
        "{} {level_token} {} {}",
        now_utc_rfc3339(),
        one_line(subsystem),
        one_line(message)
    );
    let path = state.dir.join(FILE_NAME);
    // The cap check is the write decision itself (§3.2 "on write"): a file
    // already past the cap rotates before this line appends.
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > CAP_BYTES) {
        let _ = fs::remove_file(state.dir.join(ROTATED_NAME));
        let _ = fs::rename(&path, state.dir.join(ROTATED_NAME));
    }
    // Swallowed on failure (§3.2): the log cannot create the failures it
    // records, so no write error escapes this function.
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
        let _ = file.write_all(b"\n");
    }
}

/// §3.2 line format: single-line messages. Every whitespace run (including
/// newlines) collapses to one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// RFC3339 UTC at second precision, hand-rolled (no datetime dependency):
/// days-since-civil (Hinnant) over the Unix clock. A clock before 1970
/// reads as the epoch — a wrong timestamp still beats a panic.
fn now_utc_rfc3339() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    format_utc_rfc3339(seconds)
}

fn format_utc_rfc3339(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to the
/// proleptic Gregorian (year, month, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let shifted_month = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32; // [1, 31]
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

/// The DR-352 bridge: the updater plugin reports every failed check attempt
/// through the `log` facade; forwarding its Error records is what makes
/// "update-check failures (startup and manual)" land in the log file
/// without touching the released update flow. Records below Error stay
/// where the plugin put them (nowhere — the app installs no other sink).
struct Bridge;

impl log::Log for Bridge {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() == log::Level::Error
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        error(record.target(), &record.args().to_string());
    }

    fn flush(&self) {}
}

static BRIDGE: Bridge = Bridge;

/// Install the bridge once (startup). A second call — or a foreign logger
/// already installed — is simply ignored: the bridge is an addition, never
/// a replacement the app depends on.
pub fn install_log_bridge() {
    if log::set_logger(&BRIDGE).is_ok() {
        log::set_max_level(log::LevelFilter::Error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    /// Applog tests share the process-wide `STATE` handle: this lock
    /// serializes the tests so one test's `init` cannot re-point the file
    /// under another test's assertions. Writes from other modules' tests
    /// (the import/cover failure hooks) can still land in the active
    /// file, so every assertion below filters to a subsystem token its
    /// own test owns.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn guard() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Every test owns its own log directory; `init` re-points the
    /// process-wide handle (the app calls it once at startup).
    fn init_dir(name: &str) -> TempDir {
        let temp = TempDir::new(name);
        init(temp.path());
        temp
    }

    /// The lines of one log file that belong to `subsystem`.
    fn lines_in(dir: &Path, file: &str, subsystem: &str) -> Vec<String> {
        fs::read_to_string(dir.join(file))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.split(' ').nth(2) == Some(subsystem))
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn error_level_writes_and_info_is_suppressed_by_default() {
        let _guard = guard();
        let temp = init_dir("levels");
        let logs = temp.path().join(LOGS_DIR);
        error("tle1", "library_search: Title cannot be empty");
        info("tle1", "startup");
        let lines = lines_in(&logs, FILE_NAME, "tle1");
        assert_eq!(lines.len(), 1, "default level records Error only");
        assert!(lines[0].contains("ERROR tle1"));
    }

    #[test]
    fn info_level_adds_info_lines_and_keeps_error_lines() {
        let _guard = guard();
        let temp = init_dir("levels");
        let logs = temp.path().join(LOGS_DIR);
        set_level("info").expect("valid level");
        info("tle2", "startup");
        error("tle2", "cannot read /tmp/x.pdf: not found");
        let lines = lines_in(&logs, FILE_NAME, "tle2");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("INFO tle2"));
        assert!(lines[1].contains("ERROR tle2"));
        // Back to Error: lifecycle stops, failures keep landing (AC-154).
        set_level("error").expect("valid level");
        info("tle2", "exit");
        error("tle2", "cover render failed: /tmp/a.pdf");
        let lines = lines_in(&logs, FILE_NAME, "tle2");
        assert_eq!(lines.len(), 3);
        assert!(lines[2].contains("ERROR tle2"));
        assert!(!lines[2].contains("exit"), "the Info line was suppressed");
    }

    #[test]
    fn line_format_is_timestamp_level_subsystem_message() {
        let _guard = guard();
        let temp = init_dir("format");
        error("tformat", "app_set_setting: Invalid value for theme");
        let lines = lines_in(&temp.path().join(LOGS_DIR), FILE_NAME, "tformat");
        assert_eq!(lines.len(), 1);
        let parts: Vec<&str> = lines[0].splitn(4, ' ').collect();
        assert_eq!(parts.len(), 4, "four fields: ts level subsystem message");
        // RFC3339 UTC: `YYYY-MM-DDTHH:MM:SSZ` — parseable by shape.
        let stamp = parts[0];
        let date_time = stamp.strip_suffix('Z').expect("UTC marker");
        let (date, time) = date_time.split_once('T').expect("date and time");
        let date_parts: Vec<&str> = date.split('-').collect();
        assert_eq!(date_parts.len(), 3, "YYYY-MM-DD");
        assert_eq!(date_parts[0].len(), 4);
        assert!(date_parts[1..].iter().all(|part| part.len() == 2));
        assert_eq!(time.split(':').count(), 3, "HH:MM:SS");
        assert_eq!(parts[1], "ERROR");
        assert_eq!(parts[2], "tformat");
        assert_eq!(parts[3], "app_set_setting: Invalid value for theme");
    }

    #[test]
    fn messages_are_collapsed_to_single_lines() {
        let _guard = guard();
        let temp = init_dir("oneline");
        error("tonel", "first line\nsecond\tline  again");
        let lines = lines_in(&temp.path().join(LOGS_DIR), FILE_NAME, "tonel");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("first line second line again"));
    }

    #[test]
    fn cap_rotates_to_a_single_backup_and_keeps_two_files_maximum() {
        let _guard = guard();
        let temp = init_dir("cap");
        let logs = temp.path().join(LOGS_DIR);
        // Each line is ~64 KiB + the ~30-byte prefix: 31 lines stay under
        // the 2 MiB cap (a ~60 KiB margin — foreign test lines can add at
        // most a few hundred), 32 cross it, and the cap only rotates on
        // the NEXT write — §3.2's "on write" rule exactly. A foreign line
        // crossing in between only moves WHICH write rotates, never the
        // content: every assertion below filters to `tcap`.
        let big = "x".repeat(64 * 1024);
        for _ in 0..31 {
            error("tcap", &big);
        }
        assert!(
            !logs.join(ROTATED_NAME).exists(),
            "under the cap there is no rotation"
        );
        error("tcap", &big); // the 32nd line crosses the cap...
        assert!(fs::metadata(logs.join(FILE_NAME)).expect("current file").len() > CAP_BYTES);
        assert!(
            !logs.join(ROTATED_NAME).exists(),
            "no rotation before a write"
        );
        error("tcap", "after the cap"); // ...and the next write rotates it.
        let rotated = lines_in(&logs, ROTATED_NAME, "tcap");
        assert_eq!(rotated.len(), 32, "the full history moved to .1");
        assert!(rotated[31].contains(&big), "the crossing line moved too");
        let fresh = lines_in(&logs, FILE_NAME, "tcap");
        assert_eq!(fresh.len(), 1);
        assert!(fresh[0].ends_with(" ERROR tcap after the cap"));
        assert_eq!(fs::read_dir(&logs).expect("logs dir").count(), 2, "two files maximum");
        // A second crossing replaces the old backup, never a third file.
        for _ in 0..32 {
            error("tcap", &big);
        }
        error("tcap", "second rotation");
        assert_eq!(fs::read_dir(&logs).expect("logs dir").count(), 2);
        let rotated = lines_in(&logs, ROTATED_NAME, "tcap");
        assert_eq!(
            rotated.len(),
            33,
            "the fresh file (1 line) plus the 32 lines it held"
        );
        assert_eq!(lines_in(&logs, FILE_NAME, "tcap").len(), 1);
    }

    #[test]
    fn a_directory_that_cannot_be_written_never_fails_a_product_call() {
        let _guard = guard();
        // A regular file where the logs directory would go: init and every
        // write fails at the OS level; nothing panics, nothing escapes.
        let temp = TempDir::new("swallow");
        let blocker = temp.path().join("data");
        fs::write(&blocker, "not a directory").expect("fixture file");
        init(&blocker);
        error("command", "library_search: Title cannot be empty");
        info("lifecycle", "startup");
        assert!(ensure_file().is_err(), "the action reports the IO failure");
        assert!(file_path().is_some(), "the path is still reported");
    }

    #[test]
    fn ensure_file_creates_the_log_on_a_clean_install() {
        let _guard = guard();
        let temp = init_dir("cleaninstall");
        let path = ensure_file().expect("created");
        assert_eq!(path, temp.path().join(LOGS_DIR).join(FILE_NAME));
        assert!(path.is_file(), "the empty file exists after the action");
        assert_eq!(file_path().expect("path reported"), path);
    }

    #[test]
    fn level_set_accepts_only_the_two_grammar_values() {
        let _guard = guard();
        for rejected in ["warn", "", "Error", "INFO", "error info"] {
            let rejection = set_level(rejected).expect_err("rejected");
            assert_eq!(rejection.payload().kind, "validation", "typed rejection: {rejected}");
        }
        assert!(set_level("error").is_ok());
        assert!(set_level("info").is_ok());
    }

    #[test]
    fn timestamps_match_the_utc_calendar_at_known_epochs() {
        assert_eq!(format_utc_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc_rfc3339(946_684_800), "2000-01-01T00:00:00Z");
        assert_eq!(format_utc_rfc3339(1_234_567_890), "2009-02-13T23:31:30Z");
        assert_eq!(format_utc_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc_rfc3339(1_735_689_600), "2025-01-01T00:00:00Z");
        // Leap-day and century rules (the civil algorithm's hard cases).
        assert_eq!(format_utc_rfc3339(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(format_utc_rfc3339(1_893_456_000), "2030-01-01T00:00:00Z");
    }

    #[test]
    fn the_bridge_forwards_plugin_error_records_into_the_log_file() {
        let _guard = guard();
        let temp = init_dir("bridge");
        install_log_bridge();
        log::log!(
            target: "tauri_plugin_updater::updater",
            log::Level::Error,
            "failed to check for updates: boom"
        );
        log::debug!(target: "tauri_plugin_updater::updater", "not written");
        let lines = lines_in(
            &temp.path().join(LOGS_DIR),
            FILE_NAME,
            "tauri_plugin_updater::updater",
        );
        assert_eq!(lines.len(), 1, "only Error records cross the bridge");
        assert!(lines[0].contains("ERROR tauri_plugin_updater::updater"));
        assert!(lines[0].contains("failed to check for updates: boom"));
    }
}

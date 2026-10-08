//! Structured local logging.
//!
//! Logs go to a rolling file in the AllInsight data folder and nowhere else. They
//! record what the application did, not what the user has: no file names, no
//! file contents, no paths from the user's own folders. Services follow the
//! same rule when they log, which is why the removal code reports only the
//! final component of a name.

use std::io::Write;
use std::path::{Path, PathBuf};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Name of the crash record inside the log directory.
const CRASH_FILE: &str = "crash.log";

/// Past this size the crash record is rotated to `crash.log.1`, so a panic that
/// repeats cannot fill a drive that may already be nearly full.
const CRASH_FILE_LIMIT: u64 = 1024 * 1024;

/// Keeps the non-blocking writer alive for the life of the process. Dropping
/// it would silently stop logging.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

pub fn init() -> Option<LogGuard> {
    let directory = crate::state::data_directory().join("logs");
    if std::fs::create_dir_all(&directory).is_err() {
        return None;
    }

    // Installed before anything else can fail, and independent of whether the
    // tracing subscriber below manages to initialise.
    install_panic_hook(directory.clone());

    let appender = tracing_appender::rolling::daily(&directory, "allinsight.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("ALLINSIGHT_LOG")
        .unwrap_or_else(|_| EnvFilter::new("allinsight=info,warn"));

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let registry = tracing_subscriber::registry().with(filter).with(file_layer);

    // A console layer is useful while developing and pointless in a shipped
    // windowed build, which has no console attached.
    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_target(true));

    if registry.try_init().is_err() {
        return None;
    }

    tracing::info!(
        target: "allinsight",
        version = env!("CARGO_PKG_VERSION"),
        "AllInsight started"
    );
    Some(LogGuard(guard))
}

/// Record every panic before it can vanish.
///
/// A release build is a windowed process with no console, so the default hook
/// prints a panic to a stderr nobody can see and the crash leaves no trace. The
/// Privacy screen tells the user that crashes are written to the local log;
/// this is what makes that sentence true.
///
/// The record is written synchronously to its own file rather than through the
/// non-blocking tracing writer. A panic can end the process before that
/// writer's worker thread flushes, which would lose the one line that matters.
/// It is also sent through `tracing`, so it appears in order in the daily log
/// whenever that write does get flushed.
fn install_panic_hook(directory: PathBuf) {
    let previous = std::panic::take_hook();
    // The argument type is left to inference: it was renamed from `PanicInfo`
    // to `PanicHookInfo` in Rust 1.81, and this crate still declares 1.77.
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "an unknown location".into());

        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic with a non-text payload".into());
        let message = redact_home(&message, dirs::home_dir().as_deref());

        record_panic(&directory, &location, &message);
        tracing::error!(target: "allinsight::panic", %location, "{message}");

        // Keep the default behaviour: a debug build still prints to its console.
        previous(info);
    }));
}

/// Append one crash line to `crash.log`, rotating it once it grows too large.
///
/// Every failure is ignored on purpose. This runs inside a panic, where there
/// is nothing useful left to do with an error.
fn record_panic(directory: &Path, location: &str, message: &str) {
    let _ = std::fs::create_dir_all(directory);
    let path = directory.join(CRASH_FILE);

    if std::fs::metadata(&path)
        .map(|m| m.len() > CRASH_FILE_LIMIT)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, directory.join(format!("{CRASH_FILE}.1")));
    }

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{} AllInsight {} panicked at {location}: {message}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f %z"),
            env!("CARGO_PKG_VERSION"),
        );
    }
}

/// Replace the user's home folder in a panic message with `%USERPROFILE%`.
///
/// The logs record what the application did, not what the user has. A panic
/// message is arbitrary text and can carry a path, so the one prefix that would
/// identify the user is taken out before it reaches the disk.
fn redact_home(message: &str, home: Option<&Path>) -> String {
    match home.and_then(|h| h.to_str()).filter(|h| !h.is_empty()) {
        Some(home) => message.replace(home, "%USERPROFILE%"),
        None => message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("allinsight-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_crash_is_written_with_its_location_and_message() {
        let dir = scratch("crash-record");
        record_panic(&dir, "src/example.rs:42", "index out of bounds");

        let written = std::fs::read_to_string(dir.join(CRASH_FILE)).unwrap();
        assert!(written.contains("panicked at src/example.rs:42: index out of bounds"));
        assert!(written.contains(env!("CARGO_PKG_VERSION")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crash_log_that_grew_too_large_is_rotated_rather_than_extended() {
        let dir = scratch("crash-rotate");
        let oversized = "x".repeat(CRASH_FILE_LIMIT as usize + 1);
        std::fs::write(dir.join(CRASH_FILE), &oversized).unwrap();

        record_panic(&dir, "src/example.rs:1", "again");

        let rotated = std::fs::metadata(dir.join(format!("{CRASH_FILE}.1"))).unwrap();
        let current = std::fs::metadata(dir.join(CRASH_FILE)).unwrap();
        assert_eq!(rotated.len(), oversized.len() as u64);
        assert!(current.len() < 1024, "the new record starts a fresh file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_home_folder_is_redacted_from_a_crash_message() {
        let home = Path::new("C:\\Users\\someone");
        let redacted = redact_home(
            "could not open C:\\Users\\someone\\Documents\\a.txt",
            Some(home),
        );
        assert_eq!(redacted, "could not open %USERPROFILE%\\Documents\\a.txt");
        assert!(!redacted.contains("someone"));
    }

    #[test]
    fn a_message_without_the_home_folder_is_left_alone() {
        let redacted = redact_home(
            "attempt to divide by zero",
            Some(Path::new("C:\\Users\\someone")),
        );
        assert_eq!(redacted, "attempt to divide by zero");
        assert_eq!(redact_home("anything", None), "anything");
    }
}

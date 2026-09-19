//! Structured local logging.
//!
//! Logs go to a rolling file in the AllInsight data folder and nowhere else. They
//! record what the application did, not what the user has: no file names, no
//! file contents, no paths from the user's own folders. Services follow the
//! same rule when they log, which is why the removal code reports only the
//! final component of a name.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Keeps the non-blocking writer alive for the life of the process. Dropping
/// it would silently stop logging.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

pub fn init() -> Option<LogGuard> {
    let directory = crate::state::data_directory().join("logs");
    if std::fs::create_dir_all(&directory).is_err() {
        return None;
    }

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

//! The update log: `logs/updates.log` in the AllInsight data folder.
//!
//! A separate file from the main log so that "what did the updater do" can be
//! answered, or attached to a bug report, without anything else. Every line
//! is written by the updater from a fixed sentence plus version numbers,
//! status codes and package file names it chose itself. It never receives
//! anything from the rest of the application, so it cannot contain the
//! user's files, scans, settings or assistant conversations.

use std::io::Write;
use std::path::PathBuf;

use parking_lot::Mutex;

/// Past this size the log is rotated to `updates.log.1`.
const LIMIT: u64 = 512 * 1024;

pub struct UpdateLog {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl UpdateLog {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn write(&self, message: &str) {
        tracing::info!(target: "allinsight::update", "{message}");
        let Some(path) = &self.path else {
            return;
        };
        let _guard = self.lock.lock();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::metadata(path)
            .map(|m| m.len() > LIMIT)
            .unwrap_or(false)
        {
            let _ = std::fs::rename(path, path.with_extension("log.1"));
        }
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "[{stamp}] {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_timestamped_and_appended() {
        let dir = std::env::temp_dir().join(format!("allinsight-ulog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = UpdateLog::new(Some(dir.join("updates.log")));
        log.write("Update check started");
        log.write("Up to date");
        let text = std::fs::read_to_string(dir.join("updates.log")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with('[') && lines[0].ends_with("] Update check started"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

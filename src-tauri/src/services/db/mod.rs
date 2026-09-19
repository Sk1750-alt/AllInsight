//! Local persistence.
//!
//! One SQLite file in the application data directory. SQLite is compiled into
//! the binary, so the installer never ships a loose DLL and the database works
//! on a machine with nothing else installed.
//!
//! What is stored: settings, aggregate scan and cleanup history, and a small
//! record of volume capacity over time so the Storage screen can show a trend.
//!
//! What is deliberately not stored: file names, file contents, hashes, or
//! anything that would turn this database into an index of the user's private
//! data. Cleanup history records totals and category names, never paths.

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{AllInsightError, Result};

pub mod settings;

pub use settings::Settings;

const SCHEMA_VERSION: i32 = 1;

pub struct Database {
    connection: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// Open, retrying briefly while another instance lets go.
    ///
    /// An elevated restart overlaps the two processes, so the first attempt
    /// can legitimately find the file locked. Failing here would stop AllInsight
    /// starting at all, which is a far worse outcome than waiting a moment.
    pub fn open_with_retry(path: &Path) -> Result<Self> {
        let mut last = None;
        for attempt in 0..5 {
            match Self::open(path) {
                Ok(db) => return Ok(db),
                Err(e) => {
                    tracing::warn!(
                        target: "allinsight::db",
                        "database busy on attempt {}: {e}",
                        attempt + 1
                    );
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
            }
        }
        Err(last.unwrap_or_else(|| {
            AllInsightError::Database("The database could not be opened.".into())
        }))
    }

    /// Open, creating the file and schema when they do not exist.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AllInsightError::Database(format!("Could not create the data folder: {e}"))
            })?;
        }
        let connection = Connection::open(path)?;
        // WAL keeps the background monitor's writes from blocking the UI's
        // reads, which matters because both run continuously.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;

        // Wait for a contended write instead of failing instantly.
        //
        // SQLite's default is to give up immediately, and AllInsight genuinely can
        // have two processes on one database: "Restart as administrator"
        // launches an elevated copy while the original is still shutting down,
        // and a user can start a second copy at any time. Without this, the
        // loser of that race silently fails to save settings.
        connection.busy_timeout(std::time::Duration::from_secs(5))?;

        let db = Self {
            connection: Mutex::new(connection),
            path: path.to_path_buf(),
        };
        db.migrate()?;
        Ok(db)
    }

    /// An in-memory database, used by the tests.
    pub fn in_memory() -> Result<Self> {
        let db = Self {
            connection: Mutex::new(Connection::open_in_memory()?),
            path: PathBuf::from(":memory:"),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_info (
                version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Aggregate only. No file names are ever written here.
            CREATE TABLE IF NOT EXISTS cleanup_history (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                ran_at           INTEGER NOT NULL,
                trigger          TEXT    NOT NULL,
                reclaimed_bytes  INTEGER NOT NULL,
                removed_items    INTEGER NOT NULL,
                skipped_items    INTEGER NOT NULL,
                categories       TEXT    NOT NULL
            );

            CREATE INDEX IF NOT EXISTS cleanup_history_ran_at
                ON cleanup_history (ran_at DESC);

            CREATE TABLE IF NOT EXISTS scan_history (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                ran_at       INTEGER NOT NULL,
                root         TEXT    NOT NULL,
                total_bytes  INTEGER NOT NULL,
                total_files  INTEGER NOT NULL,
                duration_ms  INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS scan_history_ran_at
                ON scan_history (ran_at DESC);

            -- One row per volume per sample, so the Storage screen can draw a
            -- capacity trend without keeping a scan in memory.
            CREATE TABLE IF NOT EXISTS volume_samples (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                sampled_at   INTEGER NOT NULL,
                mount_point  TEXT    NOT NULL,
                total_bytes  INTEGER NOT NULL,
                free_bytes   INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS volume_samples_at
                ON volume_samples (mount_point, sampled_at DESC);

            CREATE TABLE IF NOT EXISTS activity_log (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                happened_at INTEGER NOT NULL,
                kind        TEXT    NOT NULL,
                summary     TEXT    NOT NULL,
                detail      TEXT
            );

            CREATE INDEX IF NOT EXISTS activity_log_at
                ON activity_log (happened_at DESC);
            "#,
        )?;

        let current: Option<i32> = connection
            .query_row("SELECT version FROM schema_info LIMIT 1", [], |row| row.get(0))
            .ok();
        match current {
            None => {
                connection.execute("INSERT INTO schema_info (version) VALUES (?1)", params![
                    SCHEMA_VERSION
                ])?;
            }
            Some(v) if v < SCHEMA_VERSION => {
                connection.execute("UPDATE schema_info SET version = ?1", params![SCHEMA_VERSION])?;
            }
            _ => {}
        }
        Ok(())
    }

    // ---- settings ----------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let connection = self.connection.lock();
        let value = connection
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| {
                row.get::<_, String>(0)
            })
            .ok();
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---- history -----------------------------------------------------

    pub fn record_cleanup(&self, entry: &CleanupHistoryEntry) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO cleanup_history
                (ran_at, trigger, reclaimed_bytes, removed_items, skipped_items, categories)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.ran_at,
                entry.trigger,
                entry.reclaimed_bytes as i64,
                entry.removed_items as i64,
                entry.skipped_items as i64,
                entry.categories.join(", "),
            ],
        )?;
        Ok(())
    }

    pub fn cleanup_history(&self, limit: usize) -> Result<Vec<CleanupHistoryEntry>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT ran_at, trigger, reclaimed_bytes, removed_items, skipped_items, categories
             FROM cleanup_history ORDER BY ran_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok(CleanupHistoryEntry {
                ran_at: row.get(0)?,
                trigger: row.get(1)?,
                reclaimed_bytes: row.get::<_, i64>(2)? as u64,
                removed_items: row.get::<_, i64>(3)? as u64,
                skipped_items: row.get::<_, i64>(4)? as u64,
                categories: row
                    .get::<_, String>(5)?
                    .split(", ")
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
            })
        })?;
        Ok(rows.flatten().collect())
    }

    /// Lifetime totals, for the Activity screen headline.
    pub fn cleanup_totals(&self) -> Result<CleanupTotals> {
        let connection = self.connection.lock();
        let (runs, bytes, items): (i64, i64, i64) = connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(reclaimed_bytes), 0), COALESCE(SUM(removed_items), 0)
             FROM cleanup_history",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(CleanupTotals {
            runs: runs as u64,
            reclaimed_bytes: bytes as u64,
            removed_items: items as u64,
        })
    }

    pub fn record_scan(&self, root: &str, total_bytes: u64, total_files: u64, duration_ms: u64) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO scan_history (ran_at, root, total_bytes, total_files, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                chrono::Utc::now().timestamp(),
                root,
                total_bytes as i64,
                total_files as i64,
                duration_ms as i64
            ],
        )?;
        Ok(())
    }

    pub fn record_volume_sample(&self, mount_point: &str, total: u64, free: u64) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO volume_samples (sampled_at, mount_point, total_bytes, free_bytes)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                chrono::Utc::now().timestamp(),
                mount_point,
                total as i64,
                free as i64
            ],
        )?;
        // Keep roughly a year of daily samples per volume.
        connection.execute(
            "DELETE FROM volume_samples
             WHERE mount_point = ?1
               AND id NOT IN (
                   SELECT id FROM volume_samples
                   WHERE mount_point = ?1
                   ORDER BY sampled_at DESC, id DESC LIMIT 400
               )",
            params![mount_point],
        )?;
        Ok(())
    }

    pub fn volume_trend(&self, mount_point: &str, limit: usize) -> Result<Vec<VolumeSample>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT sampled_at, total_bytes, free_bytes FROM volume_samples
             WHERE mount_point = ?1 ORDER BY sampled_at DESC, id DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![mount_point, limit as i64], |row| {
            Ok(VolumeSample {
                sampled_at: row.get(0)?,
                total_bytes: row.get::<_, i64>(1)? as u64,
                free_bytes: row.get::<_, i64>(2)? as u64,
            })
        })?;
        let mut out: Vec<VolumeSample> = rows.flatten().collect();
        out.reverse();
        Ok(out)
    }

    pub fn log_activity(&self, kind: &str, summary: &str, detail: Option<&str>) -> Result<()> {
        let connection = self.connection.lock();
        connection.execute(
            "INSERT INTO activity_log (happened_at, kind, summary, detail) VALUES (?1, ?2, ?3, ?4)",
            params![chrono::Utc::now().timestamp(), kind, summary, detail],
        )?;
        connection.execute(
            "DELETE FROM activity_log WHERE id NOT IN
                (SELECT id FROM activity_log ORDER BY happened_at DESC, id DESC LIMIT 500)",
            [],
        )?;
        Ok(())
    }

    pub fn activity(&self, limit: usize) -> Result<Vec<ActivityEntry>> {
        let connection = self.connection.lock();
        let mut statement = connection.prepare(
            "SELECT happened_at, kind, summary, detail FROM activity_log
             ORDER BY happened_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit as i64], |row| {
            Ok(ActivityEntry {
                happened_at: row.get(0)?,
                kind: row.get(1)?,
                summary: row.get(2)?,
                detail: row.get(3)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupHistoryEntry {
    pub ran_at: i64,
    /// `manual`, `auto`, or `emergency`.
    pub trigger: String,
    pub reclaimed_bytes: u64,
    pub removed_items: u64,
    pub skipped_items: u64,
    /// Category display names. Never file names.
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupTotals {
    pub runs: u64,
    pub reclaimed_bytes: u64,
    pub removed_items: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSample {
    pub sampled_at: i64,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub happened_at: i64,
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let db = Database::in_memory().unwrap();
        assert!(db.get_setting("theme").unwrap().is_none());
        db.set_setting("theme", "dark").unwrap();
        assert_eq!(db.get_setting("theme").unwrap().as_deref(), Some("dark"));
        db.set_setting("theme", "light").unwrap();
        assert_eq!(db.get_setting("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn cleanup_history_stores_totals_and_never_paths() {
        let db = Database::in_memory().unwrap();
        db.record_cleanup(&CleanupHistoryEntry {
            ran_at: 1_700_000_000,
            trigger: "manual".into(),
            reclaimed_bytes: 6_871_947_674,
            removed_items: 4821,
            skipped_items: 12,
            categories: vec!["Windows temporary files".into(), "Crash dumps".into()],
        })
        .unwrap();

        let history = db.cleanup_history(10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].reclaimed_bytes, 6_871_947_674);
        assert_eq!(history[0].categories.len(), 2);

        let totals = db.cleanup_totals().unwrap();
        assert_eq!(totals.runs, 1);
        assert_eq!(totals.reclaimed_bytes, 6_871_947_674);
    }

    #[test]
    fn history_is_returned_newest_first() {
        let db = Database::in_memory().unwrap();
        for (at, bytes) in [(100i64, 1u64), (300, 3), (200, 2)] {
            db.record_cleanup(&CleanupHistoryEntry {
                ran_at: at,
                trigger: "manual".into(),
                reclaimed_bytes: bytes,
                removed_items: 1,
                skipped_items: 0,
                categories: vec![],
            })
            .unwrap();
        }
        let history = db.cleanup_history(10).unwrap();
        assert_eq!(
            history.iter().map(|h| h.ran_at).collect::<Vec<_>>(),
            vec![300, 200, 100]
        );
    }

    #[test]
    fn volume_trends_come_back_oldest_first() {
        let db = Database::in_memory().unwrap();
        db.record_volume_sample("C:\\", 1000, 400).unwrap();
        db.record_volume_sample("C:\\", 1000, 300).unwrap();
        let trend = db.volume_trend("C:\\", 10).unwrap();
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[0].free_bytes, 400);
        assert_eq!(trend[1].free_bytes, 300);
    }

    #[test]
    fn activity_is_capped_and_ordered() {
        let db = Database::in_memory().unwrap();
        for i in 0..5 {
            db.log_activity("scan", &format!("scan {i}"), None).unwrap();
        }
        let entries = db.activity(3).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].summary, "scan 4");
    }

    #[test]
    fn reopening_an_existing_file_keeps_its_data() {
        let dir = std::env::temp_dir().join(format!("allinsight-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("allinsight.db");

        {
            let db = Database::open(&path).unwrap();
            db.set_setting("first_run_complete", "true").unwrap();
        }
        {
            let db = Database::open(&path).unwrap();
            assert_eq!(
                db.get_setting("first_run_complete").unwrap().as_deref(),
                Some("true")
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}

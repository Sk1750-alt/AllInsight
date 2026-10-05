//! Settings export, import and automatic backups.
//!
//! A configuration file is one versioned JSON document holding the settings,
//! which include the alert thresholds and the user's protected folders. It
//! never holds file names, measurements or history.
//!
//! Importing is two steps, and the second cannot be reached without the
//! first:
//!
//! 1. [`preview`] reads the file and describes every change it would make,
//!    flagging the ones that weaken protection: a protected folder removed,
//!    Auto-Clean switched on or widened, a safety confirmation or alert
//!    switched off. It returns a token bound to the exact bytes it read.
//! 2. [`prepare_apply`] re-reads the file, refuses if the bytes no longer
//!    match the token, and refuses a weakening change unless the caller says
//!    the user accepted it. Only then does the command layer write a backup
//!    of the current settings and save the new ones.
//!
//! Two settings are never imported, whatever the file says: the AI model and
//! the AI engine paths. They point at files on one particular machine, and an
//! engine path is a program AllInsight would run, so taking it from a file
//! someone handed over would turn "import my settings" into "run this
//! program".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AllInsightError, Result};
use crate::services::db::Settings;
use crate::services::security::paths;

/// Identifies an AllInsight settings file.
pub const FORMAT: &str = "allinsight-config";
/// Bumped only when an older AllInsight would misread a newer file.
pub const SCHEMA_VERSION: u32 = 1;

/// A settings document is a few kilobytes; anything far larger is not one.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Daily backups kept.
const DAILY_KEEP: usize = 7;
/// Backups taken just before an import, kept.
const IMPORT_KEEP: usize = 10;

const DAILY_PREFIX: &str = "daily-";
const IMPORT_PREFIX: &str = "before-import-";

/// Settings that describe this installation rather than the user's
/// preferences. They are left out of exports and kept as they are on import.
const LOCAL_ONLY: &[&str] = &["first_run_complete", "ai_model_path", "ai_engine_path"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDocument {
    pub format: String,
    pub schema_version: u32,
    pub application_version: String,
    pub exported_at: String,
    pub settings: Settings,
}

impl ConfigDocument {
    /// The document for `settings`, with the installation-specific values
    /// cleared.
    pub fn from_settings(settings: &Settings) -> Self {
        let mut portable = settings.clone();
        portable.ai_model_path = None;
        portable.ai_engine_path = None;
        portable.first_run_complete = true;
        Self {
            format: FORMAT.into(),
            schema_version: SCHEMA_VERSION,
            application_version: env!("CARGO_PKG_VERSION").into(),
            exported_at: chrono::Local::now().to_rfc3339(),
            settings: portable,
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingChange {
    pub key: String,
    pub label: String,
    pub from: String,
    pub to: String,
    /// True when the change makes AllInsight less careful than it is now.
    pub weakens_protection: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    /// Binds the apply step to the exact file contents that were previewed.
    pub token: String,
    pub application_version: String,
    pub exported_at: String,
    pub changes: Vec<SettingChange>,
    pub protected_added: Vec<String>,
    pub protected_removed: Vec<String>,
    /// True when applying needs the user's explicit acceptance.
    pub weakens_protection: bool,
}

/// Read and validate a settings file. Returns the parsed document and the
/// token for its bytes.
fn read_document(path: &Path) -> Result<(ConfigDocument, String)> {
    let meta = std::fs::metadata(paths::long_path(path))
        .map_err(|_| AllInsightError::NotFound(path.to_path_buf()))?;
    if !meta.is_file() {
        return Err(AllInsightError::InvalidInput("Choose a settings file, not a folder.".into()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(AllInsightError::InvalidInput(
            "That file is too large to be an AllInsight settings file.".into(),
        ));
    }
    let bytes = std::fs::read(paths::long_path(path))
        .map_err(|e| AllInsightError::Other(format!("The settings file could not be read: {e}")))?;
    let token = blake3::hash(&bytes).to_hex().to_string();
    let doc = parse(&bytes)?;
    Ok((doc, token))
}

fn parse(bytes: &[u8]) -> Result<ConfigDocument> {
    let not_ours =
        || AllInsightError::InvalidInput("This is not an AllInsight settings file.".into());
    let value: Value = serde_json::from_slice(bytes).map_err(|_| not_ours())?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(not_ours());
    }
    let version = value.get("schema_version").and_then(Value::as_u64).unwrap_or(0);
    if version == 0 {
        return Err(not_ours());
    }
    if version > SCHEMA_VERSION as u64 {
        return Err(AllInsightError::InvalidInput(
            "This settings file was made by a newer version of AllInsight. Update AllInsight, then import it again."
                .into(),
        ));
    }
    let mut doc: ConfigDocument = serde_json::from_value(value).map_err(|_| not_ours())?;
    doc.settings.sanitise();
    Ok(doc)
}

/// Whether the file at `path` is an AllInsight settings file. Used to decide
/// whether an export may replace it.
pub fn is_config_file(path: &Path) -> bool {
    std::fs::read(paths::long_path(path))
        .ok()
        .filter(|b| b.len() as u64 <= MAX_FILE_BYTES)
        .map(|b| parse(&b).is_ok())
        .unwrap_or(false)
}

/// The settings an import would produce: the file's preferences with this
/// installation's local values kept.
fn merged(current: &Settings, incoming: &Settings) -> Settings {
    let mut next = incoming.clone();
    next.first_run_complete = current.first_run_complete;
    next.ai_model_path = current.ai_model_path.clone();
    next.ai_engine_path = current.ai_engine_path.clone();
    next.sanitise();
    next
}

/// Describe what importing `path` would change.
pub fn preview(current: &Settings, path: &Path) -> Result<ImportPreview> {
    let (doc, token) = read_document(path)?;
    let next = merged(current, &doc.settings);
    let (changes, protected_added, protected_removed) = diff(current, &next);
    let weakens_protection =
        !protected_removed.is_empty() || changes.iter().any(|c| c.weakens_protection);
    Ok(ImportPreview {
        token,
        application_version: doc.application_version,
        exported_at: doc.exported_at,
        changes,
        protected_added,
        protected_removed,
        weakens_protection,
    })
}

/// Check that `path` still holds what was previewed and that any weakening
/// change was accepted, and return the settings to save.
pub fn prepare_apply(
    current: &Settings,
    path: &Path,
    token: &str,
    accept_weakening: bool,
) -> Result<Settings> {
    let (doc, actual) = read_document(path)?;
    if actual != token {
        return Err(AllInsightError::InvalidInput(
            "The settings file changed after it was reviewed. Review it again before importing."
                .into(),
        ));
    }
    let next = merged(current, &doc.settings);
    let (changes, _, removed) = diff(current, &next);
    let weakens = !removed.is_empty() || changes.iter().any(|c| c.weakens_protection);
    if weakens && !accept_weakening {
        return Err(AllInsightError::InvalidInput(
            "This import removes protection. Confirm the flagged changes to continue.".into(),
        ));
    }
    Ok(next)
}

fn diff(current: &Settings, next: &Settings) -> (Vec<SettingChange>, Vec<String>, Vec<String>) {
    let shown = |p: &PathBuf| p.to_string_lossy().into_owned();
    let protected_added: Vec<String> = next
        .protected_paths
        .iter()
        .filter(|p| !current.protected_paths.iter().any(|c| paths::same_path(c, p)))
        .map(shown)
        .collect();
    let protected_removed: Vec<String> = current
        .protected_paths
        .iter()
        .filter(|p| !next.protected_paths.iter().any(|n| paths::same_path(n, p)))
        .map(shown)
        .collect();

    let before = serde_json::to_value(current).unwrap_or(Value::Null);
    let after = serde_json::to_value(next).unwrap_or(Value::Null);
    let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
        return (Vec::new(), protected_added, protected_removed);
    };

    let mut changes = Vec::new();
    for (key, new) in after {
        if key == "protected_paths" || LOCAL_ONLY.contains(&key.as_str()) {
            continue;
        }
        let old = before.get(key).unwrap_or(&Value::Null);
        if old == new {
            continue;
        }
        changes.push(SettingChange {
            key: key.clone(),
            label: label(key).to_string(),
            from: display(key, old),
            to: display(key, new),
            weakens_protection: weakens(key, old, new),
        });
    }
    changes.sort_by(|a, b| b.weakens_protection.cmp(&a.weakens_protection).then(a.label.cmp(&b.label)));
    (changes, protected_added, protected_removed)
}

/// Whether moving `key` from `old` to `new` makes AllInsight less careful.
fn weakens(key: &str, old: &Value, new: &Value) -> bool {
    let turned_off = old.as_bool() == Some(true) && new.as_bool() == Some(false);
    match key {
        "auto_clean_enabled" => old.as_bool() == Some(false) && new.as_bool() == Some(true),
        // A higher threshold means Auto-Clean starts while more space is free.
        "auto_clean_free_space_percent" => new.as_u64() > old.as_u64(),
        "auto_clean_categories" => {
            let had: Vec<&Value> = old.as_array().map(|a| a.iter().collect()).unwrap_or_default();
            new.as_array()
                .map(|a| a.iter().any(|c| !had.contains(&c)))
                .unwrap_or(false)
        }
        "require_confirmation_for_processes" | "notifications_enabled" | "notify_drive_health" => {
            turned_off
        }
        _ => false,
    }
}

fn label(key: &str) -> &str {
    match key {
        "launch_at_startup" => "Launch at sign-in",
        "minimise_to_tray" => "Keep running in the tray",
        "theme" => "Theme",
        "ui_scale" => "Interface scale",
        "reduce_motion" => "Reduce motion",
        "scan_on_launch" => "Scan on launch",
        "large_file_threshold_bytes" => "Large file size",
        "duplicate_min_bytes" => "Smallest duplicate checked",
        "auto_clean_enabled" => "Auto-Clean",
        "auto_clean_free_space_percent" => "Auto-Clean starts below this free space",
        "auto_clean_categories" => "Auto-Clean categories",
        "notifications_enabled" => "Notifications",
        "alert_at_percent" => "Storage alert levels",
        "notify_drive_health" => "Drive health alerts",
        "notification_quiet_minutes" => "Quiet time between notifications",
        "ai_enabled" => "AllInsight AI",
        "ai_context_size" => "AI context size",
        "ai_threads" => "AI processor threads",
        "ai_gpu_layers" => "AI graphics acceleration layers",
        "ai_load_automatically" => "Load the AI model at launch",
        "ai_keep_loaded" => "Keep the AI model loaded",
        "background_monitoring" => "Background monitoring",
        "monitor_interval_seconds" => "Background check interval",
        "scan_threads" => "Scan threads",
        "require_confirmation_for_processes" => "Confirm before ending a process",
        other => other,
    }
}

fn display(key: &str, value: &Value) -> String {
    match value {
        Value::Bool(true) => "On".into(),
        Value::Bool(false) => "Off".into(),
        Value::Null => "Not set".into(),
        Value::String(s) => s.replace('_', " "),
        Value::Number(n) => {
            let n = n.as_u64().unwrap_or(0);
            match key {
                "large_file_threshold_bytes" | "duplicate_min_bytes" => {
                    crate::services::storage::format_bytes(n)
                }
                "auto_clean_free_space_percent" | "ui_scale" => format!("{n}%"),
                "monitor_interval_seconds" => format!("{} min", n.div_ceil(60)),
                "notification_quiet_minutes" => format!("{n} min"),
                "ai_threads" | "scan_threads" if n == 0 => "Automatic".into(),
                _ => n.to_string(),
            }
        }
        Value::Array(items) if items.is_empty() => "None".into(),
        Value::Array(items) => items
            .iter()
            .map(|v| match v {
                Value::Number(n) if key == "alert_at_percent" => format!("{n}%"),
                other => display("", other),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(_) => "Changed".into(),
    }
}

// ── Backups ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct BackupEntry {
    pub path: String,
    pub file_name: String,
    /// "daily" or "before_import".
    pub kind: String,
    pub created: String,
}

pub fn backup_directory(data_directory: &Path) -> PathBuf {
    data_directory.join("backups")
}

/// Write today's daily backup if there is none yet, then prune.
pub fn ensure_daily_backup(dir: &Path, settings: &Settings) -> Result<Option<PathBuf>> {
    let name = format!("{DAILY_PREFIX}{}.json", chrono::Local::now().format("%Y-%m-%d"));
    let path = dir.join(&name);
    if path.exists() {
        return Ok(None);
    }
    write_backup(dir, &name, settings)?;
    prune(dir, DAILY_PREFIX, DAILY_KEEP);
    Ok(Some(path))
}

/// Back up the current settings just before an import replaces them.
pub fn backup_before_import(dir: &Path, settings: &Settings) -> Result<PathBuf> {
    let name = format!(
        "{IMPORT_PREFIX}{}.json",
        chrono::Local::now().format("%Y-%m-%d-%H%M%S")
    );
    let path = write_backup(dir, &name, settings)?;
    prune(dir, IMPORT_PREFIX, IMPORT_KEEP);
    Ok(path)
}

fn write_backup(dir: &Path, name: &str, settings: &Settings) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .map_err(|e| AllInsightError::Other(format!("The backup folder could not be created: {e}")))?;
    let path = dir.join(name);
    // Written beside the target and renamed into place, so a crash mid-write
    // never leaves a half-written backup that a restore would then refuse.
    let partial = dir.join(format!("{name}.partial"));
    std::fs::write(&partial, ConfigDocument::from_settings(settings).to_json()?)
        .and_then(|_| std::fs::rename(&partial, &path))
        .map_err(|e| AllInsightError::Other(format!("The settings backup could not be written: {e}")))?;
    Ok(path)
}

/// Keep the newest `keep` backups whose names start with `prefix`. Only files
/// this module names are ever removed.
fn prune(dir: &Path, prefix: &str, keep: usize) {
    let mut names = backup_names(dir, prefix);
    // The timestamp in the name sorts chronologically.
    names.sort();
    let excess = names.len().saturating_sub(keep);
    for name in names.into_iter().take(excess) {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

fn backup_names(dir: &Path, prefix: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".json"))
        .collect()
}

/// Every backup, newest first.
pub fn list_backups(dir: &Path) -> Vec<BackupEntry> {
    let mut entries: Vec<BackupEntry> = [(DAILY_PREFIX, "daily"), (IMPORT_PREFIX, "before_import")]
        .into_iter()
        .flat_map(|(prefix, kind)| {
            backup_names(dir, prefix).into_iter().map(move |name| {
                let stamp = name.trim_start_matches(prefix).trim_end_matches(".json").to_string();
                BackupEntry {
                    path: dir.join(&name).to_string_lossy().into_owned(),
                    file_name: name,
                    kind: kind.into(),
                    created: stamp,
                }
            })
        })
        .collect();
    entries.sort_by(|a, b| b.created.cmp(&a.created));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "allinsight-config-{tag}-{}-{}",
            std::process::id(),
            chrono::Local::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, settings: &Settings) -> PathBuf {
        let path = dir.join("exported.json");
        std::fs::write(&path, ConfigDocument::from_settings(settings).to_json().unwrap()).unwrap();
        path
    }

    fn root(name: &str) -> PathBuf {
        PathBuf::from(if cfg!(windows) { format!("D:\\{name}") } else { format!("/srv/{name}") })
    }

    #[test]
    fn an_export_imported_unchanged_changes_nothing() {
        let dir = temp_dir("roundtrip");
        let mut s = Settings::default();
        s.theme = crate::services::db::settings::Theme::Paper;
        s.protected_paths = vec![root("Projects")];
        let path = write(&dir, &s);

        let preview = preview(&s, &path).unwrap();
        assert!(preview.changes.is_empty(), "{:?}", preview.changes);
        assert!(preview.protected_added.is_empty());
        assert!(preview.protected_removed.is_empty());
        assert!(!preview.weakens_protection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ai_engine_and_model_paths_are_never_imported() {
        let dir = temp_dir("local");
        let mut theirs = Settings::default();
        theirs.ai_engine_path = Some(root("evil.exe"));
        theirs.ai_model_path = Some(root("model.gguf"));
        // Exports clear them, so hand-craft a file that carries them anyway.
        let mut doc = ConfigDocument::from_settings(&theirs);
        doc.settings.ai_engine_path = theirs.ai_engine_path.clone();
        doc.settings.ai_model_path = theirs.ai_model_path.clone();
        let path = dir.join("crafted.json");
        std::fs::write(&path, doc.to_json().unwrap()).unwrap();

        let mut mine = Settings::default();
        mine.ai_engine_path = Some(root("llama-server.exe"));
        let preview = preview(&mine, &path).unwrap();
        assert!(preview.changes.iter().all(|c| !c.key.starts_with("ai_") || !c.key.ends_with("_path")));
        let next = prepare_apply(&mine, &path, &preview.token, false).unwrap();
        assert_eq!(next.ai_engine_path, mine.ai_engine_path);
        assert_eq!(next.ai_model_path, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_protected_folder_needs_acceptance() {
        let dir = temp_dir("protected");
        let mut mine = Settings::default();
        mine.protected_paths = vec![root("Projects"), root("Thesis")];
        let mut theirs = Settings::default();
        theirs.protected_paths = vec![root("Projects"), root("Music")];
        let path = write(&dir, &theirs);

        let preview = preview(&mine, &path).unwrap();
        assert_eq!(preview.protected_removed, vec![root("Thesis").to_string_lossy().into_owned()]);
        assert_eq!(preview.protected_added, vec![root("Music").to_string_lossy().into_owned()]);
        assert!(preview.weakens_protection);

        assert!(prepare_apply(&mine, &path, &preview.token, false).is_err());
        let next = prepare_apply(&mine, &path, &preview.token, true).unwrap();
        assert_eq!(next.protected_paths.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_on_or_widening_auto_clean_is_flagged() {
        let dir = temp_dir("autoclean");
        let mine = Settings::default();
        let mut theirs = Settings::default();
        theirs.auto_clean_enabled = true;
        theirs.auto_clean_free_space_percent = 30;
        theirs.auto_clean_categories = vec!["user_temp".into()];
        theirs.require_confirmation_for_processes = false;
        let path = write(&dir, &theirs);

        let preview = preview(&mine, &path).unwrap();
        for key in [
            "auto_clean_enabled",
            "auto_clean_free_space_percent",
            "auto_clean_categories",
            "require_confirmation_for_processes",
        ] {
            let change = preview.changes.iter().find(|c| c.key == key).expect(key);
            assert!(change.weakens_protection, "{key}");
        }
        assert!(preview.weakens_protection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn harmless_changes_apply_without_acceptance() {
        let dir = temp_dir("harmless");
        let mine = Settings::default();
        let mut theirs = Settings::default();
        theirs.theme = crate::services::db::settings::Theme::Midnight;
        theirs.ui_scale = 125;
        theirs.auto_clean_free_space_percent = 10; // lower is more careful
        let path = write(&dir, &theirs);

        let preview = preview(&mine, &path).unwrap();
        assert_eq!(preview.changes.len(), 3);
        assert!(!preview.weakens_protection);
        let next = prepare_apply(&mine, &path, &preview.token, false).unwrap();
        assert_eq!(next.ui_scale, 125);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_changed_after_review_is_refused() {
        let dir = temp_dir("token");
        let mine = Settings::default();
        let path = write(&dir, &mine);
        let preview = preview(&mine, &path).unwrap();

        let mut theirs = Settings::default();
        theirs.auto_clean_enabled = true;
        write(&dir, &theirs);
        assert!(prepare_apply(&mine, &path, &preview.token, true).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn privacy_switches_stay_off_whatever_the_file_says() {
        let dir = temp_dir("privacy");
        let text = format!(
            r#"{{"format":"{FORMAT}","schema_version":1,"application_version":"9.9.9","exported_at":"","settings":{{"telemetry_enabled":true,"cloud_services_enabled":true,"crash_reporting_enabled":true}}}}"#
        );
        let path = dir.join("hostile.json");
        std::fs::write(&path, text).unwrap();
        let mine = Settings::default();
        let preview = preview(&mine, &path).unwrap();
        let next = prepare_apply(&mine, &path, &preview.token, true).unwrap();
        assert!(!next.telemetry_enabled && !next.cloud_services_enabled && !next.crash_reporting_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreign_and_future_files_are_refused() {
        let dir = temp_dir("foreign");
        let mine = Settings::default();
        let cases = [
            ("notes.json", "my own notes".to_string()),
            ("diag.json", r#"{"application":{"name":"AllInsight"}}"#.to_string()),
            ("future.json", format!(r#"{{"format":"{FORMAT}","schema_version":99,"settings":{{}}}}"#)),
        ];
        for (name, body) in cases {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            assert!(preview(&mine, &path).is_err(), "{name}");
            assert!(!is_config_file(&path) || name == "future.json");
        }
        assert!(preview(&mine, &dir.join("absent.json")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn daily_backups_are_written_once_a_day_and_pruned() {
        let dir = temp_dir("backups");
        for day in 1..=9 {
            std::fs::write(dir.join(format!("{DAILY_PREFIX}2026-01-0{day}.json")), "{}").unwrap();
        }
        std::fs::write(dir.join("someone-elses.json"), "{}").unwrap();

        let s = Settings::default();
        assert!(ensure_daily_backup(&dir, &s).unwrap().is_some());
        assert!(ensure_daily_backup(&dir, &s).unwrap().is_none());

        let dailies = backup_names(&dir, DAILY_PREFIX);
        assert_eq!(dailies.len(), DAILY_KEEP);
        assert!(dir.join("someone-elses.json").exists());
        assert!(!dir.join(format!("{DAILY_PREFIX}2026-01-01.json")).exists());

        let backup = backup_before_import(&dir, &s).unwrap();
        assert!(is_config_file(&backup));
        let listed = list_backups(&dir);
        assert_eq!(listed.len(), DAILY_KEEP + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

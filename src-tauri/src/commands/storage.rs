//! Storage commands: volumes, scanning, the treemap, large files and
//! duplicates.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AllInsightError, Result};
use crate::services::security::paths;
use crate::services::storage::{
    self, duplicates, large_files, scanner, DuplicateQuery, DuplicateReport, LargeFileQuery,
    LargeFileReport, ScanOptions, ScanProgress, StorageOverview, TreemapNode,
};
use crate::state::AppState;

use super::{EVENT_SCAN_COMPLETE, EVENT_SCAN_PROGRESS};

/// Releases the "a scan is running" flag however the worker thread ends.
///
/// Release builds unwind rather than abort, so a panic inside a worker would
/// otherwise leave the flag set and every later scan would be refused with
/// "A scan is already running" until AllInsight was restarted.
struct ScanGuard(AppHandle);

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.0.state::<AppState>().end_scan();
    }
}

#[tauri::command]
pub async fn get_storage_overview() -> StorageOverview {
    storage::overview()
}

/// A summary of the cached scan for one root, without shipping the tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub root: PathBuf,
    pub total_bytes: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub skipped_dirs: u64,
    pub skipped_links: u64,
    pub duration_ms: u64,
    pub completed_at: i64,
    pub cancelled: bool,
    pub categories: Vec<storage::CategoryTotal>,
    pub top_folders: Vec<TreemapNode>,
}

/// Validate a root the user picked before any walking starts.
fn validate_root(root: &str) -> Result<PathBuf> {
    let path = paths::normalize_lexical(std::path::Path::new(root));
    if !path.is_absolute() {
        return Err(AllInsightError::InvalidInput(
            "Choose a full path such as C:\\ or D:\\Projects.".into(),
        ));
    }
    if !path.exists() {
        return Err(AllInsightError::NotFound(path));
    }
    if !path.is_dir() {
        return Err(AllInsightError::InvalidInput(
            "Scanning starts from a folder, not a file.".into(),
        ));
    }
    Ok(path)
}

#[tauri::command]
pub async fn scan_directory(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> Result<()> {
    let path = validate_root(&root)?;
    let progress = Arc::new(ScanProgress::default());
    state.begin_scan(progress.clone())?;

    let handle = app.clone();
    let reporter = progress.clone();
    // A short-lived thread that pushes counters to the interface while the
    // walk runs, so the UI never has to poll.
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let snapshot = reporter.snapshot();
        if handle.emit(EVENT_SCAN_PROGRESS, &snapshot).is_err() {
            return;
        }
        if Arc::strong_count(&reporter) <= 1 {
            return;
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let _release = ScanGuard(handle.clone());
        let state = handle.state::<AppState>();
        let settings = state.settings();
        let mut options = ScanOptions::for_root(&path);
        options.large_file_threshold = settings.large_file_threshold_bytes.min(100 * 1024 * 1024);

        let result = scanner::scan(options, progress);
        let key = path.to_string_lossy().into_owned();

        let _ = state.db.record_scan(
            &key,
            result.total_bytes,
            result.total_files,
            result.duration_ms,
        );
        let _ = state.db.log_activity(
            "scan",
            &format!(
                "Scanned {} - {} across {} files",
                key,
                storage::format_bytes(result.total_bytes),
                result.total_files
            ),
            None,
        );

        let summary = summarise(&result);
        state.scans.write().insert(key, result);
        drop(_release);
        let _ = handle.emit(EVENT_SCAN_COMPLETE, &summary);
    });

    Ok(())
}

fn summarise(result: &storage::ScanResult) -> ScanSummary {
    ScanSummary {
        root: result.root.clone(),
        total_bytes: result.total_bytes,
        total_files: result.total_files,
        total_dirs: result.total_dirs,
        skipped_dirs: result.skipped_dirs,
        skipped_links: result.skipped_links,
        duration_ms: result.duration_ms,
        completed_at: result.completed_at,
        cancelled: result.cancelled,
        categories: result.totals.ranked(),
        top_folders: scanner::treemap_level(result, &result.root)
            .into_iter()
            .take(12)
            .collect(),
    }
}

#[tauri::command]
pub async fn get_scan_summary(
    state: State<'_, AppState>,
    root: String,
) -> Result<Option<ScanSummary>> {
    Ok({
        let key = paths::normalize_lexical(std::path::Path::new(&root))
            .to_string_lossy()
            .into_owned();
        let scans = state.scans.read();
        scans
            .get(&key)
            .or_else(|| scans.values().next())
            .map(summarise)
    })
}

#[tauri::command]
pub fn get_scan_progress(state: State<'_, AppState>) -> Option<scanner::ScanProgressSnapshot> {
    state.scan_progress.read().as_ref().map(|p| p.snapshot())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.cancel_scan();
}

/// One level of the treemap. The whole tree is never sent to the frontend:
/// a machine with millions of files would produce a payload no browser can
/// hold, so drill-down asks for exactly the level it is about to draw.
#[tauri::command]
pub async fn get_treemap_level(
    state: State<'_, AppState>,
    root: String,
    path: String,
) -> Result<Vec<TreemapNode>> {
    let key = paths::normalize_lexical(std::path::Path::new(&root))
        .to_string_lossy()
        .into_owned();
    let target = paths::normalize_lexical(std::path::Path::new(&path));

    let scans = state.scans.read();
    let result = scans
        .get(&key)
        .or_else(|| scans.values().next())
        .ok_or_else(|| {
            AllInsightError::InvalidInput("Run a storage scan before opening the map.".into())
        })?;

    // Drill-down is confined to the scanned tree.
    if !paths::is_within(&target, &result.root) {
        return Err(AllInsightError::InvalidInput(
            "That folder is outside the scanned area.".into(),
        ));
    }

    Ok(scanner::treemap_level(result, &target))
}

#[tauri::command]
pub async fn find_large_files(
    app: AppHandle,
    state: State<'_, AppState>,
    roots: Vec<String>,
    min_bytes: u64,
) -> Result<()> {
    let mut validated = Vec::new();
    for root in &roots {
        validated.push(validate_root(root)?);
    }
    if validated.is_empty() {
        validated.extend(dirs::home_dir());
    }

    let progress = Arc::new(ScanProgress::default());
    state.begin_scan(progress.clone())?;

    let handle = app.clone();
    let reporter = progress.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if handle
            .emit(EVENT_SCAN_PROGRESS, &reporter.snapshot())
            .is_err()
        {
            return;
        }
        if Arc::strong_count(&reporter) <= 1 {
            return;
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let _release = ScanGuard(handle.clone());
        let state = handle.state::<AppState>();
        let report = {
            let protected = state.protected();
            large_files::find(
                LargeFileQuery {
                    roots: validated,
                    min_bytes: min_bytes.max(1024 * 1024),
                    limit: 1000,
                },
                &protected,
                progress,
            )
        };
        *state.large_files.write() = Some(report);
        drop(_release);
        let _ = handle.emit(EVENT_SCAN_COMPLETE, "large-files");
    });

    Ok(())
}

#[tauri::command]
pub async fn get_large_files(state: State<'_, AppState>) -> Result<Option<LargeFileReport>> {
    Ok(state.large_files.read().clone())
}

#[tauri::command]
pub async fn find_duplicates(
    app: AppHandle,
    state: State<'_, AppState>,
    roots: Vec<String>,
    min_bytes: u64,
) -> Result<()> {
    let mut validated = Vec::new();
    for root in &roots {
        validated.push(validate_root(root)?);
    }
    if validated.is_empty() {
        validated.extend(dirs::home_dir());
    }

    let progress = Arc::new(ScanProgress::default());
    state.begin_scan(progress.clone())?;

    let handle = app.clone();
    let reporter = progress.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if handle
            .emit(EVENT_SCAN_PROGRESS, &reporter.snapshot())
            .is_err()
        {
            return;
        }
        if Arc::strong_count(&reporter) <= 1 {
            return;
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let _release = ScanGuard(handle.clone());
        let state = handle.state::<AppState>();
        let report = {
            let protected = state.protected();
            duplicates::find(
                DuplicateQuery {
                    roots: validated,
                    min_bytes: min_bytes.max(4096),
                    max_groups: 500,
                },
                &protected,
                progress,
            )
        };
        *state.duplicates.write() = Some(report);
        drop(_release);
        let _ = handle.emit(EVENT_SCAN_COMPLETE, "duplicates");
    });

    Ok(())
}

#[tauri::command]
pub async fn get_duplicates(state: State<'_, AppState>) -> Result<Option<DuplicateReport>> {
    Ok(state.duplicates.read().clone())
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumeTrend {
    pub mount_point: String,
    pub samples: Vec<crate::services::db::VolumeSample>,
}

#[tauri::command]
pub async fn get_volume_trend(
    state: State<'_, AppState>,
    mount_point: String,
) -> Result<VolumeTrend> {
    Ok(VolumeTrend {
        samples: state.db.volume_trend(&mount_point, 120)?,
        mount_point,
    })
}

/// Open a folder in File Explorer, selecting the file when one was given.
///
/// The path is checked for existence and normalised, and the shell is never
/// involved: `explorer.exe` is started directly with its own argument vector.
#[tauri::command]
pub async fn show_in_explorer(path: String) -> Result<()> {
    let target = paths::normalize_lexical(std::path::Path::new(&path));
    if !target.is_absolute() {
        return Err(AllInsightError::InvalidInput(
            "That location is not valid.".into(),
        ));
    }
    if !target.exists() {
        return Err(AllInsightError::NotFound(target));
    }

    #[cfg(windows)]
    {
        let mut command = std::process::Command::new("explorer.exe");
        if target.is_dir() {
            command.arg(&target);
        } else {
            command.arg(format!("/select,{}", target.display()));
        }
        command
            .spawn()
            .map_err(|e| AllInsightError::Platform(format!("Could not open File Explorer: {e}")))?;
    }

    // Elsewhere the opener plugin asks the file manager over its standard
    // interface (FileManager1 on Linux desktops, Finder on macOS) to show
    // the item, and falls back to opening its folder.
    #[cfg(not(windows))]
    {
        let result = if target.is_dir() {
            tauri_plugin_opener::open_path(&target, None::<&str>)
        } else {
            tauri_plugin_opener::reveal_item_in_dir(&target)
        };
        result.map_err(|e| {
            AllInsightError::Platform(format!("Could not open the file manager: {e}"))
        })?;
    }

    Ok(())
}

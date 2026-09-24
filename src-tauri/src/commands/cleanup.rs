//! Cleanup commands.
//!
//! This is the narrowest part of the IPC surface on purpose. Three things
//! cross the boundary: category variants, backend-issued candidate ids, and a
//! confirmation flag. A path never does.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AllInsightError, Result};
use crate::services::cleanup::{
    self, CleanupCategory, CleanupOutcome, CleanupPreview, CleanupRequest,
};
use crate::services::db::CleanupHistoryEntry;
use crate::services::storage::format_bytes;
use crate::state::{AppState, CleanupOwner};

/// Run discovery and cache the result. Returns the dry-run figures the
/// Cleanup screen shows before anything is confirmed.
#[tauri::command]
pub async fn get_cleanup_candidates(
    state: State<'_, AppState>,
    categories: Option<Vec<CleanupCategory>>,
) -> Result<CleanupPreview> {
    let _lease = state.begin_cleanup(CleanupOwner::User)?;
    state.cleanup_cancel.store(false, Ordering::SeqCst);

    let protected = state.protected();
    let (preview, scan) =
        cleanup::discover(&protected, categories.as_deref(), &state.cleanup_cancel);

    *state.cleanup_scan.write() = Arc::new(scan);
    *state.cleanup_preview.write() = Some(preview.clone());
    Ok(preview)
}

/// The dry run. Same code path as execution, with removal switched off, so the
/// preview and the action can never disagree.
#[tauri::command]
pub async fn preview_cleanup(
    state: State<'_, AppState>,
    scan_id: u64,
    categories: Vec<CleanupCategory>,
    candidate_ids: Vec<String>,
) -> Result<CleanupOutcome> {
    let protected = state.protected();
    let scan = state.cleanup_scan();
    cleanup::execute(
        &protected,
        &scan,
        &CleanupRequest {
            scan_id,
            categories,
            candidate_ids,
            confirmed: false,
        },
        &state.cleanup_cancel,
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteArgs {
    pub scan_id: u64,
    #[serde(default)]
    pub categories: Vec<CleanupCategory>,
    #[serde(default)]
    pub candidate_ids: Vec<String>,
    /// Must be true. Present so a mis-wired call cannot delete by accident.
    pub confirmed: bool,
}

#[tauri::command]
pub async fn execute_cleanup(state: State<'_, AppState>, args: ExecuteArgs) -> Result<CleanupOutcome> {
    if !args.confirmed {
        return Err(AllInsightError::InvalidInput(
            "Cleanup was not confirmed, so nothing was removed.".into(),
        ));
    }
    if args.categories.is_empty() && args.candidate_ids.is_empty() {
        return Err(AllInsightError::InvalidInput(
            "Choose at least one category to clean.".into(),
        ));
    }

    let _lease = state.begin_cleanup(CleanupOwner::User)?;
    state.cleanup_cancel.store(false, Ordering::SeqCst);

    let outcome = {
        let protected = state.protected();
        let scan = state.cleanup_scan();
        cleanup::execute(
            &protected,
            &scan,
            &CleanupRequest {
                scan_id: args.scan_id,
                categories: args.categories.clone(),
                candidate_ids: args.candidate_ids.clone(),
                confirmed: true,
            },
            &state.cleanup_cancel,
        )?
    };

    record(&state, &outcome, "manual");

    // The candidate list is now stale: the files it points at are gone.
    *state.cleanup_scan.write() = Arc::new(Default::default());
    *state.cleanup_preview.write() = None;

    Ok(outcome)
}

/// Write the run to the history table. Category names only, never paths.
pub fn record(state: &AppState, outcome: &CleanupOutcome, trigger: &str) {
    let entry = CleanupHistoryEntry {
        ran_at: outcome.finished_at,
        trigger: trigger.to_string(),
        reclaimed_bytes: outcome.reclaimed_bytes,
        removed_items: outcome.removed_items,
        skipped_items: outcome.skipped_items + outcome.protected_items + outcome.failed_items,
        categories: outcome.categories.iter().map(|c| c.name.clone()).collect(),
    };
    let _ = state.db.record_cleanup(&entry);
    let _ = state.db.log_activity(
        "cleanup",
        &format!(
            "{} cleanup reclaimed {}",
            if trigger == "auto" { "Automatic" } else { "Manual" },
            format_bytes(outcome.reclaimed_bytes)
        ),
        None,
    );
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryDescription {
    pub id: CleanupCategory,
    pub name: String,
    pub description: String,
    pub what_happens: String,
    pub what_is_untouched: String,
    pub requires_elevation: bool,
    pub auto_clean_eligible: bool,
    pub present: bool,
    pub roots: Vec<String>,
}

/// The catalogue, for Settings. Shows exactly which folders each category is
/// allowed to touch, so the safety model is inspectable rather than implied.
#[tauri::command]
pub async fn get_cleanup_categories() -> Vec<CategoryDescription> {
    cleanup::definitions()
        .into_iter()
        .map(|d| CategoryDescription {
            id: d.id,
            name: d.name.to_string(),
            description: d.description.to_string(),
            what_happens: d.what_happens.to_string(),
            what_is_untouched: d.what_is_untouched.to_string(),
            requires_elevation: d.requires_elevation,
            auto_clean_eligible: d.auto_clean_eligible,
            present: d.is_present(),
            roots: d
                .roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect(),
        })
        .collect()
}

#[tauri::command]
pub async fn get_cleanup_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<CleanupHistoryEntry>> {
    state.db.cleanup_history(limit.unwrap_or(50).min(500))
}

#[tauri::command]
pub async fn get_cleanup_totals(state: State<'_, AppState>) -> Result<crate::services::db::CleanupTotals> {
    state.db.cleanup_totals()
}

#[tauri::command]
pub async fn get_recycle_bin_state() -> Result<cleanup::RecycleBinState> {
    cleanup::recycle_bin::query()
}

/// Send one file to the Recycle Bin from the Large Files or Duplicates screen.
///
/// These screens deal in the user's own files, which no cleanup category is
/// allowed to touch, so this path is deliberately separate: it only accepts a
/// file the backend itself listed, it refuses anything on the protected list,
/// and it always uses the Recycle Bin so the action can be undone.
#[tauri::command]
pub async fn recycle_reviewed_file(state: State<'_, AppState>, path: String) -> Result<()> {
    use crate::services::security::paths;

    let target = paths::normalize_lexical(std::path::Path::new(&path));

    // The path must be one this backend produced, not one the frontend
    // invented. Both review screens keep their results in state.
    let known = {
        let large = state.large_files.read();
        let duplicates = state.duplicates.read();
        let in_large = large
            .as_ref()
            .map(|r| r.entries.iter().any(|e| paths::same_path(&e.path, &target)))
            .unwrap_or(false);
        let in_duplicates = duplicates
            .as_ref()
            .map(|r| {
                r.groups
                    .iter()
                    .any(|g| g.files.iter().any(|f| paths::same_path(&f.path, &target)))
            })
            .unwrap_or(false);
        in_large || in_duplicates
    };
    if !known {
        return Err(AllInsightError::UnknownCandidate(
            "That file was not part of the last scan. Run the scan again.".into(),
        ));
    }

    let verdict = state.protected().classify(&target);
    if verdict.protected {
        return Err(AllInsightError::Protected {
            path: target,
            reason: verdict.describe(),
        });
    }

    if paths::is_reparse_point(&target) {
        return Err(AllInsightError::InvalidInput(
            "That entry is a link and will not be removed.".into(),
        ));
    }
    let meta = std::fs::symlink_metadata(paths::long_path(&target))
        .map_err(|_| AllInsightError::NotFound(target.clone()))?;
    if !meta.is_file() {
        return Err(AllInsightError::InvalidInput(
            "Only files can be removed from this screen.".into(),
        ));
    }

    trash::delete(&target).map_err(|e| {
        AllInsightError::Other(format!(
            "Could not move that file to the {}: {e}",
            crate::platform::trash_name()
        ))
    })?;

    let _ = state.db.log_activity(
        "review",
        &format!(
            "Moved a reviewed file of {} to the {}",
            format_bytes(meta.len()),
            crate::platform::trash_name()
        ),
        None,
    );
    Ok(())
}

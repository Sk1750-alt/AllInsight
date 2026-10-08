//! The cleanup engine.
//!
//! The contract with the frontend, stated plainly:
//!
//! * The frontend never sends a path. It sends a category variant, or an
//!   opaque candidate id that this module handed out earlier.
//! * A candidate id is only valid for the scan that produced it. A stale id
//!   from a previous scan is rejected, not resolved.
//! * Every candidate is re-validated through [`DeletionGuard`] immediately
//!   before removal, so a path that became protected, became a link, or became
//!   a different kind of entry between the preview and the confirmation is
//!   skipped rather than deleted.
//! * A dry run is always produced first, and it is the same code path that
//!   execution uses, so the preview cannot drift from the action.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::categories::{
    definitions, CategoryDefinition, CleanupCategory, DeletionMode, MatchRule,
};
use crate::error::{AllInsightError, Result};
use crate::services::security::guard::EntryKind;
use crate::services::security::{paths, DeletionGuard, GuardRejection, ProtectedPaths};

/// Monotonic id for each discovery run, so candidate ids cannot be replayed
/// against a later scan.
static SCAN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Upper bound on individually tracked candidates across one discovery run.
///
/// A cache directory can hold hundreds of thousands of files, and keeping a
/// record of every one costs memory for no benefit: the user acts on
/// categories, not on individual cache files. Category totals continue to
/// accumulate past this point, so the figures shown stay correct.
const MAX_CANDIDATES: usize = 60_000;

/// One removable entry, as offered to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupCandidate {
    /// Opaque handle. The frontend passes this back; it never sees a decision
    /// made from a path it supplied.
    pub id: String,
    pub category: CleanupCategory,
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub modified: Option<i64>,
    pub kind: EntryKind,
}

/// Aggregated result for one category, which is what the Cleanup screen shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryReport {
    pub category: CleanupCategory,
    pub name: String,
    pub description: String,
    pub what_happens: String,
    pub what_is_untouched: String,
    pub bytes: u64,
    pub items: u64,
    pub requires_elevation: bool,
    pub auto_clean_eligible: bool,
    pub deletion: DeletionMode,
    pub available: bool,
    /// Set when the category could not be measured fully, e.g. because parts
    /// of it need elevation.
    pub note: Option<String>,
    /// A handful of examples, for the Review screen. Never the whole list.
    pub samples: Vec<CleanupCandidate>,
}

/// The dry run. Produced before every execution, including the automatic one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub scan_id: u64,
    pub categories: Vec<CategoryReport>,
    pub total_bytes: u64,
    pub total_items: u64,
    /// Entries the guard refused. Counted so the numbers add up for the user.
    pub protected_items: u64,
    pub skipped_items: u64,
    pub generated_at: i64,
    pub elevated: bool,
}

/// What actually happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupOutcome {
    pub reclaimed_bytes: u64,
    pub removed_items: u64,
    pub skipped_items: u64,
    pub protected_items: u64,
    pub failed_items: u64,
    pub categories: Vec<CategoryOutcome>,
    /// Human-readable reasons, deduplicated and capped. Never a file listing.
    pub notes: Vec<String>,
    pub finished_at: i64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryOutcome {
    pub category: CleanupCategory,
    pub name: String,
    pub reclaimed_bytes: u64,
    pub removed_items: u64,
    pub skipped_items: u64,
}

/// Discovery state held by the backend between preview and execution.
#[derive(Debug, Default)]
pub struct CleanupScan {
    pub scan_id: u64,
    pub candidates: HashMap<String, CleanupCandidate>,
    pub order: Vec<String>,
}

impl CleanupScan {
    pub fn candidate(&self, id: &str) -> Option<&CleanupCandidate> {
        self.candidates.get(id)
    }

    pub fn by_category(&self, category: CleanupCategory) -> Vec<&CleanupCandidate> {
        self.order
            .iter()
            .filter_map(|id| self.candidates.get(id))
            .filter(|c| c.category == category)
            .collect()
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn modified_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

fn matches_rule(rule: &MatchRule, path: &Path) -> bool {
    match rule {
        MatchRule::AllContents => true,
        MatchRule::Extensions(exts) => {
            let ext = paths::extension_lower(path);
            exts.contains(&ext.as_str())
        }
        MatchRule::NamePrefixes(prefixes) => {
            let name = paths::file_name_lower(path);
            prefixes.iter().any(|p| name.starts_with(p))
        }
        MatchRule::ShellManaged => false,
    }
}

/// What one root contributed. Merged as the parallel walk unwinds.
#[derive(Default)]
struct RootFindings {
    bytes: u64,
    items: u64,
    protected: u64,
    skipped: u64,
    /// Path, size and modification time. Ids are assigned afterwards, in a
    /// deterministic order, so a parallel walk cannot make them arbitrary.
    candidates: Vec<(PathBuf, u64, Option<i64>)>,
    needs_elevation: bool,
    truncated: bool,
}

impl RootFindings {
    fn merge(&mut self, other: RootFindings) {
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.items = self.items.saturating_add(other.items);
        self.protected = self.protected.saturating_add(other.protected);
        self.skipped = self.skipped.saturating_add(other.skipped);
        self.needs_elevation |= other.needs_elevation;
        self.truncated |= other.truncated;
        if self.candidates.len() < MAX_CANDIDATES {
            self.candidates.extend(other.candidates);
        } else {
            self.truncated = true;
        }
    }
}

/// Whether the current user owns an entry, and so may meaningfully clean it.
#[cfg(unix)]
fn owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    meta.uid() == unsafe { libc::geteuid() }
}

/// Walk one directory and everything beneath it.
///
/// Parallel across sibling directories, in the same shape as the storage
/// scanner. A browser cache holds tens of thousands of small files, and
/// walking those one at a time is the difference between a screen that fills
/// in promptly and one that sits on a skeleton for half a minute.
///
/// Only the lexical half of the guard runs here. The full sequence, including
/// canonicalisation and the reparse-point walk, runs again immediately before
/// anything is removed, which is the only moment at which a filesystem check
/// actually means anything.
fn walk_root(
    definition: &CategoryDefinition,
    dir: &Path,
    guard: &DeletionGuard<'_>,
    cutoff: i64,
    depth: u32,
    cancelled: &AtomicBool,
) -> RootFindings {
    let mut findings = RootFindings::default();

    if cancelled.load(Ordering::Relaxed) {
        return findings;
    }
    // A cache directory is never this deep. The limit stops a crafted or
    // corrupted tree from recursing without end.
    if depth > 32 {
        findings.truncated = true;
        return findings;
    }

    let entries = match std::fs::read_dir(paths::long_path(dir)) {
        Ok(e) => e,
        Err(e) => {
            findings.skipped += 1;
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                findings.needs_elevation = true;
            }
            return findings;
        }
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        if cancelled.load(Ordering::Relaxed) {
            return findings;
        }
        let Ok(file_type) = entry.file_type() else {
            findings.skipped += 1;
            continue;
        };
        let path = paths::strip_verbatim(&entry.path());

        // Links are never followed and never removed. The reparse tag is
        // already in the directory listing, so `is_symlink` costs nothing;
        // directories get one extra check because descending into a link is
        // the expensive mistake.
        let is_link =
            file_type.is_symlink() || (file_type.is_dir() && paths::is_reparse_point(&path));
        if is_link {
            findings.skipped += 1;
            continue;
        }

        // On Unix a shared folder such as `/tmp` holds other users' files,
        // the system's private directories, and sockets a running session
        // depends on. Only the user's own regular files and folders are ours
        // to consider; the rest is not reported at all, because it was never
        // a candidate.
        #[cfg(unix)]
        {
            let Ok(meta) = entry.metadata() else {
                findings.skipped += 1;
                continue;
            };
            if !(file_type.is_dir() || file_type.is_file()) || !owned_by_current_user(&meta) {
                continue;
            }
        }

        if file_type.is_dir() {
            subdirs.push(path);
            continue;
        }

        if !matches_rule(&definition.rule, &path) {
            continue;
        }

        let Ok(meta) = entry.metadata() else {
            findings.skipped += 1;
            continue;
        };
        let modified = modified_secs(&meta);
        if let Some(m) = modified {
            if m > cutoff {
                // Still fresh: something may still be using it.
                findings.skipped += 1;
                continue;
            }
        }

        match guard.permits_lexically(&path) {
            Ok(normalized) => {
                // Sizes keep accumulating past the cap so the total stays
                // honest; only the per-file list stops growing.
                findings.bytes = findings.bytes.saturating_add(meta.len());
                findings.items += 1;
                if findings.candidates.len() < MAX_CANDIDATES {
                    findings.candidates.push((normalized, meta.len(), modified));
                } else {
                    findings.truncated = true;
                }
            }
            Err(GuardRejection::Protected { .. }) => findings.protected += 1,
            Err(_) => findings.skipped += 1,
        }
    }

    let below: Vec<RootFindings> = subdirs
        .into_par_iter()
        .map(|child| walk_root(definition, &child, guard, cutoff, depth + 1, cancelled))
        .collect();

    for child in below {
        findings.merge(child);
    }

    findings
}

/// Run discovery across every category, or only the ones requested.
pub fn discover(
    protected: &ProtectedPaths,
    only: Option<&[CleanupCategory]>,
    cancelled: &AtomicBool,
) -> (CleanupPreview, CleanupScan) {
    let scan_id = SCAN_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut scan = CleanupScan {
        scan_id,
        ..Default::default()
    };
    let elevated = crate::services::security::is_elevated();
    let now = now_secs();

    let mut reports = Vec::new();
    let mut total_bytes = 0u64;
    let mut total_items = 0u64;
    let mut protected_items = 0u64;
    let mut skipped_items = 0u64;

    // Categories are independent, so they are measured together. Done one at a
    // time this takes tens of seconds on a machine with a large browser cache.
    let selected: Vec<CategoryDefinition> = definitions()
        .into_iter()
        .filter(|d| only.map(|f| f.contains(&d.id)).unwrap_or(true))
        .collect();

    let measured: Vec<(CategoryDefinition, RootFindings, Option<String>)> = selected
        .into_par_iter()
        .map(|definition| {
            let mut findings = RootFindings::default();
            let mut note: Option<String> = None;

            if definition.rule == MatchRule::ShellManaged {
                // The Recycle Bin is measured through Windows rather than
                // walked: its layout on disk is an implementation detail, and
                // its folder is on the protected list.
                match super::recycle_bin::query() {
                    Ok(state) => {
                        findings.bytes = state.bytes;
                        findings.items = state.items;
                    }
                    Err(e) => note = Some(e.to_string()),
                }
            } else if definition.is_present() {
                if definition.requires_elevation && !elevated {
                    note = Some(
                        "Administrator permission is required to measure and clean all of this category."
                            .into(),
                    );
                }
                let guard = DeletionGuard::new(protected, &definition.roots)
                    .with_name_exemptions(definition.name_exemptions);
                let cutoff = now - (definition.min_age_hours as i64) * 3600;

                for root in &definition.roots {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    let root_findings = walk_root(&definition, root, &guard, cutoff, 0, cancelled);
                    findings.merge(root_findings);
                }
            }

            if note.is_none() && findings.needs_elevation {
                note = Some(
                    "Part of this category belongs to the system and needs administrator permission to measure."
                        .into(),
                );
            }
            if note.is_none() && findings.truncated {
                note = Some("This category is unusually large and was only partly listed.".into());
            }

            (definition, findings, note)
        })
        .collect();

    // Ids are handed out here, on one thread, so they stay stable and
    // reproducible however the walk happened to be scheduled.
    for (definition, findings, note) in measured {
        total_bytes = total_bytes.saturating_add(findings.bytes);
        total_items = total_items.saturating_add(findings.items);
        protected_items = protected_items.saturating_add(findings.protected);
        skipped_items = skipped_items.saturating_add(findings.skipped);

        for (path, size, modified) in findings.candidates {
            if scan.order.len() >= MAX_CANDIDATES {
                break;
            }
            let id = format!("{}:{}", scan.scan_id, scan.order.len());
            scan.order.push(id.clone());
            scan.candidates.insert(
                id.clone(),
                CleanupCandidate {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    size_bytes: size,
                    modified,
                    kind: EntryKind::File,
                    category: definition.id,
                    path,
                    id,
                },
            );
        }

        let samples: Vec<CleanupCandidate> = scan
            .by_category(definition.id)
            .into_iter()
            .take(8)
            .cloned()
            .collect();

        reports.push(CategoryReport {
            category: definition.id,
            name: definition.name.to_string(),
            description: definition.description.to_string(),
            what_happens: definition.what_happens.to_string(),
            what_is_untouched: definition.what_is_untouched.to_string(),
            bytes: findings.bytes,
            items: findings.items,
            requires_elevation: definition.requires_elevation,
            auto_clean_eligible: definition.auto_clean_eligible,
            deletion: definition.deletion,
            available: definition.is_present(),
            note,
            samples,
        });
    }

    reports.sort_by_key(|e| std::cmp::Reverse(e.bytes));

    (
        CleanupPreview {
            scan_id,
            categories: reports,
            total_bytes,
            total_items,
            protected_items,
            skipped_items,
            generated_at: now,
            elevated,
        },
        scan,
    )
}

/// What the caller asked to remove. Categories and ids only: there is no
/// variant of this type that carries a path.
#[derive(Debug, Clone, Deserialize)]
pub struct CleanupRequest {
    pub scan_id: u64,
    #[serde(default)]
    pub categories: Vec<CleanupCategory>,
    #[serde(default)]
    pub candidate_ids: Vec<String>,
    /// The user pressed the confirm button. A request without this only ever
    /// produces a dry run.
    #[serde(default)]
    pub confirmed: bool,
}

fn push_note(notes: &mut Vec<String>, note: String) {
    if notes.len() < 12 && !notes.contains(&note) {
        notes.push(note);
    }
}

/// Execute, or dry-run, a cleanup request.
///
/// Every candidate is validated again here. That repetition is the point: the
/// preview may be minutes old, and the filesystem does not hold still.
pub fn execute(
    protected: &ProtectedPaths,
    scan: &CleanupScan,
    request: &CleanupRequest,
    cancelled: &AtomicBool,
) -> Result<CleanupOutcome> {
    if request.scan_id != scan.scan_id {
        return Err(AllInsightError::InvalidInput(
            "This cleanup preview is out of date. Run the scan again.".into(),
        ));
    }

    let dry_run = !request.confirmed;
    let definitions_by_id: HashMap<CleanupCategory, CategoryDefinition> =
        definitions().into_iter().map(|d| (d.id, d)).collect();

    // Resolve the request to a concrete candidate set, entirely from backend
    // state. An id that is not in this scan is an error, not a path.
    let mut selected: Vec<&CleanupCandidate> = Vec::new();
    for id in &request.candidate_ids {
        let candidate = scan
            .candidate(id)
            .ok_or_else(|| AllInsightError::UnknownCandidate(id.clone()))?;
        selected.push(candidate);
    }
    for category in &request.categories {
        selected.extend(scan.by_category(*category));
    }
    selected.sort_by(|a, b| a.id.cmp(&b.id));
    selected.dedup_by(|a, b| a.id == b.id);

    let mut outcome = CleanupOutcome {
        reclaimed_bytes: 0,
        removed_items: 0,
        skipped_items: 0,
        protected_items: 0,
        failed_items: 0,
        categories: Vec::new(),
        notes: Vec::new(),
        finished_at: 0,
        dry_run,
    };
    let mut per_category: HashMap<CleanupCategory, CategoryOutcome> = HashMap::new();

    for candidate in selected {
        if cancelled.load(Ordering::Relaxed) {
            push_note(&mut outcome.notes, "Cleanup was stopped early.".into());
            break;
        }
        let Some(definition) = definitions_by_id.get(&candidate.category) else {
            outcome.skipped_items += 1;
            continue;
        };

        let guard = DeletionGuard::new(protected, &definition.roots)
            .with_name_exemptions(definition.name_exemptions);

        let entry = per_category
            .entry(candidate.category)
            .or_insert_with(|| CategoryOutcome {
                category: candidate.category,
                name: definition.name.to_string(),
                reclaimed_bytes: 0,
                removed_items: 0,
                skipped_items: 0,
            });

        match guard.validate(&candidate.path, candidate.kind) {
            Ok(valid) => {
                if dry_run {
                    outcome.reclaimed_bytes =
                        outcome.reclaimed_bytes.saturating_add(valid.size_bytes());
                    outcome.removed_items += 1;
                    entry.reclaimed_bytes =
                        entry.reclaimed_bytes.saturating_add(valid.size_bytes());
                    entry.removed_items += 1;
                    continue;
                }
                let size = valid.size_bytes();
                match super::remove::remove(&valid, definition.deletion) {
                    Ok(()) => {
                        outcome.reclaimed_bytes = outcome.reclaimed_bytes.saturating_add(size);
                        outcome.removed_items += 1;
                        entry.reclaimed_bytes = entry.reclaimed_bytes.saturating_add(size);
                        entry.removed_items += 1;
                    }
                    Err(e) => {
                        outcome.failed_items += 1;
                        entry.skipped_items += 1;
                        push_note(&mut outcome.notes, e.to_string());
                    }
                }
            }
            Err(GuardRejection::Protected { reason, .. }) => {
                outcome.protected_items += 1;
                entry.skipped_items += 1;
                push_note(&mut outcome.notes, reason.explain().to_string());
            }
            Err(rejection) => {
                outcome.skipped_items += 1;
                entry.skipped_items += 1;
                push_note(&mut outcome.notes, rejection.describe());
            }
        }
    }

    // Shell-managed categories are handled after the file walk, because they
    // are one atomic operation rather than a list of entries.
    for category in &request.categories {
        let Some(definition) = definitions_by_id.get(category) else {
            continue;
        };
        if definition.deletion != DeletionMode::ShellApi {
            continue;
        }
        let state = super::recycle_bin::query().unwrap_or_default();
        let entry = per_category
            .entry(*category)
            .or_insert_with(|| CategoryOutcome {
                category: *category,
                name: definition.name.to_string(),
                reclaimed_bytes: 0,
                removed_items: 0,
                skipped_items: 0,
            });
        if dry_run {
            outcome.reclaimed_bytes = outcome.reclaimed_bytes.saturating_add(state.bytes);
            outcome.removed_items = outcome.removed_items.saturating_add(state.items);
            entry.reclaimed_bytes = state.bytes;
            entry.removed_items = state.items;
            continue;
        }
        match super::recycle_bin::empty() {
            Ok(()) => {
                outcome.reclaimed_bytes = outcome.reclaimed_bytes.saturating_add(state.bytes);
                outcome.removed_items = outcome.removed_items.saturating_add(state.items);
                entry.reclaimed_bytes = state.bytes;
                entry.removed_items = state.items;
            }
            Err(e) => {
                outcome.failed_items += 1;
                push_note(&mut outcome.notes, e.to_string());
            }
        }
    }

    outcome.categories = per_category.into_values().collect();
    outcome
        .categories
        .sort_by_key(|e| std::cmp::Reverse(e.reclaimed_bytes));
    outcome.finished_at = now_secs();
    Ok(outcome)
}

/// The categories Auto-Clean is allowed to run. Derived from the definitions
/// rather than configured, so a settings file cannot widen it.
pub fn auto_clean_categories() -> Vec<CleanupCategory> {
    definitions()
        .into_iter()
        .filter(|d| d.auto_clean_eligible)
        .map(|d| d.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn a_stale_scan_id_is_rejected() {
        let protected = ProtectedPaths::new(&[]);
        let scan = CleanupScan {
            scan_id: 7,
            ..Default::default()
        };
        let request = CleanupRequest {
            scan_id: 8,
            categories: vec![CleanupCategory::UserTemp],
            candidate_ids: vec![],
            confirmed: true,
        };
        let err = execute(&protected, &scan, &request, &no_cancel()).unwrap_err();
        assert!(matches!(err, AllInsightError::InvalidInput(_)));
    }

    #[test]
    fn an_unknown_candidate_id_is_rejected_rather_than_resolved() {
        let protected = ProtectedPaths::new(&[]);
        let scan = CleanupScan {
            scan_id: 1,
            ..Default::default()
        };
        let request = CleanupRequest {
            scan_id: 1,
            categories: vec![],
            candidate_ids: vec!["1:999".into()],
            confirmed: true,
        };
        let err = execute(&protected, &scan, &request, &no_cancel()).unwrap_err();
        assert!(matches!(err, AllInsightError::UnknownCandidate(_)));
    }

    /// A candidate whose path was tampered with in backend state must still be
    /// refused at execution time, because the guard re-checks rather than
    /// trusting the recorded verdict.
    #[test]
    fn a_candidate_pointing_outside_its_category_is_refused() {
        let system_file = if cfg!(windows) {
            PathBuf::from("C:\\Windows\\System32\\kernel32.dll")
        } else {
            PathBuf::from("/etc/hostname")
        };
        let protected = ProtectedPaths::new(&[]);
        let mut scan = CleanupScan {
            scan_id: 1,
            ..Default::default()
        };
        let id = "1:0".to_string();
        scan.order.push(id.clone());
        scan.candidates.insert(
            id.clone(),
            CleanupCandidate {
                id: id.clone(),
                category: CleanupCategory::UserTemp,
                path: system_file.clone(),
                name: "kernel32.dll".into(),
                size_bytes: 1,
                modified: None,
                kind: EntryKind::File,
            },
        );

        let request = CleanupRequest {
            scan_id: 1,
            categories: vec![],
            candidate_ids: vec![id],
            confirmed: true,
        };
        let outcome = execute(&protected, &scan, &request, &no_cancel()).unwrap();
        assert_eq!(outcome.removed_items, 0);
        assert_eq!(outcome.reclaimed_bytes, 0);
        assert!(outcome.protected_items + outcome.skipped_items >= 1);
        assert!(system_file.exists() || !cfg!(windows));
    }

    #[test]
    fn an_unconfirmed_request_never_deletes_anything() {
        let dir =
            std::env::temp_dir().join(format!("allinsight-cleanup-dry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("scratch.tmp");
        fs::write(&file, vec![0u8; 4096]).unwrap();
        let dir = paths::canonicalize(&dir).unwrap_or(dir);
        let file = dir.join("scratch.tmp");

        let protected = ProtectedPaths::new(&[]);
        let mut scan = CleanupScan {
            scan_id: 1,
            ..Default::default()
        };
        let id = "1:0".to_string();
        scan.order.push(id.clone());
        scan.candidates.insert(
            id.clone(),
            CleanupCandidate {
                id: id.clone(),
                category: CleanupCategory::UserTemp,
                path: file.clone(),
                name: "scratch.tmp".into(),
                size_bytes: 4096,
                modified: None,
                kind: EntryKind::File,
            },
        );

        let request = CleanupRequest {
            scan_id: 1,
            categories: vec![],
            candidate_ids: vec![id],
            confirmed: false,
        };
        let outcome = execute(&protected, &scan, &request, &no_cancel()).unwrap();
        assert!(outcome.dry_run);
        // The dry run reports what would happen but the file survives. It is
        // outside the UserTemp roots, so the guard also refuses it: either
        // way, nothing is removed.
        assert!(file.exists(), "a dry run must never delete");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_clean_never_includes_the_recycle_bin_or_elevated_categories() {
        let auto = auto_clean_categories();
        assert!(!auto.contains(&CleanupCategory::RecycleBin));
        assert!(!auto.contains(&CleanupCategory::WindowsUpdateCache));
        assert!(!auto.contains(&CleanupCategory::BrowserCache));
        assert!(auto.contains(&CleanupCategory::UserTemp));
    }

    #[test]
    fn discovery_produces_ids_bound_to_its_own_scan() {
        let protected = ProtectedPaths::new(&[]);
        let (preview, scan) = discover(
            &protected,
            Some(&[CleanupCategory::ThumbnailCache]),
            &no_cancel(),
        );
        assert_eq!(preview.scan_id, scan.scan_id);
        for id in &scan.order {
            assert!(id.starts_with(&format!("{}:", scan.scan_id)));
        }
    }

    #[test]
    fn match_rules_behave_as_declared() {
        assert!(matches_rule(
            &MatchRule::AllContents,
            &Path::new("a").join("b.bin")
        ));
        assert!(matches_rule(
            &MatchRule::Extensions(&["dmp"]),
            &Path::new("a").join("crash.DMP")
        ));
        assert!(!matches_rule(
            &MatchRule::Extensions(&["dmp"]),
            &Path::new("a").join("notes.txt")
        ));
        assert!(matches_rule(
            &MatchRule::NamePrefixes(&["thumbcache_"]),
            &Path::new("a").join("thumbcache_1024.db")
        ));
        assert!(!matches_rule(
            &MatchRule::NamePrefixes(&["thumbcache_"]),
            &Path::new("a").join("contacts.db")
        ));
        assert!(!matches_rule(
            &MatchRule::ShellManaged,
            &Path::new("a").join("b")
        ));
    }
}

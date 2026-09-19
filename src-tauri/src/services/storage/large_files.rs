//! The large file finder.
//!
//! A focused walk that records only files above a threshold. It does not build
//! a tree, so it is much cheaper than a full scan and can be re-run whenever
//! the user changes the size filter.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::Mutex;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::categories::{CategoryRules, StorageCategory};
use super::scanner::ScanProgress;
use crate::services::security::paths;
use crate::services::security::ProtectedPaths;

/// How risky it is to remove a given file. This is advisory: the deletion
/// guard makes the binding decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Regenerated automatically; removing it is routine.
    Low,
    /// Personal or project data. Removing it needs a deliberate decision.
    Review,
    /// On the protected list; AllInsight will not remove it at all.
    Protected,
}

impl RiskLevel {
    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low risk",
            RiskLevel::Review => "Review",
            RiskLevel::Protected => "Protected",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFileEntry {
    pub path: PathBuf,
    pub name: String,
    pub directory: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub extension: String,
    pub category: StorageCategory,
    pub risk: RiskLevel,
    /// Why it is protected, when it is.
    pub risk_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFileQuery {
    pub roots: Vec<PathBuf>,
    pub min_bytes: u64,
    pub limit: usize,
}

impl Default for LargeFileQuery {
    fn default() -> Self {
        Self {
            roots: dirs::home_dir().into_iter().collect(),
            min_bytes: 1024 * 1024 * 1024,
            limit: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFileReport {
    pub entries: Vec<LargeFileEntry>,
    pub total_bytes: u64,
    pub scanned_files: u64,
    pub skipped_dirs: u64,
    pub truncated: bool,
    pub cancelled: bool,
}

fn unix_seconds(t: std::io::Result<SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

struct Ctx<'a> {
    query: LargeFileQuery,
    rules: CategoryRules,
    protected: &'a ProtectedPaths,
    found: Mutex<Vec<LargeFileEntry>>,
    progress: Arc<ScanProgress>,
}

fn risk_for(protected: &ProtectedPaths, path: &Path, category: StorageCategory) -> (RiskLevel, Option<String>) {
    let verdict = protected.classify(path);
    if verdict.protected {
        return (RiskLevel::Protected, Some(verdict.describe()));
    }
    match category {
        StorageCategory::TemporaryFiles | StorageCategory::Cache => (RiskLevel::Low, None),
        _ => (RiskLevel::Review, None),
    }
}

fn walk(ctx: &Ctx<'_>, dir: &Path, depth: u32) {
    if ctx.progress.is_cancelled() || depth > 64 {
        return;
    }
    ctx.progress.dirs.fetch_add(1, Ordering::Relaxed);

    let Ok(entries) = std::fs::read_dir(paths::long_path(dir)) else {
        ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        if ctx.progress.is_cancelled() {
            return;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let path = paths::strip_verbatim(&entry.path());
        if ft.is_symlink() || (ft.is_dir() && paths::is_reparse_point(&path)) {
            continue;
        }
        if ft.is_dir() {
            subdirs.push(path);
            continue;
        }

        let Ok(meta) = entry.metadata() else { continue };
        let size = meta.len();
        ctx.progress.files.fetch_add(1, Ordering::Relaxed);
        if size < ctx.query.min_bytes {
            continue;
        }
        ctx.progress.bytes.fetch_add(size, Ordering::Relaxed);

        let category = ctx.rules.classify(&path);
        let (risk, risk_note) = risk_for(ctx.protected, &path, category);
        ctx.found.lock().push(LargeFileEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            directory: path.parent().map(Path::to_path_buf).unwrap_or_default(),
            extension: paths::extension_lower(&path),
            modified: unix_seconds(meta.modified()),
            accessed: unix_seconds(meta.accessed()),
            size_bytes: size,
            category,
            risk,
            risk_note,
            path,
        });
    }

    subdirs
        .into_par_iter()
        .for_each(|child| walk(ctx, &child, depth + 1));
}

/// Run the query. Roots that do not exist are skipped rather than failing the
/// whole search, because a removable drive can disappear mid-run.
pub fn find(
    query: LargeFileQuery,
    protected: &ProtectedPaths,
    progress: Arc<ScanProgress>,
) -> LargeFileReport {
    let ctx = Ctx {
        rules: CategoryRules::new(),
        protected,
        found: Mutex::new(Vec::new()),
        progress: progress.clone(),
        query,
    };

    for root in ctx.query.roots.clone() {
        if root.exists() {
            walk(&ctx, &paths::normalize_lexical(&root), 0);
        }
    }

    let mut entries = ctx.found.into_inner();
    entries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let truncated = entries.len() > ctx.query.limit;
    let total_bytes = entries.iter().map(|e| e.size_bytes).sum();
    entries.truncate(ctx.query.limit);

    LargeFileReport {
        entries,
        total_bytes,
        scanned_files: progress.files.load(Ordering::Relaxed),
        skipped_dirs: progress.errors.load(Ordering::Relaxed),
        truncated,
        cancelled: progress.is_cancelled(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sandbox(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("allinsight-large-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        paths::canonicalize(&base).unwrap_or(base)
    }

    #[test]
    fn only_files_at_or_above_the_threshold_are_reported() {
        let root = sandbox("threshold");
        fs::write(root.join("small.bin"), vec![0u8; 100]).unwrap();
        fs::write(root.join("exact.bin"), vec![0u8; 1000]).unwrap();
        fs::write(root.join("big.bin"), vec![0u8; 4000]).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let report = find(
            LargeFileQuery {
                roots: vec![root.clone()],
                min_bytes: 1000,
                limit: 100,
            },
            &protected,
            Arc::new(ScanProgress::default()),
        );

        assert_eq!(report.entries.len(), 2);
        assert_eq!(report.entries[0].size_bytes, 4000);
        assert_eq!(report.total_bytes, 5000);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn protected_files_are_listed_but_marked() {
        let root = sandbox("protected");
        fs::write(root.join("vault.kdbx"), vec![0u8; 2000]).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let report = find(
            LargeFileQuery {
                roots: vec![root.clone()],
                min_bytes: 100,
                limit: 100,
            },
            &protected,
            Arc::new(ScanProgress::default()),
        );

        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].risk, RiskLevel::Protected);
        assert!(report.entries[0].risk_note.is_some());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_root_does_not_fail_the_search() {
        let protected = ProtectedPaths::new(&[]);
        let report = find(
            LargeFileQuery {
                roots: vec![PathBuf::from("Z:\\definitely\\not\\here")],
                min_bytes: 1,
                limit: 10,
            },
            &protected,
            Arc::new(ScanProgress::default()),
        );
        assert!(report.entries.is_empty());
        assert!(!report.cancelled);
    }
}

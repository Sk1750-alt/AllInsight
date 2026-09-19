//! Duplicate detection.
//!
//! Three passes, each one cheaper than the next is expensive:
//!
//! 1. group by exact size - a single `read_dir` pass, no file is opened
//! 2. partial hash of the head and tail of each candidate - two small reads
//! 3. full BLAKE3 hash, only for files that survived pass 2
//!
//! Names are never used as evidence. Two files match only when their contents
//! hash identically, and nothing is ever removed automatically.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use parking_lot::Mutex;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::scanner::ScanProgress;
use crate::services::security::paths;
use crate::services::security::ProtectedPaths;

/// Bytes read from each end of a file during the partial-hash pass.
const PARTIAL_WINDOW: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateQuery {
    pub roots: Vec<PathBuf>,
    /// Files below this size are ignored. Small duplicates are numerous and
    /// reclaim almost nothing.
    pub min_bytes: u64,
    /// Cap on the number of groups returned.
    pub max_groups: usize,
}

impl Default for DuplicateQuery {
    fn default() -> Self {
        Self {
            roots: dirs::home_dir().into_iter().collect(),
            min_bytes: 1024 * 1024,
            max_groups: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateFile {
    pub path: PathBuf,
    pub name: String,
    pub directory: PathBuf,
    pub size_bytes: u64,
    pub modified: Option<i64>,
    /// True when the protected-path engine refuses to remove this copy, so the
    /// UI can keep it out of any bulk selection.
    pub protected: bool,
    pub protection_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Hex BLAKE3 digest, used as a stable group id.
    pub id: String,
    pub size_bytes: u64,
    pub files: Vec<DuplicateFile>,
    /// What could be reclaimed by keeping exactly one copy.
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateReport {
    pub groups: Vec<DuplicateGroup>,
    pub total_reclaimable_bytes: u64,
    pub files_compared: u64,
    pub files_hashed: u64,
    pub cancelled: bool,
    pub truncated: bool,
}

fn collect_candidates(
    roots: &[PathBuf],
    min_bytes: u64,
    progress: &ScanProgress,
) -> HashMap<u64, Vec<PathBuf>> {
    let by_size: Mutex<HashMap<u64, Vec<PathBuf>>> = Mutex::new(HashMap::new());

    fn walk(dir: &Path, depth: u32, min_bytes: u64, progress: &ScanProgress, out: &Mutex<HashMap<u64, Vec<PathBuf>>>) {
        if progress.is_cancelled() || depth > 64 {
            return;
        }
        progress.dirs.fetch_add(1, Ordering::Relaxed);
        let Ok(entries) = std::fs::read_dir(paths::long_path(dir)) else {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return;
        };

        let mut subdirs = Vec::new();
        let mut local: Vec<(u64, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
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
            if meta.len() >= min_bytes {
                local.push((meta.len(), path));
            }
            progress.files.fetch_add(1, Ordering::Relaxed);
        }
        if !local.is_empty() {
            let mut guard = out.lock();
            for (size, path) in local {
                guard.entry(size).or_default().push(path);
            }
        }
        subdirs
            .into_par_iter()
            .for_each(|child| walk(&child, depth + 1, min_bytes, progress, out));
    }

    for root in roots {
        if root.exists() {
            walk(&paths::normalize_lexical(root), 0, min_bytes, progress, &by_size);
        }
    }

    let mut map = by_size.into_inner();
    // A size seen once cannot be a duplicate.
    map.retain(|_, group| group.len() > 1);
    map
}

/// Hash the first and last window of a file. For files smaller than two
/// windows this reads the whole file, which is already the cheapest option.
fn partial_hash(path: &Path, size: u64) -> Option<[u8; 32]> {
    let mut file = std::fs::File::open(paths::long_path(path)).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());

    let window = PARTIAL_WINDOW.min(size as usize);
    let mut buf = vec![0u8; window];
    file.read_exact(&mut buf).ok()?;
    hasher.update(&buf);

    if size > (PARTIAL_WINDOW as u64) * 2 {
        file.seek(SeekFrom::End(-(PARTIAL_WINDOW as i64))).ok()?;
        let mut tail = vec![0u8; PARTIAL_WINDOW];
        file.read_exact(&mut tail).ok()?;
        hasher.update(&tail);
    }

    Some(*hasher.finalize().as_bytes())
}

/// Full content hash, streamed so a large file never lands in memory.
fn full_hash(path: &Path) -> Option<[u8; 32]> {
    let mut file = std::fs::File::open(paths::long_path(path)).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Some(*hasher.finalize().as_bytes())
}

fn describe(path: &Path, size: u64, protected: &ProtectedPaths) -> DuplicateFile {
    let verdict = protected.classify(path);
    let modified = std::fs::metadata(paths::long_path(path))
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    DuplicateFile {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        directory: path.parent().map(Path::to_path_buf).unwrap_or_default(),
        size_bytes: size,
        modified,
        protected: verdict.protected,
        protection_note: verdict.reason.map(|r| r.explain().to_string()),
        path: path.to_path_buf(),
    }
}

pub fn find(
    query: DuplicateQuery,
    protected: &ProtectedPaths,
    progress: Arc<ScanProgress>,
) -> DuplicateReport {
    let by_size = collect_candidates(&query.roots, query.min_bytes, &progress);
    let files_compared: u64 = by_size.values().map(|v| v.len() as u64).sum();

    // Pass 2: partial hash, in parallel across every candidate.
    let partial_groups: Mutex<HashMap<(u64, [u8; 32]), Vec<PathBuf>>> = Mutex::new(HashMap::new());
    by_size.into_par_iter().for_each(|(size, group)| {
        if progress.is_cancelled() {
            return;
        }
        for path in group {
            if let Some(h) = partial_hash(&path, size) {
                partial_groups.lock().entry((size, h)).or_default().push(path);
            }
        }
    });

    let mut candidates = partial_groups.into_inner();
    candidates.retain(|_, group| group.len() > 1);

    // Pass 3: full hash for the survivors.
    let confirmed: Mutex<HashMap<(u64, [u8; 32]), Vec<PathBuf>>> = Mutex::new(HashMap::new());
    let hashed = std::sync::atomic::AtomicU64::new(0);
    candidates.into_par_iter().for_each(|((size, _), group)| {
        if progress.is_cancelled() {
            return;
        }
        for path in group {
            if let Some(h) = full_hash(&path) {
                hashed.fetch_add(1, Ordering::Relaxed);
                confirmed.lock().entry((size, h)).or_default().push(path);
            }
        }
    });

    let mut groups: Vec<DuplicateGroup> = confirmed
        .into_inner()
        .into_iter()
        .filter(|(_, group)| group.len() > 1)
        .map(|((size, digest), mut group)| {
            group.sort();
            let reclaimable = size.saturating_mul(group.len() as u64 - 1);
            DuplicateGroup {
                id: hex(&digest),
                size_bytes: size,
                files: group.iter().map(|p| describe(p, size, protected)).collect(),
                reclaimable_bytes: reclaimable,
            }
        })
        .collect();

    groups.sort_by(|a, b| b.reclaimable_bytes.cmp(&a.reclaimable_bytes));
    let total_reclaimable_bytes = groups.iter().map(|g| g.reclaimable_bytes).sum();
    let truncated = groups.len() > query.max_groups;
    groups.truncate(query.max_groups);

    DuplicateReport {
        groups,
        total_reclaimable_bytes,
        files_compared,
        files_hashed: hashed.load(Ordering::Relaxed),
        cancelled: progress.is_cancelled(),
        truncated,
    }
}

fn hex(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sandbox(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("allinsight-dupe-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        paths::canonicalize(&base).unwrap_or(base)
    }

    fn query(root: &Path) -> DuplicateQuery {
        DuplicateQuery {
            roots: vec![root.to_path_buf()],
            min_bytes: 1,
            max_groups: 50,
        }
    }

    #[test]
    fn identical_content_under_different_names_is_a_duplicate() {
        let root = sandbox("identical");
        let payload = vec![7u8; 5000];
        fs::write(root.join("photo.jpg"), &payload).unwrap();
        fs::create_dir_all(root.join("backup")).unwrap();
        fs::write(root.join("backup").join("photo-copy.jpg"), &payload).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let report = find(query(&root), &protected, Arc::new(ScanProgress::default()));

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files.len(), 2);
        assert_eq!(report.groups[0].reclaimable_bytes, 5000);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn same_name_and_size_but_different_content_is_not_a_duplicate() {
        let root = sandbox("collide");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();
        let mut left = vec![1u8; 4096];
        let mut right = vec![1u8; 4096];
        left[2048] = 9;
        right[2048] = 8;
        fs::write(root.join("a").join("same.bin"), &left).unwrap();
        fs::write(root.join("b").join("same.bin"), &right).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let report = find(query(&root), &protected, Arc::new(ScanProgress::default()));
        assert!(report.groups.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn files_differing_only_in_the_middle_are_caught_by_the_full_hash() {
        let root = sandbox("middle");
        // Larger than two partial windows so the head and tail match while the
        // middle does not: this is the case a partial hash alone would miss.
        let size = PARTIAL_WINDOW * 3;
        let mut left = vec![3u8; size];
        let mut right = vec![3u8; size];
        left[size / 2] = 1;
        right[size / 2] = 2;
        fs::write(root.join("left.bin"), &left).unwrap();
        fs::write(root.join("right.bin"), &right).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let report = find(query(&root), &protected, Arc::new(ScanProgress::default()));
        assert!(report.groups.is_empty(), "middle difference must be detected");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_minimum_size_filter_is_respected() {
        let root = sandbox("minsize");
        let payload = vec![4u8; 10];
        fs::write(root.join("one.bin"), &payload).unwrap();
        fs::write(root.join("two.bin"), &payload).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let mut q = query(&root);
        q.min_bytes = 1000;
        let report = find(q, &protected, Arc::new(ScanProgress::default()));
        assert!(report.groups.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn three_copies_reclaim_two_copies_worth() {
        let root = sandbox("triple");
        let payload = vec![5u8; 2000];
        for name in ["a.bin", "b.bin", "c.bin"] {
            fs::write(root.join(name), &payload).unwrap();
        }

        let protected = ProtectedPaths::new(&[]);
        let report = find(query(&root), &protected, Arc::new(ScanProgress::default()));
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].reclaimable_bytes, 4000);
        assert_eq!(report.total_reclaimable_bytes, 4000);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn protected_copies_are_flagged_in_the_group() {
        let root = sandbox("protflag");
        let payload = vec![6u8; 3000];
        fs::write(root.join("copy.bin"), &payload).unwrap();
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("keep").join("copy.bin"), &payload).unwrap();

        let mut protected = ProtectedPaths::new(&[]);
        protected.set_user_roots(&[root.join("keep")]);
        let report = find(query(&root), &protected, Arc::new(ScanProgress::default()));

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files.iter().filter(|f| f.protected).count(), 1);

        let _ = fs::remove_dir_all(&root);
    }
}

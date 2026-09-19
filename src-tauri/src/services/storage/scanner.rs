//! The filesystem scanner.
//!
//! One walk produces everything the Storage screens need: a directory tree
//! with rolled-up sizes for the treemap, per-category totals for the
//! breakdown, and the largest files. Walking the disk is the expensive part,
//! so it happens once and every view is served from the result.
//!
//! Design notes:
//!
//! * The walk is parallel, using `rayon::join` over sibling directories. Work
//!   is naturally balanced because each subtree recurses independently.
//! * Reparse points are never followed. A junction is recorded with its own
//!   size of zero rather than being counted twice.
//! * A directory that cannot be read increments a counter and the walk
//!   continues. Permission denied is the normal case without elevation, not an
//!   error condition.
//! * Node detail stops at `max_node_depth`; deeper bytes still roll up into
//!   the ancestors, so totals stay exact while memory stays bounded.


use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use parking_lot::Mutex;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use super::categories::{CategoryRules, CategoryTotals, StorageCategory};
use crate::services::security::paths;

/// Hard recursion limit. Real trees are far shallower; this exists so a
/// pathological structure cannot exhaust the stack.
const MAX_RECURSION_DEPTH: u32 = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOptions {
    pub root: PathBuf,
    /// Directories deeper than this are still measured but not kept as
    /// individual nodes.
    pub max_node_depth: u32,
    /// Files at or above this size are recorded in `largest_files`.
    pub large_file_threshold: u64,
    /// How many large files to keep.
    pub large_file_limit: usize,
}

impl ScanOptions {
    pub fn for_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_node_depth: 8,
            large_file_threshold: 100 * 1024 * 1024,
            large_file_limit: 500,
        }
    }
}

/// Live counters, shared with the UI while a scan runs.
#[derive(Debug, Default)]
pub struct ScanProgress {
    pub bytes: AtomicU64,
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub errors: AtomicU64,
    pub cancelled: AtomicBool,
    current: Mutex<String>,
}

impl ScanProgress {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    fn note_dir(&self, path: &Path) {
        self.dirs.fetch_add(1, Ordering::Relaxed);
        // Only every so often: locking on every directory would serialise the
        // whole walk.
        if self.dirs.load(Ordering::Relaxed) % 64 == 0 {
            *self.current.lock() = path.to_string_lossy().into_owned();
        }
    }

    pub fn snapshot(&self) -> ScanProgressSnapshot {
        ScanProgressSnapshot {
            bytes: self.bytes.load(Ordering::Relaxed),
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            cancelled: self.is_cancelled(),
            current: self.current.lock().clone(),
            finished: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressSnapshot {
    pub bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub cancelled: bool,
    pub current: String,
    pub finished: bool,
}

/// One directory in the scan tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirNode {
    pub path: PathBuf,
    pub name: String,
    /// Recursive size of everything beneath this directory.
    pub size_bytes: u64,
    /// Bytes in files sitting directly in this directory.
    pub own_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub depth: u32,
    /// Indices into [`ScanResult::nodes`].
    pub children: Vec<usize>,
    /// True when the walk stopped here because of the depth limit, so the
    /// size is correct but the children were not kept.
    pub truncated: bool,
    /// True when this directory could not be read at all.
    pub unreadable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFile {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    /// Seconds since the Unix epoch, or `None` when the filesystem did not
    /// report a timestamp.
    pub modified: Option<i64>,
    pub accessed: Option<i64>,
    pub extension: String,
    pub category: StorageCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub root: PathBuf,
    pub nodes: Vec<DirNode>,
    pub root_index: usize,
    pub totals: CategoryTotals,
    pub total_bytes: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    /// Directories that could not be opened. Expected without elevation.
    pub skipped_dirs: u64,
    /// Reparse points found and deliberately not followed.
    pub skipped_links: u64,
    pub largest_files: Vec<LargeFile>,
    pub duration_ms: u64,
    pub cancelled: bool,
    /// Unix seconds when the scan finished.
    pub completed_at: i64,
}

impl ScanResult {
    pub fn node(&self, index: usize) -> Option<&DirNode> {
        self.nodes.get(index)
    }

    /// Find a directory node by path, for treemap drill-down.
    pub fn find(&self, path: &Path) -> Option<usize> {
        let key = paths::comparison_key(&paths::normalize_lexical(path));
        self.nodes
            .iter()
            .position(|n| paths::comparison_key(&n.path) == key)
    }

    /// Children of a node, largest first, as a flat list for the UI.
    pub fn children_of(&self, index: usize) -> Vec<&DirNode> {
        let Some(node) = self.nodes.get(index) else {
            return Vec::new();
        };
        let mut kids: Vec<&DirNode> = node.children.iter().filter_map(|i| self.nodes.get(*i)).collect();
        kids.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        kids
    }
}

/// What one subtree contributed, returned up the recursion.
struct Subtotal {
    node_index: Option<usize>,
    size_bytes: u64,
    file_count: u64,
    dir_count: u64,
}

struct ScanContext {
    options: ScanOptions,
    progress: Arc<ScanProgress>,
    rules: CategoryRules,
    nodes: Mutex<Vec<DirNode>>,
    totals: Mutex<CategoryTotals>,
    large: Mutex<Vec<LargeFile>>,
    skipped_dirs: AtomicU64,
    skipped_links: AtomicU64,
}

impl ScanContext {
    fn push_node(&self, node: DirNode) -> usize {
        let mut nodes = self.nodes.lock();
        nodes.push(node);
        nodes.len() - 1
    }

    fn record_large(&self, file: LargeFile) {
        let mut large = self.large.lock();
        large.push(file);
        // Trim occasionally rather than on every push, so the sort cost is
        // amortised across the walk.
        if large.len() > self.options.large_file_limit * 4 {
            large.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
            large.truncate(self.options.large_file_limit);
        }
    }
}

fn unix_seconds(t: std::io::Result<SystemTime>) -> Option<i64> {
    t.ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Walk one directory and everything beneath it.
fn walk(ctx: &ScanContext, dir: &Path, depth: u32) -> Subtotal {
    if ctx.progress.is_cancelled() || depth > MAX_RECURSION_DEPTH {
        return Subtotal {
            node_index: None,
            size_bytes: 0,
            file_count: 0,
            dir_count: 0,
        };
    }

    ctx.progress.note_dir(dir);

    let keep_node = depth <= ctx.options.max_node_depth;
    let mut own_bytes = 0u64;
    let mut own_files = 0u64;
    let mut subdirs: Vec<PathBuf> = Vec::new();
    let mut unreadable = false;
    let mut local_totals = CategoryTotals::default();

    match std::fs::read_dir(paths::long_path(dir)) {
        Ok(entries) => {
            for entry in entries {
                if ctx.progress.is_cancelled() {
                    break;
                }
                let Ok(entry) = entry else {
                    ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                // `file_type` here comes from the directory listing itself, so
                // it costs nothing extra.
                let Ok(file_type) = entry.file_type() else {
                    ctx.progress.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                };
                let path = paths::strip_verbatim(&entry.path());

                // On Windows the directory listing already carries the reparse
                // tag, so `is_symlink` is free and covers junctions as well as
                // symbolic links. Directories get one extra check because
                // descending into a link is the expensive mistake.
                let is_link = file_type.is_symlink()
                    || (file_type.is_dir() && paths::is_reparse_point(&path));
                if is_link {
                    // Counted as a link, contributing no bytes: the target is
                    // measured wherever it actually lives.
                    ctx.skipped_links.fetch_add(1, Ordering::Relaxed);
                    continue;
                }

                if file_type.is_dir() {
                    subdirs.push(path);
                    continue;
                }

                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                own_bytes = own_bytes.saturating_add(size);
                own_files += 1;
                ctx.progress.files.fetch_add(1, Ordering::Relaxed);
                ctx.progress.bytes.fetch_add(size, Ordering::Relaxed);

                let category = ctx.rules.classify(&path);
                local_totals.add(category, size);

                if size >= ctx.options.large_file_threshold {
                    let meta = entry.metadata().ok();
                    ctx.record_large(LargeFile {
                        name: paths::file_name_lower(&path),
                        extension: paths::extension_lower(&path),
                        modified: meta.as_ref().and_then(|m| unix_seconds(m.modified())),
                        accessed: meta.as_ref().and_then(|m| unix_seconds(m.accessed())),
                        size_bytes: size,
                        category,
                        path,
                    });
                }
            }
        }
        Err(_) => {
            // Permission denied, a removable drive that vanished, a locked
            // directory: all normal, none fatal.
            unreadable = true;
            ctx.skipped_dirs.fetch_add(1, Ordering::Relaxed);
        }
    }

    ctx.totals.lock().merge(&local_totals);

    // Recurse into siblings in parallel. `into_par_iter` keeps the rayon pool
    // busy without spawning a thread per directory.
    let sub: Vec<Subtotal> = subdirs
        .into_par_iter()
        .map(|child| walk(ctx, &child, depth + 1))
        .collect();

    let mut size = own_bytes;
    let mut files = own_files;
    let mut dirs = 0u64;
    let mut children = Vec::new();
    for s in sub {
        size = size.saturating_add(s.size_bytes);
        files = files.saturating_add(s.file_count);
        dirs = dirs.saturating_add(s.dir_count + 1);
        if let Some(i) = s.node_index {
            children.push(i);
        }
    }

    let node_index = if keep_node {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());
        Some(ctx.push_node(DirNode {
            path: dir.to_path_buf(),
            name,
            size_bytes: size,
            own_bytes,
            file_count: files,
            dir_count: dirs,
            depth,
            children,
            truncated: depth == ctx.options.max_node_depth,
            unreadable,
        }))
    } else {
        None
    };

    Subtotal {
        node_index,
        size_bytes: size,
        file_count: files,
        dir_count: dirs,
    }
}

/// Run a scan to completion. Cancellation through `progress` returns a partial
/// but internally consistent result rather than an error.
pub fn scan(options: ScanOptions, progress: Arc<ScanProgress>) -> ScanResult {
    let started = Instant::now();
    let root = paths::normalize_lexical(&options.root);

    let ctx = ScanContext {
        rules: CategoryRules::new(),
        nodes: Mutex::new(Vec::new()),
        totals: Mutex::new(CategoryTotals::default()),
        large: Mutex::new(Vec::new()),
        skipped_dirs: AtomicU64::new(0),
        skipped_links: AtomicU64::new(0),
        options,
        progress: progress.clone(),
    };

    let top = walk(&ctx, &root, 0);

    let mut nodes = ctx.nodes.into_inner();
    let root_index = top.node_index.unwrap_or_else(|| {
        nodes.push(DirNode {
            path: root.clone(),
            name: root.to_string_lossy().into_owned(),
            size_bytes: top.size_bytes,
            own_bytes: 0,
            file_count: top.file_count,
            dir_count: top.dir_count,
            depth: 0,
            children: Vec::new(),
            truncated: true,
            unreadable: false,
        });
        nodes.len() - 1
    });

    let mut largest = ctx.large.into_inner();
    largest.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    largest.truncate(ctx.options.large_file_limit);

    ScanResult {
        root,
        nodes,
        root_index,
        totals: ctx.totals.into_inner(),
        total_bytes: top.size_bytes,
        total_files: top.file_count,
        total_dirs: top.dir_count,
        skipped_dirs: ctx.skipped_dirs.load(Ordering::Relaxed),
        skipped_links: ctx.skipped_links.load(Ordering::Relaxed),
        largest_files: largest,
        duration_ms: started.elapsed().as_millis() as u64,
        cancelled: progress.is_cancelled(),
        completed_at: chrono::Utc::now().timestamp(),
    }
}

/// Measure a single directory without building a tree. Used by the cleanup
/// engine to size a category before the user commits to anything.
pub fn measure(dir: &Path, cancelled: &AtomicBool) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    let mut depth_guard = 0u32;

    while let Some(current) = stack.pop() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        depth_guard += 1;
        if depth_guard > 5_000_000 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(paths::long_path(&current)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            let path = entry.path();
            if ft.is_symlink() || paths::is_reparse_point(&path) {
                continue;
            }
            if ft.is_dir() {
                stack.push(path);
            } else {
                bytes = bytes.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// A compact treemap payload: one level of children plus their share of the
/// parent, ready to render without shipping the whole tree to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapNode {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub share: f64,
    pub file_count: u64,
    pub has_children: bool,
    pub is_file_bucket: bool,
}

/// Build the treemap level for `path`, including a synthetic entry for the
/// files that sit directly in the directory so the areas add up to the whole.
pub fn treemap_level(result: &ScanResult, path: &Path) -> Vec<TreemapNode> {
    let Some(index) = result.find(path) else {
        return Vec::new();
    };
    let Some(node) = result.node(index) else {
        return Vec::new();
    };
    let total = node.size_bytes.max(1) as f64;

    let mut out: Vec<TreemapNode> = result
        .children_of(index)
        .into_iter()
        .map(|c| TreemapNode {
            path: c.path.clone(),
            name: c.name.clone(),
            size_bytes: c.size_bytes,
            share: c.size_bytes as f64 / total,
            file_count: c.file_count,
            has_children: !c.children.is_empty(),
            is_file_bucket: false,
        })
        .collect();

    if node.own_bytes > 0 {
        out.push(TreemapNode {
            path: node.path.clone(),
            name: "Files in this folder".to_string(),
            size_bytes: node.own_bytes,
            share: node.own_bytes as f64 / total,
            file_count: 0,
            has_children: false,
            is_file_bucket: true,
        });
    }

    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn sandbox(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("allinsight-scan-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        paths::canonicalize(&base).unwrap_or(base)
    }

    #[test]
    fn sizes_roll_up_through_the_tree() {
        let root = sandbox("rollup");
        fs::write(root.join("a.bin"), vec![0u8; 1000]).unwrap();
        let sub = root.join("nested");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("b.bin"), vec![0u8; 2000]).unwrap();
        let deeper = sub.join("deeper");
        fs::create_dir_all(&deeper).unwrap();
        fs::write(deeper.join("c.bin"), vec![0u8; 4000]).unwrap();

        let result = scan(ScanOptions::for_root(&root), Arc::new(ScanProgress::default()));

        assert_eq!(result.total_bytes, 7000);
        assert_eq!(result.total_files, 3);
        assert_eq!(result.total_dirs, 2);

        let root_node = result.node(result.root_index).unwrap();
        assert_eq!(root_node.own_bytes, 1000);
        assert_eq!(root_node.size_bytes, 7000);

        let nested = result.find(&sub).and_then(|i| result.node(i)).unwrap();
        assert_eq!(nested.size_bytes, 6000);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_depth_limit_keeps_totals_exact() {
        let root = sandbox("depth");
        let mut current = root.clone();
        for i in 0..6 {
            current = current.join(format!("level{i}"));
            fs::create_dir_all(&current).unwrap();
            fs::write(current.join("f.bin"), vec![0u8; 100]).unwrap();
        }

        let mut options = ScanOptions::for_root(&root);
        options.max_node_depth = 2;
        let result = scan(options, Arc::new(ScanProgress::default()));

        assert_eq!(result.total_bytes, 600);
        // Only the root plus two levels are kept as nodes.
        assert!(result.nodes.len() <= 3, "nodes: {}", result.nodes.len());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn large_files_are_collected_above_the_threshold() {
        let root = sandbox("large");
        fs::write(root.join("small.bin"), vec![0u8; 10]).unwrap();
        fs::write(root.join("big.bin"), vec![0u8; 5000]).unwrap();

        let mut options = ScanOptions::for_root(&root);
        options.large_file_threshold = 1000;
        let result = scan(options, Arc::new(ScanProgress::default()));

        assert_eq!(result.largest_files.len(), 1);
        assert_eq!(result.largest_files[0].size_bytes, 5000);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancellation_returns_a_partial_result_not_an_error() {
        let root = sandbox("cancel");
        fs::write(root.join("a.bin"), vec![0u8; 10]).unwrap();

        let progress = Arc::new(ScanProgress::default());
        progress.cancel();
        let result = scan(ScanOptions::for_root(&root), progress);
        assert!(result.cancelled);

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_is_not_followed_or_double_counted() {
        let root = sandbox("junction-scan");
        let real = root.join("real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("payload.bin"), vec![0u8; 3000]).unwrap();

        let link = root.join("mirror");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&real)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let result = scan(ScanOptions::for_root(&root), Arc::new(ScanProgress::default()));
        assert_eq!(result.total_bytes, 3000, "the payload must be counted once");
        if made {
            assert!(result.skipped_links >= 1);
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn treemap_levels_add_up_to_the_parent() {
        let root = sandbox("treemap");
        fs::write(root.join("loose.bin"), vec![0u8; 500]).unwrap();
        let a = root.join("a");
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("x.bin"), vec![0u8; 1500]).unwrap();

        let result = scan(ScanOptions::for_root(&root), Arc::new(ScanProgress::default()));
        let level = treemap_level(&result, &root);

        let sum: u64 = level.iter().map(|n| n.size_bytes).sum();
        assert_eq!(sum, 2000);
        assert!(level.iter().any(|n| n.is_file_bucket));

        let _ = fs::remove_dir_all(&root);
    }
}

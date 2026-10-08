//! Shared application state.
//!
//! Everything expensive lives here and is built once: the database handle, the
//! protected-path engine, the metrics sampler and the local model. Scan
//! results are cached so the Storage screens can be served instantly, and the
//! cleanup scan is kept here rather than in the frontend, because the frontend
//! is never trusted with a path.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;

use crate::error::{AllInsightError, Result};
use crate::services::ai::facts::DuplicateFact;
use crate::services::ai::facts::{
    BatteryFact, CpuFact, DeviceFacts, DriveHealthFact, FolderFact, LargeFileFact, MemoryFact,
    StartupFact, VolumeFact,
};
use crate::services::ai::LlamaEngine;
use crate::services::cleanup::{CleanupPreview, CleanupScan};
use crate::services::db::{Database, Settings};
use crate::services::security::ProtectedPaths;
use crate::services::storage::{DuplicateReport, LargeFileReport, ScanProgress, ScanResult};
use crate::services::system::SystemMonitor;

/// Where AllInsight keeps its data. One folder, easy to find, easy to delete.
pub fn data_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AllInsight")
}

pub struct AppState {
    pub db: Database,
    pub settings: RwLock<Settings>,
    /// Held behind an `Arc` so a caller can take a snapshot and let go of
    /// the lock. See [`AppState::protected`].
    pub protected: RwLock<Arc<ProtectedPaths>>,
    pub monitor: SystemMonitor,
    pub engine: LlamaEngine,

    /// The most recent full storage scan, per root.
    pub scans: RwLock<HashMap<String, ScanResult>>,
    /// Progress handle for the scan currently running, if any.
    pub scan_progress: RwLock<Option<Arc<ScanProgress>>>,
    pub scan_running: AtomicBool,

    pub large_files: RwLock<Option<LargeFileReport>>,
    pub duplicates: RwLock<Option<DuplicateReport>>,

    /// Backend-held cleanup candidates. The frontend only ever sees ids.
    pub cleanup_scan: RwLock<Arc<CleanupScan>>,
    /// Who is running a cleanup pass, if anyone. Only one runs at a time, so
    /// a second click cannot start a rival pass over the same folders.
    cleanup_owner: Mutex<Option<CleanupOwner>>,
    pub cleanup_preview: RwLock<Option<CleanupPreview>>,
    pub cleanup_cancel: Arc<AtomicBool>,

    /// When each alert was last shown, so notifications do not repeat.
    pub last_alert: RwLock<HashMap<String, i64>>,

    /// See [`AppState::dashboard_inputs`].
    facts_cache: RwLock<Option<(Instant, DashboardInputs)>>,
    /// Serialises the refresh behind [`AppState::dashboard_inputs`] so
    /// several screens missing the cache at once gather the facts once
    /// between them instead of once each.
    facts_refresh: Mutex<()>,
    /// See [`DRIVE_TTL`] and [`BATTERY_TTL`].
    drive_cache: Cached<crate::services::health::DriveHealthReport>,
    battery_cache: Cached<crate::services::battery::BatteryStatus>,
}

/// Everything the Overview and the AI layer need, gathered once.
///
/// Assembling this touches WMI twice and enumerates the registry, which is far
/// too expensive to repeat per request: the Overview alone would otherwise pay
/// for it three times, and opening the command palette would pay for it again.
#[derive(Clone)]
pub struct DashboardInputs {
    pub facts: DeviceFacts,
    pub drives: crate::services::health::DriveHealthReport,
    pub battery: crate::services::battery::BatteryStatus,
    pub system: crate::services::system::SystemSnapshot,
}

/// How long a gathered snapshot stays fresh. Long enough that one screen load
/// pays for it once, short enough that the numbers still look live.
const FACTS_TTL: Duration = Duration::from_secs(2);

/// How long a drive report stays fresh.
///
/// The figures behind it are lifetime counters -- power-on hours, reallocated
/// sectors, bytes written -- which move over hours, not seconds. Re-reading
/// them on every Overview refresh cost more than everything else on the screen
/// put together.
const DRIVE_TTL: Duration = Duration::from_secs(60);

/// How long a battery reading stays fresh. Short enough that plugging the
/// charger in shows up almost at once, long enough to skip most of the calls.
const BATTERY_TTL: Duration = Duration::from_secs(10);

/// How long to wait for the automatic pass to stand down, and how often to
/// look. Cancellation is checked between items, so this is usually one poll.
const PREEMPT_POLL: Duration = Duration::from_millis(100);
const PREEMPT_ATTEMPTS: u32 = 50;

/// A value with an expiry date.
struct Cached<T> {
    slot: RwLock<Option<(Instant, T)>>,
    ttl: Duration,
}

impl<T: Clone> Cached<T> {
    fn new(ttl: Duration) -> Self {
        Self {
            slot: RwLock::new(None),
            ttl,
        }
    }

    /// Return the stored value, or produce a new one if it has expired.
    ///
    /// Two callers arriving together may both produce a value. That is
    /// deliberate: the alternative is a lock held across the very call this
    /// exists to keep off the critical path, and producing the value twice is
    /// cheaper than blocking a screen behind it.
    fn get_or_refresh(&self, produce: impl FnOnce() -> T) -> T {
        if let Some((at, value)) = self.slot.read().as_ref() {
            if at.elapsed() < self.ttl {
                return value.clone();
            }
        }
        let fresh = produce();
        *self.slot.write() = Some((Instant::now(), fresh.clone()));
        fresh
    }

    /// Drop the stored value, so the next read measures again.
    fn invalidate(&self) {
        *self.slot.write() = None;
    }
}

impl AppState {
    pub fn new() -> Result<Self> {
        let directory = data_directory();
        let db = Database::open_with_retry(&directory.join("allinsight.db"))?;
        let settings = Settings::load(&db)?;
        let protected = ProtectedPaths::new(&settings.protected_paths);

        Ok(Self {
            db,
            settings: RwLock::new(settings),
            protected: RwLock::new(Arc::new(protected)),
            monitor: SystemMonitor::new(),
            engine: LlamaEngine::new(),
            scans: RwLock::new(HashMap::new()),
            scan_progress: RwLock::new(None),
            scan_running: AtomicBool::new(false),
            large_files: RwLock::new(None),
            duplicates: RwLock::new(None),
            cleanup_scan: RwLock::new(Arc::new(CleanupScan::default())),
            cleanup_owner: Mutex::new(None),
            cleanup_preview: RwLock::new(None),
            cleanup_cancel: Arc::new(AtomicBool::new(false)),
            last_alert: RwLock::new(HashMap::new()),
            facts_cache: RwLock::new(None),
            facts_refresh: Mutex::new(()),
            drive_cache: Cached::new(DRIVE_TTL),
            battery_cache: Cached::new(BATTERY_TTL),
        })
    }

    /// Persist settings and rebuild anything derived from them.
    pub fn update_settings(&self, mut next: Settings) -> Result<Settings> {
        next.save(&self.db)?;
        let mut rebuilt = (**self.protected.read()).clone();
        rebuilt.set_user_roots(&next.protected_paths);
        *self.protected.write() = Arc::new(rebuilt);
        let stored = next.clone();
        *self.settings.write() = next;
        Ok(stored)
    }

    /// The drive report, re-measured at most once per [`DRIVE_TTL`].
    pub fn drive_report(&self) -> crate::services::health::DriveHealthReport {
        self.drive_cache.get_or_refresh(|| {
            crate::services::health::report().unwrap_or_else(|e| {
                crate::services::health::DriveHealthReport {
                    drives: Vec::new(),
                    elevated: false,
                    error: Some(e.to_string()),
                }
            })
        })
    }

    /// The battery reading, re-measured at most once per [`BATTERY_TTL`].
    pub fn battery_status(&self) -> crate::services::battery::BatteryStatus {
        self.battery_cache
            .get_or_refresh(|| crate::services::battery::status().unwrap_or_default())
    }

    /// Forget every cached measurement. Used when the window is shown again
    /// after being hidden, where the figures on screen may be minutes old.
    pub fn invalidate_caches(&self) {
        *self.facts_cache.write() = None;
        self.drive_cache.invalidate();
        self.battery_cache.invalidate();
    }

    /// A snapshot of the protected-path set.
    ///
    /// Always prefer this to holding the lock: the returned handle stays valid
    /// for as long as it is needed without blocking a settings save, and a
    /// save that lands mid-scan simply means the scan finishes against the
    /// rules it started with.
    pub fn protected(&self) -> Arc<ProtectedPaths> {
        Arc::clone(&self.protected.read())
    }

    /// A snapshot of the current cleanup candidates.
    pub fn cleanup_scan(&self) -> Arc<CleanupScan> {
        Arc::clone(&self.cleanup_scan.read())
    }

    /// Claim the cleanup pass.
    ///
    /// A pass the user started takes precedence: if the hourly automatic pass
    /// holds the claim, it is cancelled and this waits for it to let go. Two
    /// user-initiated passes are not allowed to overlap, and the automatic one
    /// never interrupts anything -- it simply tries again next hour.
    pub fn begin_cleanup(&self, owner: CleanupOwner) -> Result<CleanupLease<'_>> {
        {
            let mut held = self.cleanup_owner.lock();
            match *held {
                None => {
                    *held = Some(owner);
                    return Ok(CleanupLease(self));
                }
                Some(CleanupOwner::User) => {
                    return Err(AllInsightError::InvalidInput(
                        "A cleanup is already running. Wait for it to finish, or cancel it.".into(),
                    ));
                }
                Some(CleanupOwner::Automatic) => {
                    if owner == CleanupOwner::Automatic {
                        return Err(AllInsightError::InvalidInput(
                            "The automatic cleanup is already running.".into(),
                        ));
                    }
                }
            }
        }

        // The automatic pass is running and the user has asked for one. Tell it
        // to stop and wait for it to notice; it checks the flag between items,
        // so this is normally over in well under a second.
        self.cleanup_cancel.store(true, Ordering::SeqCst);
        for _ in 0..PREEMPT_ATTEMPTS {
            std::thread::sleep(PREEMPT_POLL);
            let mut held = self.cleanup_owner.lock();
            if held.is_none() {
                *held = Some(owner);
                return Ok(CleanupLease(self));
            }
        }
        Err(AllInsightError::InvalidInput(
            "AllInsight is finishing an automatic cleanup. Try again in a moment.".into(),
        ))
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Refuse to start a second scan while one is running, rather than
    /// queueing work the user did not ask for.
    pub fn begin_scan(&self, progress: Arc<ScanProgress>) -> Result<()> {
        if self.scan_running.swap(true, Ordering::SeqCst) {
            return Err(AllInsightError::InvalidInput(
                "A scan is already running. Wait for it to finish or cancel it.".into(),
            ));
        }
        *self.scan_progress.write() = Some(progress);
        Ok(())
    }

    pub fn end_scan(&self) {
        self.scan_running.store(false, Ordering::SeqCst);
        *self.scan_progress.write() = None;
    }

    pub fn cancel_scan(&self) {
        if let Some(progress) = self.scan_progress.read().as_ref() {
            progress.cancel();
        }
        self.cleanup_cancel.store(true, Ordering::SeqCst);
    }

    /// The cached gather, refreshed when it is older than [`FACTS_TTL`].
    pub fn dashboard_inputs(&self) -> DashboardInputs {
        if let Some((at, cached)) = self.facts_cache.read().as_ref() {
            if at.elapsed() < FACTS_TTL {
                return cached.clone();
            }
        }

        // One caller gathers; the rest wait here and then find the fresh copy
        // in the cache rather than repeating two WMI queries and a registry
        // walk apiece.
        let _refreshing = self.facts_refresh.lock();
        if let Some((at, cached)) = self.facts_cache.read().as_ref() {
            if at.elapsed() < FACTS_TTL {
                return cached.clone();
            }
        }

        let fresh = self.gather();
        *self.facts_cache.write() = Some((Instant::now(), fresh.clone()));
        fresh
    }

    /// Just the facts, for the AI layer.
    pub fn facts(&self) -> DeviceFacts {
        self.dashboard_inputs().facts
    }

    /// Build the fact set from what has already been measured. Nothing here
    /// starts a scan. Call [`AppState::dashboard_inputs`] instead; this is the
    /// uncached path behind it.
    fn gather(&self) -> DashboardInputs {
        let overview = crate::services::storage::overview();
        // Network shares and optical media are someone else's space: they are
        // listed on the Storage screen but never drive a "running out" insight.
        let volumes: Vec<VolumeFact> = overview
            .volumes
            .iter()
            .filter(|v| v.kind.is_scannable())
            .map(VolumeFact::from)
            .collect();

        let scans = self.scans.read();
        let system_root = overview
            .system_volume
            .clone()
            .unwrap_or_else(|| "C:\\".to_string());
        let scan = scans.get(&system_root).or_else(|| scans.values().next());

        let storage_scanned = scan.is_some();
        let storage_categories = scan.map(|s| s.totals.ranked()).unwrap_or_default();

        let top_folders: Vec<FolderFact> = scan
            .map(|s| {
                s.children_of(s.root_index)
                    .into_iter()
                    .take(8)
                    .map(|n| FolderFact {
                        path: n.path.to_string_lossy().into_owned(),
                        bytes: n.size_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let preview = self.cleanup_preview.read();
        let cleanup: Vec<crate::services::ai::facts::CleanupFact> = preview
            .as_ref()
            .map(|p| {
                p.categories
                    .iter()
                    .filter(|c| c.bytes > 0)
                    .map(|c| crate::services::ai::facts::CleanupFact {
                        category: c.name.clone(),
                        bytes: c.bytes,
                        items: c.items,
                        auto_clean_eligible: c.auto_clean_eligible,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let reclaimable_bytes = preview.as_ref().map(|p| p.total_bytes).unwrap_or(0);
        let auto_reclaimable_bytes = preview
            .as_ref()
            .map(|p| {
                p.categories
                    .iter()
                    .filter(|c| c.auto_clean_eligible)
                    .map(|c| c.bytes)
                    .sum()
            })
            .unwrap_or(0);

        let large_files = self
            .large_files
            .read()
            .as_ref()
            .map(|report| {
                const GB: u64 = 1024 * 1024 * 1024;
                let mut fact = LargeFileFact {
                    total_bytes: report.total_bytes,
                    ..Default::default()
                };
                let mut folders: HashMap<String, u64> = HashMap::new();
                for entry in &report.entries {
                    if entry.size_bytes >= GB {
                        fact.over_1gb += 1;
                    }
                    if entry.size_bytes >= 2 * GB {
                        fact.over_2gb += 1;
                    }
                    if entry.size_bytes >= 5 * GB {
                        fact.over_5gb += 1;
                    }
                    fact.largest_bytes = fact.largest_bytes.max(entry.size_bytes);
                    *folders
                        .entry(entry.directory.to_string_lossy().into_owned())
                        .or_default() += entry.size_bytes;
                }
                fact.largest_folder = folders
                    .into_iter()
                    .max_by_key(|(_, bytes)| *bytes)
                    .map(|(path, _)| path);
                fact
            })
            .unwrap_or_default();

        let duplicates = self
            .duplicates
            .read()
            .as_ref()
            .map(DuplicateFact::from_report)
            .unwrap_or_default();

        let drive_report = self.drive_report();
        let drives = drive_report
            .drives
            .iter()
            .map(DriveHealthFact::from)
            .collect();

        let startup_list = crate::services::startup::list();
        let startup = StartupFact {
            total: startup_list.items.len() as u64,
            enabled: startup_list.enabled_count as u64,
            high_impact: startup_list
                .items
                .iter()
                .filter(|i| i.enabled && i.impact == crate::services::startup::StartupImpact::High)
                .count() as u64,
            top_names: startup_list
                .items
                .iter()
                .filter(|i| i.enabled && i.impact == crate::services::startup::StartupImpact::High)
                .take(3)
                .map(|i| i.name.clone())
                .collect(),
        };

        let snapshot = self.monitor.sample();
        let history = self.monitor.history();
        let top_process = crate::services::process::list(&self.monitor, 1, false)
            .processes
            .first()
            .map(|p| p.name.clone());

        let battery = self.battery_status();

        let facts = DeviceFacts {
            volumes,
            top_folders,
            storage_categories,
            cleanup,
            reclaimable_bytes,
            auto_reclaimable_bytes,
            large_files,
            duplicates,
            drives,
            startup,
            memory: Some(MemoryFact {
                total_bytes: snapshot.memory.total_bytes,
                used_percent: snapshot.memory.used_percent,
            }),
            cpu: Some(CpuFact {
                usage_percent: snapshot.cpu.usage_percent,
                average_percent: history.cpu_average,
                peak_percent: history.cpu_peak,
                top_process,
            }),
            battery: Some(BatteryFact::from(&battery)),
            storage_scanned,
        };

        DashboardInputs {
            facts,
            drives: drive_report,
            battery,
            system: snapshot,
        }
    }

    /// True when this alert has not been shown recently enough for showing it
    /// again to count as nagging.
    ///
    /// The timestamp is kept in the database as well as in memory, so the
    /// quiet period survives a restart. Holding it only in memory meant that
    /// closing and reopening AllInsight re-announced the same low-space warning
    /// every time, which is exactly the behaviour the quiet period exists to
    /// prevent.
    pub fn should_alert(&self, key: &str, quiet_minutes: u32) -> bool {
        let now = chrono::Utc::now().timestamp();
        let quiet_seconds = (quiet_minutes as i64) * 60;
        let stored_key = format!("alert.{key}");

        let mut last = self.last_alert.write();

        let previous = last.get(key).copied().or_else(|| {
            self.db
                .get_setting(&stored_key)
                .ok()
                .flatten()
                .and_then(|v| v.parse::<i64>().ok())
        });

        if let Some(previous) = previous {
            if now - previous < quiet_seconds {
                return false;
            }
        }

        last.insert(key.to_string(), now);
        let _ = self.db.set_setting(&stored_key, &now.to_string());
        true
    }
}

/// Who asked for a cleanup pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupOwner {
    /// Started from the Cleanup screen.
    User,
    /// Started by the hourly background check.
    Automatic,
}

/// Releases the cleanup claim when it goes out of scope, including on an early
/// return or a panic, so a failed pass cannot leave the button dead.
pub struct CleanupLease<'a>(&'a AppState);

impl Drop for CleanupLease<'_> {
    fn drop(&mut self) {
        *self.0.cleanup_owner.lock() = None;
    }
}

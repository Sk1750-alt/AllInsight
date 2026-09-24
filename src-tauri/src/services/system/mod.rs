//! Live system metrics: CPU, memory, disk activity, network and uptime.
//!
//! One `sysinfo::System` is kept alive for the life of the process and
//! refreshed on a timer. Rebuilding it per request would be both slower and
//! wrong: CPU percentages are computed from the delta between two refreshes.

pub mod gpu;
#[cfg(windows)]
mod cpu_windows;

use std::collections::VecDeque;
use std::time::Duration;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sysinfo::{Networks, System};

pub use gpu::{GpuAdapter, GpuStatus};

/// How many samples of history to keep. At one sample per second this is ten
/// minutes, which is enough for the Performance screen without growing without
/// bound.
const HISTORY_LEN: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStatus {
    pub usage_percent: f32,
    pub core_usage: Vec<f32>,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub brand: String,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatus {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f32,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub download_bytes_per_sec: u64,
    pub upload_bytes_per_sec: u64,
    pub total_received_bytes: u64,
    pub total_transmitted_bytes: u64,
    pub interfaces: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskActivity {
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub cpu: CpuStatus,
    pub memory: MemoryStatus,
    pub network: NetworkStatus,
    pub disk: DiskActivity,
    pub gpu: GpuStatus,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub os_name: String,
    pub host_name: String,
    pub timestamp: i64,
}

/// A single point in the history ring buffers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    pub timestamp: i64,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub gpu_percent: Option<f32>,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub net_down_bps: u64,
    pub net_up_bps: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsHistory {
    pub samples: Vec<Sample>,
    pub cpu_average: f32,
    pub cpu_peak: f32,
    pub memory_average: f32,
    pub memory_peak: f32,
}

/// Owns the sysinfo handles and the history. One instance lives in the app
/// state for the life of the process.
pub struct SystemMonitor {
    inner: Mutex<Inner>,
}

struct Inner {
    system: System,
    networks: Networks,
    history: VecDeque<Sample>,
    last_sample_at: Option<std::time::Instant>,
    last_net_rx: u64,
    last_net_tx: u64,
    last_disk_read: u64,
    last_disk_write: u64,
    last_gpu: Option<GpuStatus>,
    gpu_polls: u32,
    /// When the process table was last enumerated, and what it produced.
    last_process_refresh: Option<std::time::Instant>,
    last_disk: DiskActivity,
    last_process_count: usize,
    /// Task Manager's own processor counters. `None` where PDH is
    /// unavailable, in which case sysinfo's figure is used.
    #[cfg(windows)]
    processor: Option<cpu_windows::ProcessorCounters>,
}

/// How often the process table may be walked from inside `sample`.
///
/// Totalling disk throughput means enumerating every process, which costs
/// well over a hundred milliseconds on a busy machine. Doing that on every
/// tick of a one-and-a-half second poll was the single largest source of
/// interface stalling, and disk rates do not need that resolution.
const PROCESS_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

impl Default for SystemMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMonitor {
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            inner: Mutex::new(Inner {
                system,
                networks: Networks::new_with_refreshed_list(),
                history: VecDeque::with_capacity(HISTORY_LEN),
                last_sample_at: None,
                last_net_rx: 0,
                last_net_tx: 0,
                last_disk_read: 0,
                last_disk_write: 0,
                last_gpu: None,
                gpu_polls: 0,
                last_process_refresh: None,
                last_disk: DiskActivity::default(),
                last_process_count: 0,
                #[cfg(windows)]
                processor: cpu_windows::ProcessorCounters::open(),
            }),
        }
    }

    /// Refresh and return the current snapshot, appending it to the history.
    pub fn sample(&self) -> SystemSnapshot {
        let mut inner = self.inner.lock();
        let now = std::time::Instant::now();
        let elapsed = inner
            .last_sample_at
            .map(|t| now.duration_since(t).as_secs_f64())
            .filter(|s| *s > 0.05)
            .unwrap_or(1.0);
        inner.last_sample_at = Some(now);

        inner.system.refresh_cpu_all();
        inner.system.refresh_memory();
        inner.networks.refresh(true);

        // The process table is only walked occasionally; see
        // PROCESS_REFRESH_INTERVAL.
        let refresh_processes = inner
            .last_process_refresh
            .map(|t| t.elapsed() >= PROCESS_REFRESH_INTERVAL)
            .unwrap_or(true);
        if refresh_processes {
            inner
                .system
                .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            inner.last_process_refresh = Some(now);
            inner.last_process_count = inner.system.processes().len();
        }

        // On Windows, match Task Manager: processor utility and the live
        // clock, rather than raw busy time and the base clock.
        #[cfg(windows)]
        let reading = inner.processor.as_mut().and_then(|p| p.read());
        #[cfg(not(windows))]
        let reading: Option<(f32, Option<u64>)> = None;
        #[cfg(windows)]
        let reading = reading.map(|r| (r.utility_percent, r.current_mhz));

        let cpu = CpuStatus {
            usage_percent: reading
                .map(|(u, _)| u)
                .unwrap_or_else(|| inner.system.global_cpu_usage()),
            core_usage: inner.system.cpus().iter().map(|c| c.cpu_usage()).collect(),
            physical_cores: inner.system.physical_core_count(),
            logical_cores: inner.system.cpus().len(),
            brand: inner
                .system
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_else(|| "Unknown processor".into()),
            frequency_mhz: reading
                .and_then(|(_, mhz)| mhz)
                .unwrap_or_else(|| inner.system.cpus().first().map(|c| c.frequency()).unwrap_or(0)),
        };

        let total = inner.system.total_memory();
        let used = inner.system.used_memory();
        let memory = MemoryStatus {
            total_bytes: total,
            used_bytes: used,
            available_bytes: inner.system.available_memory(),
            used_percent: if total == 0 {
                0.0
            } else {
                (used as f64 / total as f64 * 100.0) as f32
            },
            swap_total_bytes: inner.system.total_swap(),
            swap_used_bytes: inner.system.used_swap(),
        };

        // Network counters are cumulative, so rates come from the delta.
        let rx: u64 = inner.networks.iter().map(|(_, d)| d.total_received()).sum();
        let tx: u64 = inner
            .networks
            .iter()
            .map(|(_, d)| d.total_transmitted())
            .sum();
        let network = NetworkStatus {
            download_bytes_per_sec: rate(rx, inner.last_net_rx, elapsed),
            upload_bytes_per_sec: rate(tx, inner.last_net_tx, elapsed),
            total_received_bytes: rx,
            total_transmitted_bytes: tx,
            interfaces: inner.networks.len(),
        };
        inner.last_net_rx = rx;
        inner.last_net_tx = tx;

        // Disk throughput is summed across processes, which is what Task
        // Manager reports as disk activity. It is only recomputed on the ticks
        // that actually walked the process table; in between, the previous
        // rate stands rather than being recalculated from stale totals.
        let disk = if refresh_processes {
            let (read_total, write_total) =
                inner
                    .system
                    .processes()
                    .values()
                    .fold((0u64, 0u64), |(r, w), p| {
                        let usage = p.disk_usage();
                        (
                            r.saturating_add(usage.total_read_bytes),
                            w.saturating_add(usage.total_written_bytes),
                        )
                    });
            let since = inner
                .last_process_refresh
                .map(|_| PROCESS_REFRESH_INTERVAL.as_secs_f64())
                .unwrap_or(elapsed)
                .max(elapsed);
            let d = DiskActivity {
                read_bytes_per_sec: rate(read_total, inner.last_disk_read, since),
                write_bytes_per_sec: rate(write_total, inner.last_disk_write, since),
            };
            inner.last_disk_read = read_total;
            inner.last_disk_write = write_total;
            inner.last_disk = d.clone();
            d
        } else {
            inner.last_disk.clone()
        };

        // Querying WMI for the GPU on every tick is wasteful, so it is polled
        // roughly every fifth sample and reused in between.
        if inner.last_gpu.is_none() || inner.gpu_polls % 10 == 0 {
            inner.last_gpu = Some(gpu::status());
        }
        inner.gpu_polls = inner.gpu_polls.wrapping_add(1);
        let gpu_status = inner.last_gpu.clone().unwrap_or_default();

        let snapshot = SystemSnapshot {
            uptime_seconds: System::uptime(),
            process_count: inner.last_process_count,
            os_name: System::long_os_version().unwrap_or_else(|| crate::platform::os_name().into()),
            host_name: System::host_name().unwrap_or_default(),
            timestamp: chrono::Utc::now().timestamp(),
            gpu: gpu_status.clone(),
            cpu: cpu.clone(),
            memory: memory.clone(),
            network: network.clone(),
            disk: disk.clone(),
        };

        let sample = Sample {
            timestamp: snapshot.timestamp,
            cpu_percent: cpu.usage_percent,
            memory_percent: memory.used_percent,
            gpu_percent: gpu_status.utilization_percent,
            disk_read_bps: disk.read_bytes_per_sec,
            disk_write_bps: disk.write_bytes_per_sec,
            net_down_bps: network.download_bytes_per_sec,
            net_up_bps: network.upload_bytes_per_sec,
        };
        if inner.history.len() == HISTORY_LEN {
            inner.history.pop_front();
        }
        inner.history.push_back(sample);

        snapshot
    }

    pub fn history(&self) -> MetricsHistory {
        let inner = self.inner.lock();
        let samples: Vec<Sample> = inner.history.iter().copied().collect();
        let count = samples.len().max(1) as f32;
        MetricsHistory {
            cpu_average: samples.iter().map(|s| s.cpu_percent).sum::<f32>() / count,
            cpu_peak: samples.iter().map(|s| s.cpu_percent).fold(0.0, f32::max),
            memory_average: samples.iter().map(|s| s.memory_percent).sum::<f32>() / count,
            memory_peak: samples.iter().map(|s| s.memory_percent).fold(0.0, f32::max),
            samples,
        }
    }

    /// Give the process service access to the shared `System` handle so it does
    /// not have to maintain a second one.
    pub fn with_system<T>(&self, f: impl FnOnce(&System) -> T) -> T {
        let inner = self.inner.lock();
        f(&inner.system)
    }

    /// Refresh only what the process list needs.
    pub fn refresh_processes(&self) {
        let mut inner = self.inner.lock();
        inner
            .system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    }
}

/// Bytes per second from two cumulative readings. A counter that went
/// backwards (an interface disappeared, a process exited) yields zero rather
/// than a nonsense spike.
fn rate(current: u64, previous: u64, seconds: f64) -> u64 {
    if current <= previous || seconds <= 0.0 {
        return 0;
    }
    ((current - previous) as f64 / seconds) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_ignore_counter_resets() {
        assert_eq!(rate(100, 50, 1.0), 50);
        assert_eq!(rate(50, 100, 1.0), 0);
        assert_eq!(rate(100, 50, 0.0), 0);
        assert_eq!(rate(1000, 0, 2.0), 500);
    }

    #[test]
    fn a_snapshot_is_internally_consistent() {
        let monitor = SystemMonitor::new();
        let snapshot = monitor.sample();
        assert!(snapshot.memory.total_bytes > 0);
        assert!(snapshot.memory.used_bytes <= snapshot.memory.total_bytes);
        assert!(snapshot.cpu.logical_cores > 0);
        assert!(snapshot.cpu.usage_percent >= 0.0);
        assert!(snapshot.memory.used_percent >= 0.0 && snapshot.memory.used_percent <= 100.0);
    }

    #[test]
    fn history_grows_and_summarises() {
        let monitor = SystemMonitor::new();
        monitor.sample();
        monitor.sample();
        let history = monitor.history();
        assert_eq!(history.samples.len(), 2);
        assert!(history.cpu_peak >= history.cpu_average - f32::EPSILON);
    }
}

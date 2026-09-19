//! GPU utilisation.
//!
//! Windows exposes per-engine GPU counters through WMI. Not every driver
//! publishes them, and they are absent on some virtualised displays, so the
//! result is an `Option`: AllInsight shows "not available" rather than inventing a
//! number.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuStatus {
    /// Total utilisation across every engine, 0-100, or `None` when the
    /// counters are not published on this machine.
    pub utilization_percent: Option<f32>,
    pub adapters: Vec<GpuAdapter>,
    /// Dedicated memory in use, when the counters report it.
    pub dedicated_memory_bytes: Option<u64>,
    pub available: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuAdapter {
    pub name: String,
    pub driver_version: Option<String>,
    pub video_memory_bytes: Option<u64>,
}

impl Default for GpuStatus {
    fn default() -> Self {
        Self {
            utilization_percent: None,
            adapters: Vec::new(),
            dedicated_memory_bytes: None,
            available: false,
            note: Some("GPU statistics are not available on this device.".into()),
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use serde::Deserialize as De;
    use wmi::{COMLibrary, WMIConnection};

    #[derive(De, Debug)]
    #[serde(rename = "Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine")]
    #[serde(rename_all = "PascalCase")]
    struct GpuEngine {
        utilization_percentage: Option<u64>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory")]
    #[serde(rename_all = "PascalCase")]
    struct GpuAdapterMemory {
        dedicated_usage: Option<u64>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "Win32_VideoController")]
    #[serde(rename_all = "PascalCase")]
    struct VideoController {
        name: Option<String>,
        driver_version: Option<String>,
        adapter_ram: Option<u32>,
    }

    pub fn status() -> GpuStatus {
        let Ok(com) = COMLibrary::new() else {
            return GpuStatus::default();
        };
        let Ok(wmi) = WMIConnection::new(com) else {
            return GpuStatus::default();
        };

        let adapters: Vec<GpuAdapter> = wmi
            .query::<VideoController>()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                v.name.map(|name| GpuAdapter {
                    name,
                    driver_version: v.driver_version,
                    // `AdapterRAM` is a 32-bit field, so it saturates at 4 GB
                    // on larger cards. Reported as-is rather than corrected,
                    // because guessing would be worse.
                    video_memory_bytes: v.adapter_ram.map(u64::from),
                })
            })
            .collect();

        let engines = wmi.query::<GpuEngine>();
        let utilization = match engines {
            Ok(rows) if !rows.is_empty() => {
                let total: u64 = rows.iter().filter_map(|r| r.utilization_percentage).sum();
                Some((total as f32).min(100.0))
            }
            _ => None,
        };

        let dedicated = wmi
            .query::<GpuAdapterMemory>()
            .ok()
            .map(|rows| rows.iter().filter_map(|r| r.dedicated_usage).sum::<u64>())
            .filter(|v| *v > 0);

        let available = utilization.is_some();
        GpuStatus {
            utilization_percent: utilization,
            adapters,
            dedicated_memory_bytes: dedicated,
            available,
            note: if available {
                None
            } else {
                Some("This device does not publish GPU utilisation counters.".into())
            },
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::GpuStatus;
    pub fn status() -> GpuStatus {
        GpuStatus::default()
    }
}

/// Gather GPU counters. WMI again, so again on its own thread.
pub fn status() -> GpuStatus {
    crate::services::wmi_thread::run("gpu", imp::status).unwrap_or_default()
}

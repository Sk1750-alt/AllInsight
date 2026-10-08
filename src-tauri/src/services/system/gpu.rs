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

/// Task Manager's GPU figure. Each counter instance is one process on one
/// engine of one adapter, named like
/// `pid_1234_luid_0x0_0x1A2B_phys_0_eng_0_engtype_3D`. Task Manager adds the
/// processes up per adapter and engine type, and reports the busiest engine
/// type. Adding every instance together, as this used to, counts the same
/// work several times over.
#[cfg_attr(not(windows), allow(dead_code))]
fn task_manager_utilization<'a>(rows: impl Iterator<Item = (&'a str, u64)>) -> f32 {
    let mut per_engine: std::collections::HashMap<(String, String), u64> =
        std::collections::HashMap::new();
    for (name, percent) in rows {
        let luid = name
            .split("luid_")
            .nth(1)
            .and_then(|rest| rest.split("_phys").next())
            .unwrap_or("")
            .to_string();
        let engine = name.split("engtype_").nth(1).unwrap_or("").to_string();
        *per_engine.entry((luid, engine)).or_default() += percent;
    }
    per_engine
        .values()
        .map(|v| (*v).min(100) as f32)
        .fold(0.0, f32::max)
}

/// Virtual and remote-desktop display drivers (Parsec, RDP, Citrix, the basic
/// display driver) report no real work; the physical GPU is named first.
#[cfg_attr(not(windows), allow(dead_code))]
fn real_adapters_first(mut adapters: Vec<GpuAdapter>) -> Vec<GpuAdapter> {
    let is_virtual = |a: &GpuAdapter| {
        let n = a.name.to_lowercase();
        [
            "virtual",
            "parsec",
            "remote",
            "basic display",
            "citrix",
            "mirror",
            "idd",
        ]
        .iter()
        .any(|w| n.contains(w))
    };
    adapters.sort_by_key(|a| is_virtual(a));
    adapters
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
        name: Option<String>,
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
        let adapters = super::real_adapters_first(adapters);

        let engines = wmi.query::<GpuEngine>();
        let utilization = match engines {
            Ok(rows) if !rows.is_empty() => Some(super::task_manager_utilization(
                rows.iter()
                    .filter_map(|r| Some((r.name.as_deref()?, r.utilization_percentage?))),
            )),
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

#[cfg(target_os = "linux")]
#[path = "gpu_linux.rs"]
mod imp;

#[cfg(not(any(windows, target_os = "linux")))]
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

#[cfg(test)]
mod tm_tests {
    use super::*;

    #[test]
    fn utilisation_is_the_busiest_engine_not_the_sum() {
        let rows = [
            ("pid_1_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 20),
            ("pid_2_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 15),
            ("pid_1_luid_0x0_0xA_phys_0_eng_1_engtype_Copy", 30),
            ("pid_3_luid_0x0_0xA_phys_0_eng_2_engtype_VideoDecode", 10),
        ];
        assert_eq!(task_manager_utilization(rows.into_iter()), 35.0);
    }

    #[test]
    fn virtual_adapters_go_last() {
        let a = |n: &str| GpuAdapter {
            name: n.into(),
            driver_version: None,
            video_memory_bytes: None,
        };
        let sorted = real_adapters_first(vec![
            a("Parsec Virtual Display Adapter"),
            a("Intel(R) Iris(R) Xe Graphics"),
        ]);
        assert_eq!(sorted[0].name, "Intel(R) Iris(R) Xe Graphics");
    }
}

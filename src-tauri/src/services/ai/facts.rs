//! The structured picture of the device that both the deterministic insight
//! generator and the local model work from.
//!
//! This type is the boundary described in the product brief: the model is
//! handed facts that the Rust backend measured, and nothing else. It never
//! sees a filesystem, a command interface, or the ability to ask for one.

use serde::{Deserialize, Serialize};

use crate::services::battery::BatteryStatus;
use crate::services::health::{DriveHealth, HealthState};
use crate::services::storage::{format_bytes, CategoryTotal, VolumeInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeFact {
    pub mount_point: String,
    pub label: Option<String>,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_percent: f64,
    pub is_system: bool,
}

impl From<&VolumeInfo> for VolumeFact {
    fn from(v: &VolumeInfo) -> Self {
        Self {
            mount_point: v.mount_point.clone(),
            label: v.label.clone(),
            total_bytes: v.total_bytes,
            free_bytes: v.free_bytes,
            used_percent: v.used_percent,
            is_system: v.is_system,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderFact {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupFact {
    pub category: String,
    pub bytes: u64,
    pub items: u64,
    pub auto_clean_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveHealthFact {
    pub model: String,
    pub state: HealthState,
    pub media: String,
    pub life_remaining_percent: Option<u8>,
    pub temperature_celsius: Option<i32>,
    pub power_on_hours: Option<u64>,
    pub data_available: bool,
}

impl From<&DriveHealth> for DriveHealthFact {
    fn from(d: &DriveHealth) -> Self {
        Self {
            model: d.model.clone(),
            state: d.state,
            media: format!("{:?}", d.media),
            life_remaining_percent: d.estimated_life_remaining_percent,
            temperature_celsius: d.temperature_celsius,
            power_on_hours: d.power_on_hours,
            data_available: !d.reliability_unavailable,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LargeFileFact {
    pub over_1gb: u64,
    pub over_2gb: u64,
    pub over_5gb: u64,
    pub largest_bytes: u64,
    pub largest_folder: Option<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicateFact {
    pub groups: u64,
    pub reclaimable_bytes: u64,
}

impl DuplicateFact {
    pub fn from_report(report: &crate::services::storage::DuplicateReport) -> Self {
        Self {
            groups: report.groups.len() as u64,
            reclaimable_bytes: report.total_reclaimable_bytes,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupFact {
    pub total: u64,
    pub enabled: u64,
    pub high_impact: u64,
    pub top_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub total_bytes: u64,
    pub used_percent: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFact {
    pub usage_percent: f32,
    pub average_percent: f32,
    pub peak_percent: f32,
    pub top_process: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceFacts {
    pub volumes: Vec<VolumeFact>,
    pub top_folders: Vec<FolderFact>,
    pub storage_categories: Vec<CategoryTotal>,
    pub cleanup: Vec<CleanupFact>,
    pub reclaimable_bytes: u64,
    pub auto_reclaimable_bytes: u64,
    pub large_files: LargeFileFact,
    pub duplicates: DuplicateFact,
    pub drives: Vec<DriveHealthFact>,
    pub startup: StartupFact,
    pub memory: Option<MemoryFact>,
    pub cpu: Option<CpuFact>,
    pub battery: Option<BatteryFact>,
    /// True when no scan has run yet, so the generator can say "not measured"
    /// instead of "nothing found".
    pub storage_scanned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryFact {
    pub present: bool,
    pub charge_percent: Option<u8>,
    pub health_percent: Option<u8>,
    pub cycle_count: Option<u32>,
}

impl From<&BatteryStatus> for BatteryFact {
    fn from(b: &BatteryStatus) -> Self {
        Self {
            present: b.present,
            charge_percent: b.charge_percent,
            health_percent: b.health_percent,
            cycle_count: b.cycle_count,
        }
    }
}

impl DeviceFacts {
    pub fn system_volume(&self) -> Option<&VolumeFact> {
        self.volumes
            .iter()
            .find(|v| v.is_system)
            .or_else(|| self.volumes.first())
    }

    /// A compact plain-text briefing, used as the context block for the local
    /// model. Written as prose rather than JSON because small models follow it
    /// far more reliably.
    pub fn briefing(&self) -> String {
        let mut lines = Vec::new();

        for v in &self.volumes {
            lines.push(format!(
                "Drive {}{}: {} total, {} free, {:.0}% used{}.",
                v.mount_point,
                v.label
                    .as_ref()
                    .map(|l| format!(" ({l})"))
                    .unwrap_or_default(),
                format_bytes(v.total_bytes),
                format_bytes(v.free_bytes),
                v.used_percent,
                if v.is_system {
                    ", holds the operating system"
                } else {
                    ""
                }
            ));
        }

        if !self.storage_scanned {
            lines.push(
                "No storage scan has been run yet, so folder-level detail is unknown.".into(),
            );
        } else {
            for c in self.storage_categories.iter().take(6) {
                lines.push(format!("{}: {}.", c.label, format_bytes(c.bytes)));
            }
            for f in self.top_folders.iter().take(6) {
                lines.push(format!("Folder {} uses {}.", f.path, format_bytes(f.bytes)));
            }
        }

        if self.reclaimable_bytes > 0 {
            lines.push(format!(
                "{} can be reclaimed from safe categories.",
                format_bytes(self.reclaimable_bytes)
            ));
            for c in self.cleanup.iter().take(6) {
                if c.bytes > 0 {
                    lines.push(format!("  {}: {}.", c.category, format_bytes(c.bytes)));
                }
            }
        } else {
            lines.push("No safe cleanup categories currently hold anything worth removing.".into());
        }

        if self.large_files.over_1gb > 0 {
            lines.push(format!(
                "{} files are larger than 1 GB, totalling {}.",
                self.large_files.over_1gb,
                format_bytes(self.large_files.total_bytes)
            ));
        }
        if self.duplicates.groups > 0 {
            lines.push(format!(
                "{} duplicate groups were found, holding {} of redundant copies.",
                self.duplicates.groups,
                format_bytes(self.duplicates.reclaimable_bytes)
            ));
        }

        for d in &self.drives {
            if d.data_available {
                lines.push(format!(
                    "Drive {} reports health {}{}{}.",
                    d.model,
                    d.state.label(),
                    d.life_remaining_percent
                        .map(|l| format!(", {l}% of rated life remaining"))
                        .unwrap_or_default(),
                    d.temperature_celsius
                        .map(|t| format!(", {t} degrees Celsius"))
                        .unwrap_or_default()
                ));
            } else {
                lines.push(format!(
                    "Drive {} does not expose detailed health data, so its condition is unknown.",
                    d.model
                ));
            }
        }

        if self.startup.total > 0 {
            lines.push(format!(
                "{} startup entries exist, {} enabled, {} estimated high impact.",
                self.startup.total, self.startup.enabled, self.startup.high_impact
            ));
        }

        if let Some(m) = &self.memory {
            lines.push(format!(
                "Memory: {} installed, {:.0}% in use.",
                format_bytes(m.total_bytes),
                m.used_percent
            ));
        }
        if let Some(c) = &self.cpu {
            lines.push(format!(
                "CPU usage is {:.0}% now, {:.0}% on average{}.",
                c.usage_percent,
                c.average_percent,
                c.top_process
                    .as_ref()
                    .map(|p| format!(", led by {p}"))
                    .unwrap_or_default()
            ));
        }
        if let Some(b) = &self.battery {
            if b.present {
                lines.push(format!(
                    "Battery at {}%{}.",
                    b.charge_percent
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown ".into()),
                    b.health_percent
                        .map(|h| format!(", holding {h}% of design capacity"))
                        .unwrap_or_else(|| ", health not reported by this device".into())
                ));
            }
        }

        lines.join("\n")
    }
}

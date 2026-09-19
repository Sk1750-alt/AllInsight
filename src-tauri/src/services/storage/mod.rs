//! Storage discovery: which volumes exist, what is on them, and which files
//! are worth a second look.

pub mod categories;
pub mod duplicates;
pub mod large_files;
pub mod scanner;
pub mod volumes;

use serde::{Deserialize, Serialize};

pub use categories::{CategoryTotal, CategoryTotals, StorageCategory};
pub use duplicates::{DuplicateGroup, DuplicateQuery, DuplicateReport};
pub use large_files::{LargeFileEntry, LargeFileQuery, LargeFileReport, RiskLevel};
pub use scanner::{ScanOptions, ScanProgress, ScanResult, TreemapNode};
pub use volumes::{DriveKind, VolumeInfo};

/// Everything the Storage screen needs before any scan has run, so the UI can
/// render immediately on launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOverview {
    pub volumes: Vec<VolumeInfo>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub system_volume: Option<String>,
}

pub fn overview() -> StorageOverview {
    let volumes = volumes::list_volumes();
    // Network volumes are shown but excluded from the totals: they are not
    // this device's storage.
    let local: Vec<&VolumeInfo> = volumes
        .iter()
        .filter(|v| v.is_ready && v.kind.is_scannable())
        .collect();

    StorageOverview {
        total_bytes: local.iter().map(|v| v.total_bytes).sum(),
        used_bytes: local.iter().map(|v| v.used_bytes).sum(),
        free_bytes: local.iter().map(|v| v.free_bytes).sum(),
        system_volume: volumes.iter().find(|v| v.is_system).map(|v| v.mount_point.clone()),
        volumes,
    }
}

/// Format a byte count the way the UI shows it. Kept in Rust so every number
/// in a notification, a log line and the interface reads identically.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_formatting_matches_the_interface() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024 * 3 / 2), "1.50 MB");
        assert_eq!(format_bytes(15 * 1024 * 1024), "15.0 MB");
        assert_eq!(format_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(format_bytes(8 * 1024 * 1024 * 1024), "8.00 GB");
    }
}

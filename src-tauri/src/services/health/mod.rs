//! Drive health.
//!
//! Windows exposes two layers of information about a physical disk:
//!
//! * `MSFT_PhysicalDisk` - model, bus, media type and the driver's own health
//!   verdict. Readable without elevation.
//! * `MSFT_StorageReliabilityCounter` - wear, temperature, power-on hours and
//!   error counts. On most machines this requires elevation, and on some
//!   drives the counters are simply not published at all.
//!
//! The consequence is a deliberate one: when the second layer is unavailable,
//! AllInsight reports `Unknown` with an explanation. It never fills the gap with a
//! plausible-looking number, and it never calls a drive healthy on the basis of
//! silence.

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[cfg(any(windows, test))]
mod nvme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

impl HealthState {
    pub fn label(&self) -> &'static str {
        match self {
            HealthState::Healthy => "Healthy",
            HealthState::Warning => "Warning",
            HealthState::Critical => "Critical",
            HealthState::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Hdd,
    Ssd,
    StorageClassMemory,
    Unspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveHealth {
    pub device_id: String,
    pub model: String,
    pub serial_number: Option<String>,
    pub firmware: Option<String>,
    pub media: MediaKind,
    pub bus: String,
    pub size_bytes: Option<u64>,
    pub spindle_speed_rpm: Option<u32>,
    pub state: HealthState,
    /// What Windows itself said, before AllInsight added anything.
    pub windows_health: Option<String>,
    pub operational_status: Vec<String>,

    // Reliability counters. Every one is optional on purpose.
    pub temperature_celsius: Option<i32>,
    pub temperature_max_celsius: Option<i32>,
    /// Percentage of rated write endurance consumed, when the drive reports it.
    pub wear_percent: Option<u8>,
    /// Remaining life, derived from `wear_percent` only. `None` when wear is
    /// unknown; it is never estimated from anything else.
    pub estimated_life_remaining_percent: Option<u8>,
    pub power_on_hours: Option<u64>,
    pub read_errors_total: Option<u64>,
    pub read_errors_uncorrected: Option<u64>,
    pub write_errors_total: Option<u64>,
    pub write_errors_uncorrected: Option<u64>,
    pub start_stop_cycles: Option<u64>,

    /// Drive letters that live on this disk, when the mapping is readable.
    pub volumes: Vec<String>,

    /// True when the reliability layer could not be read.
    pub reliability_unavailable: bool,
    /// Whether elevation would change that.
    pub elevation_would_help: bool,
    /// Sentences the UI shows under the status badge.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveHealthReport {
    pub drives: Vec<DriveHealth>,
    pub elevated: bool,
    /// Set when nothing at all could be read, e.g. the Storage WMI provider is
    /// missing on this installation.
    pub error: Option<String>,
}

#[cfg(windows)]
fn media_from_code(code: Option<u16>) -> MediaKind {
    match code {
        Some(3) => MediaKind::Hdd,
        Some(4) => MediaKind::Ssd,
        Some(5) => MediaKind::StorageClassMemory,
        _ => MediaKind::Unspecified,
    }
}

#[cfg(windows)]
fn bus_from_code(code: Option<u16>) -> String {
    match code {
        Some(1) => "SCSI",
        Some(2) => "ATAPI",
        Some(3) => "ATA",
        Some(4) => "IEEE 1394",
        Some(5) => "SSA",
        Some(6) => "Fibre Channel",
        Some(7) => "USB",
        Some(8) => "RAID",
        Some(9) => "iSCSI",
        Some(10) => "SAS",
        Some(11) => "SATA",
        Some(12) => "SD",
        Some(13) => "MMC",
        Some(15) => "File-backed virtual",
        Some(16) => "Storage spaces",
        Some(17) => "NVMe",
        Some(18) => "Microsoft reserved",
        _ => "Unknown",
    }
    .to_string()
}

#[cfg(windows)]
fn windows_health_label(code: Option<u16>) -> Option<String> {
    match code {
        Some(0) => Some("Healthy".into()),
        Some(1) => Some("Warning".into()),
        Some(2) => Some("Unhealthy".into()),
        _ => None,
    }
}

#[cfg(windows)]
fn operational_labels(codes: &[u16]) -> Vec<String> {
    codes
        .iter()
        .map(|c| {
            match c {
                0 => "Unknown",
                1 => "Other",
                2 => "OK",
                3 => "Degraded",
                4 => "Stressed",
                5 => "Predictive failure",
                6 => "Error",
                7 => "Non-recoverable error",
                8 => "Starting",
                9 => "Stopping",
                10 => "Stopped",
                11 => "In service",
                12 => "No contact",
                13 => "Lost communication",
                14 => "Aborted",
                15 => "Dormant",
                16 => "Supporting entity in error",
                17 => "Completed",
                _ => "Other",
            }
            .to_string()
        })
        .collect()
}

/// Combine everything known into a single verdict.
///
/// The rules are conservative in both directions: an explicit warning from
/// Windows or an uncorrected error always downgrades the state, and an absence
/// of reliability data always produces `Unknown` rather than `Healthy`.
fn decide(drive: &mut DriveHealth) {
    let mut notes = Vec::new();

    let windows_says = drive.windows_health.as_deref();
    let mut state = match windows_says {
        Some("Healthy") => HealthState::Healthy,
        Some("Warning") => HealthState::Warning,
        Some("Unhealthy") => HealthState::Critical,
        _ => HealthState::Unknown,
    };

    if drive
        .operational_status
        .iter()
        .any(|s| s == "Predictive failure" || s == "Non-recoverable error")
    {
        state = HealthState::Critical;
        notes.push("The drive reported a predictive failure. Back up your data now.".to_string());
    } else if drive
        .operational_status
        .iter()
        .any(|s| s == "Degraded" || s == "Stressed" || s == "Error")
    {
        state = HealthState::Warning;
        notes.push("The drive is reported as degraded.".to_string());
    }

    if let Some(uncorrected) = drive.read_errors_uncorrected {
        if uncorrected > 0 {
            state = HealthState::Warning;
            notes.push(format!(
                "{uncorrected} uncorrected read errors have been recorded."
            ));
        }
    }
    if let Some(uncorrected) = drive.write_errors_uncorrected {
        if uncorrected > 0 {
            state = HealthState::Warning;
            notes.push(format!(
                "{uncorrected} uncorrected write errors have been recorded."
            ));
        }
    }

    if let Some(wear) = drive.wear_percent {
        drive.estimated_life_remaining_percent = Some(100u8.saturating_sub(wear.min(100)));
        if wear >= 90 {
            state = HealthState::Critical;
            notes.push("This drive has used most of its rated write endurance.".to_string());
        } else if wear >= 75 {
            state = HealthState::Warning;
            notes.push(
                "This drive has used a large share of its rated write endurance.".to_string(),
            );
        }
    }

    if let Some(temp) = drive.temperature_celsius {
        if temp >= 70 {
            state = if state == HealthState::Critical {
                state
            } else {
                HealthState::Warning
            };
            notes.push(format!(
                "The drive is running hot at {temp} degrees Celsius."
            ));
        }
    }

    if drive.reliability_unavailable {
        // Windows saying "Healthy" only means the driver has no complaint. It
        // is not the same as a clean SMART read, so the state stays Unknown
        // unless something concrete downgraded it.
        if state == HealthState::Healthy {
            state = HealthState::Unknown;
        }
        notes.push(if drive.elevation_would_help {
            "Detailed health data needs administrator permission. Restart AllInsight as administrator to read wear, temperature and error counts.".to_string()
        } else {
            format!("This drive does not report detailed health data to {}.", crate::platform::os_name())
        });
        if let Some(w) = windows_says {
            notes.push(format!(
                "{} reports the drive status as {w}.",
                crate::platform::os_name()
            ));
        }
    }

    drive.state = state;
    drive.notes = notes;
}

#[cfg(windows)]
mod imp {
    use super::*;
    use serde::Deserialize as De;
    use std::collections::HashMap;
    use wmi::{COMLibrary, WMIConnection};

    #[derive(De, Debug)]
    #[serde(rename = "MSFT_PhysicalDisk")]
    #[serde(rename_all = "PascalCase")]
    struct PhysicalDisk {
        /// The instance's own WMI reference. Needed to walk the association to
        /// its reliability counter, and the only form of the reference that
        /// survives WQL quoting intact.
        #[serde(rename = "__Path")]
        wmi_path: Option<String>,
        device_id: Option<String>,
        friendly_name: Option<String>,
        serial_number: Option<String>,
        firmware_version: Option<String>,
        media_type: Option<u16>,
        bus_type: Option<u16>,
        size: Option<u64>,
        spindle_speed: Option<u32>,
        health_status: Option<u16>,
        operational_status: Option<Vec<u16>>,
    }

    #[derive(De, Debug)]
    #[serde(rename = "MSFT_StorageReliabilityCounter")]
    #[serde(rename_all = "PascalCase")]
    // Widths match what the provider actually declares. Temperature, wear and
    // the temperature high-water mark are UInt8 there, and the start/stop count
    // is UInt32; asking for wider integers makes the row fail to deserialise
    // rather than arriving with the value truncated.
    struct ReliabilityCounter {
        temperature: Option<u8>,
        temperature_max: Option<u8>,
        wear: Option<u8>,
        power_on_hours: Option<u32>,
        read_errors_total: Option<u64>,
        read_errors_uncorrected: Option<u64>,
        write_errors_total: Option<u64>,
        write_errors_uncorrected: Option<u64>,
        start_stop_cycle_count: Option<u32>,
    }

    /// Read one disk's reliability counter by walking the association from it.
    ///
    /// `MSFT_StorageReliabilityCounter` cannot be enumerated. A plain
    /// `SELECT * FROM MSFT_StorageReliabilityCounter` returns zero rows even
    /// with administrator rights, which is what made every field on this screen
    /// read `Not reported` no matter how the application was started. The class
    /// is only reachable through the association from a specific physical disk,
    /// which is what `Get-StorageReliabilityCounter` does underneath.
    fn reliability_for(storage: &WMIConnection, disk_path: &str) -> Option<ReliabilityCounter> {
        let query = format!(
            "ASSOCIATORS OF {{{disk_path}}} WHERE ResultClass = MSFT_StorageReliabilityCounter"
        );
        storage
            .raw_query::<ReliabilityCounter>(query)
            .ok()?
            .into_iter()
            .next()
    }

    #[derive(De, Debug)]
    #[serde(rename = "MSFT_Partition")]
    #[serde(rename_all = "PascalCase")]
    struct Partition {
        disk_number: Option<u32>,
        drive_letter: Option<String>,
    }

    pub fn report() -> Result<DriveHealthReport> {
        let elevated = crate::services::security::is_elevated();

        let com = match COMLibrary::new() {
            Ok(c) => c,
            Err(e) => {
                return Ok(DriveHealthReport {
                    drives: Vec::new(),
                    elevated,
                    error: Some(format!(
                        "Windows storage services could not be reached: {e}"
                    )),
                })
            }
        };
        let storage =
            match WMIConnection::with_namespace_path("root\\Microsoft\\Windows\\Storage", com) {
                Ok(c) => c,
                Err(e) => {
                    return Ok(DriveHealthReport {
                        drives: Vec::new(),
                        elevated,
                        error: Some(format!(
                            "The Windows storage management provider is unavailable: {e}"
                        )),
                    })
                }
            };

        let disks: Vec<PhysicalDisk> = match storage.query() {
            Ok(rows) => rows,
            Err(e) => {
                return Ok(DriveHealthReport {
                    drives: Vec::new(),
                    elevated,
                    error: Some(format!("Windows did not return any physical drives: {e}")),
                })
            }
        };

        // Drive letters, so the health card can say which volumes are affected.
        let mut letters: HashMap<u32, Vec<String>> = HashMap::new();
        for partition in storage.query::<Partition>().unwrap_or_default() {
            if let (Some(number), Some(letter)) = (partition.disk_number, partition.drive_letter) {
                let letter = letter.trim_matches('\0').trim().to_string();
                if !letter.is_empty() {
                    letters
                        .entry(number)
                        .or_default()
                        .push(format!("{letter}:"));
                }
            }
        }

        let mut drives = Vec::new();
        for disk in disks {
            let device_id = disk.device_id.clone().unwrap_or_default();
            // Per disk, not one enumeration for all of them. Reliability
            // counters usually need elevation, so a miss here is expected
            // rather than exceptional.
            let counter = disk
                .wmi_path
                .as_deref()
                .and_then(|path| reliability_for(&storage, path));
            let counter = counter.as_ref();
            let reliability_unavailable = counter.is_none();

            let mut drive = DriveHealth {
                model: disk
                    .friendly_name
                    .clone()
                    .unwrap_or_else(|| "Unknown drive".into()),
                // Serial numbers are padded and sometimes contain a trailing
                // dot; they are shown only for identification.
                serial_number: disk
                    .serial_number
                    .map(|s| s.trim().trim_end_matches('.').to_string())
                    .filter(|s| !s.is_empty()),
                firmware: disk.firmware_version.map(|s| s.trim().to_string()),
                media: media_from_code(disk.media_type),
                bus: bus_from_code(disk.bus_type),
                size_bytes: disk.size,
                spindle_speed_rpm: disk.spindle_speed.filter(|s| *s > 0 && *s != u32::MAX),
                state: HealthState::Unknown,
                windows_health: windows_health_label(disk.health_status),
                operational_status: operational_labels(
                    disk.operational_status.as_deref().unwrap_or(&[]),
                ),
                temperature_celsius: counter
                    .and_then(|c| c.temperature)
                    .filter(|t| *t > 0 && *t < 200)
                    .map(i32::from),
                temperature_max_celsius: counter
                    .and_then(|c| c.temperature_max)
                    .filter(|t| *t > 0 && *t < 200)
                    .map(i32::from),
                wear_percent: counter.and_then(|c| c.wear),
                estimated_life_remaining_percent: None,
                power_on_hours: counter.and_then(|c| c.power_on_hours).map(u64::from),
                read_errors_total: counter.and_then(|c| c.read_errors_total),
                read_errors_uncorrected: counter.and_then(|c| c.read_errors_uncorrected),
                write_errors_total: counter.and_then(|c| c.write_errors_total),
                write_errors_uncorrected: counter.and_then(|c| c.write_errors_uncorrected),
                start_stop_cycles: counter
                    .and_then(|c| c.start_stop_cycle_count)
                    .map(u64::from),
                volumes: device_id
                    .parse::<u32>()
                    .ok()
                    .and_then(|n| letters.get(&n).cloned())
                    .unwrap_or_default(),
                reliability_unavailable,
                elevation_would_help: reliability_unavailable && !elevated,
                notes: Vec::new(),
                device_id,
            };

            // Without elevation the reliability counters are out of reach,
            // but the drive's own health log usually is not.
            if drive.reliability_unavailable {
                if let Ok(number) = drive.device_id.parse::<u32>() {
                    if let Some(h) = super::nvme::nvme_health(number) {
                        drive.temperature_celsius = h.temperature_celsius;
                        drive.wear_percent = Some(h.percentage_used.min(100));
                        drive.power_on_hours = Some(h.power_on_hours);
                        drive.read_errors_uncorrected = Some(h.media_errors);
                        drive
                            .operational_status
                            .extend(super::nvme::warning_labels(h.critical_warning));
                        drive.reliability_unavailable = false;
                        drive.elevation_would_help = false;
                    } else if drive.temperature_celsius.is_none() {
                        drive.temperature_celsius = super::nvme::temperature(number);
                    }
                }
            }

            decide(&mut drive);
            drives.push(drive);
        }

        drives.sort_by(|a, b| a.device_id.cmp(&b.device_id));

        Ok(DriveHealthReport {
            error: if drives.is_empty() {
                Some("No physical drives were reported by Windows.".into())
            } else {
                None
            },
            drives,
            elevated,
        })
    }
}

#[cfg(not(windows))]
#[path = "linux.rs"]
mod imp;

/// Gather drive health.
///
/// The query runs on its own thread because WMI needs a multi-threaded COM
/// apartment and a Tauri command does not get one. See
/// [`crate::services::wmi_thread`].
pub fn report() -> Result<DriveHealthReport> {
    match crate::services::wmi_thread::run("drive health", imp::report) {
        Ok(result) => result,
        Err(message) => Ok(DriveHealthReport {
            drives: Vec::new(),
            elevated: crate::services::security::is_elevated(),
            error: Some(message),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> DriveHealth {
        DriveHealth {
            device_id: "0".into(),
            model: "Test drive".into(),
            serial_number: None,
            firmware: None,
            media: MediaKind::Ssd,
            bus: "NVMe".into(),
            size_bytes: Some(512_000_000_000),
            spindle_speed_rpm: None,
            state: HealthState::Unknown,
            windows_health: Some("Healthy".into()),
            operational_status: vec!["OK".into()],
            temperature_celsius: None,
            temperature_max_celsius: None,
            wear_percent: None,
            estimated_life_remaining_percent: None,
            power_on_hours: None,
            read_errors_total: None,
            read_errors_uncorrected: None,
            write_errors_total: None,
            write_errors_uncorrected: None,
            start_stop_cycles: None,
            volumes: vec!["C:".into()],
            reliability_unavailable: true,
            elevation_would_help: true,
            notes: Vec::new(),
        }
    }

    #[test]
    fn silence_is_reported_as_unknown_not_healthy() {
        let mut drive = blank();
        decide(&mut drive);
        assert_eq!(drive.state, HealthState::Unknown);
        assert!(drive.estimated_life_remaining_percent.is_none());
        assert!(drive.notes.iter().any(|n| n.contains("administrator")));
    }

    #[test]
    fn full_counters_allow_a_healthy_verdict() {
        let mut drive = blank();
        drive.reliability_unavailable = false;
        drive.elevation_would_help = false;
        drive.wear_percent = Some(8);
        drive.temperature_celsius = Some(41);
        drive.read_errors_uncorrected = Some(0);
        drive.write_errors_uncorrected = Some(0);
        decide(&mut drive);
        assert_eq!(drive.state, HealthState::Healthy);
        assert_eq!(drive.estimated_life_remaining_percent, Some(92));
    }

    #[test]
    fn a_predictive_failure_is_always_critical() {
        let mut drive = blank();
        drive.reliability_unavailable = false;
        drive.operational_status = vec!["Predictive failure".into()];
        decide(&mut drive);
        assert_eq!(drive.state, HealthState::Critical);
        assert!(drive.notes.iter().any(|n| n.contains("Back up")));
    }

    #[test]
    fn uncorrected_errors_downgrade_a_healthy_drive() {
        let mut drive = blank();
        drive.reliability_unavailable = false;
        drive.read_errors_uncorrected = Some(3);
        decide(&mut drive);
        assert_eq!(drive.state, HealthState::Warning);
    }

    #[test]
    fn heavy_wear_is_critical_and_light_wear_is_not() {
        let mut heavy = blank();
        heavy.reliability_unavailable = false;
        heavy.wear_percent = Some(94);
        decide(&mut heavy);
        assert_eq!(heavy.state, HealthState::Critical);
        assert_eq!(heavy.estimated_life_remaining_percent, Some(6));

        let mut light = blank();
        light.reliability_unavailable = false;
        light.wear_percent = Some(12);
        decide(&mut light);
        assert_eq!(light.state, HealthState::Healthy);
    }

    #[cfg(windows)]
    #[test]
    fn bus_and_media_codes_map_to_readable_names() {
        assert_eq!(bus_from_code(Some(17)), "NVMe");
        assert_eq!(bus_from_code(Some(11)), "SATA");
        assert_eq!(bus_from_code(None), "Unknown");
        assert_eq!(media_from_code(Some(4)), MediaKind::Ssd);
        assert_eq!(media_from_code(Some(3)), MediaKind::Hdd);
        assert_eq!(media_from_code(None), MediaKind::Unspecified);
    }

    #[cfg(windows)]
    #[test]
    fn a_real_report_never_invents_a_life_figure() {
        let report = report().expect("report must not fail");
        for drive in report.drives {
            if drive.wear_percent.is_none() {
                assert!(
                    drive.estimated_life_remaining_percent.is_none(),
                    "life remaining must not be synthesised"
                );
            }
            if drive.reliability_unavailable {
                assert_ne!(drive.state, HealthState::Healthy);
                assert!(!drive.notes.is_empty());
            }
        }
    }
}

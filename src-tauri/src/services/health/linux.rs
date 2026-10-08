//! Drive health on Linux.
//!
//! Two sources, in order of preference:
//!
//! 1. **UDisks2**, over the system bus. It is present on every mainstream
//!    desktop distribution, reads SMART data itself as root, and publishes the
//!    results to ordinary users: the ATA failure prediction, temperature,
//!    power-on time and bad-sector count, and the NVMe critical-warning flags.
//!    It is what GNOME Disks shows, so AllInsight agrees with it.
//! 2. **sysfs**, when UDisks2 is absent (servers, minimal installs, WSL): the
//!    model, size and media type are always readable, and an NVMe drive's
//!    temperature usually is. Everything else is reported as unknown.
//!
//! AllInsight never runs `smartctl` or asks for root to fill the gaps.

use std::collections::HashMap;
use std::path::Path;

use super::{decide, DriveHealth, DriveHealthReport, HealthState, MediaKind};
use crate::error::Result;

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn empty_drive(device_id: String, model: String) -> DriveHealth {
    DriveHealth {
        device_id,
        model,
        serial_number: None,
        firmware: None,
        media: MediaKind::Unspecified,
        bus: "Unknown".into(),
        size_bytes: None,
        spindle_speed_rpm: None,
        state: HealthState::Unknown,
        windows_health: None,
        operational_status: Vec::new(),
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
        volumes: Vec::new(),
        reliability_unavailable: true,
        elevation_would_help: false,
        notes: Vec::new(),
    }
}

/// Whole disks worth reporting: not loop devices, RAM disks, device-mapper
/// volumes, software RAID or optical drives.
fn is_physical_block(name: &str) -> bool {
    !["loop", "ram", "zram", "dm-", "md", "sr", "fd", "nbd", "zd"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// Mount points of a disk and its partitions, from `/proc/self/mounts`.
fn mounts_for(device: &str, table: &str) -> Vec<String> {
    table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?;
            let mount = fields.next()?;
            let on_disk = source == device
                || source
                    .strip_prefix(device)
                    .map(|rest| {
                        let rest = rest.strip_prefix('p').unwrap_or(rest);
                        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
                    })
                    .unwrap_or(false);
            on_disk.then(|| mount.replace("\\040", " "))
        })
        .collect()
}

/// A hwmon temperature under a sysfs device directory, in whole degrees.
fn hwmon_temperature(device_dir: &Path) -> Option<i32> {
    for base in [device_dir.to_path_buf(), device_dir.join("hwmon")] {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("hwmon") {
                continue;
            }
            if let Some(milli) =
                read_trimmed(&entry.path().join("temp1_input")).and_then(|t| t.parse::<i64>().ok())
            {
                return Some((milli / 1000) as i32);
            }
        }
    }
    None
}

/// What sysfs alone can say about every physical disk.
fn sysfs_drives() -> Vec<DriveHealth> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let Ok(entries) = std::fs::read_dir("/sys/block") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_physical_block(&name) {
            continue;
        }
        let dir = entry.path();
        let device_dir = dir.join("device");
        let model = read_trimmed(&device_dir.join("model"))
            .or_else(|| {
                let vendor = read_trimmed(&device_dir.join("vendor"))?;
                Some(vendor)
            })
            .unwrap_or_else(|| name.clone());
        let device = format!("/dev/{name}");
        let mut drive = empty_drive(device.clone(), model);

        drive.size_bytes = read_trimmed(&dir.join("size"))
            .and_then(|s| s.parse::<u64>().ok())
            .map(|sectors| sectors * 512)
            .filter(|b| *b > 0);
        drive.media = match read_trimmed(&dir.join("queue/rotational")).as_deref() {
            Some("1") => MediaKind::Hdd,
            Some("0") => MediaKind::Ssd,
            _ => MediaKind::Unspecified,
        };
        drive.serial_number = read_trimmed(&device_dir.join("serial"));
        drive.firmware = read_trimmed(&device_dir.join("firmware_rev"))
            .or_else(|| read_trimmed(&device_dir.join("rev")));

        let resolved = std::fs::canonicalize(&dir).unwrap_or_default();
        let path = resolved.to_string_lossy();
        drive.bus = if name.starts_with("nvme") {
            "NVMe"
        } else if path.contains("/usb") {
            "USB"
        } else if path.contains("/virtio") {
            "Virtual"
        } else if path.contains("/ata") {
            "SATA"
        } else if name.starts_with("mmcblk") {
            "SD/MMC"
        } else {
            "Unknown"
        }
        .to_string();

        drive.temperature_celsius = hwmon_temperature(&device_dir);
        drive.volumes = mounts_for(&device, &mounts);
        out.push(drive);
    }
    out.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    out
}

/// The SMART figures UDisks2 publishes for one drive.
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Smart {
    pub updated: bool,
    pub failing: Option<bool>,
    pub temperature_kelvin: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub bad_sectors: Option<i64>,
    pub attributes_failing: Option<i32>,
    /// NVMe critical-warning flags, e.g. `spare`, `temperature`, `degraded`.
    pub nvme_warnings: Vec<String>,
    pub is_nvme: bool,
}

/// Fold UDisks2's figures into a drive. Pure, so the verdicts are tested
/// without a system bus.
pub(super) fn apply_smart(drive: &mut DriveHealth, smart: &Smart) {
    if !smart.updated {
        return;
    }
    drive.reliability_unavailable = false;
    if let Some(k) = smart.temperature_kelvin.filter(|k| *k > 0.0) {
        drive.temperature_celsius = Some((k - 273.15).round() as i32);
    }
    drive.power_on_hours = smart.power_on_hours;

    let mut status = Vec::new();
    if smart.is_nvme {
        for warning in &smart.nvme_warnings {
            status.push(
                match warning.as_str() {
                    "spare" | "degraded" | "readonly" | "volatile_mem" | "pmr_readonly" => {
                        "Predictive failure"
                    }
                    "temperature" => "Stressed",
                    _ => "Degraded",
                }
                .to_string(),
            );
        }
        drive.windows_health = Some(
            if smart.nvme_warnings.is_empty() {
                "Healthy"
            } else {
                "Warning"
            }
            .into(),
        );
    } else {
        if smart.failing == Some(true) {
            status.push("Predictive failure".into());
        } else if smart.attributes_failing.unwrap_or(0) > 0 || smart.bad_sectors.unwrap_or(0) > 0 {
            status.push("Degraded".into());
        }
        drive.windows_health = Some(
            if smart.failing == Some(true) {
                "Unhealthy"
            } else {
                "Healthy"
            }
            .into(),
        );
    }
    if let Some(bad) = smart.bad_sectors.filter(|b| *b > 0) {
        drive
            .notes
            .push(format!("{bad} bad sectors have been recorded."));
    }
    status.dedup();
    drive.operational_status = status;
}

#[cfg(target_os = "linux")]
mod udisks {
    use super::*;
    use zbus::zvariant::OwnedValue;

    type Props = HashMap<String, OwnedValue>;
    type Interfaces = HashMap<String, Props>;

    const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
    const ATA: &str = "org.freedesktop.UDisks2.Drive.Ata";
    const NVME: &str = "org.freedesktop.UDisks2.NVMe.Controller";
    const BLOCK: &str = "org.freedesktop.UDisks2.Block";
    const PARTITION: &str = "org.freedesktop.UDisks2.Partition";
    const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";

    fn get<'a, T>(props: &'a Props, key: &str) -> Option<T>
    where
        T: TryFrom<&'a zbus::zvariant::Value<'a>>,
    {
        props.get(key).and_then(|v| T::try_from(&**v).ok())
    }

    fn get_string(props: &Props, key: &str) -> Option<String> {
        props
            .get(key)
            .and_then(|v| <&str>::try_from(&**v).ok())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }

    fn get_bytes_string(props: &Props, key: &str) -> Option<String> {
        let value = props.get(key)?.try_clone().ok()?;
        let bytes = Vec::<u8>::try_from(value).ok()?;
        let text = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .to_string();
        (!text.is_empty()).then_some(text)
    }

    fn get_mount_points(props: &Props) -> Vec<String> {
        let Some(value) = props.get("MountPoints").and_then(|v| v.try_clone().ok()) else {
            return Vec::new();
        };
        let Ok(list) = Vec::<Vec<u8>>::try_from(value) else {
            return Vec::new();
        };
        list.into_iter()
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .trim_end_matches('\0')
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn get_path(props: &Props, key: &str) -> Option<String> {
        props
            .get(key)
            .and_then(|v| zbus::zvariant::ObjectPath::try_from(&**v).ok())
            .map(|p| p.to_string())
    }

    fn managed_objects() -> Option<HashMap<String, Interfaces>> {
        let connection = zbus::blocking::Connection::system().ok()?;
        let proxy = zbus::blocking::fdo::ObjectManagerProxy::builder(&connection)
            .destination("org.freedesktop.UDisks2")
            .ok()?
            .path("/org/freedesktop/UDisks2")
            .ok()?
            .build()
            .ok()?;
        let objects = proxy.get_managed_objects().ok()?;
        Some(
            objects
                .into_iter()
                .map(|(path, interfaces)| {
                    (
                        path.to_string(),
                        interfaces
                            .into_iter()
                            .map(|(name, props)| (name.to_string(), props))
                            .collect(),
                    )
                })
                .collect(),
        )
    }

    pub fn drives() -> Option<Vec<DriveHealth>> {
        let objects = managed_objects()?;

        // Block devices by the drive they belong to.
        let mut whole_disk: HashMap<String, String> = HashMap::new();
        let mut mounts: HashMap<String, Vec<String>> = HashMap::new();
        for interfaces in objects.values() {
            let Some(block) = interfaces.get(BLOCK) else {
                continue;
            };
            let Some(drive) = get_path(block, "Drive").filter(|d| d != "/") else {
                continue;
            };
            if let Some(fs) = interfaces.get(FILESYSTEM) {
                mounts
                    .entry(drive.clone())
                    .or_default()
                    .extend(get_mount_points(fs));
            }
            if !interfaces.contains_key(PARTITION) {
                if let Some(device) = get_bytes_string(block, "Device") {
                    whole_disk.insert(drive, device);
                }
            }
        }

        let mut out = Vec::new();
        for (path, interfaces) in &objects {
            let Some(props) = interfaces.get(DRIVE) else {
                continue;
            };
            let device = whole_disk
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.clone());
            // Optical drives have no health story to tell.
            if device.starts_with("/dev/sr") {
                continue;
            }
            let vendor = get_string(props, "Vendor");
            let model = get_string(props, "Model")
                .map(|m| match &vendor {
                    Some(v) if !m.starts_with(v.as_str()) => format!("{v} {m}"),
                    _ => m,
                })
                .unwrap_or_else(|| device.clone());
            let mut drive = empty_drive(device, model);
            drive.serial_number = get_string(props, "Serial");
            drive.firmware = get_string(props, "Revision");
            drive.size_bytes = get::<u64>(props, "Size").filter(|s| *s > 0);
            let rotation = get::<i32>(props, "RotationRate").unwrap_or(-1);
            drive.media = match rotation {
                0 => MediaKind::Ssd,
                r if r > 0 => MediaKind::Hdd,
                _ => MediaKind::Unspecified,
            };
            if rotation > 0 {
                drive.spindle_speed_rpm = Some(rotation as u32);
            }
            let bus = get_string(props, "ConnectionBus").unwrap_or_default();
            drive.bus = match bus.as_str() {
                "usb" => "USB".to_string(),
                "sdio" => "SD/MMC".to_string(),
                "" if interfaces.contains_key(NVME) => "NVMe".to_string(),
                "" if interfaces.contains_key(ATA) => "SATA".to_string(),
                "" => "Internal".to_string(),
                other => other.to_uppercase(),
            };
            drive.volumes = mounts.remove(path).unwrap_or_default();

            let smart = if let Some(ata) = interfaces.get(ATA) {
                Smart {
                    updated: get::<u64>(ata, "SmartUpdated").unwrap_or(0) > 0,
                    failing: get::<bool>(ata, "SmartFailing"),
                    temperature_kelvin: get::<f64>(ata, "SmartTemperature"),
                    power_on_hours: get::<u64>(ata, "SmartPowerOnSeconds")
                        .filter(|s| *s > 0)
                        .map(|s| s / 3600),
                    bad_sectors: get::<i64>(ata, "SmartNumBadSectors").filter(|b| *b >= 0),
                    attributes_failing: get::<i32>(ata, "SmartNumAttributesFailing"),
                    nvme_warnings: Vec::new(),
                    is_nvme: false,
                }
            } else if let Some(nvme) = interfaces.get(NVME) {
                let warnings = nvme
                    .get("SmartCriticalWarning")
                    .and_then(|v| v.try_clone().ok())
                    .and_then(|v| Vec::<String>::try_from(v).ok())
                    .unwrap_or_default();
                Smart {
                    updated: get::<u64>(nvme, "SmartUpdated").unwrap_or(0) > 0,
                    failing: None,
                    temperature_kelvin: get::<u16>(nvme, "SmartTemperature").map(f64::from),
                    power_on_hours: get::<u64>(nvme, "SmartPowerOnHours"),
                    bad_sectors: None,
                    attributes_failing: None,
                    nvme_warnings: warnings,
                    is_nvme: true,
                }
            } else {
                Smart::default()
            };
            apply_smart(&mut drive, &smart);
            out.push(drive);
        }
        out.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        Some(out)
    }
}

pub fn report() -> Result<DriveHealthReport> {
    #[cfg(target_os = "linux")]
    let from_udisks = udisks::drives().filter(|d| !d.is_empty());
    #[cfg(not(target_os = "linux"))]
    let from_udisks: Option<Vec<DriveHealth>> = None;

    let used_udisks = from_udisks.is_some();
    let mut drives = match from_udisks {
        Some(mut drives) => {
            // UDisks2 does not publish temperature for every bus; sysfs
            // sometimes has it.
            let sysfs: HashMap<String, Option<i32>> = sysfs_drives()
                .into_iter()
                .map(|d| (d.device_id, d.temperature_celsius))
                .collect();
            for d in drives.iter_mut() {
                if d.temperature_celsius.is_none() {
                    d.temperature_celsius = sysfs.get(&d.device_id).copied().flatten();
                }
            }
            drives
        }
        None => sysfs_drives(),
    };

    for drive in drives.iter_mut() {
        let extra = std::mem::take(&mut drive.notes);
        decide(drive);
        drive.notes.extend(extra);
        if drive.reliability_unavailable {
            drive
                .notes
                .retain(|n| !n.contains("does not report detailed health data"));
            drive.notes.push(if used_udisks {
                "This drive has not reported SMART data to the system. USB enclosures often block it.".into()
            } else {
                "Detailed health data comes from the UDisks2 service, which is not running here. Install udisks2 (it ships with every major desktop) to see wear, temperature and error counts.".into()
            });
        }
    }

    Ok(DriveHealthReport {
        error: drives
            .is_empty()
            .then(|| "No physical drives could be read on this system.".to_string()),
        drives,
        elevated: crate::services::security::is_elevated(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_block_devices_are_not_drives() {
        for name in ["loop0", "ram1", "zram0", "dm-2", "md127", "sr0"] {
            assert!(!is_physical_block(name), "{name}");
        }
        for name in ["sda", "nvme0n1", "mmcblk0", "vda"] {
            assert!(is_physical_block(name), "{name}");
        }
    }

    #[test]
    fn partitions_are_mapped_back_to_their_disk() {
        let table = "/dev/nvme0n1p2 / ext4 rw 0 0\n/dev/nvme0n1p1 /boot/efi vfat rw 0 0\n/dev/nvme0n10p1 /other ext4 rw 0 0\n/dev/sda1 /media/My\\040Disk ext4 rw 0 0\n";
        assert_eq!(mounts_for("/dev/nvme0n1", table), vec!["/", "/boot/efi"]);
        assert_eq!(mounts_for("/dev/sda", table), vec!["/media/My Disk"]);
    }

    #[test]
    fn a_failing_ata_drive_is_critical() {
        let mut d = empty_drive("/dev/sda".into(), "Disk".into());
        apply_smart(
            &mut d,
            &Smart {
                updated: true,
                failing: Some(true),
                temperature_kelvin: Some(313.15),
                power_on_hours: Some(20_000),
                ..Default::default()
            },
        );
        decide(&mut d);
        assert_eq!(d.state, HealthState::Critical);
        assert_eq!(d.temperature_celsius, Some(40));
        assert!(!d.reliability_unavailable);
    }

    #[test]
    fn a_clean_nvme_drive_is_healthy_and_a_warning_is_not() {
        let mut d = empty_drive("/dev/nvme0n1".into(), "SSD".into());
        apply_smart(
            &mut d,
            &Smart {
                updated: true,
                is_nvme: true,
                ..Default::default()
            },
        );
        decide(&mut d);
        assert_eq!(d.state, HealthState::Healthy);

        let mut d = empty_drive("/dev/nvme0n1".into(), "SSD".into());
        apply_smart(
            &mut d,
            &Smart {
                updated: true,
                is_nvme: true,
                nvme_warnings: vec!["spare".into()],
                ..Default::default()
            },
        );
        decide(&mut d);
        assert_eq!(d.state, HealthState::Critical);
    }

    #[test]
    fn a_drive_without_smart_data_stays_unknown() {
        let mut d = empty_drive("/dev/sdb".into(), "USB stick".into());
        apply_smart(&mut d, &Smart::default());
        decide(&mut d);
        assert_eq!(d.state, HealthState::Unknown);
    }

    #[test]
    fn the_report_reads_without_error() {
        let report = report().unwrap();
        for d in &report.drives {
            assert!(d.device_id.starts_with('/'));
        }
    }
}

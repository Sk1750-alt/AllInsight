//! GPU status on Linux.
//!
//! Adapters come from the DRM class in sysfs, with their names looked up in
//! the system's PCI ID database. Utilisation is only reported where a driver
//! publishes it without privilege:
//!
//! - `amdgpu`: `gpu_busy_percent` and the VRAM counters in sysfs
//! - NVIDIA's proprietary driver: `nvidia-smi`, when it is installed
//!
//! Intel and nouveau publish no unprivileged utilisation counter, so for
//! them the figure is honestly absent.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{GpuAdapter, GpuStatus};

fn read_trimmed(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn hex_id(path: &Path) -> Option<u16> {
    let raw = read_trimmed(path)?;
    u16::from_str_radix(raw.trim_start_matches("0x"), 16).ok()
}

/// Look a vendor and device up in a `pci.ids` listing.
pub(super) fn pci_name(ids: &str, vendor: u16, device: u16) -> Option<String> {
    let vendor_prefix = format!("{vendor:04x}  ");
    let device_prefix = format!("\t{device:04x}  ");
    let mut vendor_name: Option<&str> = None;
    for line in ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if vendor_name.is_none() {
            if let Some(name) = line.strip_prefix(&vendor_prefix) {
                vendor_name = Some(name.trim());
            }
            continue;
        }
        // The vendor's block ends at the next unindented line.
        if !line.starts_with('\t') {
            break;
        }
        if let Some(name) = line.strip_prefix(&device_prefix) {
            return Some(format!(
                "{} {}",
                short_vendor(vendor, vendor_name?),
                name.trim()
            ));
        }
    }
    vendor_name.map(|v| short_vendor(vendor, v).to_string())
}

fn short_vendor(vendor: u16, fallback: &str) -> &str {
    match vendor {
        0x10de => "NVIDIA",
        0x1002 => "AMD",
        0x8086 => "Intel",
        _ => fallback,
    }
}

fn pci_ids() -> String {
    for path in [
        "/usr/share/hwdata/pci.ids",
        "/usr/share/misc/pci.ids",
        "/usr/share/pci.ids",
    ] {
        if let Ok(text) = std::fs::read_to_string(path) {
            return text;
        }
    }
    String::new()
}

/// The PCI device directory of every display controller DRM knows about.
fn drm_devices() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // `card0`, not `card0-HDMI-A-1`.
            name.starts_with("card") && name[4..].chars().all(|c| c.is_ascii_digit())
        })
        .filter_map(|e| std::fs::canonicalize(e.path().join("device")).ok())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Parse `nvidia-smi --query-gpu=name,driver_version,utilization.gpu,memory.used,memory.total
/// --format=csv,noheader,nounits`.
pub(super) fn parse_nvidia_smi(
    text: &str,
) -> Vec<(
    String,
    Option<String>,
    Option<f32>,
    Option<u64>,
    Option<u64>,
)> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            if f.len() != 5 || f[0].is_empty() {
                return None;
            }
            let mib = |s: &str| s.parse::<u64>().ok().map(|m| m * 1024 * 1024);
            Some((
                f[0].to_string(),
                Some(f[1].to_string()).filter(|s| !s.is_empty()),
                f[2].parse::<f32>().ok(),
                mib(f[3]),
                mib(f[4]),
            ))
        })
        .collect()
}

fn nvidia_smi() -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn status() -> GpuStatus {
    let devices = drm_devices();
    let ids = if devices.is_empty() {
        String::new()
    } else {
        pci_ids()
    };

    let mut adapters = Vec::new();
    let mut busy: Vec<f32> = Vec::new();
    let mut vram_used: Option<u64> = None;
    let mut has_nvidia = false;

    for dev in &devices {
        let vendor = hex_id(&dev.join("vendor"));
        let device = hex_id(&dev.join("device"));
        if vendor == Some(0x10de) {
            has_nvidia = true;
        }
        let name = match (vendor, device) {
            (Some(v), Some(d)) => pci_name(&ids, v, d),
            _ => None,
        }
        .unwrap_or_else(|| "Display adapter".to_string());
        let driver = std::fs::read_link(dev.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));

        if let Some(b) =
            read_trimmed(&dev.join("gpu_busy_percent")).and_then(|b| b.parse::<f32>().ok())
        {
            busy.push(b);
        }
        if let Some(used) =
            read_trimmed(&dev.join("mem_info_vram_used")).and_then(|v| v.parse::<u64>().ok())
        {
            vram_used = Some(vram_used.unwrap_or(0) + used);
        }
        adapters.push(GpuAdapter {
            name,
            driver_version: driver,
            video_memory_bytes: read_trimmed(&dev.join("mem_info_vram_total"))
                .and_then(|v| v.parse::<u64>().ok()),
        });
    }

    // The proprietary NVIDIA driver publishes nothing useful in sysfs.
    if has_nvidia {
        if let Some(text) = nvidia_smi() {
            let rows = parse_nvidia_smi(&text);
            adapters.retain(|a| !a.name.starts_with("NVIDIA"));
            for (name, driver, util, used, total) in rows {
                if let Some(u) = util {
                    busy.push(u);
                }
                if let Some(u) = used {
                    vram_used = Some(vram_used.unwrap_or(0) + u);
                }
                adapters.push(GpuAdapter {
                    name,
                    driver_version: driver,
                    video_memory_bytes: total,
                });
            }
        }
    }

    let utilization = busy.iter().cloned().fold(None, |acc: Option<f32>, b| {
        Some(acc.map_or(b, |a| a.max(b)))
    });
    GpuStatus {
        available: utilization.is_some(),
        note: if adapters.is_empty() {
            Some("No display adapter was found.".into())
        } else if utilization.is_none() {
            Some(
                "This graphics driver does not publish a utilisation counter to ordinary users."
                    .into(),
            )
        } else {
            None
        },
        utilization_percent: utilization,
        dedicated_memory_bytes: vram_used,
        adapters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &str = "\
# comment
10de  NVIDIA Corporation
\t2684  AD102 [GeForce RTX 4090]
\t\t1043 889d  subsystem line
1002  Advanced Micro Devices, Inc. [AMD/ATI]
\t73bf  Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]
8086  Intel Corporation
";

    #[test]
    fn device_names_come_from_the_pci_database() {
        assert_eq!(
            pci_name(IDS, 0x10de, 0x2684).as_deref(),
            Some("NVIDIA AD102 [GeForce RTX 4090]")
        );
        assert_eq!(
            pci_name(IDS, 0x1002, 0x73bf).as_deref(),
            Some("AMD Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]")
        );
        assert_eq!(pci_name(IDS, 0x8086, 0x9999).as_deref(), Some("Intel"));
        assert_eq!(pci_name(IDS, 0x1234, 0x0001), None);
    }

    #[test]
    fn nvidia_smi_rows_are_parsed() {
        let rows = parse_nvidia_smi("NVIDIA GeForce RTX 3060, 550.54.14, 37, 1024, 12288\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, Some(37.0));
        assert_eq!(rows[0].4, Some(12288 * 1024 * 1024));
    }

    #[test]
    fn status_reads_without_error() {
        let s = status();
        assert!(s.available == s.utilization_percent.is_some());
    }
}

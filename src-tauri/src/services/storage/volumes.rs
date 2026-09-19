//! Volume enumeration.
//!
//! Every drive letter currently mounted is reported, including removable and
//! network volumes, because assuming `C:` is the only drive is one of the
//! quickest ways to make a storage tool wrong.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    RamDisk,
    Unknown,
}

impl DriveKind {
    pub fn label(&self) -> &'static str {
        match self {
            DriveKind::Fixed => "Internal drive",
            DriveKind::Removable => "Removable drive",
            DriveKind::Network => "Network drive",
            DriveKind::Optical => "Optical drive",
            DriveKind::RamDisk => "RAM disk",
            DriveKind::Unknown => "Unknown",
        }
    }

    /// Only fixed and removable volumes are worth scanning; optical and
    /// network volumes are listed but never walked by default.
    pub fn is_scannable(&self) -> bool {
        matches!(self, DriveKind::Fixed | DriveKind::Removable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeInfo {
    /// `C:\`
    pub mount_point: String,
    /// `C:`
    pub letter: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub kind: DriveKind,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
    /// True for the volume holding the running Windows installation.
    pub is_system: bool,
    /// False when the volume reported no capacity, which happens for an
    /// optical drive with no disc or a card reader with no card.
    pub is_ready: bool,
}

impl VolumeInfo {
    pub fn path(&self) -> PathBuf {
        PathBuf::from(&self.mount_point)
    }
}

/// Percentage used, guarded against a zero-capacity volume.
fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}

#[cfg(windows)]
fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end]).to_string_lossy().into_owned()
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// List every mounted volume.
#[cfg(windows)]
pub fn list_volumes() -> Vec<VolumeInfo> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    };

    // `GetDriveTypeW` return values from winbase.h. Declared here because
    // windows-sys does not re-export them.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;

    let system_root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let mut out = Vec::new();

    // Two calls: one to size the buffer, one to fill it.
    let needed = unsafe { GetLogicalDriveStringsW(0, std::ptr::null_mut()) };
    if needed == 0 {
        return out;
    }
    let mut buf = vec![0u16; needed as usize + 1];
    let written = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
    if written == 0 {
        return out;
    }

    // The result is a run of NUL-terminated strings ending with a double NUL.
    for chunk in buf[..written as usize].split(|&c| c == 0) {
        if chunk.is_empty() {
            continue;
        }
        let mount_point = OsString::from_wide(chunk).to_string_lossy().into_owned();
        let wide_root = wide(&mount_point);

        let kind = match unsafe { GetDriveTypeW(wide_root.as_ptr()) } {
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_REMOTE => DriveKind::Network,
            DRIVE_CDROM => DriveKind::Optical,
            DRIVE_RAMDISK => DriveKind::RamDisk,
            _ => DriveKind::Unknown,
        };

        let mut label_buf = [0u16; 261];
        let mut fs_buf = [0u16; 64];
        let mut serial: u32 = 0;
        let mut max_component: u32 = 0;
        let mut flags: u32 = 0;
        let named = unsafe {
            GetVolumeInformationW(
                wide_root.as_ptr(),
                label_buf.as_mut_ptr(),
                label_buf.len() as u32,
                &mut serial,
                &mut max_component,
                &mut flags,
                fs_buf.as_mut_ptr(),
                fs_buf.len() as u32,
            )
        } != 0;

        let mut free_to_caller: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        let sized = unsafe {
            GetDiskFreeSpaceExW(
                wide_root.as_ptr(),
                &mut free_to_caller,
                &mut total,
                &mut total_free,
            )
        } != 0;

        let is_ready = sized && total > 0;
        // `free_to_caller` honours per-user quotas, which is the number the
        // user can actually use, so it is the one shown.
        let free = if is_ready { free_to_caller } else { 0 };
        let used = total.saturating_sub(if is_ready { total_free } else { 0 });

        let letter = mount_point.trim_end_matches('\\').to_string();
        out.push(VolumeInfo {
            is_system: letter.eq_ignore_ascii_case(&system_root),
            label: if named {
                let l = wide_to_string(&label_buf);
                if l.is_empty() {
                    None
                } else {
                    Some(l)
                }
            } else {
                None
            },
            filesystem: if named {
                let f = wide_to_string(&fs_buf);
                if f.is_empty() {
                    None
                } else {
                    Some(f)
                }
            } else {
                None
            },
            used_percent: percent(used, total),
            mount_point,
            letter,
            kind,
            total_bytes: total,
            free_bytes: free,
            used_bytes: used,
            is_ready,
        });
    }

    out
}

/// Non-Windows builds exist only so the safety tests can run in CI; they
/// report a single synthetic root rather than pretending to know more.
#[cfg(not(windows))]
pub fn list_volumes() -> Vec<VolumeInfo> {
    Vec::new()
}

/// The volume that holds Windows, when it can be identified.
pub fn system_volume() -> Option<VolumeInfo> {
    list_volumes().into_iter().find(|v| v.is_system)
}

/// Look up one volume by its mount point.
pub fn volume_for(mount_point: &str) -> Option<VolumeInfo> {
    let wanted = mount_point.trim_end_matches('\\').to_lowercase();
    list_volumes()
        .into_iter()
        .find(|v| v.letter.to_lowercase() == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_survive_a_zero_capacity_volume() {
        assert_eq!(percent(0, 0), 0.0);
        assert!((percent(50, 100) - 50.0).abs() < f64::EPSILON);
    }

    #[cfg(windows)]
    #[test]
    fn the_system_volume_is_found_and_self_consistent() {
        let volumes = list_volumes();
        assert!(!volumes.is_empty(), "at least one volume must be reported");
        let system = volumes.iter().find(|v| v.is_system).expect("system volume");
        assert!(system.total_bytes > 0);
        assert!(system.used_bytes <= system.total_bytes);
        assert!(system.used_percent >= 0.0 && system.used_percent <= 100.0);
    }
}

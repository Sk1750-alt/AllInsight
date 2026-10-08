//! The process monitor.
//!
//! Reads from the shared `sysinfo::System` owned by the metrics service, adds
//! the publisher recorded in each executable's version resource, and refuses
//! outright to terminate the processes Windows needs in order to keep running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{AllInsightError, Result};
use crate::services::security::paths;
use crate::services::system::SystemMonitor;

/// How dangerous it is to end a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRisk {
    /// Ending this would stop the operating system or end the session.
    /// AllInsight refuses.
    Critical,
    /// Part of the operating system or desktop, but recoverable.
    /// Confirmation required.
    SystemComponent,
    /// An ordinary application.
    Normal,
}

/// Processes that keep the session alive. Ending any of these either
/// bugchecks the machine or forces a sign-out, so the answer is always no.
#[cfg(windows)]
const NEVER_TERMINATE: &[&str] = &[
    "system",
    "system idle process",
    "registry",
    "memory compression",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "sihost.exe",
    "ntoskrnl.exe",
    "secure system",
];

/// Windows components that can be ended, at a cost the user should be warned
/// about first.
#[cfg(windows)]
const CONFIRM_BEFORE_TERMINATING: &[&str] = &[
    "explorer.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchindexer.exe",
    "runtimebroker.exe",
    "taskhostw.exe",
    "ctfmon.exe",
    "audiodg.exe",
];

/// Linux: init, the display server and compositor, the login manager, and
/// the system services a desktop session cannot outlive. Most of these run
/// as root and could not be ended anyway, but the user-owned ones (the
/// compositor, the session bus) can, and ending them logs the user out.
#[cfg(not(windows))]
const NEVER_TERMINATE: &[&str] = &[
    "systemd",
    "init",
    "kthreadd",
    "systemd-journald",
    "systemd-logind",
    "systemd-udevd",
    "dbus-daemon",
    "dbus-broker",
    "dbus-broker-launch",
    "xorg",
    "xwayland",
    "gnome-shell",
    "gnome-session-binary",
    "plasmashell",
    "kwin_wayland",
    "kwin_x11",
    "ksmserver",
    "mutter",
    "xfce4-session",
    "xfwm4",
    "cinnamon",
    "cinnamon-session",
    "mate-session",
    "sway",
    "hyprland",
    "gdm",
    "gdm3",
    "sddm",
    "lightdm",
    "login",
    "agetty",
    "polkitd",
    "launchd",
    "windowserver",
    "loginwindow",
    "kernel_task",
];

/// Linux components whose loss is recoverable but noticeable: sound, the
/// network, panels and file managers restart, often after a sign-out.
#[cfg(not(windows))]
const CONFIRM_BEFORE_TERMINATING: &[&str] = &[
    "pipewire",
    "pipewire-pulse",
    "wireplumber",
    "pulseaudio",
    "networkmanager",
    "nm-applet",
    "gnome-settings-daemon",
    "gsd-power",
    "xdg-desktop-portal",
    "xdg-desktop-portal-gtk",
    "xdg-desktop-portal-gnome",
    "xdg-desktop-portal-kde",
    "nautilus",
    "dolphin",
    "nemo",
    "thunar",
    "xfce4-panel",
    "waybar",
    "ibus-daemon",
    "fcitx5",
    "finder",
    "dock",
    "systemuiserver",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub executable: Option<PathBuf>,
    pub publisher: Option<String>,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub memory_percent: f32,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub run_time_seconds: u64,
    pub risk: ProcessRisk,
    /// Set for the process using the most CPU, so the UI can point at it.
    pub is_top_cpu: bool,
    pub is_top_memory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessList {
    pub processes: Vec<ProcessInfo>,
    pub total: usize,
    pub sampled_at: i64,
}

fn risk_for(name: &str) -> ProcessRisk {
    let lower = name.to_lowercase();
    if NEVER_TERMINATE.contains(&lower.as_str()) {
        ProcessRisk::Critical
    } else if CONFIRM_BEFORE_TERMINATING.contains(&lower.as_str()) {
        ProcessRisk::SystemComponent
    } else {
        ProcessRisk::Normal
    }
}

/// Publisher lookups hit the disk, so results are cached for the life of the
/// process. Executable paths do not change under a running PID.
static PUBLISHER_CACHE: once_cell::sync::Lazy<Mutex<HashMap<PathBuf, Option<String>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

pub fn publisher_of(path: &Path) -> Option<String> {
    if let Some(hit) = PUBLISHER_CACHE.lock().get(path) {
        return hit.clone();
    }
    let value = read_company_name(path);
    PUBLISHER_CACHE
        .lock()
        .insert(path.to_path_buf(), value.clone());
    value
}

/// Read `CompanyName` from an executable's version resource.
#[cfg(windows)]
fn read_company_name(path: &Path) -> Option<String> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut handle: u32 = 0;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buffer.as_mut_ptr().cast()) == 0 {
            return None;
        }

        // The translation table says which language and codepage the strings
        // are stored under. Reading it is the only reliable way in.
        let translation_key: Vec<u16> = "\\VarFileInfo\\Translation"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut translation_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut translation_len: u32 = 0;
        if VerQueryValueW(
            buffer.as_ptr().cast(),
            translation_key.as_ptr(),
            &mut translation_ptr,
            &mut translation_len,
        ) == 0
            || translation_len < 4
        {
            return None;
        }
        let language = *(translation_ptr as *const u16);
        let codepage = *(translation_ptr as *const u16).add(1);

        let query = format!("\\StringFileInfo\\{language:04x}{codepage:04x}\\CompanyName");
        let query_wide: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();
        let mut value_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut value_len: u32 = 0;
        if VerQueryValueW(
            buffer.as_ptr().cast(),
            query_wide.as_ptr(),
            &mut value_ptr,
            &mut value_len,
        ) == 0
            || value_len == 0
        {
            return None;
        }

        let slice = std::slice::from_raw_parts(value_ptr as *const u16, value_len as usize);
        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
        let name = std::ffi::OsString::from_wide(&slice[..end])
            .to_string_lossy()
            .trim()
            .to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }
}

#[cfg(not(windows))]
fn read_company_name(_path: &Path) -> Option<String> {
    None
}

/// Snapshot every process. `limit` caps the returned list after sorting by the
/// requested column, because a machine can easily have 400 processes and the
/// UI only ever shows a page of them.
pub fn list(monitor: &SystemMonitor, limit: usize, resolve_publishers: bool) -> ProcessList {
    monitor.refresh_processes();

    let total_memory = monitor.with_system(|s| s.total_memory()).max(1);
    let mut processes: Vec<ProcessInfo> = monitor.with_system(|system| {
        system
            .processes()
            .iter()
            .map(|(pid, p)| {
                let name = p.name().to_string_lossy().into_owned();
                let executable = p.exe().map(Path::to_path_buf);
                let usage = p.disk_usage();
                ProcessInfo {
                    pid: pid.as_u32(),
                    parent_pid: p.parent().map(|p| p.as_u32()),
                    risk: risk_for(&name),
                    publisher: None,
                    cpu_percent: p.cpu_usage(),
                    memory_bytes: p.memory(),
                    memory_percent: (p.memory() as f64 / total_memory as f64 * 100.0) as f32,
                    disk_read_bytes: usage.read_bytes,
                    disk_write_bytes: usage.written_bytes,
                    run_time_seconds: p.run_time(),
                    is_top_cpu: false,
                    is_top_memory: false,
                    name,
                    executable,
                }
            })
            .collect()
    });

    processes.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
    });

    let total = processes.len();
    if let Some(first) = processes.first_mut() {
        first.is_top_cpu = true;
    }
    if let Some(index) = processes
        .iter()
        .enumerate()
        .max_by_key(|(_, p)| p.memory_bytes)
        .map(|(i, _)| i)
    {
        processes[index].is_top_memory = true;
    }

    processes.truncate(limit);

    // Version resources are only read for the rows that will be shown.
    if resolve_publishers {
        for p in processes.iter_mut() {
            if let Some(exe) = &p.executable {
                p.publisher = publisher_of(exe);
            }
        }
    }

    ProcessList {
        processes,
        total,
        sampled_at: chrono::Utc::now().timestamp(),
    }
}

/// End a process.
///
/// `confirmed` corresponds to the user having accepted the warning dialog. A
/// critical process is refused whether or not that flag is set.
pub fn terminate(monitor: &SystemMonitor, pid: u32, confirmed: bool) -> Result<()> {
    let (name, risk) = monitor.with_system(|system| {
        system
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| {
                let name = p.name().to_string_lossy().into_owned();
                let risk = risk_for(&name);
                (name, risk)
            })
            .unwrap_or_else(|| (String::new(), ProcessRisk::Normal))
    });

    if name.is_empty() {
        return Err(AllInsightError::InvalidInput(
            "That process is no longer running.".into(),
        ));
    }

    let os = crate::platform::os_name();
    // PID 1 is init on every Unix, and ending AllInsight from its own
    // process list would only lose whatever it was doing.
    if risk == ProcessRisk::Critical || pid == 1 || pid == std::process::id() {
        return Err(AllInsightError::InvalidInput(format!(
            "{name} is required by {os} and cannot be ended from AllInsight."
        )));
    }

    if risk == ProcessRisk::SystemComponent && !confirmed {
        return Err(AllInsightError::InvalidInput(format!(
            "{name} is part of {os}. Confirm before ending it."
        )));
    }

    let killed = monitor.with_system(|system| {
        system
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| {
                // On Unix, ask first: SIGTERM lets the program save and exit
                // cleanly, where SIGKILL (what `kill` sends) does not.
                #[cfg(unix)]
                {
                    p.kill_with(sysinfo::Signal::Term)
                        .unwrap_or_else(|| p.kill())
                }
                #[cfg(not(unix))]
                {
                    p.kill()
                }
            })
            .unwrap_or(false)
    });

    if killed {
        Ok(())
    } else {
        Err(AllInsightError::Platform(format!(
            "{os} refused to end {name}. It may belong to another user or need administrator permission."
        )))
    }
}

/// The folder holding a process's executable, for "Show in Explorer".
pub fn location_of(monitor: &SystemMonitor, pid: u32) -> Option<PathBuf> {
    monitor.with_system(|system| {
        system
            .process(sysinfo::Pid::from_u32(pid))
            .and_then(|p| p.exe().map(Path::to_path_buf))
            .map(|exe| paths::strip_verbatim(&exe))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn session_essentials_are_never_terminable() {
        for name in [
            "systemd",
            "Xwayland",
            "gnome-shell",
            "kwin_wayland",
            "dbus-daemon",
        ] {
            assert_eq!(risk_for(name), ProcessRisk::Critical, "{name}");
        }
        assert_eq!(risk_for("pipewire"), ProcessRisk::SystemComponent);
        assert_eq!(risk_for("firefox"), ProcessRisk::Normal);
    }

    #[cfg(windows)]
    #[test]
    fn windows_essentials_are_never_terminable() {
        for name in ["csrss.exe", "LSASS.EXE", "Registry", "System"] {
            assert_eq!(risk_for(name), ProcessRisk::Critical, "{name}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn shell_components_require_confirmation() {
        assert_eq!(risk_for("explorer.exe"), ProcessRisk::SystemComponent);
        assert_eq!(risk_for("RuntimeBroker.exe"), ProcessRisk::SystemComponent);
    }

    #[test]
    fn ordinary_applications_are_normal() {
        assert_eq!(risk_for("notepad.exe"), ProcessRisk::Normal);
        assert_eq!(risk_for("chrome.exe"), ProcessRisk::Normal);
    }

    #[test]
    fn terminating_a_critical_process_is_refused_before_anything_happens() {
        let monitor = SystemMonitor::new();
        monitor.refresh_processes();
        let critical = monitor.with_system(|system| {
            system
                .processes()
                .iter()
                .find(|(_, p)| risk_for(&p.name().to_string_lossy()) == ProcessRisk::Critical)
                .map(|(pid, _)| pid.as_u32())
        });
        if let Some(pid) = critical {
            let err = terminate(&monitor, pid, true).unwrap_err();
            assert!(err.to_string().contains("cannot be ended"));
        }
    }

    #[test]
    fn the_process_list_is_sorted_and_capped() {
        let monitor = SystemMonitor::new();
        let list = list(&monitor, 10, false);
        assert!(list.processes.len() <= 10);
        assert!(list.total >= list.processes.len());
        for pair in list.processes.windows(2) {
            assert!(pair[0].cpu_percent >= pair[1].cpu_percent - f32::EPSILON);
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_publisher_of_a_windows_binary_reads_as_microsoft() {
        let explorer = paths::expand_env("%SystemRoot%\\explorer.exe").unwrap();
        if explorer.exists() {
            let publisher = publisher_of(&explorer).unwrap_or_default();
            assert!(
                publisher.to_lowercase().contains("microsoft"),
                "unexpected publisher: {publisher}"
            );
        }
    }
}

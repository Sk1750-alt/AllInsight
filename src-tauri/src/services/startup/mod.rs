//! The startup manager.
//!
//! Each platform keeps its startup entries somewhere different:
//!
//! - Windows: the `Run` registry keys and the Startup folders, with the
//!   enabled state in `StartupApproved`, which is what Task Manager writes.
//! - Linux: freedesktop autostart entries in `~/.config/autostart` and
//!   `/etc/xdg/autostart`, which every major desktop honours.
//!
//! On both, AllInsight disables an entry the way the platform's own tools do
//! and never deletes the entry itself, so nothing is lost and every change can
//! be undone from AllInsight or from the system settings.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::services::security::paths;

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "linux.rs"]
mod imp;

#[cfg(target_os = "macos")]
mod imp {
    use super::StartupList;
    use crate::error::{AllInsightError, Result};

    pub fn list() -> StartupList {
        StartupList {
            items: Vec::new(),
            enabled_count: 0,
            elevated: crate::services::security::is_elevated(),
        }
    }

    pub fn set_enabled(_id: &str, _enabled: bool) -> Result<()> {
        Err(AllInsightError::Platform(
            "Managing login items is not available on macOS yet.".into(),
        ))
    }
}

pub use imp::{list, set_enabled};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupImpact {
    Low,
    Medium,
    High,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupLocation {
    /// `HKCU\...\Run`
    UserRun,
    /// `HKLM\...\Run`, 64-bit view
    MachineRun,
    /// `HKLM\...\WOW6432Node\...\Run`
    MachineRun32,
    /// The per-user Startup folder
    UserStartupFolder,
    /// The all-users Startup folder
    CommonStartupFolder,
    /// `~/.config/autostart`
    UserAutostart,
    /// `/etc/xdg/autostart`, overridable per user
    SystemAutostart,
}

impl StartupLocation {
    pub fn label(&self) -> &'static str {
        match self {
            StartupLocation::UserRun => "Registry (this user)",
            StartupLocation::MachineRun => "Registry (all users)",
            StartupLocation::MachineRun32 => "Registry (all users, 32-bit)",
            StartupLocation::UserStartupFolder => "Startup folder (this user)",
            StartupLocation::CommonStartupFolder => "Startup folder (all users)",
            StartupLocation::UserAutostart => "Autostart (this user)",
            StartupLocation::SystemAutostart => "Autostart (all users)",
        }
    }

    pub fn is_machine_scope(&self) -> bool {
        matches!(
            self,
            StartupLocation::MachineRun
                | StartupLocation::MachineRun32
                | StartupLocation::CommonStartupFolder
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupItem {
    /// `location:name`, the handle the frontend passes back.
    pub id: String,
    pub name: String,
    pub command: String,
    pub executable: Option<PathBuf>,
    pub publisher: Option<String>,
    pub location: StartupLocation,
    pub location_label: String,
    pub enabled: bool,
    pub impact: StartupImpact,
    /// True because no platform exposes a measured startup impact to
    /// applications; the value shown is derived from the program's size.
    pub impact_is_estimated: bool,
    /// False when this entry lives in a machine-wide location and AllInsight is
    /// not elevated, so the toggle is shown but disabled.
    pub can_toggle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupList {
    pub items: Vec<StartupItem>,
    pub enabled_count: usize,
    pub elevated: bool,
}

/// Estimate the startup cost.
///
/// Task Manager measures this during boot and does not publish the result, and
/// Linux desktops do not measure it at all, so AllInsight derives a band from
/// the size of the executable and says plainly that it is an estimate. It is a
/// rough proxy for how much has to be paged in.
fn estimate_impact(executable: Option<&PathBuf>) -> StartupImpact {
    let Some(path) = executable else {
        return StartupImpact::Unknown;
    };
    let Ok(meta) = std::fs::metadata(paths::long_path(path)) else {
        return StartupImpact::Unknown;
    };
    match meta.len() {
        0..=2_000_000 => StartupImpact::Low,
        2_000_001..=25_000_000 => StartupImpact::Medium,
        _ => StartupImpact::High,
    }
}

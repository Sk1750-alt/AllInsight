//! The installed application list.
//!
//! Each platform reads the inventory its own system tools use, so what
//! AllInsight shows matches what the user sees elsewhere:
//!
//! - Windows: the uninstall registry hives behind Settings and Control Panel.
//! - Linux: the desktop entries a launcher shows, each traced back to the
//!   package that owns it (dpkg, rpm or pacman), plus Flatpak and Snap.
//!
//! Uninstalling always goes through the owner of the software - the vendor's
//! uninstaller, Flatpak or snapd - never by deleting a folder, because that
//! leaves the rest of the installation behind. Where that owner needs an
//! administrator (apt, dnf, pacman), AllInsight shows the command instead of
//! running a package manager with elevated rights on the user's behalf.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;

pub mod linux_parse;

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::InstalledApp;
    use crate::error::{AllInsightError, Result};

    pub fn collect() -> Vec<InstalledApp> {
        Vec::new()
    }

    pub fn uninstall(_id: &str) -> Result<()> {
        Err(AllInsightError::Platform(
            "Uninstalling is not available on this operating system yet.".into(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    /// Stable handle the frontend passes back: the registry key name on
    /// Windows, `<source>:<package>` on Linux.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// As reported by the installer or package database. Frequently absent
    /// or wrong on Windows, so it is optional and labelled as an estimate.
    pub estimated_size_bytes: Option<u64>,
    /// Measured by walking `install_location`, when AllInsight was asked to.
    pub measured_size_bytes: Option<u64>,
    pub install_date: Option<String>,
    pub install_location: Option<PathBuf>,
    pub scope: AppScope,
    /// True when AllInsight can start the removal itself.
    pub has_uninstaller: bool,
    pub is_windows_component: bool,
    pub source: AppSource,
    /// When removal needs an administrator, the exact command to run in a
    /// terminal. AllInsight never runs it itself.
    pub uninstall_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppScope {
    /// Installed for every user.
    AllUsers,
    /// Installed for the signed-in user only.
    CurrentUser,
}

/// Where an application's record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    Registry,
    Dpkg,
    Rpm,
    Pacman,
    Flatpak,
    Snap,
    /// A desktop entry no package claims, e.g. something unpacked into
    /// `/opt` or `~/.local`. Listed, never removed.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppList {
    pub apps: Vec<InstalledApp>,
    pub total: usize,
}

pub fn list(measure_sizes: bool) -> AppList {
    let mut apps = imp::collect();

    if measure_sizes {
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        for app in apps.iter_mut() {
            if let Some(location) = &app.install_location {
                // Only measure locations that look like an installation, never
                // a whole drive because a vendor wrote `C:\` into the registry.
                if location.components().count() >= 3 {
                    let (bytes, _) =
                        crate::services::storage::scanner::measure(location, &cancelled);
                    app.measured_size_bytes = Some(bytes);
                }
            }
        }
    }

    apps.sort_by(|a, b| {
        b.measured_size_bytes
            .or(b.estimated_size_bytes)
            .unwrap_or(0)
            .cmp(
                &a.measured_size_bytes
                    .or(a.estimated_size_bytes)
                    .unwrap_or(0),
            )
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let total = apps.len();
    AppList { apps, total }
}

/// Start removing one application. The id must have come from [`list`].
pub fn uninstall(id: &str) -> Result<()> {
    imp::uninstall(id)
}

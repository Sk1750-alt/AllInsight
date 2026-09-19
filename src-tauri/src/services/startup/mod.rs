//! The startup manager.
//!
//! Startup entries live in two places Windows treats as equivalent: the `Run`
//! registry keys, and the Startup folders. Their enabled state lives in a
//! third place, `StartupApproved`, which is what Task Manager writes when you
//! disable something.
//!
//! AllInsight disables and re-enables entries by writing that same approval value.
//! It never deletes a `Run` value or a shortcut, so nothing is lost and the
//! change is always reversible from Task Manager as well.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_32KEY,
    KEY_WOW64_64KEY,
};
use winreg::RegKey;

use crate::error::{AllInsightError, Result};
use crate::services::security::paths;

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
}

impl StartupLocation {
    pub fn label(&self) -> &'static str {
        match self {
            StartupLocation::UserRun => "Registry (this user)",
            StartupLocation::MachineRun => "Registry (all users)",
            StartupLocation::MachineRun32 => "Registry (all users, 32-bit)",
            StartupLocation::UserStartupFolder => "Startup folder (this user)",
            StartupLocation::CommonStartupFolder => "Startup folder (all users)",
        }
    }

    /// Which `StartupApproved` subkey records this location's state.
    fn approval_key(&self) -> Option<(&'static str, bool)> {
        match self {
            StartupLocation::UserRun => Some(("Run", false)),
            StartupLocation::MachineRun => Some(("Run", true)),
            StartupLocation::MachineRun32 => Some(("Run32", true)),
            StartupLocation::UserStartupFolder => Some(("StartupFolder", false)),
            StartupLocation::CommonStartupFolder => Some(("StartupFolder", true)),
        }
    }

    fn is_machine_scope(&self) -> bool {
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
    /// True because Windows does not expose Task Manager's measured impact to
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

const RUN_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_PATH: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// Pull the executable out of a command line so its size and publisher can be
/// read. Uses the same conservative split as the uninstaller launcher.
fn executable_from_command(command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    let candidate = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next()?.to_string()
    } else {
        let lower = trimmed.to_lowercase();
        match lower.find(".exe") {
            Some(pos) => trimmed[..pos + 4].to_string(),
            None => trimmed.split_whitespace().next()?.to_string(),
        }
    };
    let path = paths::normalize_lexical(std::path::Path::new(&candidate));
    if path.is_absolute() && path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Estimate the startup cost.
///
/// Task Manager measures this during boot and does not publish the result, so
/// AllInsight derives a band from the size of the executable and says plainly that
/// it is an estimate. It is a rough proxy for how much has to be paged in.
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

/// Read the approval byte. Absent means enabled, `2` means enabled, and
/// anything else (`3`, `6`) means the user or a tool disabled it.
fn read_approval(location: StartupLocation, name: &str) -> bool {
    let Some((leaf, machine)) = location.approval_key() else {
        return true;
    };
    let root = RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    });
    let Ok(key) = root.open_subkey_with_flags(format!("{APPROVED_PATH}\\{leaf}"), KEY_READ) else {
        return true;
    };
    match key.get_raw_value(name) {
        Ok(value) => value.bytes.first().map(|b| *b == 2 || *b == 0).unwrap_or(true),
        Err(_) => true,
    }
}

fn write_approval(location: StartupLocation, name: &str, enabled: bool) -> Result<()> {
    let Some((leaf, machine)) = location.approval_key() else {
        return Err(AllInsightError::InvalidInput(
            "This startup entry cannot be toggled.".into(),
        ));
    };
    if machine && !crate::services::security::is_elevated() {
        return Err(AllInsightError::ElevationRequired(
            "change a startup item that applies to every user on this PC".into(),
        ));
    }

    let root = RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    });
    let (key, _) = root
        .create_subkey_with_flags(format!("{APPROVED_PATH}\\{leaf}"), KEY_SET_VALUE | KEY_READ)
        .map_err(|e| AllInsightError::Platform(format!("Could not open the startup settings: {e}")))?;

    // The value is 12 bytes: a state byte, three reserved bytes, then a
    // FILETIME recording when it changed. Windows only reads the first byte.
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    if !enabled {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Unix epoch to FILETIME: 100ns ticks since 1601.
        let filetime = (now + 11_644_473_600) * 10_000_000;
        bytes[4..12].copy_from_slice(&filetime.to_le_bytes());
    }

    let value = winreg::RegValue {
        vtype: winreg::enums::RegType::REG_BINARY,
        bytes,
    };
    key.set_raw_value(name, &value)
        .map_err(|e| AllInsightError::Platform(format!("Could not update the startup setting: {e}")))
}

fn read_run_key(location: StartupLocation, out: &mut Vec<StartupItem>) {
    let (hive, flags) = match location {
        StartupLocation::UserRun => (HKEY_CURRENT_USER, 0),
        StartupLocation::MachineRun => (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        StartupLocation::MachineRun32 => (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
        _ => return,
    };
    let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(RUN_PATH, KEY_READ | flags) else {
        return;
    };
    for (name, value) in key.enum_values().flatten() {
        let command = value.to_string();
        if command.trim().is_empty() {
            continue;
        }
        out.push(build_item(location, name, command));
    }
}

fn read_startup_folder(location: StartupLocation, folder: Option<PathBuf>, out: &mut Vec<StartupItem>) {
    let Some(folder) = folder else { return };
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        out.push(build_item(
            location,
            name,
            path.to_string_lossy().into_owned(),
        ));
    }
}

fn build_item(location: StartupLocation, name: String, command: String) -> StartupItem {
    let executable = executable_from_command(&command);
    let publisher = executable
        .as_ref()
        .and_then(|e| crate::services::process::publisher_of(e));
    let elevated = crate::services::security::is_elevated();

    StartupItem {
        id: format!("{}:{}", location_tag(location), name),
        enabled: read_approval(location, &name),
        impact: estimate_impact(executable.as_ref()),
        impact_is_estimated: true,
        can_toggle: !location.is_machine_scope() || elevated,
        location_label: location.label().to_string(),
        location,
        publisher,
        executable,
        command,
        name,
    }
}

fn location_tag(location: StartupLocation) -> &'static str {
    match location {
        StartupLocation::UserRun => "user_run",
        StartupLocation::MachineRun => "machine_run",
        StartupLocation::MachineRun32 => "machine_run32",
        StartupLocation::UserStartupFolder => "user_folder",
        StartupLocation::CommonStartupFolder => "common_folder",
    }
}

fn location_from_tag(tag: &str) -> Option<StartupLocation> {
    match tag {
        "user_run" => Some(StartupLocation::UserRun),
        "machine_run" => Some(StartupLocation::MachineRun),
        "machine_run32" => Some(StartupLocation::MachineRun32),
        "user_folder" => Some(StartupLocation::UserStartupFolder),
        "common_folder" => Some(StartupLocation::CommonStartupFolder),
        _ => None,
    }
}

fn user_startup_folder() -> Option<PathBuf> {
    paths::expand_env(r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup")
}

fn common_startup_folder() -> Option<PathBuf> {
    paths::expand_env(r"%ProgramData%\Microsoft\Windows\Start Menu\Programs\Startup")
}

pub fn list() -> StartupList {
    let mut items = Vec::new();
    read_run_key(StartupLocation::UserRun, &mut items);
    read_run_key(StartupLocation::MachineRun, &mut items);
    read_run_key(StartupLocation::MachineRun32, &mut items);
    read_startup_folder(
        StartupLocation::UserStartupFolder,
        user_startup_folder(),
        &mut items,
    );
    read_startup_folder(
        StartupLocation::CommonStartupFolder,
        common_startup_folder(),
        &mut items,
    );

    items.sort_by(|a, b| {
        b.enabled
            .cmp(&a.enabled)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    StartupList {
        enabled_count: items.iter().filter(|i| i.enabled).count(),
        elevated: crate::services::security::is_elevated(),
        items,
    }
}

/// Enable or disable one entry, addressed by the id `list` produced.
pub fn set_enabled(id: &str, enabled: bool) -> Result<()> {
    let (tag, name) = id
        .split_once(':')
        .ok_or_else(|| AllInsightError::InvalidInput("That startup item reference is not valid.".into()))?;
    let location = location_from_tag(tag)
        .ok_or_else(|| AllInsightError::InvalidInput("Unknown startup location.".into()))?;
    if name.is_empty() {
        return Err(AllInsightError::InvalidInput(
            "That startup item reference is not valid.".into(),
        ));
    }
    // The reference came from `list`, so confirm it still names a real entry
    // rather than writing whatever the caller asked for.
    if !list().items.iter().any(|i| i.id == id) {
        return Err(AllInsightError::NotFound(std::path::PathBuf::from(name)));
    }
    write_approval(location, name, enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executables_are_extracted_from_both_quoting_styles() {
        let system_root = paths::expand_env("%SystemRoot%").unwrap();
        let explorer = system_root.join("explorer.exe");
        if !explorer.exists() {
            return;
        }
        let quoted = format!("\"{}\" -flag", explorer.display());
        assert_eq!(executable_from_command(&quoted), Some(explorer.clone()));
        let bare = format!("{} -flag", explorer.display());
        assert_eq!(executable_from_command(&bare), Some(explorer));
    }

    #[test]
    fn a_command_pointing_nowhere_yields_no_executable() {
        assert!(executable_from_command("\"Z:\\gone\\missing.exe\" /s").is_none());
        assert_eq!(estimate_impact(None), StartupImpact::Unknown);
    }

    #[test]
    fn ids_round_trip_through_their_tag() {
        for location in [
            StartupLocation::UserRun,
            StartupLocation::MachineRun,
            StartupLocation::MachineRun32,
            StartupLocation::UserStartupFolder,
            StartupLocation::CommonStartupFolder,
        ] {
            let tag = location_tag(location);
            assert_eq!(location_from_tag(tag), Some(location));
        }
    }

    #[test]
    fn a_malformed_id_is_refused() {
        assert!(set_enabled("no-separator", false).is_err());
        assert!(set_enabled("unknown_tag:Thing", false).is_err());
        assert!(set_enabled("user_run:", false).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn machine_entries_are_not_toggleable_without_elevation() {
        let list = list();
        if !list.elevated {
            for item in list.items.iter().filter(|i| i.location.is_machine_scope()) {
                assert!(!item.can_toggle);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_startup_list_reads_without_error() {
        let list = list();
        assert!(list.enabled_count <= list.items.len());
        for item in &list.items {
            assert!(!item.id.is_empty());
            assert!(item.impact_is_estimated);
        }
    }
}

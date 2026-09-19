//! The installed application list.
//!
//! Read from the three uninstall registry hives Windows itself uses, so what
//! AllInsight shows matches Settings and Control Panel. Uninstalling always hands
//! control to the vendor's own uninstaller: AllInsight never deletes an
//! application's folder as a substitute, because that leaves the registry,
//! services and scheduled tasks behind.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

use crate::error::{AllInsightError, Result};
use crate::services::security::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    /// The registry key name. Stable, and the handle the frontend passes back.
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// From `EstimatedSize`, which vendors report in kilobytes. Frequently
    /// absent or wrong, so it is optional and labelled as an estimate.
    pub estimated_size_bytes: Option<u64>,
    /// Measured by walking `InstallLocation`, when AllInsight was asked to.
    pub measured_size_bytes: Option<u64>,
    pub install_date: Option<String>,
    pub install_location: Option<PathBuf>,
    pub scope: AppScope,
    pub has_uninstaller: bool,
    pub is_windows_component: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppScope {
    /// Installed for every user, from `HKEY_LOCAL_MACHINE`.
    AllUsers,
    /// Installed for the signed-in user only.
    CurrentUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppList {
    pub apps: Vec<InstalledApp>,
    pub total: usize,
}

/// Where the uninstall entries live. The 32-bit view is read explicitly so
/// applications installed by a 32-bit installer are not missed.
const UNINSTALL_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

struct RawEntry {
    key_name: String,
    values: HashMap<String, String>,
    numbers: HashMap<String, u32>,
    scope: AppScope,
}

fn read_hive(root: RegKey, scope: AppScope, flags: u32, out: &mut Vec<RawEntry>) {
    let Ok(uninstall) = root.open_subkey_with_flags(UNINSTALL_PATH, KEY_READ | flags) else {
        return;
    };
    for key_name in uninstall.enum_keys().flatten() {
        let Ok(entry) = uninstall.open_subkey_with_flags(&key_name, KEY_READ | flags) else {
            continue;
        };
        let mut values = HashMap::new();
        let mut numbers = HashMap::new();
        for name in [
            "DisplayName",
            "DisplayVersion",
            "Publisher",
            "InstallDate",
            "InstallLocation",
            "UninstallString",
            "QuietUninstallString",
            "ParentKeyName",
            "ReleaseType",
        ] {
            if let Ok(v) = entry.get_value::<String, _>(name) {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    values.insert(name.to_string(), v);
                }
            }
        }
        for name in ["EstimatedSize", "SystemComponent", "WindowsInstaller"] {
            if let Ok(v) = entry.get_value::<u32, _>(name) {
                numbers.insert(name.to_string(), v);
            }
        }
        out.push(RawEntry {
            key_name,
            values,
            numbers,
            scope,
        });
    }
}

fn collect_raw() -> Vec<RawEntry> {
    let mut raw = Vec::new();
    read_hive(
        RegKey::predef(HKEY_LOCAL_MACHINE),
        AppScope::AllUsers,
        KEY_WOW64_64KEY,
        &mut raw,
    );
    read_hive(
        RegKey::predef(HKEY_LOCAL_MACHINE),
        AppScope::AllUsers,
        KEY_WOW64_32KEY,
        &mut raw,
    );
    read_hive(
        RegKey::predef(HKEY_CURRENT_USER),
        AppScope::CurrentUser,
        0,
        &mut raw,
    );
    raw
}

/// Normalise the `InstallDate` vendors write as `YYYYMMDD`.
fn format_install_date(raw: &str) -> Option<String> {
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8]))
    } else {
        Some(raw.to_string())
    }
}

pub fn list(measure_sizes: bool) -> AppList {
    let mut apps: Vec<InstalledApp> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for entry in collect_raw() {
        let Some(name) = entry.values.get("DisplayName").cloned() else {
            continue;
        };
        // Update packages and patches register themselves as children of a
        // parent product; Windows hides them and so does AllInsight.
        if entry.values.contains_key("ParentKeyName") {
            continue;
        }
        let is_component = entry.numbers.get("SystemComponent").copied().unwrap_or(0) == 1;
        let is_update = entry
            .values
            .get("ReleaseType")
            .map(|r| r.eq_ignore_ascii_case("Security Update") || r.eq_ignore_ascii_case("Update"))
            .unwrap_or(false);
        if is_update {
            continue;
        }

        let install_location = entry
            .values
            .get("InstallLocation")
            .map(|p| paths::normalize_lexical(std::path::Path::new(p)))
            .filter(|p| p.is_absolute() && p.exists());

        let app = InstalledApp {
            id: entry.key_name.clone(),
            publisher: entry.values.get("Publisher").cloned(),
            version: entry.values.get("DisplayVersion").cloned(),
            estimated_size_bytes: entry
                .numbers
                .get("EstimatedSize")
                .copied()
                .filter(|s| *s > 0)
                .map(|kb| u64::from(kb) * 1024),
            measured_size_bytes: None,
            install_date: entry
                .values
                .get("InstallDate")
                .and_then(|d| format_install_date(d)),
            install_location,
            scope: entry.scope,
            has_uninstaller: entry.values.contains_key("UninstallString")
                || entry.values.contains_key("QuietUninstallString"),
            is_windows_component: is_component,
            name,
        };

        // The 32-bit and 64-bit views can both surface the same product.
        let dedup_key = format!(
            "{}|{}",
            app.name.to_lowercase(),
            app.version.clone().unwrap_or_default()
        );
        if let Some(existing) = seen.get(&dedup_key) {
            if apps[*existing].install_location.is_none() && app.install_location.is_some() {
                apps[*existing] = app;
            }
            continue;
        }
        seen.insert(dedup_key, apps.len());
        apps.push(app);
    }

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
            .cmp(&a.measured_size_bytes.or(a.estimated_size_bytes).unwrap_or(0))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let total = apps.len();
    AppList { apps, total }
}

/// Split an uninstall command into a program and its arguments.
///
/// The registry value is a command line, and passing it to a shell would let a
/// crafted entry run anything it liked. Instead it is split here and the two
/// halves are handed to `ShellExecute`, which never involves a shell.
fn split_command(command: &str) -> Option<(String, String)> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let end = rest.find('"')?;
        let program = rest[..end].to_string();
        let args = rest[end + 1..].trim().to_string();
        return Some((program, args));
    }
    // Unquoted: split after the executable extension. A folder can be named
    // "tools.exe files", so the first match is not necessarily the program;
    // take the first split that names a file that actually exists, and fall
    // back to the first match only when none does.
    let lower = trimmed.to_lowercase();
    let mut first: Option<(String, String)> = None;
    for ext in [".exe", ".msi", ".bat", ".cmd", ".com"] {
        let mut from = 0;
        while let Some(offset) = lower[from..].find(ext) {
            let split = from + offset + ext.len();
            let candidate = (
                trimmed[..split].to_string(),
                trimmed[split..].trim().to_string(),
            );
            if Path::new(&candidate.0).is_file() {
                return Some(candidate);
            }
            first.get_or_insert(candidate);
            from = split;
        }
    }
    if let Some(candidate) = first {
        return Some(candidate);
    }
    Some((trimmed.to_string(), String::new()))
}

fn uninstall_command(id: &str) -> Option<(String, AppScope)> {
    for (root, scope, flags) in [
        (HKEY_LOCAL_MACHINE, AppScope::AllUsers, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, AppScope::AllUsers, KEY_WOW64_32KEY),
        (HKEY_CURRENT_USER, AppScope::CurrentUser, 0),
    ] {
        let Ok(uninstall) =
            RegKey::predef(root).open_subkey_with_flags(UNINSTALL_PATH, KEY_READ | flags)
        else {
            continue;
        };
        let Ok(entry) = uninstall.open_subkey_with_flags(id, KEY_READ | flags) else {
            continue;
        };
        for name in ["QuietUninstallString", "UninstallString"] {
            if let Ok(v) = entry.get_value::<String, _>(name) {
                if !v.trim().is_empty() {
                    return Some((v, scope));
                }
            }
        }
    }
    None
}

/// Start the vendor's uninstaller.
///
/// AllInsight launches it and steps back: the uninstaller owns the flow from that
/// point, including any elevation prompt Windows decides to show.
#[cfg(windows)]
pub fn uninstall(id: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // The id came from `list`, so confirm it is still a real key rather than
    // trusting the string.
    if id.is_empty() || id.contains('\\') || id.contains('/') {
        return Err(AllInsightError::InvalidInput(
            "That application reference is not valid.".into(),
        ));
    }

    let (command, _scope) = uninstall_command(id).ok_or_else(|| {
        AllInsightError::InvalidInput(
            "This application did not register an uninstaller with Windows.".into(),
        )
    })?;
    let (program, args) = split_command(&command).ok_or_else(|| {
        AllInsightError::InvalidInput("The registered uninstall command is empty.".into())
    })?;

    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let program_w = wide(&program);
    let args_w = wide(&args);

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            program_w.as_ptr(),
            if args.is_empty() {
                std::ptr::null()
            } else {
                args_w.as_ptr()
            },
            std::ptr::null(),
            SW_SHOWNORMAL as i32,
        )
    };

    // ShellExecute returns a value greater than 32 on success.
    if (result as isize) > 32 {
        Ok(())
    } else {
        Err(AllInsightError::Platform(
            "Windows could not start the uninstaller for this application.".into(),
        ))
    }
}

#[cfg(not(windows))]
pub fn uninstall(_id: &str) -> Result<()> {
    Err(AllInsightError::Platform(
        "Uninstalling is only available on Windows.".into(),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_quoted_uninstall_command_keeps_its_spaces() {
        let (program, args) =
            split_command(r#""C:\Program Files\App\unins000.exe" /SILENT"#).unwrap();
        assert_eq!(program, r"C:\Program Files\App\unins000.exe");
        assert_eq!(args, "/SILENT");
    }

    #[test]
    fn an_unquoted_uninstall_command_splits_at_the_extension() {
        let (program, args) = split_command(r"C:\Apps\unins000.exe /S").unwrap();
        assert_eq!(program, r"C:\Apps\unins000.exe");
        assert_eq!(args, "/S");
    }

    #[test]
    fn a_folder_named_like_an_executable_does_not_capture_the_split() {
        // The first ".exe" here is part of a directory name. Splitting there
        // would hand ShellExecute "C:\tools.exe", which is a different file
        // and might well exist.
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let real = format!(r"{system_root}\System32\cmd.exe /c exit");
        let (program, _) = split_command(&real).unwrap();
        assert!(
            std::path::Path::new(&program).is_file(),
            "split produced {program:?}, which is not a file"
        );

        let tricky = r"C:\tools.exe files\app\unins.exe /S";
        let (program, args) = split_command(tricky).unwrap();
        // Neither candidate exists on this machine, so the first match stands
        // and the call will simply fail. What matters is that an existing file
        // always wins, which the case above shows.
        assert_eq!(program, r"C:\tools.exe");
        assert!(args.contains("unins.exe"));
    }

    use super::*;

    #[test]
    fn quoted_commands_split_cleanly() {
        let (program, args) =
            split_command("\"C:\\Program Files\\App\\uninstall.exe\" --silent /norestart").unwrap();
        assert_eq!(program, "C:\\Program Files\\App\\uninstall.exe");
        assert_eq!(args, "--silent /norestart");
    }

    #[test]
    fn unquoted_commands_split_after_the_extension() {
        let (program, args) =
            split_command("C:\\Windows\\System32\\msiexec.exe /X{1234-5678}").unwrap();
        assert_eq!(program, "C:\\Windows\\System32\\msiexec.exe");
        assert_eq!(args, "/X{1234-5678}");
    }

    /// A shell metacharacter in the registry must end up as an argument, never
    /// as a second command, because the launcher never goes through a shell.
    #[test]
    fn shell_metacharacters_stay_inside_the_argument_string() {
        let (program, args) =
            split_command("\"C:\\App\\u.exe\" /q & calc.exe && del /f /q C:\\*").unwrap();
        assert_eq!(program, "C:\\App\\u.exe");
        assert!(args.contains("calc.exe"));
        assert!(!program.contains('&'));
    }

    #[test]
    fn an_empty_command_is_refused() {
        assert!(split_command("   ").is_none());
    }

    #[test]
    fn an_id_containing_a_path_separator_is_refused() {
        let err = uninstall("..\\..\\evil").unwrap_err();
        assert!(err.to_string().contains("not valid"));
    }

    #[test]
    fn install_dates_are_normalised() {
        assert_eq!(format_install_date("20240115").unwrap(), "2024-01-15");
        assert_eq!(format_install_date("15 Jan 2024").unwrap(), "15 Jan 2024");
    }

    #[cfg(windows)]
    #[test]
    fn the_installed_list_is_readable_and_sorted_by_size() {
        let list = list(false);
        assert_eq!(list.total, list.apps.len());
        for app in &list.apps {
            assert!(!app.name.is_empty());
            assert!(!app.id.is_empty());
        }
        for pair in list.apps.windows(2) {
            let a = pair[0].estimated_size_bytes.unwrap_or(0);
            let b = pair[1].estimated_size_bytes.unwrap_or(0);
            if a != b {
                assert!(a >= b);
            }
        }
    }
}

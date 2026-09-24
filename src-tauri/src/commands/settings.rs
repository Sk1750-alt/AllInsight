//! Settings, protected paths, and the first-run flow.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::error::{AllInsightError, Result};
use crate::services::db::Settings;
use crate::services::security::paths;
use crate::state::AppState;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    Ok({
    state.settings()
})
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let previous = state.settings();
    let saved = state.update_settings(settings)?;

    // Launching at sign-in is a change to the user's machine outside AllInsight,
    // so it is applied here rather than silently at startup.
    if saved.launch_at_startup != previous.launch_at_startup {
        apply_launch_at_startup(saved.launch_at_startup)?;
    }

    // Turning the assistant off must actually release the memory.
    if !saved.ai_enabled && previous.ai_enabled {
        state.engine.stop();
    }

    Ok(saved)
}

/// The screen AllInsight was last on.
///
/// Stored separately from the settings document so that navigating does not
/// rewrite the whole document and does not make every click a settings change
/// the interface has to react to.
const LAST_ROUTE_KEY: &str = "ui.last_route";

#[tauri::command]
pub fn get_last_route(state: State<'_, AppState>) -> Option<String> {
    state.db.get_setting(LAST_ROUTE_KEY).ok().flatten()
}

#[tauri::command]
pub async fn set_last_route(state: State<'_, AppState>, route: String) -> Result<()> {
    // A route is an identifier from a fixed list in the interface, so anything
    // long or oddly shaped is a bug or a forgery; either way, drop it.
    if route.is_empty()
        || route.len() > 32
        || !route.chars().all(|c| c.is_ascii_lowercase() || c == '-')
    {
        return Err(AllInsightError::InvalidInput("Unknown screen.".into()));
    }
    state.db.set_setting(LAST_ROUTE_KEY, &route)
}

#[tauri::command]
pub async fn complete_first_run(state: State<'_, AppState>) -> Result<Settings> {
    let mut settings = state.settings();
    settings.first_run_complete = true;
    state.update_settings(settings)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtectedPathView {
    pub path: String,
    pub reason: String,
    pub explanation: String,
    pub user_added: bool,
}

/// The whole protected list, so the Security screen can show exactly what
/// AllInsight refuses to touch rather than asking to be trusted.
#[tauri::command]
pub async fn get_protected_paths(state: State<'_, AppState>) -> Result<Vec<ProtectedPathView>> {
    Ok({
    let protected = state.protected();
    let user_roots: Vec<PathBuf> = protected.user_roots().to_vec();
    let mut views: Vec<ProtectedPathView> = protected
        .all_roots()
        .into_iter()
        .map(|(path, reason)| ProtectedPathView {
            user_added: user_roots.iter().any(|u| paths::same_path(u, &path)),
            path: path.to_string_lossy().into_owned(),
            reason: format!("{reason:?}"),
            explanation: reason.explain().to_string(),
        })
        .collect();
    views.sort_by(|a, b| {
        b.user_added
            .cmp(&a.user_added)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
    views
})
}

/// The narrow exceptions inside protected roots that cleanup categories may
/// use. Published for the same reason as the list above.
#[tauri::command]
pub async fn get_cleanup_exceptions(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok({
    state
        .protected
        .read()
        .carve_outs()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
})
}

#[tauri::command]
pub async fn add_protected_path(state: State<'_, AppState>, path: String) -> Result<Settings> {
    let target = paths::normalize_lexical(std::path::Path::new(&path));
    if !target.is_absolute() {
        return Err(AllInsightError::InvalidInput(
            "Choose a full folder path.".into(),
        ));
    }
    if !target.exists() {
        return Err(AllInsightError::NotFound(target));
    }
    let mut settings = state.settings();
    if settings.protected_paths.iter().any(|p| paths::same_path(p, &target)) {
        return Ok(settings);
    }
    settings.protected_paths.push(target);
    state.update_settings(settings)
}

#[tauri::command]
pub async fn remove_protected_path(state: State<'_, AppState>, path: String) -> Result<Settings> {
    let target = paths::normalize_lexical(std::path::Path::new(&path));
    let mut settings = state.settings();
    settings
        .protected_paths
        .retain(|p| !paths::same_path(p, &target));
    state.update_settings(settings)
}

/// Write the run-at-sign-in registry value for the current user only.
///
/// `HKEY_CURRENT_USER` needs no elevation and affects nobody else on the
/// machine, which is the right scope for a personal utility.
#[cfg(windows)]
fn apply_launch_at_startup(enabled: bool) -> Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    const RUN_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "AllInsight";

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_PATH, KEY_SET_VALUE)
        .map_err(|e| AllInsightError::Platform(format!("Could not open startup settings: {e}")))?;

    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| AllInsightError::Platform(format!("Could not locate AllInsight: {e}")))?;
        key.set_value(VALUE_NAME, &format!("\"{}\" --background", exe.display()))
            .map_err(|e| AllInsightError::Platform(format!("Could not enable startup: {e}")))?;
    } else {
        // Absent is the desired end state, so a missing value is success.
        let _ = key.delete_value(VALUE_NAME);
    }
    Ok(())
}

/// Write or remove `~/.config/autostart/allinsight.desktop`, the per-user
/// autostart entry every freedesktop desktop honours. Only AllInsight's own
/// entry is ever created or deleted here.
#[cfg(all(unix, not(target_os = "macos")))]
fn apply_launch_at_startup(enabled: bool) -> Result<()> {
    let dir = dirs::config_dir()
        .ok_or_else(|| AllInsightError::Platform("Your autostart folder could not be located.".into()))?
        .join("autostart");
    let entry = dir.join("allinsight.desktop");

    if !enabled {
        // Absent is the desired end state, so a missing file is success.
        let _ = std::fs::remove_file(&entry);
        return Ok(());
    }

    // An AppImage runs from a temporary mount that changes every launch; the
    // image itself is the stable path.
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| AllInsightError::Platform("Could not locate AllInsight.".into()))?;
    // Exec= quoting needs these escaped twice over (once for the quoted
    // argument, once for the desktop-file string). A path that contains them
    // is refused rather than risk an entry that runs something else.
    let quoted = exe.to_string_lossy().into_owned();
    if quoted.contains(['"', '`', '$', '\\', '\n']) {
        return Err(AllInsightError::Platform(
            "AllInsight is installed at a path autostart cannot express. Move it and try again.".into(),
        ));
    }
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=AllInsight\nComment=Watch storage and drive health in the background\nExec=\"{quoted}\" --background\nIcon=allinsight\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::write(&entry, text))
        .map_err(|e| AllInsightError::Platform(format!("Could not enable startup: {e}")))
}

#[cfg(target_os = "macos")]
fn apply_launch_at_startup(_enabled: bool) -> Result<()> {
    Ok(())
}

/// Export a diagnostics bundle the user can inspect before sharing.
///
/// It contains settings with paths removed, the environment, and the tail of
/// the log. It is written to a file the user chose; nothing is transmitted,
/// because AllInsight has no code that transmits.
#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>, destination: String) -> Result<String> {
    let target = paths::normalize_lexical(std::path::Path::new(&destination));
    if !target.is_absolute() {
        return Err(AllInsightError::InvalidInput(
            "Choose where to save the diagnostics file.".into(),
        ));
    }
    if paths::extension_lower(&target) != "json" {
        return Err(AllInsightError::InvalidInput(
            "The diagnostics file must be saved with a .json name.".into(),
        ));
    }
    // The destination normally comes from a save dialog, but this command must
    // hold on its own: without these checks it is a way to overwrite any file
    // the user can write, which is exactly what the rest of AllInsight refuses to
    // do.
    let verdict = state.protected().classify(&target);
    if verdict.protected {
        return Err(AllInsightError::Protected {
            path: target,
            reason: verdict.describe(),
        });
    }
    if let Some(link) = paths::first_reparse_ancestor(&target, None) {
        return Err(AllInsightError::InvalidInput(format!(
            "That location is reached through a link ({}), so it will not be written to.",
            link.display()
        )));
    }
    if let Ok(existing) = std::fs::symlink_metadata(paths::long_path(&target)) {
        // Replacing an earlier diagnostics file is the expected case.
        // Replacing anything else is not.
        if !existing.is_file() || !is_previous_diagnostics(&target) {
            return Err(AllInsightError::InvalidInput(
                "A different file already exists there. Choose a new name."
                    .into(),
            ));
        }
    }

    let settings = state.settings();
    let report = serde_json::json!({
        "application": {
            "name": "AllInsight",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "environment": {
            "os": sysinfo::System::long_os_version(),
            "kernel": sysinfo::System::kernel_version(),
            "elevated": crate::services::security::is_elevated(),
        },
        "settings": {
            "theme": settings.theme,
            "background_monitoring": settings.background_monitoring,
            "auto_clean_enabled": settings.auto_clean_enabled,
            "auto_clean_free_space_percent": settings.auto_clean_free_space_percent,
            "notifications_enabled": settings.notifications_enabled,
            "ai_enabled": settings.ai_enabled,
            "ai_model_configured": settings.ai_model_path.is_some(),
            "protected_path_count": settings.protected_paths.len(),
            "telemetry_enabled": settings.telemetry_enabled,
            "cloud_services_enabled": settings.cloud_services_enabled,
        },
        "storage": crate::services::storage::overview(),
        "drives": crate::services::health::report().ok(),
        "cleanup_totals": state.db.cleanup_totals().ok(),
        "activity": state.db.activity(50).unwrap_or_default(),
        "note": "This file contains no file names and no personal data. Review it before sharing."
    });

    std::fs::write(&target, serde_json::to_string_pretty(&report)?)
        .map_err(|e| AllInsightError::Other(format!("Could not write the diagnostics file: {e}")))?;

    Ok(target.to_string_lossy().into_owned())
}

/// Whether the file already at the destination is one AllInsight wrote, which is
/// the only file this command is willing to replace.
fn is_previous_diagnostics(path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string(paths::long_path(path)) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get("application")
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        == Some("AllInsight")
}

/// Restart AllInsight with administrator rights.
///
/// Used only where elevation genuinely unlocks something: full SMART counters
/// and the machine-wide startup and cleanup categories. The user asks for it
/// explicitly; AllInsight never requests it at launch.
#[cfg(windows)]
#[tauri::command]
pub async fn restart_elevated(app: tauri::AppHandle) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if crate::services::security::is_elevated() {
        return Err(AllInsightError::InvalidInput(
            "AllInsight is already running with administrator permission.".into(),
        ));
    }

    let exe = std::env::current_exe()
        .map_err(|e| AllInsightError::Platform(format!("Could not locate AllInsight: {e}")))?;
    let wide = |s: &std::ffi::OsStr| -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    };
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(exe.as_os_str());

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL as i32,
        )
    };

    if (result as isize) > 32 {
        // Give the elevated copy a moment to start before this one lets go of
        // the database, so the two do not fight over it on the way past.
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1200));
            handle.exit(0);
        });
        Ok(())
    } else {
        Err(AllInsightError::Platform(
            "Windows declined the administrator prompt.".into(),
        ))
    }
}

/// On Linux and macOS AllInsight deliberately never runs as root: everything
/// it cleans belongs to the signed-in user, and a root process drawing a
/// window is exactly the kind of privilege this design avoids.
#[cfg(not(windows))]
#[tauri::command]
pub fn restart_elevated(_app: tauri::AppHandle) -> Result<()> {
    Err(AllInsightError::Platform(
        "AllInsight runs as your own user on this system and does not restart as root.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The destination guard is what stops `export_diagnostics` from being a
    /// way to overwrite any file the user can write.
    #[test]
    fn only_an_earlier_diagnostics_file_is_recognised_as_replaceable() {
        let dir = std::env::temp_dir().join(format!("allinsight-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let ours = dir.join("ours.json");
        std::fs::write(&ours, br#"{"application":{"name":"AllInsight","version":"1.0.0"}}"#).unwrap();
        assert!(is_previous_diagnostics(&ours));

        let theirs = dir.join("theirs.json");
        std::fs::write(&theirs, br#"{"application":{"name":"Something Else"}}"#).unwrap();
        assert!(!is_previous_diagnostics(&theirs));

        let notes = dir.join("notes.json");
        std::fs::write(&notes, b"the user's own file").unwrap();
        assert!(!is_previous_diagnostics(&notes));

        assert!(!is_previous_diagnostics(&dir.join("absent.json")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

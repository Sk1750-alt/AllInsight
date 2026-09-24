//! The startup manager on Linux: freedesktop autostart entries.
//!
//! GNOME, KDE Plasma, Xfce, Cinnamon, MATE, LXQt and Budgie all start the
//! `.desktop` files found in `$XDG_CONFIG_HOME/autostart` and in each
//! `$XDG_CONFIG_DIRS/autostart`, with a file in the user's folder overriding
//! a system one of the same name.
//!
//! Disabling uses the mechanism the specification provides: a user copy of the
//! entry with `Hidden=true`. That needs no administrator even for a system
//! entry, affects only this user, and is what the desktops' own "Startup
//! Applications" tools write. Files are never deleted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{estimate_impact, StartupItem, StartupList, StartupLocation};
use crate::error::{AllInsightError, Result};
use crate::services::apps::linux_parse;

fn user_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|c| c.join("autostart"))
}

fn system_dirs() -> Vec<PathBuf> {
    std::env::var("XDG_CONFIG_DIRS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "/etc/xdg".into())
        .split(':')
        .filter(|d| d.starts_with('/'))
        .map(|d| Path::new(d).join("autostart"))
        .collect()
}

fn desktop_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    listing
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "desktop").unwrap_or(false))
        .filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), p)))
        .collect()
}

/// A program name resolved through `PATH`, or an absolute path that exists.
fn resolve_program(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return (path.is_absolute() && path.is_file()).then_some(path);
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

/// The effective entry for each file name: the user's copy when there is one,
/// otherwise the first system directory that has it.
fn effective_entries() -> Vec<(String, PathBuf, StartupLocation)> {
    let mut by_name: HashMap<String, (PathBuf, StartupLocation)> = HashMap::new();
    for dir in system_dirs().iter().rev() {
        for (name, path) in desktop_files(dir) {
            by_name.insert(name, (path, StartupLocation::SystemAutostart));
        }
    }
    if let Some(dir) = user_dir() {
        for (name, path) in desktop_files(&dir) {
            by_name.insert(name, (path, StartupLocation::UserAutostart));
        }
    }
    by_name
        .into_iter()
        .map(|(name, (path, location))| (name, path, location))
        .collect()
}

fn location_tag(location: StartupLocation) -> &'static str {
    match location {
        StartupLocation::SystemAutostart => "xdg_system",
        _ => "xdg_user",
    }
}

pub fn list() -> StartupList {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let mut items = Vec::new();

    for (file_name, path, location) in effective_entries() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let group = linux_parse::desktop_group(&text, "Desktop Entry");
        let Some(entry) = linux_parse::parse_desktop_entry(&text) else {
            continue;
        };
        // An entry meant for a different desktop never runs here, so listing
        // it would only offer a switch that does nothing.
        if !linux_parse::shown_in(&group, &desktop) {
            continue;
        }
        let command = entry.exec.clone().unwrap_or_default();
        let executable = linux_parse::exec_program(&command).and_then(|p| resolve_program(&p));

        items.push(StartupItem {
            id: format!("{}:{}", location_tag(location), file_name),
            name: entry.name,
            enabled: !entry.hidden && !entry.gnome_autostart_disabled,
            impact: estimate_impact(executable.as_ref()),
            impact_is_estimated: true,
            // A per-user override is always possible, even for a system entry.
            can_toggle: true,
            location_label: location.label().to_string(),
            location,
            publisher: None,
            executable,
            command,
        });
    }

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
    let invalid = || AllInsightError::InvalidInput("That startup item reference is not valid.".into());
    let (_, file_name) = id.split_once(':').ok_or_else(invalid)?;
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.starts_with('.')
        || !file_name.ends_with(".desktop")
    {
        return Err(invalid());
    }
    // The reference came from `list`, so confirm it still names a real entry
    // rather than writing whatever the caller asked for.
    if !list().items.iter().any(|i| i.id == id) {
        return Err(AllInsightError::NotFound(PathBuf::from(file_name)));
    }
    let (_, source, _) = effective_entries()
        .into_iter()
        .find(|(name, _, _)| name == file_name)
        .ok_or_else(|| AllInsightError::NotFound(PathBuf::from(file_name)))?;

    let user_dir = user_dir().ok_or_else(|| {
        AllInsightError::Platform("Your autostart folder could not be located.".into())
    })?;
    let target = user_dir.join(file_name);

    let original = std::fs::read_to_string(&source)
        .map_err(|e| AllInsightError::Platform(format!("Could not read the startup entry: {e}")))?;
    let mut text = linux_parse::set_desktop_key(&original, "Hidden", if enabled { "false" } else { "true" });
    if enabled && original.contains("X-GNOME-Autostart-enabled") {
        text = linux_parse::set_desktop_key(&text, "X-GNOME-Autostart-enabled", "true");
    }

    // A link planted in the autostart folder must not redirect this write.
    if crate::services::security::paths::is_reparse_point(&target) {
        return Err(AllInsightError::InvalidInput(
            "That startup entry is a link and will not be changed.".into(),
        ));
    }
    std::fs::create_dir_all(&user_dir)
        .and_then(|_| std::fs::write(&target, text))
        .map_err(|e| AllInsightError::Platform(format!("Could not update the startup setting: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_malformed_id_is_refused() {
        assert!(set_enabled("no-separator", false).is_err());
        assert!(set_enabled("xdg_user:../../.bashrc", false).is_err());
        assert!(set_enabled("xdg_user:", false).is_err());
        assert!(set_enabled("xdg_user:not-a-desktop-file.sh", false).is_err());
        assert!(set_enabled("xdg_user:allinsight-no-such-entry.desktop", false).is_err());
    }

    #[test]
    fn programs_are_resolved_through_path() {
        assert!(resolve_program("sh").is_some());
        assert!(resolve_program("/definitely/not/here").is_none());
    }

    #[test]
    fn the_startup_list_reads_without_error() {
        let list = list();
        assert!(list.enabled_count <= list.items.len());
        for item in &list.items {
            assert!(item.id.ends_with(".desktop"));
            assert!(item.can_toggle);
        }
    }
}

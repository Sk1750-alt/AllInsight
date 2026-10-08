//! The installed application list on Linux.
//!
//! A Linux system has thousands of packages, and nearly all of them are
//! libraries nobody thinks of as an application. What a person means by
//! "installed applications" is what their launcher shows, so the list starts
//! from the visible desktop entries and traces each one back to its owner:
//!
//! - a Snap or Flatpak, recognised by where the entry is exported from
//! - a dpkg, pacman or rpm package, looked up in that package database
//! - nothing at all, for software unpacked by hand, which is listed but
//!   never removed
//!
//! Only the owner of an application is ever asked to remove it. Flatpak and
//! snapd do that for an ordinary user (snapd asks for authorisation through
//! polkit itself). apt, dnf and pacman need root, and AllInsight never runs a
//! package manager as root on someone's behalf, so for those it shows the
//! exact command to run instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::linux_parse::{self, PackageRecord};
use super::{AppScope, AppSource, InstalledApp};
use crate::error::{AllInsightError, Result};

/// One visible desktop entry and the file it came from.
struct Entry {
    path: PathBuf,
    name: String,
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Every `applications` directory a launcher would read, most specific first.
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs_out = Vec::new();
    if let Some(data_home) = dirs::data_dir() {
        dirs_out.push(data_home.join("applications"));
        dirs_out.push(data_home.join("flatpak/exports/share/applications"));
    }
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in system.split(':').filter(|d| d.starts_with('/')) {
        dirs_out.push(Path::new(dir).join("applications"));
    }
    for extra in [
        "/var/lib/flatpak/exports/share/applications",
        "/var/lib/snapd/desktop/applications",
        "/usr/share/applications",
    ] {
        dirs_out.push(PathBuf::from(extra));
    }
    let mut seen = std::collections::HashSet::new();
    dirs_out.retain(|d| seen.insert(d.clone()));
    dirs_out
}

/// Visible application entries. An entry in a more specific directory hides
/// one with the same file name further down, as the specification says.
fn visible_entries() -> Vec<Entry> {
    let mut by_id: HashMap<String, Option<Entry>> = HashMap::new();
    for dir in application_dirs() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for item in listing.flatten() {
            let path = item.path();
            if path.extension().map(|e| e != "desktop").unwrap_or(true) {
                continue;
            }
            let id = item.file_name().to_string_lossy().into_owned();
            if by_id.contains_key(&id) {
                continue;
            }
            let parsed = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| linux_parse::parse_desktop_entry(&t));
            let visible = parsed
                .filter(|e| e.is_application && !e.no_display && !e.hidden)
                .map(|e| Entry { path, name: e.name });
            by_id.insert(id, visible);
        }
    }
    by_id.into_values().flatten().collect()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Package ownership of desktop files, and the package records themselves.
#[derive(Default)]
struct PackageDb {
    source: Option<AppSource>,
    owner: HashMap<PathBuf, String>,
    records: HashMap<String, PackageRecord>,
}

fn read_dpkg() -> Option<PackageDb> {
    let status = std::fs::read_to_string("/var/lib/dpkg/status").ok()?;
    let records = linux_parse::parse_dpkg_status(&status);
    let mut owner = HashMap::new();
    if let Ok(info) = std::fs::read_dir("/var/lib/dpkg/info") {
        for item in info.flatten() {
            let path = item.path();
            if path.extension().map(|e| e != "list").unwrap_or(true) {
                continue;
            }
            // `libfoo:amd64.list` belongs to package `libfoo`.
            let package = stem(&path)
                .split(':')
                .next()
                .unwrap_or_default()
                .to_string();
            let Ok(listing) = std::fs::read_to_string(&path) else {
                continue;
            };
            for file in linux_parse::desktop_files_in_listing(&listing) {
                owner.insert(PathBuf::from(file), package.clone());
            }
        }
    }
    Some(PackageDb {
        source: Some(AppSource::Dpkg),
        owner,
        records,
    })
}

fn read_pacman() -> Option<PackageDb> {
    let local = std::fs::read_dir("/var/lib/pacman/local").ok()?;
    let mut db = PackageDb {
        source: Some(AppSource::Pacman),
        ..Default::default()
    };
    for item in local.flatten() {
        let dir = item.path();
        let Some(record) = std::fs::read_to_string(dir.join("desc"))
            .ok()
            .and_then(|t| linux_parse::parse_pacman_desc(&t))
        else {
            continue;
        };
        if let Ok(files) = std::fs::read_to_string(dir.join("files")) {
            for file in linux_parse::desktop_files_in_listing(&files) {
                db.owner.insert(PathBuf::from(file), record.name.clone());
            }
        }
        db.records.insert(record.name.clone(), record);
    }
    Some(db)
}

/// rpm has no stable on-disk format worth parsing, so it is asked about each
/// desktop file. One file per query, because current rpm reports an unowned
/// file on stderr and prints nothing on stdout, so a batched answer cannot be
/// lined up with the files that were asked about. The queries run in
/// parallel. Arguments are file paths read from disk, passed as separate
/// arguments; no shell is involved.
fn read_rpm(files: &[PathBuf]) -> Option<PackageDb> {
    use rayon::prelude::*;

    let has_db = ["/var/lib/rpm", "/usr/lib/sysimage/rpm"]
        .iter()
        .any(|p| Path::new(p).is_dir());
    if !has_db || files.is_empty() || Command::new("rpm").arg("--version").output().is_err() {
        return None;
    }
    let answers: Vec<(PathBuf, PackageRecord)> = files
        .par_iter()
        .filter_map(|file| {
            let output = Command::new("rpm")
                .arg("-qf")
                .arg("--qf")
                .arg(linux_parse::RPM_QUERY_FORMAT)
                .arg(file)
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&output.stdout);
            // A file shared by two packages yields two lines; the first owner
            // is as good an answer as any.
            let record = linux_parse::parse_rpm_query(&text)
                .into_iter()
                .flatten()
                .next()?;
            Some((file.clone(), record))
        })
        .collect();

    let mut db = PackageDb {
        source: Some(AppSource::Rpm),
        ..Default::default()
    };
    for (file, record) in answers {
        db.owner.insert(file, record.name.clone());
        db.records.entry(record.name.clone()).or_insert(record);
    }
    Some(db)
}

fn is_under(path: &Path, dir: &str) -> bool {
    path.to_string_lossy().contains(dir)
}

/// The command that removes a system package, for display only.
fn removal_command(source: AppSource, package: &str) -> Option<String> {
    let exists = |p: &str| Path::new(p).exists();
    Some(match source {
        AppSource::Dpkg => format!("sudo apt remove {package}"),
        AppSource::Pacman => format!("sudo pacman -R {package}"),
        AppSource::Rpm if exists("/usr/bin/dnf") || exists("/usr/bin/dnf5") => {
            format!("sudo dnf remove {package}")
        }
        AppSource::Rpm if exists("/usr/bin/zypper") => format!("sudo zypper remove {package}"),
        AppSource::Rpm => format!("sudo rpm -e {package}"),
        _ => return None,
    })
}

fn source_tag(source: AppSource, scope: AppScope) -> &'static str {
    match (source, scope) {
        (AppSource::Flatpak, AppScope::CurrentUser) => "flatpak-user",
        (AppSource::Flatpak, AppScope::AllUsers) => "flatpak-system",
        (AppSource::Snap, _) => "snap",
        (AppSource::Dpkg, _) => "dpkg",
        (AppSource::Rpm, _) => "rpm",
        (AppSource::Pacman, _) => "pacman",
        (AppSource::Manual, _) | (AppSource::Registry, _) => "manual",
    }
}

fn snap_details(name: &str) -> (Option<String>, Option<PathBuf>, Option<u64>) {
    let current = PathBuf::from("/snap").join(name).join("current");
    let version = std::fs::read_to_string(current.join("meta/snap.yaml"))
        .ok()
        .and_then(|y| linux_parse::snap_version(&y));
    // The installed snap is a single squashfs image; its size is the size of
    // the application.
    let revision = std::fs::read_link(&current)
        .ok()
        .map(|r| r.to_string_lossy().into_owned());
    let size = revision.and_then(|rev| {
        std::fs::metadata(format!("/var/lib/snapd/snaps/{name}_{rev}.snap"))
            .ok()
            .map(|m| m.len())
    });
    (version, Some(current).filter(|c| c.exists()), size)
}

fn flatpak_location(id: &str, scope: AppScope) -> Option<PathBuf> {
    let base = match scope {
        AppScope::CurrentUser => dirs::data_dir()?.join("flatpak/app"),
        AppScope::AllUsers => PathBuf::from("/var/lib/flatpak/app"),
    };
    let dir = base.join(id);
    dir.is_dir().then_some(dir)
}

pub fn collect() -> Vec<InstalledApp> {
    let entries = visible_entries();
    let home = home();

    // Snap and Flatpak are recognised by path; the rest go to the package
    // databases.
    let native: Vec<PathBuf> = entries
        .iter()
        .filter(|e| {
            !is_under(&e.path, "/snapd/desktop/applications")
                && !is_under(&e.path, "/flatpak/exports/")
        })
        .map(|e| e.path.clone())
        .collect();
    let db = read_dpkg()
        .or_else(read_pacman)
        .or_else(|| read_rpm(&native))
        .unwrap_or_default();

    // One application per owner, however many launchers it installs.
    let mut apps: HashMap<String, InstalledApp> = HashMap::new();
    for entry in entries {
        let (source, scope, package) = if is_under(&entry.path, "/snapd/desktop/applications") {
            // `firefox_firefox.desktop` belongs to snap `firefox`.
            let file = stem(&entry.path);
            let name = file.split('_').next().unwrap_or(&file).to_string();
            (AppSource::Snap, AppScope::AllUsers, name)
        } else if is_under(&entry.path, "/flatpak/exports/") {
            let user = home
                .as_ref()
                .map(|h| entry.path.starts_with(h))
                .unwrap_or(false);
            let scope = if user {
                AppScope::CurrentUser
            } else {
                AppScope::AllUsers
            };
            (AppSource::Flatpak, scope, stem(&entry.path))
        } else if let (Some(source), Some(package)) = (db.source, db.owner.get(&entry.path)) {
            (source, AppScope::AllUsers, package.clone())
        } else {
            let user = home
                .as_ref()
                .map(|h| entry.path.starts_with(h))
                .unwrap_or(false);
            let scope = if user {
                AppScope::CurrentUser
            } else {
                AppScope::AllUsers
            };
            (
                AppSource::Manual,
                scope,
                entry.path.to_string_lossy().into_owned(),
            )
        };

        let id = format!("{}:{}", source_tag(source, scope), package);
        let entry_is_primary = stem(&entry.path).eq_ignore_ascii_case(&package)
            || stem(&entry.path).ends_with(&format!(".{package}"));

        if let Some(existing) = apps.get_mut(&id) {
            if entry_is_primary {
                existing.name = entry.name;
            }
            continue;
        }

        let mut app = InstalledApp {
            id: id.clone(),
            name: entry.name,
            publisher: None,
            version: None,
            estimated_size_bytes: None,
            measured_size_bytes: None,
            install_date: None,
            install_location: None,
            scope,
            has_uninstaller: false,
            is_windows_component: false,
            source,
            uninstall_hint: None,
        };
        match source {
            AppSource::Snap => {
                let (version, location, size) = snap_details(&package);
                app.version = version;
                app.install_location = location;
                app.estimated_size_bytes = size;
                app.has_uninstaller = true;
            }
            AppSource::Flatpak => {
                app.install_location = flatpak_location(&package, scope);
                app.has_uninstaller = true;
            }
            AppSource::Dpkg | AppSource::Rpm | AppSource::Pacman => {
                if let Some(record) = db.records.get(&package) {
                    app.version = record.version.clone();
                    app.publisher = record.publisher.clone();
                    app.estimated_size_bytes = record.size_bytes;
                    app.install_date = record.install_date.clone();
                }
                app.uninstall_hint = removal_command(source, &package);
            }
            AppSource::Manual | AppSource::Registry => {}
        }
        apps.insert(id, app);
    }
    apps.into_values().collect()
}

/// Package names are handed to flatpak and snap as single arguments, but they
/// are still held to the characters those tools allow.
fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

fn run(program: &str, args: &[&str], what: &str) -> Result<()> {
    let output = Command::new(program).args(args).output().map_err(|e| {
        AllInsightError::Platform(format!("Could not start {program} to remove {what}: {e}"))
    })?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let last = detail
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    Err(AllInsightError::Platform(format!(
        "{program} could not remove {what}. {last}"
    )))
}

pub fn uninstall(id: &str) -> Result<()> {
    let invalid =
        || AllInsightError::InvalidInput("That application reference is not valid.".into());
    let (tag, package) = id.split_once(':').ok_or_else(invalid)?;
    // The id came from `collect`, so confirm it is still listed rather than
    // trusting the string.
    let app = collect()
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(invalid)?;
    if app.has_uninstaller && !valid_package_name(package) {
        return Err(invalid());
    }

    match tag {
        "flatpak-user" => run(
            "flatpak",
            &["uninstall", "--user", "--noninteractive", "-y", package],
            &app.name,
        ),
        "flatpak-system" => run(
            "flatpak",
            &["uninstall", "--system", "--noninteractive", "-y", package],
            &app.name,
        ),
        "snap" => run("snap", &["remove", package], &app.name),
        _ => Err(AllInsightError::Platform(match app.uninstall_hint {
            Some(cmd) => format!(
                "Removing {} needs administrator rights. Run this in a terminal: {cmd}",
                app.name
            ),
            None => format!(
                "{} was not installed by a package manager, so AllInsight cannot remove it safely.",
                app.name
            ),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_with_shell_or_option_syntax_are_refused() {
        assert!(valid_package_name("org.gimp.GIMP"));
        assert!(valid_package_name("libstdc++6"));
        assert!(!valid_package_name("--help"));
        assert!(!valid_package_name("a;rm -rf ~"));
        assert!(!valid_package_name("../x"));
    }

    #[test]
    fn a_forged_id_is_refused_before_anything_runs() {
        assert!(uninstall("snap:not-a-real-snap-allinsight").is_err());
        assert!(uninstall("no-separator").is_err());
    }

    #[test]
    fn the_list_is_readable() {
        for app in collect() {
            assert!(!app.name.is_empty());
            assert!(app.id.contains(':'));
        }
    }
}

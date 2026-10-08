//! Handing a verified package to the operating system, and checking afterwards
//! that it worked.
//!
//! The order is fixed:
//!
//! ```text
//! verified package in updates/  ->  re-hash it  ->  back up the database
//!   ->  write updates/pending.json  ->  hand over to the installer
//!   ->  (next launch) compare the running version with pending.json
//! ```
//!
//! What can and cannot be rolled back is stated plainly in
//! docs/UPDATE_SYSTEM.md. In short: user data is never touched by an
//! application update (it lives outside the files an installer replaces, and
//! the NSIS hook skips data removal in update mode), the database is copied
//! before every install, an AppImage swap restores the previous file if it
//! fails, and a Windows install that is interrupted is repaired by running
//! the same verified installer again, which stays in `updates/` until the new
//! version has started successfully.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::metadata::PackageFormat;

/// Written just before handing over, read on the next launch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingInstall {
    pub from_version: String,
    pub to_version: String,
    pub package: String,
    pub started_at: i64,
}

/// What the next launch found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum InstallReport {
    Completed {
        version: String,
    },
    /// Still running the version that was there before. The installer was
    /// cancelled, failed, or was interrupted.
    NotCompleted {
        attempted: String,
        running: String,
    },
}

/// What the caller must do once the package has been handed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handover {
    /// The installer is running; AllInsight must exit so it can replace files.
    ExitForInstaller,
    /// The new version is in place; restarting will start it.
    RestartInto(PathBuf),
    /// The package must be installed by the system package manager; it has
    /// been verified and is waiting at this path.
    InstallManually(PathBuf),
}

pub fn pending_path(staging: &Path) -> PathBuf {
    staging.join("pending.json")
}

pub fn write_pending(staging: &Path, pending: &PendingInstall) -> std::io::Result<()> {
    std::fs::create_dir_all(staging)?;
    let json = serde_json::to_string_pretty(pending).map_err(std::io::Error::other)?;
    write_atomically(&pending_path(staging), json.as_bytes())
}

/// Look at what the last install left behind and tidy up.
///
/// On success the staged packages are removed. On failure they are kept, so
/// the verified installer can be run again without downloading it.
pub fn reconcile(staging: &Path, running: &str) -> Option<InstallReport> {
    let path = pending_path(staging);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let pending: PendingInstall = serde_json::from_str(&text).ok()?;

    if running == pending.to_version {
        clear_packages(staging);
        Some(InstallReport::Completed {
            version: running.to_string(),
        })
    } else {
        Some(InstallReport::NotCompleted {
            attempted: pending.to_version,
            running: running.to_string(),
        })
    }
}

/// Remove downloaded packages and partial downloads, keeping nothing else.
pub fn clear_packages(staging: &Path) {
    let Ok(entries) = std::fs::read_dir(staging) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_package = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("AllInsight-") || n.ends_with(".part"));
        if is_package && path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Write `bytes` to `path` so that a reader sees either the old file or the
/// new one, never half of each.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

/// Start installing a verified package.
pub fn hand_over(format: PackageFormat, package: &Path) -> std::io::Result<Handover> {
    match format {
        PackageFormat::Nsis => run_nsis(package),
        PackageFormat::Appimage => match std::env::var_os("APPIMAGE").map(PathBuf::from) {
            Some(current) if current.is_file() => {
                replace_file_with_rollback(&current, package)?;
                Ok(Handover::RestartInto(current))
            }
            // Not running from an AppImage (a .deb/.rpm install, or a
            // developer build): there is nothing for us to replace.
            _ => Ok(Handover::InstallManually(package.to_path_buf())),
        },
        PackageFormat::Deb | PackageFormat::Rpm | PackageFormat::Dmg => {
            Ok(Handover::InstallManually(package.to_path_buf()))
        }
    }
}

#[cfg(windows)]
fn run_nsis(package: &Path) -> std::io::Result<Handover> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    // /P   passive: progress only, no questions
    // /UPDATE  update mode: the uninstall step keeps user data (see hooks.nsh)
    // /R   start AllInsight again when done
    std::process::Command::new(package)
        .args(["/P", "/UPDATE", "/R"])
        .creation_flags(DETACHED_PROCESS)
        .spawn()?;
    Ok(Handover::ExitForInstaller)
}

#[cfg(not(windows))]
fn run_nsis(package: &Path) -> std::io::Result<Handover> {
    Ok(Handover::InstallManually(package.to_path_buf()))
}

/// Replace `current` with `new`, keeping `current.previous` until the swap
/// has succeeded, and putting it back if any step fails.
pub fn replace_file_with_rollback(current: &Path, new: &Path) -> std::io::Result<()> {
    let previous = sibling(current, "previous");
    let incoming = sibling(current, "incoming");

    // Stage next to the target so the final step is a rename on one volume.
    std::fs::copy(new, &incoming)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&incoming, std::fs::Permissions::from_mode(0o755))?;
    }

    let _ = std::fs::remove_file(&previous);
    if let Err(e) = std::fs::rename(current, &previous) {
        let _ = std::fs::remove_file(&incoming);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&incoming, current) {
        // Roll back: the old file goes back where it was.
        let _ = std::fs::rename(&previous, current);
        let _ = std::fs::remove_file(&incoming);
        return Err(e);
    }
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("allinsight-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pending(to: &str) -> PendingInstall {
        PendingInstall {
            from_version: "1.0.0".into(),
            to_version: to.into(),
            package: "AllInsight-1.1.0-windows-x64.exe".into(),
            started_at: 0,
        }
    }

    #[test]
    fn a_completed_install_is_recognised_and_tidied() {
        let dir = temp("done");
        std::fs::write(dir.join("AllInsight-1.1.0-windows-x64.exe"), b"x").unwrap();
        std::fs::write(dir.join("keep.txt"), b"x").unwrap();
        write_pending(&dir, &pending("1.1.0")).unwrap();

        assert_eq!(
            reconcile(&dir, "1.1.0"),
            Some(InstallReport::Completed {
                version: "1.1.0".into()
            })
        );
        assert!(!dir.join("AllInsight-1.1.0-windows-x64.exe").exists());
        assert!(dir.join("keep.txt").exists(), "only packages are removed");
        assert!(!pending_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_interrupted_install_is_reported_and_the_package_kept() {
        let dir = temp("interrupted");
        std::fs::write(dir.join("AllInsight-1.1.0-windows-x64.exe"), b"x").unwrap();
        write_pending(&dir, &pending("1.1.0")).unwrap();

        assert_eq!(
            reconcile(&dir, "1.0.0"),
            Some(InstallReport::NotCompleted {
                attempted: "1.1.0".into(),
                running: "1.0.0".into()
            })
        );
        assert!(
            dir.join("AllInsight-1.1.0-windows-x64.exe").exists(),
            "the verified installer stays so it can be run again"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_pending_reports_nothing() {
        let dir = temp("none");
        assert_eq!(reconcile(&dir, "1.0.0"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_swap_replaces_the_target() {
        let dir = temp("swap");
        let current = dir.join("AllInsight.AppImage");
        let new = dir.join("download.AppImage");
        std::fs::write(&current, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        replace_file_with_rollback(&current, &new).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new");
        assert_eq!(
            std::fs::read(sibling(&current, "previous")).unwrap(),
            b"old"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_swap_leaves_the_original_in_place() {
        let dir = temp("rollback");
        let current = dir.join("AllInsight.AppImage");
        std::fs::write(&current, b"old").unwrap();
        // The new package does not exist, so staging fails.
        assert!(replace_file_with_rollback(&current, &dir.join("missing")).is_err());
        assert_eq!(std::fs::read(&current).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn system_packages_are_left_for_the_package_manager() {
        let path = PathBuf::from("/tmp/AllInsight-1.1.0-linux-x64.deb");
        assert_eq!(
            hand_over(PackageFormat::Deb, &path).unwrap(),
            Handover::InstallManually(path.clone())
        );
    }
}

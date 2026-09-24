//! Removal primitives.
//!
//! Every function here takes a [`ValidatedPath`], which can only come from the
//! deletion guard. There is deliberately no function in this module that takes
//! a `&Path`.

use crate::error::{AllInsightError, Result};
use crate::services::security::guard::EntryKind;
use crate::services::security::{paths, ValidatedPath};

use super::categories::DeletionMode;

/// Remove one validated entry.
pub fn remove(entry: &ValidatedPath, mode: DeletionMode) -> Result<()> {
    match mode {
        DeletionMode::Recycle => recycle(entry),
        DeletionMode::Permanent => permanent(entry),
        DeletionMode::ShellApi => Err(AllInsightError::InvalidInput(
            format!("This category is handled by {}, not by file removal.", crate::platform::os_name()),
        )),
    }
}

/// Send to the Recycle Bin or Trash, so the user can undo it.
fn recycle(entry: &ValidatedPath) -> Result<()> {
    trash::delete(entry.as_path()).map_err(|e| {
        AllInsightError::Other(format!(
            "Could not move {} to the {}: {e}",
            display_name(entry),
            crate::platform::trash_name()
        ))
    })
}

/// Delete outright. Used only for caches that are regenerated automatically,
/// where a Recycle Bin copy would occupy exactly the space being reclaimed.
fn permanent(entry: &ValidatedPath) -> Result<()> {
    let path = paths::long_path(entry.as_path());
    let result = match entry.kind() {
        EntryKind::File => std::fs::remove_file(&path),
        // Directories are only ever removed when empty, so a mistake in a
        // matching rule cannot take a populated folder with it.
        EntryKind::Directory => std::fs::remove_dir(&path),
    };

    match result {
        Ok(()) => Ok(()),
        Err(e) => match e.kind() {
            // Something recreated or removed it in the meantime. The end state
            // is what was wanted, so this is not a failure.
            std::io::ErrorKind::NotFound => Ok(()),
            std::io::ErrorKind::PermissionDenied => Err(AllInsightError::Other(format!(
                "{} is in use or protected by {} and was left alone.",
                display_name(entry),
                crate::platform::os_name()
            ))),
            _ => Err(AllInsightError::Other(format!(
                "{} could not be removed: {e}",
                display_name(entry)
            ))),
        },
    }
}

/// Only the final component is ever surfaced, so a log line or a notification
/// never carries a full personal path.
fn display_name(entry: &ValidatedPath) -> String {
    entry
        .as_path()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "This item".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::security::{DeletionGuard, ProtectedPaths};
    use std::fs;
    use std::path::PathBuf;

    fn sandbox(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("allinsight-remove-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        paths::canonicalize(&base).unwrap_or(base)
    }

    #[test]
    fn permanent_removal_deletes_a_validated_file() {
        let root = sandbox("permanent");
        let file = root.join("cache.bin");
        fs::write(&file, vec![0u8; 128]).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[root.clone()]);
        let valid = guard.validate(&file, EntryKind::File).unwrap();

        remove(&valid, DeletionMode::Permanent).unwrap();
        assert!(!file.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_an_entry_that_vanished_is_not_an_error() {
        let root = sandbox("vanished");
        let file = root.join("gone.bin");
        fs::write(&file, b"x").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[root.clone()]);
        let valid = guard.validate(&file, EntryKind::File).unwrap();

        fs::remove_file(&file).unwrap();
        assert!(remove(&valid, DeletionMode::Permanent).is_ok());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_populated_directory_is_never_removed_recursively() {
        let root = sandbox("populated");
        let dir = root.join("full");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("child.bin"), b"keep me").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[root.clone()]);
        let valid = guard.validate(&dir, EntryKind::Directory).unwrap();

        assert!(remove(&valid, DeletionMode::Permanent).is_err());
        assert!(dir.join("child.bin").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_shell_mode_is_refused_here() {
        let root = sandbox("shellmode");
        let file = root.join("a.bin");
        fs::write(&file, b"x").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[root.clone()]);
        let valid = guard.validate(&file, EntryKind::File).unwrap();

        assert!(remove(&valid, DeletionMode::ShellApi).is_err());
        assert!(file.exists());

        let _ = fs::remove_dir_all(&root);
    }
}

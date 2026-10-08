//! The deletion guard.
//!
//! Nothing in AllInsight deletes a path directly. Removal takes a
//! [`ValidatedPath`], and the only way to obtain one is to pass a path through
//! [`DeletionGuard::validate`], which re-checks the path against the protected
//! list and the filesystem *at the moment of deletion* rather than trusting a
//! verdict recorded during a scan minutes earlier.
//!
//! The checks run in this order, and any failure ends the sequence:
//!
//! 1. lexical normalisation, so `..` cannot smuggle a path past the list
//! 2. the protected-path engine
//! 3. membership of the allow-list for the requesting cleanup category
//! 4. the entry still exists
//! 5. the entry is not itself a reparse point
//! 6. no directory between the allowed root and the entry is a reparse point
//! 7. the fully canonicalised path is re-checked against 2 and 3
//! 8. the entry is still the kind of thing the caller expected

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths;
use super::protected::{ProtectedPaths, ProtectionReason, ProtectionVerdict};

/// A path that has passed every safety check and may be deleted.
///
/// The inner field is private and there is no public constructor, so a
/// `ValidatedPath` cannot be forged: possession of one is proof that
/// [`DeletionGuard::validate`] approved it.
#[derive(Debug, Clone)]
pub struct ValidatedPath {
    path: PathBuf,
    kind: EntryKind,
    size_bytes: u64,
}

impl ValidatedPath {
    pub fn as_path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
}

/// Why the guard refused. Every variant carries enough context to explain the
/// refusal to the user without guessing.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GuardRejection {
    Protected {
        path: PathBuf,
        reason: ProtectionReason,
        explanation: String,
    },
    OutsideAllowedRoots {
        path: PathBuf,
    },
    Missing {
        path: PathBuf,
    },
    ReparsePoint {
        path: PathBuf,
        at: PathBuf,
    },
    Unreadable {
        path: PathBuf,
        detail: String,
    },
    KindChanged {
        path: PathBuf,
        expected: EntryKind,
    },
    Locked {
        path: PathBuf,
    },
}

impl GuardRejection {
    pub fn path(&self) -> &Path {
        match self {
            GuardRejection::Protected { path, .. }
            | GuardRejection::OutsideAllowedRoots { path }
            | GuardRejection::Missing { path }
            | GuardRejection::ReparsePoint { path, .. }
            | GuardRejection::Unreadable { path, .. }
            | GuardRejection::KindChanged { path, .. }
            | GuardRejection::Locked { path } => path,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            GuardRejection::Protected { explanation, .. } => explanation.clone(),
            GuardRejection::OutsideAllowedRoots { .. } => {
                "This is outside the area this cleanup category is allowed to touch.".into()
            }
            GuardRejection::Missing { .. } => "This item no longer exists.".into(),
            GuardRejection::ReparsePoint { at, .. } => format!(
                "{} is a link that points somewhere else, so it was skipped.",
                at.display()
            ),
            GuardRejection::Unreadable { detail, .. } => {
                format!("This item could not be inspected: {detail}")
            }
            GuardRejection::KindChanged { .. } => {
                "This item changed between the scan and now, so it was skipped.".into()
            }
            GuardRejection::Locked { .. } => "This item is in use by another program.".into(),
        }
    }
}

/// One entry on a category's allow-list.
///
/// Both forms are kept because they can differ: `%TEMP%` on some machines sits
/// behind a junction, so the path the category declared and the path
/// `canonicalize` returns are different strings for the same directory. Step 7
/// of validation compares a canonicalised candidate, and without the canonical
/// form here it would fall outside every root and the category would silently
/// find nothing.
#[derive(Debug, Clone)]
struct AllowedRoot {
    /// As the category declared it. Used for the carve-out lookup, because
    /// the carve-out templates are written in the same declared form.
    declared: PathBuf,
    /// As the filesystem resolves it.
    canonical: PathBuf,
}

/// Validates paths on behalf of one cleanup category.
///
/// `allowed_roots` is the narrow allow-list for that category, for example the
/// two temp directories. A path outside every allowed root is refused even
/// when the protected list has nothing to say about it: cleanup is opt-in by
/// location, never opt-out.
pub struct DeletionGuard<'a> {
    protected: &'a ProtectedPaths,
    allowed_roots: Vec<AllowedRoot>,
    /// Lower-case filename prefixes that may override the *filename* rules of
    /// the protected engine, and nothing else. Windows names its thumbnail and
    /// icon caches `thumbcache_*.db`, which the blanket `.db` rule would
    /// otherwise refuse. Supplied by the static cleanup category definitions,
    /// never by the frontend.
    name_exemptions: Vec<String>,
}

impl<'a> DeletionGuard<'a> {
    pub fn new(protected: &'a ProtectedPaths, allowed_roots: &[PathBuf]) -> Self {
        let allowed_roots = allowed_roots
            .iter()
            .map(|r| paths::normalize_lexical(r))
            .filter(|r| r.is_absolute())
            .map(|declared| AllowedRoot {
                canonical: paths::canonicalize(&declared).unwrap_or_else(|_| declared.clone()),
                declared,
            })
            .collect();
        Self {
            protected,
            allowed_roots,
            name_exemptions: Vec::new(),
        }
    }

    /// Declare the cache filename prefixes this category is allowed to remove.
    pub fn with_name_exemptions(mut self, prefixes: &[&str]) -> Self {
        self.name_exemptions = prefixes.iter().map(|p| p.to_lowercase()).collect();
        self
    }

    /// The allow-list entry that contains `path`, matched against either form
    /// of the root.
    fn allowed_root_for(&self, path: &Path) -> Option<&AllowedRoot> {
        self.allowed_roots.iter().find(|root| {
            paths::is_strictly_within(path, &root.canonical)
                || paths::is_strictly_within(path, &root.declared)
        })
    }

    /// Steps 1 to 3 only: normalisation, the protected list, and allow-list
    /// membership. No filesystem calls at all.
    ///
    /// Discovery uses this to decide what to *offer*, because the full
    /// sequence canonicalises and stats every entry, and a browser cache can
    /// hold tens of thousands of them. Nothing is removed on the strength of
    /// this answer: [`DeletionGuard::validate`] still runs in full immediately
    /// before deletion, which is where the filesystem checks belong anyway,
    /// since the filesystem can change between the two moments.
    pub fn permits_lexically(&self, candidate: &Path) -> Result<PathBuf, GuardRejection> {
        let normalized = paths::normalize_lexical(candidate);
        self.check_protected(&normalized)?;
        if self.allowed_root_for(&normalized).is_none() {
            return Err(GuardRejection::OutsideAllowedRoots { path: normalized });
        }
        Ok(normalized)
    }

    /// Run the full sequence. On success the returned token may be handed to
    /// the deletion routines; on failure the reason is reported to the user
    /// and the item is counted as skipped, never silently dropped.
    pub fn validate(
        &self,
        candidate: &Path,
        expected: EntryKind,
    ) -> Result<ValidatedPath, GuardRejection> {
        // 1. Lexical normalisation.
        let normalized = paths::normalize_lexical(candidate);

        // 2. Protected-path engine, on the lexical form.
        self.check_protected(&normalized)?;

        // 3. Allow-list membership.
        let root = self.allowed_root_for(&normalized).cloned().ok_or_else(|| {
            GuardRejection::OutsideAllowedRoots {
                path: normalized.clone(),
            }
        })?;
        // The reparse walk below starts from whichever form of the root the
        // candidate actually sits under.
        let boundary = if paths::is_strictly_within(&normalized, &root.declared) {
            root.declared.clone()
        } else {
            root.canonical.clone()
        };

        // 4. The entry still exists. `symlink_metadata` does not follow links,
        //    which matters for step 5.
        let meta = std::fs::symlink_metadata(paths::long_path(&normalized)).map_err(|e| match e
            .kind()
        {
            std::io::ErrorKind::NotFound => GuardRejection::Missing {
                path: normalized.clone(),
            },
            std::io::ErrorKind::PermissionDenied => GuardRejection::Locked {
                path: normalized.clone(),
            },
            _ => GuardRejection::Unreadable {
                path: normalized.clone(),
                detail: e.to_string(),
            },
        })?;

        // 5. The entry itself must not be a link.
        if paths::reparse_state(&normalized) == paths::ReparseState::Reparse {
            return Err(GuardRejection::ReparsePoint {
                path: normalized.clone(),
                at: normalized,
            });
        }

        // 6. No directory between the allowed root and the entry may be a
        //    link. A junction dropped inside a temp folder that points at
        //    Documents is exactly the attack this closes.
        if let Some(at) = paths::first_reparse_ancestor(&normalized, Some(&boundary)) {
            if !paths::same_path(&at, &normalized) {
                return Err(GuardRejection::ReparsePoint {
                    path: normalized.clone(),
                    at,
                });
            }
        }

        // 7. Canonicalise and re-run the location checks on the real target.
        //    Everything above is defence in depth for this step; this is the
        //    one that is authoritative.
        let real = paths::canonicalize(&normalized).map_err(|e| GuardRejection::Unreadable {
            path: normalized.clone(),
            detail: e.to_string(),
        })?;
        self.check_protected(&real)?;
        if self.allowed_root_for(&real).is_none() {
            return Err(GuardRejection::OutsideAllowedRoots { path: real });
        }

        // 8. The entry is still what the scan said it was.
        let kind = if meta.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        if kind != expected {
            return Err(GuardRejection::KindChanged {
                path: normalized,
                expected,
            });
        }

        Ok(ValidatedPath {
            path: real,
            kind,
            size_bytes: if kind == EntryKind::File {
                meta.len()
            } else {
                0
            },
        })
    }

    fn check_protected(&self, path: &Path) -> Result<(), GuardRejection> {
        let verdict: ProtectionVerdict = self.protected.classify(path);
        if !verdict.protected {
            return Ok(());
        }
        let reason = verdict.reason.unwrap_or(ProtectionReason::Unresolvable);

        // Exemption 1: a declared cache filename, refused only because of the
        // blanket database-extension rule.
        if reason == ProtectionReason::Database && self.name_is_exempt(path) {
            return Ok(());
        }

        // Exemption 2: a path inside a directory that is both a static
        // carve-out and one of this category's static roots. Both halves are
        // compiled in, so no runtime input can widen this.
        if ProtectedPaths::reason_is_system_root(reason) {
            if let Some(root) = self.allowed_root_for(path) {
                // Either form may match a carve-out template: the declared one
                // normally does, and the canonical one covers a root that is
                // reached through a link.
                if self.protected.is_carve_out(&root.declared)
                    || self.protected.is_carve_out(&root.canonical)
                {
                    return Ok(());
                }
            }
        }

        Err(GuardRejection::Protected {
            path: path.to_path_buf(),
            reason,
            explanation: reason.explain().to_string(),
        })
    }

    fn name_is_exempt(&self, path: &Path) -> bool {
        if self.name_exemptions.is_empty() {
            return false;
        }
        let name = paths::file_name_lower(path);
        self.name_exemptions.iter().any(|p| name.starts_with(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("allinsight-guard-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("create sandbox");
        // `TEMP` can itself sit behind a link on some machines; canonicalise
        // so the allow-list and the guard agree.
        paths::canonicalize(&base).unwrap_or(base)
    }

    #[test]
    fn a_plain_file_inside_an_allowed_root_is_approved() {
        let root = temp_root("plain");
        let file = root.join("scratch.tmp");
        fs::write(&file, b"junk").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));
        let ok = guard.validate(&file, EntryKind::File).expect("approved");
        assert_eq!(ok.size_bytes(), 4);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_outside_every_allowed_root_is_refused() {
        let root = temp_root("outside");
        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

        let elsewhere = if cfg!(windows) {
            "D:\\somewhere\\else\\file.tmp"
        } else {
            "/somewhere/else/file.tmp"
        };
        let err = guard
            .validate(Path::new(elsewhere), EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(err, GuardRejection::OutsideAllowedRoots { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn traversal_out_of_an_allowed_root_is_refused() {
        let root = temp_root("traversal");
        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

        let hostile = if cfg!(windows) {
            root.join("..\\..\\Windows\\System32\\config")
        } else {
            root.join("../../etc/shadow")
        };
        let err = guard
            .validate(&hostile, EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(
            err,
            GuardRejection::Protected { .. } | GuardRejection::OutsideAllowedRoots { .. }
        ));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_allowed_root_itself_is_never_deletable() {
        let root = temp_root("selfroot");
        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

        let err = guard
            .validate(&root, EntryKind::Directory)
            .expect_err("root must be refused");
        assert!(matches!(err, GuardRejection::OutsideAllowedRoots { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_entry_is_refused_rather_than_deleted() {
        let root = temp_root("missing");
        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

        let err = guard
            .validate(&root.join("gone.tmp"), EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(err, GuardRejection::Missing { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_that_became_a_directory_is_refused() {
        let root = temp_root("kind");
        let dir = root.join("was-a-file");
        fs::create_dir_all(&dir).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));
        let err = guard
            .validate(&dir, EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(err, GuardRejection::KindChanged { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn protected_extensions_are_refused_inside_an_allowed_root() {
        let root = temp_root("dbext");
        let db = root.join("app.sqlite");
        fs::write(&db, b"not really a database").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));
        let err = guard
            .validate(&db, EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(err, GuardRejection::Protected { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn user_protected_roots_beat_the_allow_list() {
        let root = temp_root("userprot");
        let keep = root.join("keep");
        fs::create_dir_all(&keep).unwrap();
        let file = keep.join("a.tmp");
        fs::write(&file, b"x").unwrap();

        let mut protected = ProtectedPaths::new(&[]);
        protected.set_user_roots(std::slice::from_ref(&keep));
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

        let err = guard
            .validate(&file, EntryKind::File)
            .expect_err("must refuse");
        assert!(matches!(err, GuardRejection::Protected { .. }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cache_name_exemption_covers_only_the_declared_prefix() {
        let root = temp_root("exempt");
        let cache = root.join("thumbcache_256.db");
        let other = root.join("contacts.db");
        fs::write(&cache, b"cache").unwrap();
        fs::write(&other, b"data").unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root))
            .with_name_exemptions(&["thumbcache_"]);

        assert!(guard.validate(&cache, EntryKind::File).is_ok());
        assert!(matches!(
            guard.validate(&other, EntryKind::File),
            Err(GuardRejection::Protected { .. })
        ));

        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn a_name_exemption_cannot_reach_a_protected_root() {
        let protected = ProtectedPaths::new(&[]);
        let windows = paths::expand_env("%SystemRoot%").unwrap();
        let guard = DeletionGuard::new(&protected, &[windows.join("System32")])
            .with_name_exemptions(&["thumbcache_"]);

        let err = guard
            .validate(
                &windows.join("System32\\thumbcache_evil.db"),
                EntryKind::File,
            )
            .expect_err("System32 must stay protected");
        assert!(matches!(
            err,
            GuardRejection::Protected { .. } | GuardRejection::Missing { .. }
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn a_name_exemption_cannot_reach_a_protected_root() {
        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[PathBuf::from("/usr/lib")])
            .with_name_exemptions(&["thumbcache_"]);
        let err = guard
            .validate(Path::new("/usr/lib/thumbcache_evil.db"), EntryKind::File)
            .expect_err("/usr/lib must stay protected");
        assert!(matches!(
            err,
            GuardRejection::Protected { .. } | GuardRejection::Missing { .. }
        ));
    }

    /// Pointing a category at a system directory must not lift its
    /// protection: only the compiled-in carve-outs can, and on Linux the only
    /// one is Snap Firefox's cache.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_carve_out_only_applies_to_the_exact_declared_directory() {
        let protected = ProtectedPaths::new(&[]);
        let etc_guard = DeletionGuard::new(&protected, &[PathBuf::from("/etc")]);
        assert!(etc_guard.check_protected(Path::new("/etc/passwd")).is_err());
        let var_guard = DeletionGuard::new(&protected, &[PathBuf::from("/var/tmp")]);
        assert!(var_guard
            .check_protected(Path::new("/var/tmp/x.tmp"))
            .is_err());

        let home = dirs::home_dir().unwrap();
        let cache = home.join("snap/firefox/common/.cache/mozilla/firefox/p.default/cache2");
        let snap_guard = DeletionGuard::new(&protected, &[cache.clone()]);
        assert!(snap_guard
            .check_protected(&cache.join("entries/ABC"))
            .is_ok());
        // The profile beside it is not.
        let profile = home.join("snap/firefox/common/.mozilla/firefox/p.default");
        let profile_guard = DeletionGuard::new(&protected, &[profile.clone()]);
        assert!(profile_guard
            .check_protected(&profile.join("key4.db"))
            .is_err());
        assert!(profile_guard
            .check_protected(&profile.join("prefs.js"))
            .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_outside_the_allowed_root_is_refused() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-target");
        fs::write(outside.join("important.txt"), b"user data").unwrap();
        let link = root.join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        let protected = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&protected, &[root.clone()]);
        let err = guard
            .validate(&link, EntryKind::Directory)
            .expect_err("symlink must be refused");
        assert!(matches!(err, GuardRejection::ReparsePoint { .. }));
        let err = guard
            .validate(&link.join("important.txt"), EntryKind::File)
            .expect_err("path through a symlink must be refused");
        assert!(matches!(
            err,
            GuardRejection::ReparsePoint { .. } | GuardRejection::OutsideAllowedRoots { .. }
        ));
        assert!(outside.join("important.txt").exists());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[cfg(windows)]
    #[test]
    fn a_carve_out_only_applies_to_the_exact_declared_directory() {
        let protected = ProtectedPaths::new(&[]);
        let windows = paths::expand_env("%SystemRoot%").unwrap();

        // `C:\Windows\Temp` is a carve-out, so a file inside it is allowed
        // through the protected-root rule.
        let temp_guard = DeletionGuard::new(&protected, &[windows.join("Temp")]);
        assert!(temp_guard
            .check_protected(&windows.join("Temp\\leftover.tmp"))
            .is_ok());

        // `C:\Windows\System32` is not a carve-out, and pointing a category at
        // it must not lift the protection.
        let system_guard = DeletionGuard::new(&protected, &[windows.join("System32")]);
        assert!(system_guard
            .check_protected(&windows.join("System32\\drivers\\etc\\hosts"))
            .is_err());

        // Nor may a category claim `C:\Windows` itself as its root.
        let windows_guard = DeletionGuard::new(&protected, std::slice::from_ref(&windows));
        assert!(windows_guard
            .check_protected(&windows.join("explorer.exe"))
            .is_err());
    }

    #[test]
    fn a_user_protected_root_is_never_lifted_by_a_carve_out() {
        let root = temp_root("userbeatscarve");
        let mut protected = ProtectedPaths::new(&[]);
        protected.set_user_roots(std::slice::from_ref(&root));

        let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));
        assert!(guard.check_protected(&root.join("anything.tmp")).is_err());

        let _ = fs::remove_dir_all(&root);
    }

    /// Creating a junction needs no elevation, so this runs everywhere. If the
    /// call fails on a filesystem that does not support reparse points the
    /// test degrades to a no-op rather than failing spuriously.
    #[cfg(windows)]
    #[test]
    fn a_junction_pointing_outside_the_allowed_root_is_refused() {
        let root = temp_root("junction");
        let outside = temp_root("junction-target");
        let secret = outside.join("important.txt");
        fs::write(&secret, b"user data").unwrap();

        let link = root.join("link");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output();
        let created = made.map(|o| o.status.success()).unwrap_or(false);
        if created {
            let protected = ProtectedPaths::new(&[]);
            let guard = DeletionGuard::new(&protected, std::slice::from_ref(&root));

            // The junction itself.
            let err = guard
                .validate(&link, EntryKind::Directory)
                .expect_err("junction must be refused");
            assert!(matches!(err, GuardRejection::ReparsePoint { .. }));

            // And anything reached through it.
            let through = link.join("important.txt");
            let err = guard
                .validate(&through, EntryKind::File)
                .expect_err("path through a junction must be refused");
            assert!(matches!(
                err,
                GuardRejection::ReparsePoint { .. } | GuardRejection::OutsideAllowedRoots { .. }
            ));
        }

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }
}

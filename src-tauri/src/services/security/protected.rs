//! The protected path engine.
//!
//! This is the single place that answers "may AllInsight touch this?". Every
//! destructive code path consults it, and it is deliberately conservative:
//! when it cannot prove a path is safe it reports the path as protected.
//!
//! The list is built at startup from Windows known folders rather than from
//! hard-coded strings, because Documents and Desktop are frequently redirected
//! to OneDrive or to a second drive, and a hard-coded `C:\Users\x\Documents`
//! would silently miss the real location.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths;

/// Why a path is refused. Surfaced to the user verbatim so a refusal is always
/// explainable rather than mysterious.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionReason {
    WindowsDirectory,
    ProgramFiles,
    ProgramData,
    BootOrRecovery,
    SystemVolumeMetadata,
    UserDocuments,
    UserDesktop,
    UserPictures,
    UserVideos,
    UserMusic,
    UserDownloads,
    UserProfileRoot,
    OneDrive,
    BrowserProfile,
    CredentialStore,
    Database,
    SourceControl,
    UserConfigured,
    DriveRoot,
    NotAbsolute,
    OutsideAllowedRoot,
    ReparsePoint,
    Unresolvable,
    HostileName,
}

impl ProtectionReason {
    /// One short sentence, written for the person reading a refusal dialog.
    pub fn explain(&self) -> &'static str {
        match self {
            ProtectionReason::WindowsDirectory => "This is part of the Windows installation.",
            ProtectionReason::ProgramFiles => "This belongs to an installed application.",
            ProtectionReason::ProgramData => "This holds shared application data.",
            ProtectionReason::BootOrRecovery => "This is boot or recovery data.",
            ProtectionReason::SystemVolumeMetadata => "This is volume metadata managed by Windows.",
            ProtectionReason::UserDocuments => "This is inside your Documents folder.",
            ProtectionReason::UserDesktop => "This is on your Desktop.",
            ProtectionReason::UserPictures => "This is inside your Pictures folder.",
            ProtectionReason::UserVideos => "This is inside your Videos folder.",
            ProtectionReason::UserMusic => "This is inside your Music folder.",
            ProtectionReason::UserDownloads => "This is inside your Downloads folder.",
            ProtectionReason::UserProfileRoot => "This is the root of your user profile.",
            ProtectionReason::OneDrive => "This is synchronised by OneDrive.",
            ProtectionReason::BrowserProfile => "This is a browser profile with saved data.",
            ProtectionReason::CredentialStore => "This may contain keys or credentials.",
            ProtectionReason::Database => "This looks like an application database.",
            ProtectionReason::SourceControl => "This is version control data.",
            ProtectionReason::UserConfigured => "You added this to the protected list.",
            ProtectionReason::DriveRoot => "This is the root of a drive.",
            ProtectionReason::NotAbsolute => "The location could not be resolved to a full path.",
            ProtectionReason::OutsideAllowedRoot => {
                "This is outside the area this cleanup category is allowed to touch."
            }
            ProtectionReason::ReparsePoint => {
                "This location, or a folder above it, is a link that points somewhere else."
            }
            ProtectionReason::Unresolvable => "This location could not be verified.",
            ProtectionReason::HostileName => "This name contains characters that cannot be trusted.",
        }
    }
}

/// The verdict for one path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectionVerdict {
    pub protected: bool,
    pub reason: Option<ProtectionReason>,
    /// The protected root or rule that matched, for display.
    pub matched: Option<PathBuf>,
}

impl ProtectionVerdict {
    pub fn allowed() -> Self {
        Self {
            protected: false,
            reason: None,
            matched: None,
        }
    }

    fn refuse(reason: ProtectionReason, matched: Option<PathBuf>) -> Self {
        Self {
            protected: true,
            reason: Some(reason),
            matched,
        }
    }

    pub fn describe(&self) -> String {
        match self.reason {
            Some(r) => r.explain().to_string(),
            None => "Allowed.".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct ProtectedRoot {
    path: PathBuf,
    reason: ProtectionReason,
    /// The lower-cased components of `path`, computed once.
    ///
    /// `classify` is called for every file a cleanup scan considers, and there
    /// are dozens of protected roots. Deriving this per call meant re-walking
    /// and re-allocating both sides of every comparison, which dominated the
    /// cost of a scan across a large cache.
    key: Vec<String>,
}

/// The engine itself. Built once at startup and shared behind the app state.
#[derive(Debug, Clone)]
pub struct ProtectedPaths {
    roots: Vec<ProtectedRoot>,
    user_roots: Vec<PathBuf>,
    /// Exact lower-case file names that are never deleted, wherever they live.
    file_names: HashSet<String>,
    /// Lower-case extensions that are never deleted, wherever they live.
    extensions: HashSet<String>,
    /// Lower-case directory names that protect everything beneath them.
    dir_names: HashSet<String>,
    /// See [`ProtectedPaths::is_carve_out`].
    carve_outs: Vec<PathBuf>,
    /// Comparison keys for `carve_outs`, computed once for the same reason
    /// the roots carry theirs.
    carve_out_keys: Vec<Vec<String>>,
}

/// Reasons that come from an enclosing root rather than from the entry itself.
/// Only these can be lifted by a carve-out.
///
/// `UserConfigured` is deliberately absent: a folder the user added to the
/// protected list is never cleaned, whatever else is true about it.
fn is_system_root_reason(reason: ProtectionReason) -> bool {
    matches!(
        reason,
        ProtectionReason::WindowsDirectory
            | ProtectionReason::ProgramFiles
            | ProtectionReason::ProgramData
            | ProtectionReason::BrowserProfile
    )
}

/// The only directories inside a protected root that a cleanup category may
/// ever be pointed at.
///
/// `C:\Windows` is protected, but `C:\Windows\Temp` exists precisely to hold
/// disposable files, and refusing to clean it would make the product useless
/// for its main job. The same applies to the cache folders that sit inside a
/// browser profile. The compromise is this list: it is a compile-time
/// constant, it is intersected with the equally static allow-list of the
/// requesting cleanup category, and it is never influenced by the frontend, by
/// settings, or by the local model.
///
/// A `*` matches exactly one path component, which is how Chromium profile
/// directories (`Default`, `Profile 1`, ...) are covered without listing them.
const CARVE_OUT_TEMPLATES: &[&str] = &[
    "%SystemRoot%\\Temp",
    "%SystemRoot%\\SoftwareDistribution\\Download",
    "%SystemRoot%\\Logs\\CBS",
    "%ProgramData%\\Microsoft\\Windows\\WER\\ReportArchive",
    "%ProgramData%\\Microsoft\\Windows\\WER\\ReportQueue",
    "%ProgramData%\\Microsoft\\Windows\\DeliveryOptimization\\Cache",
    "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Cache\\Cache_Data",
    "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\Code Cache",
    "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\*\\GPUCache",
    "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Cache\\Cache_Data",
    "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\Code Cache",
    "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\*\\GPUCache",
    "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\*\\Cache\\Cache_Data",
    "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\*\\Code Cache",
    "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\*\\GPUCache",
    "%LOCALAPPDATA%\\Vivaldi\\User Data\\*\\Cache\\Cache_Data",
    "%LOCALAPPDATA%\\Vivaldi\\User Data\\*\\Code Cache",
    "%LOCALAPPDATA%\\Vivaldi\\User Data\\*\\GPUCache",
];

/// Component-wise match of two prepared comparison keys, where `*` in the
/// template stands for exactly one component.
fn key_matches_template(path: &[String], template: &[String]) -> bool {
    path.len() == template.len()
        && path
            .iter()
            .zip(template.iter())
            .all(|(a, b)| b == "*" || a == b)
}

/// True when `path` is `ancestor` itself or lives beneath it, both already
/// reduced to comparison keys.
fn key_is_within(path: &[String], ancestor: &[String]) -> bool {
    !ancestor.is_empty() && path.len() >= ancestor.len() && path[..ancestor.len()] == *ancestor
}

impl Default for ProtectedPaths {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl ProtectedPaths {
    /// Build the engine. `extra` is the user-configured additional list from
    /// settings; entries that cannot be resolved are ignored rather than
    /// weakening the rest of the list.
    pub fn new(extra: &[PathBuf]) -> Self {
        let mut roots: Vec<ProtectedRoot> = Vec::new();

        let mut push = |path: Option<PathBuf>, reason: ProtectionReason| {
            if let Some(p) = path {
                let normalized = paths::normalize_lexical(&p);
                if normalized.is_absolute()
                    && !roots.iter().any(|r| paths::same_path(&r.path, &normalized))
                {
                    roots.push(ProtectedRoot {
                        key: paths::comparison_key(&normalized),
                        path: normalized,
                        reason,
                    });
                }
            }
        };

        // --- Windows and shared application roots -------------------------
        push(
            paths::expand_env("%SystemRoot%").or_else(|| Some(PathBuf::from("C:\\Windows"))),
            ProtectionReason::WindowsDirectory,
        );
        push(
            paths::expand_env("%ProgramFiles%"),
            ProtectionReason::ProgramFiles,
        );
        push(
            paths::expand_env("%ProgramFiles(x86)%"),
            ProtectionReason::ProgramFiles,
        );
        push(
            paths::expand_env("%ProgramW6432%"),
            ProtectionReason::ProgramFiles,
        );
        push(
            paths::expand_env("%ProgramData%"),
            ProtectionReason::ProgramData,
        );

        // --- Per-drive boot, recovery and volume metadata -----------------
        for drive in enumerate_drive_roots() {
            for (leaf, reason) in [
                ("System Volume Information", ProtectionReason::SystemVolumeMetadata),
                ("$Recycle.Bin", ProtectionReason::SystemVolumeMetadata),
                ("$RECYCLE.BIN", ProtectionReason::SystemVolumeMetadata),
                ("Recovery", ProtectionReason::BootOrRecovery),
                ("$WinREAgent", ProtectionReason::BootOrRecovery),
                ("Boot", ProtectionReason::BootOrRecovery),
                ("EFI", ProtectionReason::BootOrRecovery),
                ("bootmgr", ProtectionReason::BootOrRecovery),
                ("hiberfil.sys", ProtectionReason::BootOrRecovery),
                ("pagefile.sys", ProtectionReason::BootOrRecovery),
                ("swapfile.sys", ProtectionReason::BootOrRecovery),
            ] {
                push(Some(drive.join(leaf)), reason);
            }
        }

        // --- Windows known folders (may be redirected) --------------------
        push(dirs::document_dir(), ProtectionReason::UserDocuments);
        push(dirs::desktop_dir(), ProtectionReason::UserDesktop);
        push(dirs::picture_dir(), ProtectionReason::UserPictures);
        push(dirs::video_dir(), ProtectionReason::UserVideos);
        push(dirs::audio_dir(), ProtectionReason::UserMusic);
        push(dirs::download_dir(), ProtectionReason::UserDownloads);

        if let Some(home) = dirs::home_dir() {
            for (leaf, reason) in [
                ("Favorites", ProtectionReason::UserProfileRoot),
                ("Links", ProtectionReason::UserProfileRoot),
                ("Contacts", ProtectionReason::UserProfileRoot),
                ("Searches", ProtectionReason::UserProfileRoot),
                ("Saved Games", ProtectionReason::UserProfileRoot),
                (".ssh", ProtectionReason::CredentialStore),
                (".gnupg", ProtectionReason::CredentialStore),
                (".aws", ProtectionReason::CredentialStore),
                (".config", ProtectionReason::UserProfileRoot),
                ("OneDrive", ProtectionReason::OneDrive),
            ] {
                push(Some(home.join(leaf)), reason);
            }
        }
        push(
            paths::expand_env("%OneDrive%"),
            ProtectionReason::OneDrive,
        );
        push(
            paths::expand_env("%OneDriveCommercial%"),
            ProtectionReason::OneDrive,
        );

        // --- Public profile mirrors of the same folders -------------------
        if let Some(public) = paths::expand_env("%PUBLIC%") {
            for (leaf, reason) in [
                ("Documents", ProtectionReason::UserDocuments),
                ("Desktop", ProtectionReason::UserDesktop),
                ("Pictures", ProtectionReason::UserPictures),
                ("Videos", ProtectionReason::UserVideos),
                ("Music", ProtectionReason::UserMusic),
                ("Downloads", ProtectionReason::UserDownloads),
            ] {
                push(Some(public.join(leaf)), reason);
            }
        }

        // --- Browser profiles: the parent of the cache directories --------
        // Cache subfolders inside these are cleanable, the profile itself is
        // not. Ordering does not matter because the cleanup engine checks the
        // narrower allow-list first and this list second.
        if let Some(local) = dirs::data_local_dir() {
            for leaf in [
                "Google\\Chrome\\User Data\\Default",
                "Microsoft\\Edge\\User Data\\Default",
                "BraveSoftware\\Brave-Browser\\User Data\\Default",
                "Vivaldi\\User Data\\Default",
                "Opera Software",
            ] {
                push(Some(local.join(leaf)), ProtectionReason::BrowserProfile);
            }
        }
        if let Some(roaming) = dirs::data_dir() {
            push(
                Some(roaming.join("Mozilla\\Firefox\\Profiles")),
                ProtectionReason::BrowserProfile,
            );
            push(
                Some(roaming.join("Thunderbird\\Profiles")),
                ProtectionReason::BrowserProfile,
            );
        }

        for e in extra {
            push(Some(e.clone()), ProtectionReason::UserConfigured);
        }

        let user_roots = extra
            .iter()
            .map(|p| paths::normalize_lexical(p))
            .collect::<Vec<_>>();

        let carve_outs: Vec<PathBuf> = CARVE_OUT_TEMPLATES
            .iter()
            .filter_map(|t| paths::expand_env(t))
            .collect();
        let carve_out_keys = carve_outs.iter().map(|c| paths::comparison_key(c)).collect();

        Self {
            roots,
            user_roots,
            file_names: default_protected_file_names(),
            extensions: default_protected_extensions(),
            dir_names: default_protected_dir_names(),
            carve_outs,
            carve_out_keys,
        }
    }

    /// True when `path` is one of the statically declared carve-out
    /// directories. Callers must still confirm that the requesting cleanup
    /// category actually declared this directory as one of its roots.
    pub fn is_carve_out(&self, path: &Path) -> bool {
        let key = paths::comparison_key(&paths::normalize_lexical(path));
        self.carve_out_keys
            .iter()
            .any(|template| key_matches_template(&key, template))
    }

    /// Whether a refusal may be lifted for a path inside a carve-out.
    pub fn reason_is_system_root(reason: ProtectionReason) -> bool {
        is_system_root_reason(reason)
    }

    /// The carve-out directories, for the Settings screen and diagnostics.
    pub fn carve_outs(&self) -> &[PathBuf] {
        &self.carve_outs
    }

    /// Replace the user-configured portion of the list without rebuilding the
    /// Windows-derived portion.
    pub fn set_user_roots(&mut self, extra: &[PathBuf]) {
        self.roots
            .retain(|r| r.reason != ProtectionReason::UserConfigured);
        self.user_roots.clear();
        for e in extra {
            let normalized = paths::normalize_lexical(e);
            if !normalized.is_absolute() {
                continue;
            }
            self.user_roots.push(normalized.clone());
            self.roots.push(ProtectedRoot {
                key: paths::comparison_key(&normalized),
                path: normalized,
                reason: ProtectionReason::UserConfigured,
            });
        }
    }

    /// The user-configured entries, for display in Settings.
    pub fn user_roots(&self) -> &[PathBuf] {
        &self.user_roots
    }

    /// Every protected root, for the Settings screen.
    pub fn all_roots(&self) -> Vec<(PathBuf, ProtectionReason)> {
        self.roots.iter().map(|r| (r.path.clone(), r.reason)).collect()
    }

    /// The core question. Purely lexical: no filesystem access, so it is cheap
    /// enough to call for every entry produced by a scan.
    ///
    /// Filesystem-level checks (reparse points, existence) live in
    /// [`crate::services::security::guard`], which calls this first.
    pub fn classify(&self, path: &Path) -> ProtectionVerdict {
        let normalized = paths::normalize_lexical(path);

        if !normalized.is_absolute() {
            return ProtectionVerdict::refuse(ProtectionReason::NotAbsolute, None);
        }

        if let Some(name) = normalized.file_name() {
            if paths::has_hostile_name(name) {
                return ProtectionVerdict::refuse(ProtectionReason::HostileName, None);
            }
        }

        // A bare drive root is never a deletion target.
        if paths::drive_root(&normalized)
            .map(|r| paths::same_path(&r, &normalized))
            .unwrap_or(false)
        {
            return ProtectionVerdict::refuse(ProtectionReason::DriveRoot, Some(normalized));
        }

        // Protected roots: the path is refused if it is the root itself or
        // anything beneath it. The candidate is reduced to its comparison key
        // once and matched against the keys prepared at construction.
        let key = paths::comparison_key(&normalized);
        for root in &self.roots {
            if key_is_within(&key, &root.key) {
                return ProtectionVerdict::refuse(root.reason, Some(root.path.clone()));
            }
        }

        // Directory-name rules, e.g. anything under a `.git` folder.
        if paths::contains_component(&normalized, &self.dir_names) {
            return ProtectionVerdict::refuse(ProtectionReason::SourceControl, None);
        }

        let file_name = paths::file_name_lower(&normalized);
        if self.file_names.contains(&file_name) {
            return ProtectionVerdict::refuse(ProtectionReason::CredentialStore, None);
        }

        let ext = paths::extension_lower(&normalized);
        if !ext.is_empty() && self.extensions.contains(&ext) {
            return ProtectionVerdict::refuse(ProtectionReason::Database, None);
        }

        ProtectionVerdict::allowed()
    }

    /// Convenience predicate.
    pub fn is_protected(&self, path: &Path) -> bool {
        self.classify(path).protected
    }
}

/// Every fixed and removable drive root currently present, e.g. `C:\`, `D:\`.
fn enumerate_drive_roots() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        // `GetLogicalDrives` returns a bitmask, bit 0 is A:. Reading it through
        // the standard library keeps this module free of unsafe code; the
        // volume services use the Win32 API directly where richer data is
        // needed.
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if root.exists() {
                out.push(root);
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}

fn default_protected_file_names() -> HashSet<String> {
    [
        "login data",
        "login data for account",
        "cookies",
        "web data",
        "history",
        "bookmarks",
        "places.sqlite",
        "key3.db",
        "key4.db",
        "cert9.db",
        "logins.json",
        "credentials",
        "id_rsa",
        "id_ed25519",
        "ntuser.dat",
        "usrclass.dat",
        "bootmgr",
        "hiberfil.sys",
        "pagefile.sys",
        "swapfile.sys",
        "desktop.ini",
        ".env",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_protected_extensions() -> HashSet<String> {
    [
        // Databases and mail stores.
        "db", "db3", "sqlite", "sqlite3", "sqlitedb", "mdb", "accdb", "mdf", "ldf", "pst", "ost",
        "edb", "fdb", "realm", "sdf",
        // Keys, certificates and password vaults.
        "kdbx", "kdb", "pem", "key", "pfx", "p12", "ppk", "jks", "keystore", "asc", "gpg",
        // Virtual machines and disk images that are expensive to recreate.
        "vhd", "vhdx", "vmdk", "vdi", "qcow2", "hds",
        // Backups.
        "bak", "bkp", "tib", "wim",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_protected_dir_names() -> HashSet<String> {
    [".git", ".svn", ".hg", ".ssh", ".gnupg", "$recycle.bin", "system volume information"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> ProtectedPaths {
        ProtectedPaths::new(&[])
    }

    #[test]
    fn windows_directory_is_protected() {
        let e = engine();
        assert!(e.is_protected(Path::new("C:\\Windows")));
        assert!(e.is_protected(Path::new("C:\\Windows\\System32\\kernel32.dll")));
        assert!(e.is_protected(Path::new("c:\\windows\\system32")));
    }

    #[test]
    fn program_files_is_protected() {
        let e = engine();
        assert!(e.is_protected(Path::new("C:\\Program Files\\Something\\app.exe")));
        assert!(e.is_protected(Path::new("C:\\Program Files (x86)\\Old\\app.exe")));
        assert!(e.is_protected(Path::new("C:\\ProgramData\\Vendor\\state.bin")));
    }

    #[test]
    fn drive_roots_are_never_targets() {
        let e = engine();
        let v = e.classify(Path::new("C:\\"));
        assert!(v.protected);
        assert_eq!(v.reason, Some(ProtectionReason::DriveRoot));
    }

    #[test]
    fn traversal_out_of_a_temp_dir_is_caught() {
        let e = engine();
        assert!(e.is_protected(Path::new(
            "C:\\Windows\\Temp\\..\\..\\Windows\\System32\\config"
        )));
    }

    #[test]
    fn relative_paths_are_refused() {
        let e = engine();
        let v = e.classify(Path::new("some\\relative\\path.tmp"));
        assert!(v.protected);
        assert_eq!(v.reason, Some(ProtectionReason::NotAbsolute));
    }

    #[test]
    fn database_extensions_are_protected_anywhere() {
        let e = engine();
        assert!(e.is_protected(Path::new("D:\\scratch\\cache\\app.sqlite")));
        assert!(e.is_protected(Path::new("D:\\scratch\\cache\\vault.kdbx")));
        assert!(!e.is_protected(Path::new("D:\\scratch\\cache\\chunk.tmp")));
    }

    #[test]
    fn git_metadata_is_protected() {
        let e = engine();
        assert!(e.is_protected(Path::new("D:\\code\\project\\.git\\objects\\ab\\cd")));
    }

    #[test]
    fn user_configured_roots_apply() {
        let mut e = engine();
        e.set_user_roots(&[PathBuf::from("D:\\Archive")]);
        assert!(e.is_protected(Path::new("D:\\Archive\\2019\\notes.txt")));
        assert!(!e.is_protected(Path::new("D:\\Archived\\notes.txt")));
        e.set_user_roots(&[]);
        assert!(!e.is_protected(Path::new("D:\\Archive\\2019\\notes.txt")));
    }

    #[test]
    fn hostile_names_are_refused() {
        let e = engine();
        let v = e.classify(Path::new("D:\\scratch\\bad\u{202e}name.tmp"));
        assert!(v.protected);
        assert_eq!(v.reason, Some(ProtectionReason::HostileName));
    }

    #[test]
    fn every_refusal_has_a_human_explanation() {
        let e = engine();
        let v = e.classify(Path::new("C:\\Windows\\System32"));
        assert!(v.protected);
        assert!(!v.describe().is_empty());
    }
}

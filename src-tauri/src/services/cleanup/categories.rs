//! Cleanup category definitions.
//!
//! Every category is a compile-time constant: an enum variant, a fixed set of
//! directories, and a fixed matching rule. The frontend can only name a
//! variant, never a path, and the local model can only describe a category it
//! is told about. There is no code path that turns a string into a deletion.
//!
//! Categories are classified as SAFE only when the data they cover is
//! regenerated automatically by the operating system or by the owning
//! application. Anything
//! that represents a decision the user made - a download, a document, an
//! installer they kept on purpose - belongs on the Review screens instead,
//! where nothing is ever removed without an explicit selection.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::services::security::paths;

/// The complete set of operations the cleanup engine can perform.
///
/// This enum is the security boundary described in the product brief: the
/// backend accepts `CleanupCategory::WindowsTemp`, never
/// `execute("del C:\\Windows\\Temp\\*")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupCategory {
    WindowsTemp,
    UserTemp,
    CrashDumps,
    WindowsErrorReporting,
    ThumbnailCache,
    IconCache,
    ShaderCache,
    BrowserCache,
    WindowsUpdateCache,
    DeliveryOptimizationCache,
    ComponentStoreLogs,
    FontCache,
    RecycleBin,
}

pub const ALL_CLEANUP_CATEGORIES: [CleanupCategory; 13] = [
    CleanupCategory::WindowsTemp,
    CleanupCategory::UserTemp,
    CleanupCategory::CrashDumps,
    CleanupCategory::WindowsErrorReporting,
    CleanupCategory::ThumbnailCache,
    CleanupCategory::IconCache,
    CleanupCategory::ShaderCache,
    CleanupCategory::BrowserCache,
    CleanupCategory::WindowsUpdateCache,
    CleanupCategory::DeliveryOptimizationCache,
    CleanupCategory::ComponentStoreLogs,
    CleanupCategory::FontCache,
    CleanupCategory::RecycleBin,
];

/// How a matched entry is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionMode {
    /// Sent to the Recycle Bin, so the user can undo it.
    Recycle,
    /// Removed outright. Reserved for caches that Windows or the owning
    /// application rebuilds on demand, where a Recycle Bin copy would consume
    /// exactly the space the cleanup was meant to reclaim.
    Permanent,
    /// Handled by a dedicated system API (the Windows Recycle Bin, the
    /// freedesktop Trash) rather than by file deletion.
    ShellApi,
}

/// Which entries inside a category root are eligible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchRule {
    /// Every file beneath the root. The root directory itself is never
    /// removed, only emptied.
    AllContents,
    /// Files whose lower-case extension is in the list.
    Extensions(&'static [&'static str]),
    /// Files whose lower-case name starts with one of the prefixes.
    NamePrefixes(&'static [&'static str]),
    /// Not driven by file matching at all.
    ShellManaged,
}

#[derive(Debug, Clone)]
pub struct CategoryDefinition {
    pub id: CleanupCategory,
    pub name: &'static str,
    pub description: &'static str,
    /// Shown verbatim in the confirmation sheet.
    pub what_happens: &'static str,
    pub what_is_untouched: &'static str,
    pub roots: Vec<PathBuf>,
    pub rule: MatchRule,
    pub deletion: DeletionMode,
    pub requires_elevation: bool,
    /// Whether Auto-Clean may run this category unattended. Only categories
    /// whose contents Windows rebuilds without user involvement qualify.
    pub auto_clean_eligible: bool,
    /// Entries younger than this are left alone, because something may still
    /// be using them.
    pub min_age_hours: u64,
    /// Cache filename prefixes exempted from the blanket database-extension
    /// rule. Empty for every category that does not need one.
    pub name_exemptions: &'static [&'static str],
}

impl CategoryDefinition {
    /// True when at least one of the category's directories exists on this
    /// machine. Categories with no roots are hidden rather than shown empty.
    pub fn is_present(&self) -> bool {
        self.roots.iter().any(|r| r.exists()) || self.rule == MatchRule::ShellManaged
    }
}

#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    dirs::data_local_dir()
}

fn existing(candidates: Vec<Option<PathBuf>>) -> Vec<PathBuf> {
    candidates.into_iter().flatten().collect()
}

/// Directory names that hold the disposable half of a Chromium profile. The
/// profile also holds passwords, cookies and history in sibling files, which
/// is why only these exact subfolders are ever listed as roots. Written as
/// components so the same list works with either path separator.
const CHROMIUM_CACHE_DIRS: &[&[&str]] = &[&["Cache", "Cache_Data"], &["Code Cache"], &["GPUCache"]];

/// Every Chromium cache directory that exists beneath `user_data`.
fn chromium_cache_dirs(user_data: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for profile in chromium_profile_dirs(user_data) {
        for cache in CHROMIUM_CACHE_DIRS {
            let mut dir = profile.clone();
            dir.extend(cache.iter());
            if dir.exists() {
                out.push(dir);
            }
        }
    }
    out
}

/// Chromium keeps one directory per profile: `Default`, then `Profile 1` and
/// upwards. They are enumerated rather than assumed so a second profile is not
/// silently left uncleaned.
fn chromium_profile_dirs(user_data: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(user_data) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            let name = paths::file_name_lower(p);
            name == "default" || name.starts_with("profile ")
        })
        .collect()
}

/// Firefox stores its cache outside the profile that holds bookmarks and
/// logins, so the cache directory can be listed directly.
fn firefox_cache_dirs(profiles: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(profiles) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path().join("cache2"))
        .filter(|p| p.exists())
        .collect()
}

#[cfg(windows)]
fn browser_cache_roots() -> Vec<PathBuf> {
    let Some(local) = local_app_data() else {
        return Vec::new();
    };
    let mut roots = Vec::new();

    for browser in [
        "Google\\Chrome\\User Data",
        "Microsoft\\Edge\\User Data",
        "BraveSoftware\\Brave-Browser\\User Data",
        "Vivaldi\\User Data",
        "Chromium\\User Data",
    ] {
        roots.extend(chromium_cache_dirs(&local.join(browser)));
    }

    roots.extend(firefox_cache_dirs(
        &local.join("Mozilla\\Firefox\\Profiles"),
    ));
    roots
}

/// Build every category definition for this machine.
pub fn definitions() -> Vec<CategoryDefinition> {
    platform_definitions()
}

#[cfg(windows)]
// One push per category reads as a catalogue; a vec! literal would not.
#[allow(clippy::vec_init_then_push)]
fn platform_definitions() -> Vec<CategoryDefinition> {
    let local = local_app_data();
    let mut out = Vec::new();

    out.push(CategoryDefinition {
        id: CleanupCategory::WindowsTemp,
        name: "Windows temporary files",
        description: "Files left in the shared Windows temporary folder by installers and system components.",
        what_happens: "Temporary files in the Windows temporary folder are deleted permanently. Windows recreates this folder's contents as needed.",
        what_is_untouched: "Nothing outside the Windows temporary folder. No documents, downloads, or installed programs.",
        roots: existing(vec![paths::expand_env("%SystemRoot%\\Temp")]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        // Many entries here belong to SYSTEM. Without elevation the readable
        // subset is still cleaned and the rest is reported as skipped.
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 24,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::UserTemp,
        name: "Application temporary files",
        description: "Scratch files written by applications to your personal temporary folder.",
        what_happens: "Temporary files older than a day are deleted permanently. Applications recreate them when they next need scratch space.",
        what_is_untouched: "Files created in the last day, in case a program is still using them. Nothing outside your temporary folder.",
        roots: existing(vec![
            paths::expand_env("%TEMP%"),
            local.as_ref().map(|l| l.join("Temp")),
        ]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 24,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::CrashDumps,
        name: "Crash dumps",
        description: "Memory snapshots written when an application stopped responding.",
        what_happens: "Saved crash dumps are deleted permanently. They are only useful to a developer diagnosing a crash that already happened.",
        what_is_untouched: "Application settings and data. Only the dump files themselves are removed.",
        roots: existing(vec![
            local.as_ref().map(|l| l.join("CrashDumps")),
            paths::expand_env("%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportArchive"),
        ]),
        rule: MatchRule::Extensions(&["dmp", "mdmp", "hdmp", "wer"]),
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 24,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::WindowsErrorReporting,
        name: "Windows error reports",
        description: "Queued and archived reports about application faults.",
        what_happens: "Archived error reports are deleted permanently. Windows regenerates them the next time a program faults.",
        what_is_untouched: "Everything outside the error reporting folders.",
        roots: existing(vec![
            paths::expand_env("%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportQueue"),
            paths::expand_env("%ProgramData%\\Microsoft\\Windows\\WER\\ReportArchive"),
            paths::expand_env("%ProgramData%\\Microsoft\\Windows\\WER\\ReportQueue"),
        ]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: true,
        auto_clean_eligible: true,
        min_age_hours: 24,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::ThumbnailCache,
        name: "Thumbnail cache",
        description: "Preview images Explorer keeps so folders open quickly.",
        what_happens: "Thumbnail cache files are deleted permanently. Explorer rebuilds them the next time you browse a folder, which can make the first visit slightly slower.",
        what_is_untouched: "Your actual pictures and videos. Only the generated previews are removed.",
        roots: existing(vec![
            paths::expand_env("%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer")
        ]),
        rule: MatchRule::NamePrefixes(&["thumbcache_"]),
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &["thumbcache_"],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::IconCache,
        name: "Icon cache",
        description: "Cached application icons used by the taskbar and Start menu.",
        what_happens:
            "Icon cache files are deleted permanently. Windows rebuilds them automatically.",
        what_is_untouched: "Installed applications and their shortcuts.",
        roots: existing(vec![paths::expand_env(
            "%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer",
        )]),
        rule: MatchRule::NamePrefixes(&["iconcache_"]),
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &["iconcache_"],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::ShaderCache,
        name: "Graphics shader cache",
        description: "Compiled shaders kept by DirectX and by your graphics driver.",
        what_happens: "Shader caches are deleted permanently. Games and applications recompile them on first launch, which can add a short delay once.",
        what_is_untouched: "Game installations, saves and settings.",
        roots: existing(vec![
            local.as_ref().map(|l| l.join("D3DSCache")),
            local.as_ref().map(|l| l.join("NVIDIA\\DXCache")),
            local.as_ref().map(|l| l.join("NVIDIA\\GLCache")),
            local.as_ref().map(|l| l.join("AMD\\DxCache")),
            local.as_ref().map(|l| l.join("AMD\\DxcCache")),
            local.as_ref().map(|l| l.join("AMD\\GLCache")),
            local.as_ref().map(|l| l.join("Intel\\ShaderCache")),
        ]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::BrowserCache,
        name: "Browser cache",
        description: "Cached web pages and images. Sign-ins, history and bookmarks are stored separately and are not touched.",
        what_happens: "Cached page data is deleted permanently. Sites will be fetched fresh on your next visit.",
        what_is_untouched: "Passwords, cookies, history, bookmarks, extensions and open tabs. Only the cache subfolders are cleaned.",
        roots: browser_cache_roots(),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        // Cleaning a cache under a running browser can make it rewrite the
        // whole cache immediately, so this one waits for a deliberate choice.
        auto_clean_eligible: false,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::WindowsUpdateCache,
        name: "Windows Update cache",
        description: "Installer payloads kept after updates were applied.",
        what_happens: "Downloaded update packages are deleted permanently. Windows re-downloads anything it still needs.",
        what_is_untouched: "Installed updates themselves, and your ability to use Windows Update normally.",
        roots: existing(vec![paths::expand_env("%SystemRoot%\\SoftwareDistribution\\Download")]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: true,
        // Removing update payloads while an update is mid-flight is the kind
        // of surprise Auto-Clean must never spring on someone.
        auto_clean_eligible: false,
        min_age_hours: 72,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::DeliveryOptimizationCache,
        name: "Delivery Optimization cache",
        description: "Update fragments Windows keeps to share with other machines on your network.",
        what_happens: "Cached update fragments are deleted permanently. Windows rebuilds the cache as new updates arrive.",
        what_is_untouched: "Installed updates and network settings.",
        roots: existing(vec![paths::expand_env(
            "%ProgramData%\\Microsoft\\Windows\\DeliveryOptimization\\Cache",
        )]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: true,
        auto_clean_eligible: false,
        min_age_hours: 72,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::ComponentStoreLogs,
        name: "Servicing logs",
        description: "Text logs written while Windows installs components.",
        what_happens:
            "Old servicing logs are deleted permanently. Windows writes new ones as needed.",
        what_is_untouched: "The component store itself and every installed component.",
        roots: existing(vec![paths::expand_env("%SystemRoot%\\Logs\\CBS")]),
        rule: MatchRule::Extensions(&["log", "cab", "etl"]),
        deletion: DeletionMode::Permanent,
        requires_elevation: true,
        auto_clean_eligible: false,
        min_age_hours: 168,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::FontCache,
        name: "Font cache",
        description: "Rendered font data cached by Windows.",
        what_happens: "Font cache files are deleted permanently and rebuilt automatically.",
        what_is_untouched: "Installed fonts themselves.",
        roots: existing(vec![paths::expand_env(
            "%LOCALAPPDATA%\\Microsoft\\Windows\\INetCache\\fontcache",
        )]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::RecycleBin,
        name: "Recycle Bin",
        description: "Items you already deleted, still recoverable until the bin is emptied.",
        what_happens:
            "The Recycle Bin is emptied through Windows. After this, the items cannot be restored.",
        what_is_untouched: "Everything that is not already in the Recycle Bin.",
        roots: Vec::new(),
        rule: MatchRule::ShellManaged,
        deletion: DeletionMode::ShellApi,
        requires_elevation: false,
        // Emptying the bin is not reversible, so it always waits for an
        // explicit confirmation, even when Auto-Clean is enabled.
        auto_clean_eligible: false,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out
}

/// Browser caches on Linux all live under `~/.cache`, which holds nothing
/// but regenerable data. Snap Firefox is the exception, with its own cache
/// inside `~/snap`, reached through the one carve-out the protected list has
/// on Linux.
#[cfg(not(windows))]
fn browser_cache_roots() -> Vec<PathBuf> {
    let Some(cache) = dirs::cache_dir() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    for browser in [
        "google-chrome",
        "google-chrome-beta",
        "chromium",
        "microsoft-edge",
        "BraveSoftware/Brave-Browser",
        "vivaldi",
        "opera",
    ] {
        let mut user_data = cache.clone();
        user_data.extend(browser.split('/'));
        roots.extend(chromium_cache_dirs(&user_data));
    }
    roots.extend(firefox_cache_dirs(&cache.join("mozilla").join("firefox")));
    if let Some(home) = dirs::home_dir() {
        roots.extend(firefox_cache_dirs(
            &home.join("snap/firefox/common/.cache/mozilla/firefox"),
        ));
    }
    roots
}

/// Linux categories. Every root is inside the user's own home or temporary
/// folder: AllInsight runs unprivileged and never offers to clean anything
/// that belongs to the distribution. Package caches such as `/var/cache/apt`
/// are left to the package manager, which knows what it still needs.
#[cfg(not(windows))]
// One push per category reads as a catalogue; a vec! literal would not.
#[allow(clippy::vec_init_then_push)]
fn platform_definitions() -> Vec<CategoryDefinition> {
    let cache = dirs::cache_dir();
    let in_cache = |leaf: &str| {
        cache.as_ref().map(|c| {
            let mut p = c.clone();
            p.extend(leaf.split('/'));
            p
        })
    };
    let mut out = Vec::new();

    out.push(CategoryDefinition {
        id: CleanupCategory::UserTemp,
        name: "Temporary files",
        description: "Scratch files that programs you ran left in the temporary folder.",
        what_happens: "Your own temporary files that have not changed for three days are deleted permanently. Programs recreate them when they next need scratch space.",
        what_is_untouched: "Files belonging to other users or to the system, sockets and other special files, and anything changed in the last three days.",
        roots: existing(vec![Some(std::env::temp_dir())]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 72,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::ThumbnailCache,
        name: "Thumbnail cache",
        description: "Preview images your file manager keeps so folders open quickly.",
        what_happens: "Thumbnail files are deleted permanently. They are rebuilt the next time you browse a folder, which can make the first visit slightly slower.",
        what_is_untouched: "Your actual pictures and videos. Only the generated previews are removed.",
        roots: existing(vec![in_cache("thumbnails")]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::ShaderCache,
        name: "Graphics shader cache",
        description: "Compiled shaders kept by Mesa and by the NVIDIA driver.",
        what_happens: "Shader caches are deleted permanently. Games and applications recompile them on first launch, which can add a short delay once.",
        what_is_untouched: "Game installations, saves and settings.",
        roots: existing(vec![
            in_cache("mesa_shader_cache"),
            in_cache("mesa_shader_cache_db"),
            in_cache("nvidia/GLCache"),
        ]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        // Mesa's single-file cache is `mesa_cache.db`.
        name_exemptions: &["mesa_cache"],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::BrowserCache,
        name: "Browser cache",
        description: "Cached web pages and images. Sign-ins, history and bookmarks are stored separately and are not touched.",
        what_happens: "Cached page data is deleted permanently. Sites will be fetched fresh on your next visit.",
        what_is_untouched: "Passwords, cookies, history, bookmarks, extensions and open tabs. Only the cache folders are cleaned.",
        roots: browser_cache_roots(),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: false,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out.push(CategoryDefinition {
        id: CleanupCategory::FontCache,
        name: "Font cache",
        description: "Font lists cached by fontconfig.",
        what_happens: "Font cache files are deleted permanently and rebuilt automatically the next time a program starts.",
        what_is_untouched: "Installed fonts themselves.",
        roots: existing(vec![in_cache("fontconfig")]),
        rule: MatchRule::AllContents,
        deletion: DeletionMode::Permanent,
        requires_elevation: false,
        auto_clean_eligible: true,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    #[cfg(not(target_os = "macos"))]
    out.push(CategoryDefinition {
        id: CleanupCategory::RecycleBin,
        name: "Trash",
        description: "Items you already deleted, still recoverable until the Trash is emptied.",
        what_happens: "The Trash is emptied. After this, the items cannot be restored.",
        what_is_untouched: "Everything that is not already in the Trash.",
        roots: Vec::new(),
        rule: MatchRule::ShellManaged,
        deletion: DeletionMode::ShellApi,
        requires_elevation: false,
        auto_clean_eligible: false,
        min_age_hours: 0,
        name_exemptions: &[],
    });

    out
}

/// Look up one definition.
pub fn definition_for(category: CleanupCategory) -> Option<CategoryDefinition> {
    definitions().into_iter().find(|d| d.id == category)
}

#[cfg(test)]
// The platform is a compile-time constant here on purpose.
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;
    use crate::services::security::ProtectedPaths;

    #[cfg(windows)]
    #[test]
    fn every_variant_has_exactly_one_definition() {
        let defs = definitions();
        for variant in ALL_CLEANUP_CATEGORIES {
            let matches = defs.iter().filter(|d| d.id == variant).count();
            assert_eq!(matches, 1, "{variant:?} must have one definition");
        }
        assert_eq!(defs.len(), ALL_CLEANUP_CATEGORIES.len());
    }

    /// Elsewhere only the categories that exist as a concept are defined, and
    /// none of the Windows-only ones may leak through.
    #[cfg(not(windows))]
    #[test]
    fn only_portable_categories_are_defined_once_each() {
        let defs = definitions();
        for d in &defs {
            assert_eq!(defs.iter().filter(|o| o.id == d.id).count(), 1);
            assert!(!matches!(
                d.id,
                CleanupCategory::WindowsTemp
                    | CleanupCategory::WindowsErrorReporting
                    | CleanupCategory::WindowsUpdateCache
                    | CleanupCategory::DeliveryOptimizationCache
                    | CleanupCategory::ComponentStoreLogs
                    | CleanupCategory::IconCache
                    | CleanupCategory::CrashDumps
            ));
            for root in &d.roots {
                assert!(root.is_absolute());
                assert!(!root.to_string_lossy().contains('\\'), "{}", root.display());
            }
        }
    }

    #[test]
    fn every_category_explains_itself() {
        for d in definitions() {
            assert!(!d.name.is_empty());
            assert!(!d.description.is_empty());
            assert!(!d.what_happens.is_empty());
            assert!(!d.what_is_untouched.is_empty());
        }
    }

    /// The central safety property: no category may be pointed at a protected
    /// location unless that exact directory is one of the compiled-in
    /// carve-outs.
    #[test]
    fn no_category_root_reaches_protected_storage() {
        let protected = ProtectedPaths::new(&[]);
        for d in definitions() {
            for root in &d.roots {
                let verdict = protected.classify(root);
                if verdict.protected {
                    assert!(
                        protected.is_carve_out(root),
                        "{:?} declares protected root {} which is not a carve-out",
                        d.id,
                        root.display()
                    );
                }
            }
        }
    }

    /// Auto-Clean is the unattended path, so its eligible set must stay
    /// narrow: nothing that needs elevation, nothing irreversible, and nothing
    /// that touches user-chosen files.
    #[test]
    fn auto_clean_is_limited_to_regenerable_data() {
        for d in definitions() {
            if !d.auto_clean_eligible {
                continue;
            }
            assert_ne!(
                d.deletion,
                DeletionMode::ShellApi,
                "{:?} must not run unattended",
                d.id
            );
            assert!(
                matches!(
                    d.id,
                    CleanupCategory::WindowsTemp
                        | CleanupCategory::UserTemp
                        | CleanupCategory::CrashDumps
                        | CleanupCategory::WindowsErrorReporting
                        | CleanupCategory::ThumbnailCache
                        | CleanupCategory::IconCache
                        | CleanupCategory::ShaderCache
                        | CleanupCategory::FontCache
                ),
                "{:?} is not on the reviewed Auto-Clean list",
                d.id
            );
        }
    }

    #[test]
    fn the_recycle_bin_is_never_automatic() {
        // macOS has no Trash category at all, which trivially satisfies this.
        let Some(d) = definition_for(CleanupCategory::RecycleBin) else {
            assert!(cfg!(target_os = "macos"));
            return;
        };
        assert!(!d.auto_clean_eligible);
        assert_eq!(d.deletion, DeletionMode::ShellApi);
    }

    #[test]
    fn name_exemptions_are_declared_only_where_the_rule_needs_them() {
        for d in definitions() {
            for exemption in d.name_exemptions {
                match &d.rule {
                    MatchRule::NamePrefixes(prefixes) => assert!(
                        prefixes.contains(exemption),
                        "{:?} exempts {exemption} without matching it",
                        d.id
                    ),
                    // Mesa's shader cache is a folder of regenerable files,
                    // one of which is named `mesa_cache.db`. It is the only
                    // category allowed an exemption under a blanket rule.
                    MatchRule::AllContents if d.id == CleanupCategory::ShaderCache => {}
                    other => panic!("{:?} exempts {exemption} under rule {other:?}", d.id),
                }
            }
        }
    }

    #[test]
    fn temporary_categories_wait_before_deleting() {
        for id in [CleanupCategory::WindowsTemp, CleanupCategory::UserTemp] {
            let Some(d) = definition_for(id) else {
                assert!(!cfg!(windows), "{id:?} must exist on Windows");
                continue;
            };
            assert!(d.min_age_hours >= 24, "{id:?} must leave fresh files alone");
        }
    }
}

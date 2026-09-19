//! Classifying a file into the buckets the Storage screen shows.
//!
//! Classification is by location first and by extension second, because a
//! `.mp4` inside a game installation is part of that game, not part of the
//! user's video library.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::services::security::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageCategory {
    Applications,
    Windows,
    Downloads,
    Desktop,
    Documents,
    Pictures,
    Videos,
    Music,
    TemporaryFiles,
    Cache,
    Games,
    Development,
    VirtualMachines,
    Archives,
    UserFiles,
    Other,
}

/// Fixed order, used for the per-category totals array and for display.
pub const ALL_CATEGORIES: [StorageCategory; 16] = [
    StorageCategory::Applications,
    StorageCategory::Windows,
    StorageCategory::Downloads,
    StorageCategory::Desktop,
    StorageCategory::Documents,
    StorageCategory::Pictures,
    StorageCategory::Videos,
    StorageCategory::Music,
    StorageCategory::TemporaryFiles,
    StorageCategory::Cache,
    StorageCategory::Games,
    StorageCategory::Development,
    StorageCategory::VirtualMachines,
    StorageCategory::Archives,
    StorageCategory::UserFiles,
    StorageCategory::Other,
];

impl StorageCategory {
    pub fn index(&self) -> usize {
        ALL_CATEGORIES.iter().position(|c| c == self).unwrap_or(15)
    }

    pub fn label(&self) -> &'static str {
        match self {
            StorageCategory::Applications => "Applications",
            StorageCategory::Windows => "Windows",
            StorageCategory::Downloads => "Downloads",
            StorageCategory::Desktop => "Desktop",
            StorageCategory::Documents => "Documents",
            StorageCategory::Pictures => "Pictures",
            StorageCategory::Videos => "Videos",
            StorageCategory::Music => "Music",
            StorageCategory::TemporaryFiles => "Temporary files",
            StorageCategory::Cache => "Cache",
            StorageCategory::Games => "Games",
            StorageCategory::Development => "Development",
            StorageCategory::VirtualMachines => "Virtual machines",
            StorageCategory::Archives => "Archives",
            StorageCategory::UserFiles => "User files",
            StorageCategory::Other => "Other",
        }
    }
}

/// Known-folder anchors resolved once, then reused for every file in a scan.
#[derive(Debug, Clone)]
pub struct CategoryRules {
    windows: Option<std::path::PathBuf>,
    program_files: Vec<std::path::PathBuf>,
    downloads: Option<std::path::PathBuf>,
    desktop: Option<std::path::PathBuf>,
    documents: Option<std::path::PathBuf>,
    pictures: Option<std::path::PathBuf>,
    videos: Option<std::path::PathBuf>,
    music: Option<std::path::PathBuf>,
    home: Option<std::path::PathBuf>,
}

impl Default for CategoryRules {
    fn default() -> Self {
        Self::new()
    }
}

impl CategoryRules {
    pub fn new() -> Self {
        let mut program_files: Vec<std::path::PathBuf> = Vec::new();
        for var in ["%ProgramFiles%", "%ProgramFiles(x86)%", "%ProgramW6432%"] {
            if let Some(p) = paths::expand_env(var) {
                if !program_files.iter().any(|e| paths::same_path(e, &p)) {
                    program_files.push(p);
                }
            }
        }
        if let Some(p) = paths::expand_env("%ProgramData%") {
            program_files.push(p);
        }
        Self {
            windows: paths::expand_env("%SystemRoot%"),
            program_files,
            downloads: dirs::download_dir(),
            desktop: dirs::desktop_dir(),
            documents: dirs::document_dir(),
            pictures: dirs::picture_dir(),
            videos: dirs::video_dir(),
            music: dirs::audio_dir(),
            home: dirs::home_dir(),
        }
    }

    /// Classify one file. `path` is expected to be absolute and normalised.
    pub fn classify(&self, path: &Path) -> StorageCategory {
        if let Some(c) = self.classify_by_location(path) {
            return c;
        }
        if let Some(c) = classify_by_component(path) {
            return c;
        }
        if let Some(c) = classify_by_extension(&paths::extension_lower(path)) {
            return c;
        }
        if let Some(home) = &self.home {
            if paths::is_within(path, home) {
                return StorageCategory::UserFiles;
            }
        }
        StorageCategory::Other
    }

    fn classify_by_location(&self, path: &Path) -> Option<StorageCategory> {
        if let Some(w) = &self.windows {
            if paths::is_within(path, w) {
                return Some(StorageCategory::Windows);
            }
        }
        for pf in &self.program_files {
            if paths::is_within(path, pf) {
                return Some(StorageCategory::Applications);
            }
        }
        for (root, cat) in [
            (&self.downloads, StorageCategory::Downloads),
            (&self.desktop, StorageCategory::Desktop),
            (&self.documents, StorageCategory::Documents),
            (&self.pictures, StorageCategory::Pictures),
            (&self.videos, StorageCategory::Videos),
            (&self.music, StorageCategory::Music),
        ] {
            if let Some(r) = root {
                if paths::is_within(path, r) {
                    return Some(cat);
                }
            }
        }
        None
    }
}

/// Directory names that decide the category regardless of extension.
fn classify_by_component(path: &Path) -> Option<StorageCategory> {
    let lowered = path.to_string_lossy().to_lowercase();
    let has = |needle: &str| lowered.contains(needle);

    if has("\\steamapps") || has("\\epic games") || has("\\gog galaxy") || has("\\riot games") {
        return Some(StorageCategory::Games);
    }
    if has("\\node_modules")
        || has("\\.cargo")
        || has("\\.gradle")
        || has("\\.nuget")
        || has("\\.m2")
        || has("\\site-packages")
        || has("\\.venv")
    {
        return Some(StorageCategory::Development);
    }
    if has("\\temp\\") || has("\\tmp\\") || lowered.ends_with(".tmp") {
        return Some(StorageCategory::TemporaryFiles);
    }
    if has("\\cache") || has("cache\\") || has("\\code cache") || has("\\gpucache") {
        return Some(StorageCategory::Cache);
    }
    None
}

fn classify_by_extension(ext: &str) -> Option<StorageCategory> {
    const VIDEO: &[&str] = &[
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "ts", "m2ts",
    ];
    const IMAGE: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "raw", "cr2", "nef",
        "arw", "dng", "psd", "svg",
    ];
    const AUDIO: &[&str] = &["mp3", "flac", "wav", "aac", "m4a", "ogg", "wma", "opus", "aiff"];
    const ARCHIVE: &[&str] = &[
        "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "img", "cab", "msi", "msix", "appx",
    ];
    const VM: &[&str] = &["vhd", "vhdx", "vmdk", "vdi", "qcow2", "hds", "ova", "ovf"];
    const DOC: &[&str] = &[
        "doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf", "odt", "ods", "odp", "rtf", "txt",
        "md", "csv", "epub",
    ];
    const CODE: &[&str] = &[
        "rs", "c", "h", "cpp", "hpp", "cs", "java", "kt", "py", "js", "ts", "tsx", "jsx", "go",
        "rb", "php", "swift", "sql", "sh", "ps1", "toml", "yaml", "yml", "gradle",
    ];
    const CACHE: &[&str] = &["log", "etl", "dmp", "chk", "old", "bak~"];

    if VIDEO.contains(&ext) {
        Some(StorageCategory::Videos)
    } else if IMAGE.contains(&ext) {
        Some(StorageCategory::Pictures)
    } else if AUDIO.contains(&ext) {
        Some(StorageCategory::Music)
    } else if VM.contains(&ext) {
        Some(StorageCategory::VirtualMachines)
    } else if ARCHIVE.contains(&ext) {
        Some(StorageCategory::Archives)
    } else if DOC.contains(&ext) {
        Some(StorageCategory::Documents)
    } else if CODE.contains(&ext) {
        Some(StorageCategory::Development)
    } else if CACHE.contains(&ext) {
        Some(StorageCategory::Cache)
    } else {
        None
    }
}

/// Per-category byte and file totals, rolled up across a whole scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryTotals {
    pub bytes: [u64; 16],
    pub files: [u64; 16],
}

impl CategoryTotals {
    pub fn add(&mut self, category: StorageCategory, size: u64) {
        let i = category.index();
        self.bytes[i] = self.bytes[i].saturating_add(size);
        self.files[i] = self.files[i].saturating_add(1);
    }

    pub fn merge(&mut self, other: &CategoryTotals) {
        for i in 0..16 {
            self.bytes[i] = self.bytes[i].saturating_add(other.bytes[i]);
            self.files[i] = self.files[i].saturating_add(other.files[i]);
        }
    }

    /// Sorted largest-first, dropping empty categories.
    pub fn ranked(&self) -> Vec<CategoryTotal> {
        let mut out: Vec<CategoryTotal> = ALL_CATEGORIES
            .iter()
            .enumerate()
            .filter(|(i, _)| self.bytes[*i] > 0)
            .map(|(i, c)| CategoryTotal {
                category: *c,
                label: c.label().to_string(),
                bytes: self.bytes[i],
                files: self.files[i],
            })
            .collect();
        out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryTotal {
    pub category: StorageCategory,
    pub label: String,
    pub bytes: u64,
    pub files: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_map_to_the_expected_buckets() {
        assert_eq!(classify_by_extension("mkv"), Some(StorageCategory::Videos));
        assert_eq!(classify_by_extension("vhdx"), Some(StorageCategory::VirtualMachines));
        assert_eq!(classify_by_extension("iso"), Some(StorageCategory::Archives));
        assert_eq!(classify_by_extension("zzz"), None);
    }

    #[test]
    fn game_and_dev_folders_win_over_extension() {
        assert_eq!(
            classify_by_component(Path::new("D:\\SteamLibrary\\steamapps\\common\\Game\\intro.mp4")),
            Some(StorageCategory::Games)
        );
        assert_eq!(
            classify_by_component(Path::new("D:\\code\\app\\node_modules\\pkg\\logo.png")),
            Some(StorageCategory::Development)
        );
    }

    #[test]
    fn totals_roll_up_and_rank() {
        let mut a = CategoryTotals::default();
        a.add(StorageCategory::Videos, 100);
        a.add(StorageCategory::Cache, 10);
        let mut b = CategoryTotals::default();
        b.add(StorageCategory::Cache, 500);
        a.merge(&b);

        let ranked = a.ranked();
        assert_eq!(ranked[0].category, StorageCategory::Cache);
        assert_eq!(ranked[0].bytes, 510);
        assert_eq!(ranked[0].files, 2);
        assert_eq!(ranked.len(), 2);
    }

    #[test]
    fn every_category_has_a_stable_index() {
        for (i, c) in ALL_CATEGORIES.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
    }
}

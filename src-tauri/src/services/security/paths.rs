//! Windows path primitives used by every safety decision in AllInsight.
//!
//! Three rules drive this module:
//!
//! 1. Ancestry is decided component-by-component, never with string
//!    `starts_with`. `C:\Users\Bob` is not an ancestor of `C:\Users\Bobby`.
//! 2. Comparison is case-insensitive because NTFS is, but the original casing
//!    is always preserved for display.
//! 3. Reparse points (symlinks, junctions, volume mount points) are detected
//!    and refused rather than followed. A junction planted inside a temp
//!    directory that points at Documents is the attack this defends against.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

/// `FILE_ATTRIBUTE_REPARSE_POINT` from winnt.h.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Lower-cased string form of a single path component, used for comparison.
fn component_key(c: &Component<'_>) -> String {
    match c {
        Component::Prefix(p) => p.as_os_str().to_string_lossy().to_lowercase(),
        Component::RootDir => "\\".to_string(),
        Component::CurDir => ".".to_string(),
        Component::ParentDir => "..".to_string(),
        Component::Normal(s) => s.to_string_lossy().to_lowercase(),
    }
}

/// Comparison key for a whole path: the sequence of lower-cased components.
pub fn comparison_key(path: &Path) -> Vec<String> {
    strip_verbatim(path)
        .components()
        .map(|c| component_key(&c))
        .collect()
}

/// Remove a verbatim prefix so that paths produced by `fs::canonicalize`
/// compare equal to paths typed by a human.
pub fn strip_verbatim(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix("\\\\?\\UNC\\") {
        PathBuf::from(format!("\\\\{rest}"))
    } else if let Some(rest) = s.strip_prefix("\\\\?\\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

/// Add the verbatim prefix so Win32 calls are not capped at MAX_PATH (260).
/// Only meaningful for already-absolute, already-normalised paths.
pub fn long_path(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with("\\\\?\\") {
        return path.to_path_buf();
    }
    if let Some(rest) = s.strip_prefix("\\\\") {
        return PathBuf::from(format!("\\\\?\\UNC\\{rest}"));
    }
    if path.is_absolute() {
        return PathBuf::from(format!("\\\\?\\{s}"));
    }
    path.to_path_buf()
}

/// Resolve `.` and `..` purely lexically, without touching the disk.
///
/// Used before any filesystem call so a traversal payload such as
/// `C:\Windows\Temp\..\..\Users\Me\Documents` is collapsed to its real target
/// and can then be tested against the protected list.
pub fn normalize_lexical(path: &Path) -> PathBuf {
    let stripped = strip_verbatim(path);
    let mut out: Vec<Component<'_>> = Vec::new();
    for c in stripped.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // `..` above the root is meaningless; drop it rather than
                // letting it escape.
                _ => {}
            },
            other => out.push(other),
        }
    }
    let mut buf = PathBuf::new();
    for c in out {
        buf.push(c.as_os_str());
    }
    // `PathBuf::push` of a bare prefix loses the separator: fix `C:` to `C:\`.
    let s = buf.as_os_str().to_string_lossy().to_string();
    if s.len() == 2 && s.ends_with(':') {
        return PathBuf::from(format!("{s}\\"));
    }
    buf
}

/// True when `candidate` is `ancestor` itself or lives beneath it.
///
/// Both sides are compared component-wise and case-insensitively, so
/// `C:\Users\Bobby` is not within `C:\Users\Bob`.
pub fn is_within(candidate: &Path, ancestor: &Path) -> bool {
    let c = comparison_key(&normalize_lexical(candidate));
    let a = comparison_key(&normalize_lexical(ancestor));
    if a.is_empty() || c.len() < a.len() {
        return false;
    }
    c[..a.len()] == a[..]
}

/// True when `candidate` lives strictly beneath `ancestor`.
pub fn is_strictly_within(candidate: &Path, ancestor: &Path) -> bool {
    let c = comparison_key(&normalize_lexical(candidate));
    let a = comparison_key(&normalize_lexical(ancestor));
    if a.is_empty() || c.len() <= a.len() {
        return false;
    }
    c[..a.len()] == a[..]
}

/// True when the two paths refer to the same location.
pub fn same_path(a: &Path, b: &Path) -> bool {
    comparison_key(&normalize_lexical(a)) == comparison_key(&normalize_lexical(b))
}

/// Canonicalise through the filesystem, then drop the verbatim prefix.
///
/// Fails when the path does not exist, which is deliberate: cleanup only ever
/// operates on entries that are present right now.
pub fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    let real = std::fs::canonicalize(path)?;
    Ok(strip_verbatim(&real))
}

/// Tri-state answer to "is this a reparse point", because "we could not tell"
/// must not silently collapse into "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparseState {
    Plain,
    Reparse,
    Unknown,
}

#[cfg(windows)]
pub fn reparse_state(path: &Path) -> ReparseState {
    match std::fs::symlink_metadata(long_path(path)) {
        Ok(m) => {
            if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                ReparseState::Reparse
            } else {
                ReparseState::Plain
            }
        }
        Err(_) => ReparseState::Unknown,
    }
}

#[cfg(not(windows))]
pub fn reparse_state(path: &Path) -> ReparseState {
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            if m.file_type().is_symlink() {
                ReparseState::Reparse
            } else {
                ReparseState::Plain
            }
        }
        Err(_) => ReparseState::Unknown,
    }
}

/// True when this exact entry is a reparse point. Does not follow the link.
/// A path that cannot be inspected returns `false`; callers that need
/// certainty use [`reparse_state`] instead.
pub fn is_reparse_point(path: &Path) -> bool {
    reparse_state(path) == ReparseState::Reparse
}

/// Walk from the drive root down to `path` and report the first component that
/// is a reparse point. `boundary`, when given, stops the walk: callers that
/// already trust a root do not need to re-check it on every candidate.
pub fn first_reparse_ancestor(path: &Path, boundary: Option<&Path>) -> Option<PathBuf> {
    let normalized = normalize_lexical(path);
    let mut current = PathBuf::new();
    let mut past_boundary = boundary.is_none();

    for component in normalized.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        if !past_boundary {
            if let Some(b) = boundary {
                if same_path(&current, b) {
                    past_boundary = true;
                }
            }
            continue;
        }
        if reparse_state(&current) == ReparseState::Reparse {
            return Some(current);
        }
    }
    None
}

/// Expand a Windows environment string of the form `%VAR%\sub\dir`. Unknown
/// variables cause the whole expansion to fail rather than silently producing
/// a path with a literal `%VAR%` component in it.
pub fn expand_env(template: &str) -> Option<PathBuf> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        let value = std::env::var(name).ok()?;
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    if out.is_empty() {
        None
    } else {
        Some(normalize_lexical(Path::new(&out)))
    }
}

/// Lower-cased file extension, or an empty string.
pub fn extension_lower(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Lower-cased final component, or an empty string.
pub fn file_name_lower(path: &Path) -> String {
    path.file_name()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// True when any component of the path matches one of `names` (lower-case).
pub fn contains_component(path: &Path, names: &HashSet<String>) -> bool {
    normalize_lexical(path)
        .components()
        .any(|c| matches!(c, Component::Normal(_)) && names.contains(&component_key(&c)))
}

/// True for the invisible bidirectional and formatting characters used to
/// disguise a filename, e.g. `invoice\u{202E}cod.exe` rendering as
/// `invoiceexe.doc`. These are Unicode category Cf, not Cc, so
/// `char::is_control` does not catch them.
fn is_bidi_or_format_control(c: char) -> bool {
    matches!(c,
        '\u{00AD}'              // soft hyphen
        | '\u{200B}'..='\u{200F}' // zero width space through RLM
        | '\u{202A}'..='\u{202E}' // embedding and override
        | '\u{2060}'..='\u{2064}' // word joiner and invisible operators
        | '\u{2066}'..='\u{2069}' // isolates
        | '\u{FEFF}'              // zero width no-break space
        | '\u{FFF9}'..='\u{FFFB}' // interlinear annotation
    )
}

/// A filename is hostile when it contains characters Windows forbids, control
/// codes, or invisible reordering marks. These show up in crafted archives and
/// must never be deleted, launched or rendered without notice.
pub fn has_hostile_name(name: &OsStr) -> bool {
    let s = name.to_string_lossy();
    s.chars().any(|c| {
        c.is_control()
            || is_bidi_or_format_control(c)
            || matches!(c, '<' | '>' | '"' | '|' | '?' | '*')
    })
}

/// The drive root (`C:\`) that owns this path, when it has one.
pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let normalized = normalize_lexical(path);
    let mut comps = normalized.components();
    let prefix = comps.next()?;
    if !matches!(prefix, Component::Prefix(_)) {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}\\",
        prefix.as_os_str().to_string_lossy()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_similarity_is_not_ancestry() {
        assert!(!is_within(
            Path::new("C:\\Users\\Bobby"),
            Path::new("C:\\Users\\Bob")
        ));
        assert!(is_within(
            Path::new("C:\\Users\\Bob\\file.txt"),
            Path::new("C:\\Users\\Bob")
        ));
    }

    #[test]
    fn ancestry_is_case_insensitive() {
        assert!(is_within(
            Path::new("c:\\windows\\temp\\a.tmp"),
            Path::new("C:\\Windows\\Temp")
        ));
    }

    #[test]
    fn is_within_includes_self_but_strict_does_not() {
        let p = Path::new("C:\\Windows\\Temp");
        assert!(is_within(p, p));
        assert!(!is_strictly_within(p, p));
    }

    #[test]
    fn traversal_is_collapsed_before_comparison() {
        let hostile = Path::new("C:\\Windows\\Temp\\..\\..\\Users\\Me\\Documents");
        let resolved = normalize_lexical(hostile);
        assert_eq!(resolved, PathBuf::from("C:\\Users\\Me\\Documents"));
        assert!(!is_within(hostile, Path::new("C:\\Windows\\Temp")));
    }

    #[test]
    fn parent_dir_cannot_escape_the_root() {
        let escaped = normalize_lexical(Path::new("C:\\..\\..\\..\\Windows"));
        assert_eq!(escaped, PathBuf::from("C:\\Windows"));
    }

    #[test]
    fn verbatim_prefix_round_trips() {
        let verbatim = Path::new("\\\\?\\C:\\Windows\\Temp");
        assert_eq!(strip_verbatim(verbatim), PathBuf::from("C:\\Windows\\Temp"));
        assert!(is_within(verbatim, Path::new("C:\\Windows")));
        assert_eq!(
            long_path(Path::new("C:\\Windows")),
            PathBuf::from("\\\\?\\C:\\Windows")
        );
        assert_eq!(long_path(verbatim), verbatim);
    }

    #[test]
    fn bare_drive_keeps_its_separator() {
        assert_eq!(normalize_lexical(Path::new("C:\\")), PathBuf::from("C:\\"));
        assert_eq!(
            drive_root(Path::new("D:\\Games\\Steam")),
            Some(PathBuf::from("D:\\"))
        );
    }

    #[test]
    fn hostile_names_are_flagged() {
        assert!(has_hostile_name(OsStr::new("re\u{202e}gnp.exe")));
        assert!(!has_hostile_name(OsStr::new("holiday photo (1).jpg")));
    }

    #[test]
    fn forward_slashes_are_understood() {
        assert!(is_within(
            Path::new("C:/Windows/Temp/x.tmp"),
            Path::new("C:\\Windows\\Temp")
        ));
    }
}

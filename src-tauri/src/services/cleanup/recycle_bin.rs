//! Recycle Bin (Windows) and Trash (Linux) support.
//!
//! Windows owns the Recycle Bin, so AllInsight asks Windows about it and asks
//! Windows to empty it. It never walks `$Recycle.Bin` directly: that folder is
//! on the protected list precisely because its layout is an implementation
//! detail.

use serde::{Deserialize, Serialize};

use crate::error::{AllInsightError, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecycleBinState {
    pub bytes: u64,
    pub items: u64,
    /// False when Windows declined to report, e.g. on a volume that has no bin.
    pub available: bool,
}

/// `SHEmptyRecycleBin` flags from shellapi.h. AllInsight suppresses the Windows
/// confirmation because it has already shown its own, and suppresses the sound
/// because a utility should not make noise.
#[cfg(windows)]
const SHERB_NOCONFIRMATION: u32 = 0x0000_0001;
#[cfg(windows)]
const SHERB_NOPROGRESSUI: u32 = 0x0000_0002;
#[cfg(windows)]
const SHERB_NOSOUND: u32 = 0x0000_0004;

/// How much is currently in the bin, across every volume.
#[cfg(windows)]
pub fn query() -> Result<RecycleBinState> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };

    // A null root path asks about every volume at once.
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr != 0 {
        return Ok(RecycleBinState {
            bytes: 0,
            items: 0,
            available: false,
        });
    }

    Ok(RecycleBinState {
        bytes: info.i64Size.max(0) as u64,
        items: info.i64NumItems.max(0) as u64,
        available: true,
    })
}

/// Empty the bin. Irreversible, which is why the only caller is an explicitly
/// confirmed request and never Auto-Clean.
#[cfg(windows)]
pub fn empty() -> Result<()> {
    use windows_sys::Win32::UI::Shell::SHEmptyRecycleBinW;

    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };

    // `S_OK` is success. `E_UNEXPECTED` (0x8000FFFF) is what Windows returns
    // when the bin was already empty, which is the desired end state.
    const S_OK: i32 = 0;
    const E_UNEXPECTED: i32 = -2147418113;
    match hr {
        S_OK | E_UNEXPECTED => Ok(()),
        other => Err(AllInsightError::Platform(format!(
            "Windows could not empty the Recycle Bin (code {other:#x})."
        ))),
    }
}

/// The freedesktop Trash, which every Linux desktop and file manager shares.
/// The `trash` crate reads its `info` records and removes entries the way the
/// specification requires, so AllInsight never picks the folder apart itself.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn query() -> Result<RecycleBinState> {
    let Ok(items) = trash::os_limited::list() else {
        return Ok(RecycleBinState::default());
    };
    // Item metadata only counts a trashed folder's direct children, so the
    // byte total is measured from the trash folders themselves.
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let bytes = trash::os_limited::trash_folders()
        .map(|folders| {
            folders
                .iter()
                .map(|f| crate::services::storage::scanner::measure(&f.join("files"), &cancelled).0)
                .sum()
        })
        .unwrap_or(0);
    Ok(RecycleBinState {
        bytes,
        items: items.len() as u64,
        available: true,
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn empty() -> Result<()> {
    let items = trash::os_limited::list()
        .map_err(|e| AllInsightError::Platform(format!("Could not read the Trash: {e}")))?;
    if items.is_empty() {
        return Ok(());
    }
    trash::os_limited::purge_all(items)
        .map_err(|e| AllInsightError::Platform(format!("Could not empty the Trash: {e}")))
}

#[cfg(target_os = "macos")]
pub fn query() -> Result<RecycleBinState> {
    Ok(RecycleBinState::default())
}

#[cfg(target_os = "macos")]
pub fn empty() -> Result<()> {
    Err(AllInsightError::Platform(
        "Empty the Trash from the Finder on macOS.".into(),
    ))
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn querying_the_bin_reports_consistent_numbers() {
        let state = query().expect("query must not fail");
        if state.available && state.items == 0 {
            assert_eq!(state.bytes, 0);
        }
    }
}

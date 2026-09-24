//! What the running platform is called, for the few places where AllInsight
//! has to name it to the user ("required by Windows", "moved to the Trash").
//!
//! The frontend receives the same facts through `get_environment` and picks
//! its own wording from them, so no user-facing string has to guess.

use serde::{Deserialize, Serialize};

/// The operating system family, as a stable tag the frontend switches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Windows,
    Linux,
    Macos,
    Other,
}

pub const fn current() -> Platform {
    if cfg!(windows) {
        Platform::Windows
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Other
    }
}

/// The operating system's everyday name.
pub const fn os_name() -> &'static str {
    match current() {
        Platform::Windows => "Windows",
        Platform::Linux => "Linux",
        Platform::Macos => "macOS",
        Platform::Other => "the operating system",
    }
}

/// Where deleted files go and can be restored from.
pub const fn trash_name() -> &'static str {
    match current() {
        Platform::Windows => "Recycle Bin",
        _ => "Trash",
    }
}

/// The desktop environment, from `XDG_CURRENT_DESKTOP` (e.g. `GNOME`,
/// `KDE`). `None` on Windows and macOS, and on a bare window manager.
pub fn desktop() -> Option<String> {
    if current() != Platform::Linux {
        return None;
    }
    std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .filter(|d| !d.trim().is_empty())
}

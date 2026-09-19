//! Everything that decides whether AllInsight is allowed to touch something.
//!
//! The rest of the backend is free to discover, measure and describe files.
//! Only this module may authorise their removal, and it does so through
//! [`guard::ValidatedPath`], a token that cannot be constructed anywhere else.

pub mod guard;
pub mod paths;
pub mod protected;

pub use guard::{DeletionGuard, GuardRejection, ValidatedPath};
pub use protected::{ProtectedPaths, ProtectionReason, ProtectionVerdict};

/// True when the current process holds an elevated token.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

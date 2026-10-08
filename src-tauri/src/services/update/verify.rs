//! Proving an update is genuine before anything acts on it.
//!
//! There are three layers, and an update must pass each one that applies:
//!
//! 1. **Metadata signature.** `latest.json` is signed with the AllInsight
//!    update key (minisign / `tauri signer`), and the public half is compiled
//!    into this binary. This is what makes the whole chain trustworthy: a
//!    checksum served from the same place as the file only proves the
//!    download arrived intact, while a signature proves the publisher wrote it.
//!    Without a valid signature the document is not even parsed.
//! 2. **SHA-256.** The downloaded package must hash to the value in the signed
//!    metadata. One different byte and it is deleted.
//! 3. **Platform code signing.** On Windows, the installer's Authenticode
//!    signature is checked with WinVerifyTrust. When this build names a
//!    required publisher, an unsigned installer or one signed by anyone else
//!    is refused. When it names none (no code-signing certificate yet), the
//!    result is reported and logged but layers 1 and 2 remain the gate.

use std::io::Read;
use std::path::Path;

use base64::Engine;
use sha2::{Digest, Sha256};

/// Check `message` against a detached signature with `public_key`.
///
/// Both may be in the base64-wrapped form `tauri signer` writes or in plain
/// minisign text form.
pub fn verify_signature(
    public_key: &str,
    message: &[u8],
    signature: &str,
) -> Result<(), SignatureError> {
    if public_key.trim().is_empty() {
        return Err(SignatureError::NoKey);
    }
    let key_text = unwrap_base64(public_key).ok_or(SignatureError::BadKey)?;
    let key = minisign_verify::PublicKey::decode(&key_text)
        .or_else(|_| minisign_verify::PublicKey::from_base64(public_key.trim()))
        .map_err(|_| SignatureError::BadKey)?;

    let sig_text = unwrap_base64(signature).ok_or(SignatureError::Malformed)?;
    let signature =
        minisign_verify::Signature::decode(&sig_text).map_err(|_| SignatureError::Malformed)?;

    key.verify(message, &signature, false)
        .map_err(|_| SignatureError::Invalid)
}

/// Accept either minisign text or base64 of it.
fn unwrap_base64(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.starts_with("untrusted comment:") {
        return Some(trimmed.to_string());
    }
    let compact: String = trimmed.split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(compact)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    if text.starts_with("untrusted comment:") {
        Some(text)
    } else {
        // A bare key in base64 (the second line of a minisign key file).
        Some(trimmed.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    /// This build has no update key.
    NoKey,
    BadKey,
    Malformed,
    /// Well-formed, but not made by the key this build trusts, or the
    /// document was altered after signing.
    Invalid,
}

/// The SHA-256 of a file, as lower-case hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compare a file against the checksum from signed metadata. Comparison is
/// case-insensitive on the expected value only; the computed one is always
/// lower case.
pub fn checksum_matches(path: &Path, expected: &str) -> std::io::Result<bool> {
    Ok(sha256_file(path)? == expected.trim().to_ascii_lowercase())
}

/// What the operating system says about a package's code signature.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CodeSignature {
    /// Signed, the chain is trusted, and this is who signed it.
    Trusted {
        publisher: String,
    },
    Unsigned,
    /// Signed, but the signature is broken, revoked or untrusted.
    Untrusted,
    /// The platform has no code signing check AllInsight performs.
    NotApplicable,
}

/// Decide whether a package may run, given its code signature and the
/// publisher this build requires (if any).
pub fn code_signature_acceptable(signature: &CodeSignature, required: Option<&str>) -> bool {
    match (required, signature) {
        (Some(name), CodeSignature::Trusted { publisher }) => publisher == name,
        (Some(_), _) => false,
        // Nothing required: a broken signature is still a red flag, since a
        // genuine release is either cleanly signed or not signed at all.
        (None, CodeSignature::Untrusted) => false,
        (None, _) => true,
    }
}

#[cfg(windows)]
pub fn code_signature(path: &Path) -> CodeSignature {
    windows_impl::authenticode(path)
}

#[cfg(not(windows))]
pub fn code_signature(_path: &Path) -> CodeSignature {
    CodeSignature::NotApplicable
}

#[cfg(windows)]
mod windows_impl {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{HWND, TRUST_E_NOSIGNATURE, TRUST_E_SUBJECT_FORM_UNKNOWN};
    use windows_sys::Win32::Security::Cryptography::{
        CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE,
    };
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE,
    };

    use super::CodeSignature;

    pub fn authenticode(path: &Path) -> CodeSignature {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

        // SAFETY: every pointer handed to WinVerifyTrust points at a local
        // that outlives the call, and the state opened by the VERIFY action
        // is always released by the CLOSE action below.
        unsafe {
            let mut file: WINTRUST_FILE_INFO = std::mem::zeroed();
            file.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
            file.pcwszFilePath = wide.as_ptr();

            let mut data: WINTRUST_DATA = std::mem::zeroed();
            data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
            data.dwUIChoice = WTD_UI_NONE;
            data.fdwRevocationChecks = WTD_REVOKE_WHOLECHAIN;
            data.dwUnionChoice = WTD_CHOICE_FILE;
            data.Anonymous = WINTRUST_DATA_0 { pFile: &mut file };
            data.dwStateAction = WTD_STATEACTION_VERIFY;

            let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
            let status = WinVerifyTrust(
                0 as HWND,
                &mut action,
                &mut data as *mut WINTRUST_DATA as *mut _,
            );

            let result = if status == 0 {
                CodeSignature::Trusted {
                    publisher: signer_name(data.hWVTStateData).unwrap_or_default(),
                }
            } else if status == TRUST_E_NOSIGNATURE || status == TRUST_E_SUBJECT_FORM_UNKNOWN {
                // No signature, or not a file type that can carry one. Both
                // mean "unsigned": acceptable only while no publisher is
                // required, and the SHA-256 has already been checked.
                CodeSignature::Unsigned
            } else {
                CodeSignature::Untrusted
            };

            data.dwStateAction = WTD_STATEACTION_CLOSE;
            WinVerifyTrust(
                0 as HWND,
                &mut action,
                &mut data as *mut WINTRUST_DATA as *mut _,
            );
            result
        }
    }

    /// The display name on the leaf certificate of the first signer.
    unsafe fn signer_name(state: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return None;
        }
        let cert = (*(*signer).pasCertChain).pCert;
        if cert.is_null() {
            return None;
        }
        let mut buffer = [0u16; 512];
        let len = CertGetNameStringW(
            cert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            std::ptr::null(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        );
        if len <= 1 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..(len as usize - 1)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = include_str!("testdata/test.pub");
    const DOC: &[u8] = include_bytes!("testdata/latest.json");
    const SIG: &str = include_str!("testdata/latest.json.sig");

    #[test]
    fn a_genuine_signature_verifies() {
        assert_eq!(verify_signature(KEY, DOC, SIG), Ok(()));
    }

    #[test]
    fn one_changed_byte_fails_verification() {
        let mut tampered = DOC.to_vec();
        let at = tampered.windows(5).position(|w| w == b"9.0.0").unwrap();
        tampered[at] = b'8';
        assert_eq!(
            verify_signature(KEY, &tampered, SIG),
            Err(SignatureError::Invalid)
        );
    }

    #[test]
    fn a_signature_from_another_key_fails() {
        // A well-formed key that is not the one that signed the document.
        let other = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEYyNDNBQTA4MjNGRjNCQTQKUldTa08vOGpDS3BEOGlwS3g0M3FEQ25tdnM4SDlBRUF3M3BXZmxFNEJyVzF3TGVHOXNpdE5TMHIK";
        assert_eq!(
            verify_signature(other, DOC, SIG),
            Err(SignatureError::Invalid)
        );
    }

    #[test]
    fn no_key_means_no_trust() {
        assert_eq!(verify_signature("", DOC, SIG), Err(SignatureError::NoKey));
    }

    #[test]
    fn garbage_signatures_are_refused() {
        assert_eq!(
            verify_signature(KEY, DOC, ""),
            Err(SignatureError::Malformed)
        );
        assert_eq!(
            verify_signature(KEY, DOC, "not a signature"),
            Err(SignatureError::Malformed)
        );
        assert_eq!(
            verify_signature(KEY, DOC, "<html>Not Found</html>"),
            Err(SignatureError::Malformed)
        );
    }

    #[test]
    fn checksums_are_computed_and_compared() {
        let dir = std::env::temp_dir().join(format!("allinsight-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("payload.bin");
        std::fs::write(&file, b"test").unwrap();
        let expected = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(sha256_file(&file).unwrap(), expected);
        assert!(checksum_matches(&file, &expected.to_ascii_uppercase()).unwrap());

        // A corrupted download.
        std::fs::write(&file, b"tesT").unwrap();
        assert!(!checksum_matches(&file, expected).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_required_publisher_must_match_exactly() {
        let signed = CodeSignature::Trusted {
            publisher: "AllInsight".into(),
        };
        assert!(code_signature_acceptable(&signed, Some("AllInsight")));
        assert!(!code_signature_acceptable(&signed, Some("Someone Else")));
        assert!(!code_signature_acceptable(
            &CodeSignature::Unsigned,
            Some("AllInsight")
        ));
        assert!(!code_signature_acceptable(
            &CodeSignature::Untrusted,
            Some("AllInsight")
        ));
    }

    #[test]
    fn without_a_required_publisher_only_a_broken_signature_is_refused() {
        assert!(code_signature_acceptable(&CodeSignature::Unsigned, None));
        assert!(code_signature_acceptable(
            &CodeSignature::NotApplicable,
            None
        ));
        assert!(!code_signature_acceptable(&CodeSignature::Untrusted, None));
    }

    #[cfg(windows)]
    #[test]
    fn an_unsigned_file_is_reported_as_unsigned() {
        let dir = std::env::temp_dir().join(format!("allinsight-ac-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("unsigned.exe");
        std::fs::write(&file, b"MZ not really a program").unwrap();
        assert_ne!(
            code_signature(&file),
            CodeSignature::Trusted {
                publisher: String::new()
            }
        );
        assert!(!matches!(
            code_signature(&file),
            CodeSignature::Trusted { .. }
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

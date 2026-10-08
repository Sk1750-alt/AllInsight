//! The release metadata document, `latest.json`.
//!
//! ```json
//! {
//!   "schema": 1,
//!   "product": "AllInsight",
//!   "channel": "stable",
//!   "kind": "application",
//!   "version": "1.0.1",
//!   "release_date": "2026-10-20",
//!   "minimum_supported_version": "1.0.0",
//!   "security": false,
//!   "release_notes": ["Faster storage scans", "Bug fixes"],
//!   "installers": {
//!     "windows-x64": {
//!       "url": "https://github.com/.../AllInsight_1.0.1_x64-setup.exe",
//!       "sha256": "64 hex characters",
//!       "size": 9437184,
//!       "format": "nsis"
//!     },
//!     "linux-x64": { "url": "...AppImage", "sha256": "...", "size": 0, "format": "appimage" }
//!   }
//! }
//! ```
//!
//! One document lists every platform, so the client downloads the same file
//! as everyone else and picks its own entry locally. The request itself says
//! nothing about the machine.
//!
//! The document is only parsed after its signature has been verified; see
//! `signature`. Everything below is a second line of defence against a
//! correctly signed but wrong document: wrong product, wrong channel, an old
//! release replayed, or an installer on a host we do not trust.

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};

use super::channel::{Channel, UpdateKind};
use super::config::{check_url, UrlRefusal, MAX_PACKAGE_BYTES};
use super::version;

pub const PRODUCT: &str = "AllInsight";
/// The metadata format this build understands.
pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    #[serde(default = "default_schema")]
    pub schema: u32,
    pub product: String,
    pub channel: Channel,
    #[serde(default)]
    pub kind: UpdateKind,
    pub version: String,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub minimum_supported_version: Option<String>,
    #[serde(default)]
    pub security: bool,
    #[serde(default)]
    pub release_notes: Vec<String>,
    pub installers: BTreeMap<String, InstallerEntry>,
}

fn default_schema() -> u32 {
    SCHEMA
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallerEntry {
    pub url: String,
    pub sha256: String,
    /// Bytes. Zero when the publisher did not state it.
    #[serde(default)]
    pub size: u64,
    pub format: PackageFormat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageFormat {
    /// The Windows NSIS installer Tauri builds.
    Nsis,
    /// A self-contained Linux AppImage, replaced in place.
    Appimage,
    /// Debian and RPM packages, installed by the system package manager.
    Deb,
    Rpm,
    /// macOS disk image.
    Dmg,
}

/// Why a document was refused. Each maps to one plain sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    Malformed(String),
    UnsupportedSchema(u32),
    WrongProduct,
    WrongChannel,
    BadVersion,
    BadChecksum,
    BadUrl(UrlRefusal),
    TooLarge,
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataError::Malformed(_) => {
                write!(f, "The update information was not in the expected format.")
            }
            MetadataError::UnsupportedSchema(n) => write!(
                f,
                "The update information uses format {n}, which this version cannot read."
            ),
            MetadataError::WrongProduct => {
                write!(f, "The update information is for a different product.")
            }
            MetadataError::WrongChannel => write!(
                f,
                "The update information is for a different release channel."
            ),
            MetadataError::BadVersion => {
                write!(f, "The update information has an invalid version number.")
            }
            MetadataError::BadChecksum => {
                write!(f, "The update information has an invalid checksum.")
            }
            MetadataError::BadUrl(_) => write!(
                f,
                "The update points at an address AllInsight does not trust."
            ),
            MetadataError::TooLarge => {
                write!(f, "The update is larger than AllInsight will download.")
            }
        }
    }
}

/// Parse a document whose signature has already been verified, and check it
/// is one this build should act on.
pub fn parse(bytes: &[u8], expected_channel: Channel) -> Result<ReleaseMetadata, MetadataError> {
    let doc: ReleaseMetadata =
        serde_json::from_slice(bytes).map_err(|e| MetadataError::Malformed(e.to_string()))?;
    if doc.schema != SCHEMA {
        return Err(MetadataError::UnsupportedSchema(doc.schema));
    }
    if doc.product != PRODUCT {
        return Err(MetadataError::WrongProduct);
    }
    if doc.channel != expected_channel {
        return Err(MetadataError::WrongChannel);
    }
    if version::parse(&doc.version).is_none() {
        return Err(MetadataError::BadVersion);
    }
    if let Some(min) = &doc.minimum_supported_version {
        if version::parse(min).is_none() {
            return Err(MetadataError::BadVersion);
        }
    }
    Ok(doc)
}

impl ReleaseMetadata {
    pub fn parsed_version(&self) -> Version {
        version::parse(&self.version).expect("checked in parse")
    }

    /// Whether an installation at `installed` can move to this release
    /// directly. A release may require a stepping-stone version first.
    pub fn supports_upgrade_from(&self, installed: &Version) -> bool {
        match self
            .minimum_supported_version
            .as_deref()
            .and_then(version::parse)
        {
            Some(min) => installed.cmp_precedence(&min) != std::cmp::Ordering::Less,
            None => true,
        }
    }

    /// The package for `platform`, checked for a trusted address, a
    /// well-formed checksum, and a sane size.
    pub fn installer_for(
        &self,
        platform: &str,
        allowed_hosts: &[String],
    ) -> Result<Option<InstallerEntry>, MetadataError> {
        let Some(entry) = self.installers.get(platform) else {
            return Ok(None);
        };
        check_url(&entry.url, allowed_hosts).map_err(MetadataError::BadUrl)?;
        if !is_sha256_hex(&entry.sha256) {
            return Err(MetadataError::BadChecksum);
        }
        if entry.size > MAX_PACKAGE_BYTES {
            return Err(MetadataError::TooLarge);
        }
        let mut entry = entry.clone();
        entry.sha256 = entry.sha256.to_ascii_lowercase();
        Ok(Some(entry))
    }
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    pub fn sample(version: &str) -> String {
        format!(
            r#"{{
              "schema": 1,
              "product": "AllInsight",
              "channel": "stable",
              "version": "{version}",
              "release_date": "2026-10-20",
              "minimum_supported_version": "1.0.0",
              "release_notes": ["Faster scans", "Bug fixes"],
              "installers": {{
                "windows-x64": {{ "url": "https://github.com/Sk1750-alt/AllInsight/releases/download/v{version}/AllInsight_{version}_x64-setup.exe", "sha256": "{SHA}", "size": 1000, "format": "nsis" }},
                "linux-x64": {{ "url": "https://github.com/Sk1750-alt/AllInsight/releases/download/v{version}/AllInsight_{version}_amd64.AppImage", "sha256": "{SHA}", "size": 1000, "format": "appimage" }}
              }}
            }}"#
        )
    }

    fn hosts() -> Vec<String> {
        vec!["github.com".into()]
    }

    #[test]
    fn a_valid_document_parses() {
        let doc = parse(sample("1.4.0").as_bytes(), Channel::Stable).unwrap();
        assert_eq!(doc.version, "1.4.0");
        assert_eq!(doc.kind, UpdateKind::Application);
        assert_eq!(doc.release_notes.len(), 2);
        let entry = doc.installer_for("windows-x64", &hosts()).unwrap().unwrap();
        assert_eq!(entry.format, PackageFormat::Nsis);
    }

    #[test]
    fn invalid_json_is_refused() {
        assert!(matches!(
            parse(b"{ not json", Channel::Stable),
            Err(MetadataError::Malformed(_))
        ));
        assert!(matches!(
            parse(b"", Channel::Stable),
            Err(MetadataError::Malformed(_))
        ));
        assert!(matches!(
            parse(b"<html>404</html>", Channel::Stable),
            Err(MetadataError::Malformed(_))
        ));
    }

    #[test]
    fn missing_fields_are_refused() {
        let doc = r#"{"product":"AllInsight","channel":"stable","version":"1.4.0"}"#;
        assert!(matches!(
            parse(doc.as_bytes(), Channel::Stable),
            Err(MetadataError::Malformed(_))
        ));
    }

    #[test]
    fn the_wrong_product_or_channel_is_refused() {
        let other = sample("1.4.0").replace("\"AllInsight\"", "\"Other\"");
        assert_eq!(
            parse(other.as_bytes(), Channel::Stable).unwrap_err(),
            MetadataError::WrongProduct
        );
        assert_eq!(
            parse(sample("1.4.0").as_bytes(), Channel::Beta).unwrap_err(),
            MetadataError::WrongChannel
        );
    }

    #[test]
    fn a_future_schema_is_refused() {
        let doc = sample("1.4.0").replace("\"schema\": 1", "\"schema\": 2");
        assert_eq!(
            parse(doc.as_bytes(), Channel::Stable).unwrap_err(),
            MetadataError::UnsupportedSchema(2)
        );
    }

    #[test]
    fn a_bad_version_is_refused() {
        assert_eq!(
            parse(sample("latest").as_bytes(), Channel::Stable).unwrap_err(),
            MetadataError::BadVersion
        );
    }

    #[test]
    fn an_http_installer_url_is_refused() {
        let doc = sample("1.4.0").replace("https://github.com", "http://github.com");
        let doc = parse(doc.as_bytes(), Channel::Stable).unwrap();
        assert_eq!(
            doc.installer_for("windows-x64", &hosts()).unwrap_err(),
            MetadataError::BadUrl(UrlRefusal::NotHttps)
        );
    }

    #[test]
    fn an_installer_on_an_untrusted_host_is_refused() {
        let doc = sample("1.4.0").replace("https://github.com", "https://evil.example");
        let doc = parse(doc.as_bytes(), Channel::Stable).unwrap();
        assert!(matches!(
            doc.installer_for("windows-x64", &hosts()).unwrap_err(),
            MetadataError::BadUrl(UrlRefusal::UntrustedHost(_))
        ));
    }

    #[test]
    fn a_malformed_checksum_is_refused() {
        let doc = sample("1.4.0").replace(SHA, "REPLACE_WITH_REAL_SHA256");
        let doc = parse(doc.as_bytes(), Channel::Stable).unwrap();
        assert_eq!(
            doc.installer_for("windows-x64", &hosts()).unwrap_err(),
            MetadataError::BadChecksum
        );
    }

    #[test]
    fn a_platform_without_a_package_is_not_an_error() {
        let doc = parse(sample("1.4.0").as_bytes(), Channel::Stable).unwrap();
        assert_eq!(doc.installer_for("macos-arm64", &hosts()).unwrap(), None);
    }

    #[test]
    fn minimum_supported_version_is_respected() {
        let doc = parse(sample("2.0.0").as_bytes(), Channel::Stable).unwrap();
        assert!(doc.supports_upgrade_from(&version::parse("1.0.0").unwrap()));
        assert!(doc.supports_upgrade_from(&version::parse("1.5.0").unwrap()));
        let doc = parse(
            sample("2.0.0").replace("\"1.0.0\"", "\"1.2.0\"").as_bytes(),
            Channel::Stable,
        )
        .unwrap();
        assert!(!doc.supports_upgrade_from(&version::parse("1.1.9").unwrap()));
    }

    #[test]
    fn future_platforms_and_kinds_parse() {
        let doc = sample("1.4.0")
            .replace("\"linux-x64\"", "\"macos-arm64\"")
            .replace("\"appimage\"", "\"dmg\"")
            .replace("\"schema\": 1,", "\"schema\": 1, \"kind\": \"model\",");
        let doc = parse(doc.as_bytes(), Channel::Stable).unwrap();
        assert_eq!(doc.kind, UpdateKind::Model);
        assert!(doc.installers.contains_key("macos-arm64"));
    }
}

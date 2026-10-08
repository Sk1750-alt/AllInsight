//! Where updates come from and who is trusted to sign them.
//!
//! Every value here is fixed when AllInsight is built, from environment
//! variables read by the compiler, and none of it is a user setting. That is
//! deliberate and mirrors why `services::config` never imports the AI engine
//! path: if a settings file could change the update address or the signing
//! key, "import my settings" would become "install whatever this file points
//! at".
//!
//! | Build variable                   | Meaning                                         |
//! |----------------------------------|-------------------------------------------------|
//! | `ALLINSIGHT_UPDATE_URL`          | Folder that holds `latest.json` (HTTPS only)    |
//! | `ALLINSIGHT_UPDATE_PUBKEY`       | Public key that signs `latest.json`             |
//! | `ALLINSIGHT_UPDATE_PUBLISHER`    | Required Authenticode signer name (Windows)     |
//! | `ALLINSIGHT_UPDATE_ENABLED`      | `false` builds without the updater (e.g. AUR)   |
//! | `ALLINSIGHT_UPDATE_EXTRA_HOSTS`  | Comma-separated extra hosts downloads may use   |

use super::channel::Channel;

/// The folder that holds the release metadata. GitHub serves
/// `releases/latest/download/<file>` from the newest non-prerelease, so the
/// stable channel needs no server of its own.
const DEFAULT_BASE_URL: &str = "https://github.com/Sk1750-alt/AllInsight/releases/latest/download";

/// Hosts a request may reach, including through redirects. GitHub answers a
/// release download with a redirect to its asset storage hosts.
const DEFAULT_ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "updates.allinsight.biz",
];

/// The update signing public key, in the form `tauri signer generate` prints
/// (base64 of a minisign public key file).
const DEFAULT_PUBLIC_KEY: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI1MzQ2NDdGQjY5NUFDNgpSV1RHV21uN1IwWlRBdUF1TE4zdVBkY2k3MjhSSlFuLzdVY2VxOUhYYzdBb2pFWFppQklVbzE0Tgo=";

/// A day, the shortest interval automatic checking allows.
pub const DEFAULT_INTERVAL_HOURS: u32 = 24;

/// Metadata is a few kilobytes; anything larger is not ours.
pub const MAX_METADATA_BYTES: u64 = 256 * 1024;
/// The signature file is a few hundred bytes.
pub const MAX_SIGNATURE_BYTES: u64 = 8 * 1024;
/// An installer larger than this is refused before it is downloaded.
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

/// The updater's fixed configuration.
#[derive(Debug, Clone)]
pub struct UpdateConfig {
    pub enabled: bool,
    pub base_url: String,
    pub public_key: String,
    pub required_publisher: Option<String>,
    pub allowed_hosts: Vec<String>,
}

impl UpdateConfig {
    /// The configuration this binary was built with.
    pub fn compiled() -> Self {
        let enabled = !matches!(
            option_env!("ALLINSIGHT_UPDATE_ENABLED").map(str::trim),
            Some("false") | Some("0") | Some("no")
        );
        let base_url = option_env!("ALLINSIGHT_UPDATE_URL")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(DEFAULT_BASE_URL)
            .trim()
            .trim_end_matches('/')
            .to_string();
        let public_key = option_env!("ALLINSIGHT_UPDATE_PUBKEY")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(DEFAULT_PUBLIC_KEY)
            .trim()
            .to_string();
        let required_publisher = option_env!("ALLINSIGHT_UPDATE_PUBLISHER")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let mut allowed_hosts: Vec<String> = DEFAULT_ALLOWED_HOSTS
            .iter()
            .map(|h| h.to_string())
            .collect();
        if let Some(extra) = option_env!("ALLINSIGHT_UPDATE_EXTRA_HOSTS") {
            allowed_hosts.extend(
                extra
                    .split(',')
                    .map(|h| h.trim().to_ascii_lowercase())
                    .filter(|h| !h.is_empty()),
            );
        }
        // The configured server is always allowed, whatever it is.
        if let Some(host) = host_of(&base_url) {
            allowed_hosts.push(host);
        }
        allowed_hosts.sort();
        allowed_hosts.dedup();

        Self {
            enabled,
            base_url,
            public_key,
            required_publisher,
            allowed_hosts,
        }
    }

    /// The metadata document for a channel. Stable is `latest.json`; the
    /// others are `latest-<channel>.json` beside it.
    pub fn metadata_url(&self, channel: Channel) -> String {
        format!("{}/{}", self.base_url, channel.metadata_file())
    }

    /// The detached signature for a metadata document.
    pub fn signature_url(&self, channel: Channel) -> String {
        format!("{}.sig", self.metadata_url(channel))
    }

    /// Why this build cannot check for updates, if it cannot.
    pub fn unavailable_reason(&self) -> Option<&'static str> {
        if !self.enabled {
            return Some(
                "This copy of AllInsight is updated by your package manager, not by AllInsight itself.",
            );
        }
        if self.public_key.is_empty() {
            return Some(
                "This build of AllInsight was made without an update signing key, so it cannot verify updates and will not look for them.",
            );
        }
        if check_url(&self.base_url, &self.allowed_hosts).is_err() {
            return Some("This build of AllInsight has an invalid update address.");
        }
        None
    }
}

/// Why a URL was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlRefusal {
    NotHttps,
    Malformed,
    UntrustedHost(String),
    CarriesCredentials,
}

/// Accept only `https://` URLs on an allowed host, with no user information.
pub fn check_url(url: &str, allowed_hosts: &[String]) -> Result<(), UrlRefusal> {
    let lower = url.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://") else {
        return Err(if lower.contains("://") || lower.starts_with("http") {
            UrlRefusal::NotHttps
        } else {
            UrlRefusal::Malformed
        });
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() {
        return Err(UrlRefusal::Malformed);
    }
    if authority.contains('@') {
        return Err(UrlRefusal::CarriesCredentials);
    }
    let host = host_of(url).ok_or(UrlRefusal::Malformed)?;
    if allowed_hosts.iter().any(|h| h == &host) {
        Ok(())
    } else {
        Err(UrlRefusal::UntrustedHost(host))
    }
}

/// The lower-cased host of an `http(s)` URL, without port.
pub fn host_of(url: &str) -> Option<String> {
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[')
    } else {
        authority.split(':').next()?
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Vec<String> {
        vec!["github.com".into(), "updates.allinsight.biz".into()]
    }

    #[test]
    fn https_on_an_allowed_host_is_accepted() {
        assert!(check_url("https://updates.allinsight.biz/latest.json", &hosts()).is_ok());
        assert!(check_url("HTTPS://GitHub.com/x/y", &hosts()).is_ok());
    }

    #[test]
    fn plain_http_is_refused() {
        assert_eq!(
            check_url("http://updates.allinsight.biz/latest.json", &hosts()),
            Err(UrlRefusal::NotHttps)
        );
    }

    #[test]
    fn other_schemes_are_refused() {
        assert!(check_url("file:///C:/evil.json", &hosts()).is_err());
        assert!(check_url("ftp://github.com/x", &hosts()).is_err());
        assert!(check_url("not a url", &hosts()).is_err());
    }

    #[test]
    fn an_untrusted_host_is_refused() {
        assert_eq!(
            check_url("https://evil.example/latest.json", &hosts()),
            Err(UrlRefusal::UntrustedHost("evil.example".into()))
        );
    }

    #[test]
    fn look_alike_hosts_are_refused() {
        assert!(check_url("https://github.com.evil.example/x", &hosts()).is_err());
        assert!(check_url("https://evilgithub.com/x", &hosts()).is_err());
    }

    #[test]
    fn credentials_in_the_url_are_refused() {
        assert_eq!(
            check_url("https://user:pw@github.com/x", &hosts()),
            Err(UrlRefusal::CarriesCredentials)
        );
        // The classic confusion: the real host is the part after '@'.
        assert!(check_url("https://github.com@evil.example/x", &hosts()).is_err());
    }

    #[test]
    fn the_compiled_default_points_at_https() {
        let config = UpdateConfig::compiled();
        assert!(config.base_url.starts_with("https://"));
        assert!(check_url(&config.metadata_url(Channel::Stable), &config.allowed_hosts).is_ok());
        assert!(config
            .metadata_url(Channel::Stable)
            .ends_with("/latest.json"));
        assert!(config
            .signature_url(Channel::Stable)
            .ends_with("/latest.json.sig"));
    }
}

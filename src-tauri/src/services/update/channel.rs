//! Release channels and the kinds of update a release can carry.

use serde::{Deserialize, Serialize};

/// A release channel. Only `Stable` is offered in the interface today; the
/// others exist so that adding them later changes no part of the updater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Beta,
    Dev,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
            Channel::Dev => "dev",
        }
    }

    pub fn metadata_file(&self) -> &'static str {
        match self {
            Channel::Stable => "latest.json",
            Channel::Beta => "latest-beta.json",
            Channel::Dev => "latest-dev.json",
        }
    }

    /// The channels a user can choose from in this version.
    pub fn offered() -> &'static [Channel] {
        &[Channel::Stable]
    }
}

/// What a release replaces.
///
/// Application, model and configuration updates are independent so that a
/// change to the interface never makes anyone download a multi-gigabyte model
/// again. This version installs `Application` releases; the others are
/// recognised and reported, and get their own installers when they exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    #[default]
    Application,
    Model,
    Configuration,
    Security,
}

impl UpdateKind {
    /// Whether this version knows how to install the kind.
    pub fn installable(&self) -> bool {
        // A security release is an application release that is flagged as
        // urgent; it installs the same way.
        matches!(self, UpdateKind::Application | UpdateKind::Security)
    }
}

/// The platform key this build looks for in the metadata, such as
/// `windows-x64` or `linux-x64`.
pub fn current_platform_key() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        std::env::consts::OS
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        other => other,
    };
    format!("{os}-{arch}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stable_is_offered() {
        assert_eq!(Channel::offered(), &[Channel::Stable]);
    }

    #[test]
    fn each_channel_has_its_own_document() {
        assert_eq!(Channel::Stable.metadata_file(), "latest.json");
        assert_eq!(Channel::Beta.metadata_file(), "latest-beta.json");
        assert_eq!(Channel::Dev.metadata_file(), "latest-dev.json");
    }

    #[test]
    fn the_platform_key_has_an_os_and_an_architecture() {
        let key = current_platform_key();
        let (os, arch) = key.split_once('-').unwrap();
        assert!(!os.is_empty() && !arch.is_empty());
        #[cfg(all(windows, target_arch = "x86_64"))]
        assert_eq!(key, "windows-x64");
    }

    #[test]
    fn models_are_not_installed_as_applications() {
        assert!(UpdateKind::Application.installable());
        assert!(UpdateKind::Security.installable());
        assert!(!UpdateKind::Model.installable());
        assert!(!UpdateKind::Configuration.installable());
    }
}

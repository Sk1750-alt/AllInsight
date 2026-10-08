//! Semantic version comparison.
//!
//! Versions are compared as versions, never as strings: as text, "1.10.0"
//! sorts before "1.9.0". Parsing is done by the `semver` crate, which also
//! orders pre-releases correctly (`1.4.0-beta.2 < 1.4.0`).

use std::cmp::Ordering;

use semver::Version;

/// Parse a version, accepting an optional leading `v`.
pub fn parse(text: &str) -> Option<Version> {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(trimmed).ok()
}

/// The version of this running build.
pub fn current() -> Version {
    parse(env!("CARGO_PKG_VERSION")).expect("the package version is valid semver")
}

/// How the offered version relates to the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    /// The offered version is newer: an update.
    Newer,
    Same,
    /// The installed build is newer than anything offered, as on a
    /// developer's machine. Never offered as a "downgrade".
    Older,
}

pub fn relation(installed: &Version, offered: &Version) -> Relation {
    match offered.cmp_precedence(installed) {
        Ordering::Greater => Relation::Newer,
        Ordering::Equal => Relation::Same,
        Ordering::Less => Relation::Older,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        parse(s).unwrap()
    }

    #[test]
    fn minor_and_patch_steps_are_updates() {
        assert_eq!(relation(&v("1.0.0"), &v("1.1.0")), Relation::Newer);
        assert_eq!(relation(&v("1.1.0"), &v("1.1.1")), Relation::Newer);
        assert_eq!(relation(&v("1.3.2"), &v("1.4.0")), Relation::Newer);
        assert_eq!(relation(&v("1.4.0"), &v("1.4.1")), Relation::Newer);
    }

    #[test]
    fn a_major_step_is_an_update() {
        assert_eq!(relation(&v("1.9.0"), &v("2.0.0")), Relation::Newer);
    }

    #[test]
    fn versions_are_not_compared_as_text() {
        // As strings "1.10.0" < "1.9.0".
        assert_eq!(relation(&v("1.9.0"), &v("1.10.0")), Relation::Newer);
        assert_eq!(relation(&v("1.10.0"), &v("1.9.0")), Relation::Older);
    }

    #[test]
    fn equal_versions_are_up_to_date() {
        assert_eq!(relation(&v("1.4.0"), &v("1.4.0")), Relation::Same);
        assert_eq!(relation(&v("v1.4.0"), &v("1.4.0")), Relation::Same);
    }

    #[test]
    fn a_newer_installed_build_is_never_downgraded() {
        assert_eq!(relation(&v("2.0.0"), &v("1.9.9")), Relation::Older);
    }

    #[test]
    fn prereleases_order_before_their_release() {
        assert_eq!(relation(&v("1.4.0-beta.1"), &v("1.4.0")), Relation::Newer);
        assert_eq!(
            relation(&v("1.4.0-beta.1"), &v("1.4.0-beta.2")),
            Relation::Newer
        );
        assert_eq!(relation(&v("1.4.0"), &v("1.4.0-beta.9")), Relation::Older);
    }

    #[test]
    fn build_metadata_does_not_make_an_update() {
        assert_eq!(relation(&v("1.4.0"), &v("1.4.0+build.7")), Relation::Same);
    }

    #[test]
    fn malformed_versions_do_not_parse() {
        for bad in ["", "1", "1.2", "1.2.x", "latest", "1.2.3.4", "01.2.3"] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_running_version_parses() {
        assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
    }
}

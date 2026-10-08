//! Application updates.
//!
//! This module is deliberately an island. It imports nothing from the rest of
//! AllInsight's services: not the database, not the scanner, not the
//! assistant, not the settings document. What it knows is the version it is
//! running, the platform it runs on, and the fixed configuration in
//! [`config`]. The command layer passes in the two things it needs from
//! outside (when it last checked, and a way to back up the database before an
//! install), so there is no path by which user data could reach a request.
//!
//! ```text
//! check     GET latest.json + latest.json.sig  ->  verify signature
//!           ->  parse and validate  ->  compare versions locally
//! download  GET package  ->  SHA-256 against signed metadata
//!           ->  platform code signature  ->  keep in updates/
//! install   re-hash  ->  back up database  ->  pending.json  ->  installer
//! ```
//!
//! Nothing is downloaded, installed or restarted without the user pressing a
//! button, including when automatic checking is on: an automatic check only
//! reads the metadata and tells the user what it found.

// `UpdateStatus` is the error type as well as the success type: a failed
// check is still a status the interface shows. It is returned once per user
// action, so its size does not matter.
#![allow(clippy::result_large_err)]

pub mod channel;
pub mod config;
pub mod install;
pub mod log;
pub mod metadata;
pub mod transport;
pub mod verify;
pub mod version;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;

use channel::{Channel, UpdateKind};
use config::{UpdateConfig, MAX_METADATA_BYTES, MAX_PACKAGE_BYTES, MAX_SIGNATURE_BYTES};
use install::{Handover, InstallReport, PendingInstall};
use log::UpdateLog;
use metadata::{InstallerEntry, ReleaseMetadata};
use transport::{NetError, Transport};
use version::Relation;

/// The release the user is being offered.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub release_date: Option<String>,
    pub notes: Vec<String>,
    pub security: bool,
    pub kind: UpdateKind,
    /// Bytes, when the publisher stated them.
    pub size: u64,
    /// False when this platform has no package, or this version is too old
    /// to move to it directly. The interface then points to the website.
    pub installable: bool,
    /// Why `installable` is false.
    pub note: Option<String>,
}

/// What went wrong, in the classes the interface words differently.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// No connection. Not an error in AllInsight; it works offline.
    Offline,
    /// The server could be reached but did not give a usable answer.
    Network,
    /// A signature or checksum did not match. Nothing was installed.
    Verification,
    /// The installer could not be started, or the files could not be saved.
    Install,
}

/// Where the updater is. Serialised for the interface as `{ "state": ... }`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Phase {
    Idle,
    /// This build cannot update itself; `reason` says why.
    Unavailable {
        reason: String,
    },
    Checking,
    UpToDate {
        latest: String,
    },
    Available {
        release: ReleaseInfo,
    },
    Downloading {
        release: ReleaseInfo,
        downloaded: u64,
        total: Option<u64>,
    },
    Verifying {
        release: ReleaseInfo,
    },
    /// Verified and waiting for the user to restart into it.
    Ready {
        release: ReleaseInfo,
        manual_install: bool,
    },
    Failed {
        kind: FailureKind,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub phase: Phase,
    pub current_version: String,
    pub channel: Channel,
    pub platform: String,
    /// What the last install attempt turned out to be, once, after a restart.
    pub last_install: Option<InstallReport>,
}

/// A release that has been checked and accepted, with its package.
#[derive(Debug, Clone)]
struct Offer {
    info: ReleaseInfo,
    package: Option<InstallerEntry>,
}

pub struct UpdateManager {
    config: UpdateConfig,
    transport: Arc<dyn Transport>,
    staging: PathBuf,
    log: UpdateLog,
    channel: Channel,
    platform: String,
    current: semver::Version,
    phase: Mutex<Phase>,
    offer: Mutex<Option<Offer>>,
    ready_package: Mutex<Option<PathBuf>>,
    last_install: Mutex<Option<InstallReport>>,
    busy: AtomicBool,
}

impl UpdateManager {
    /// The manager the application uses: compiled configuration, real HTTPS.
    pub fn for_application(data_directory: &Path) -> Self {
        let config = UpdateConfig::compiled();
        let transport = Arc::new(transport::HttpsTransport::new(config.allowed_hosts.clone()));
        Self::new(
            config,
            transport,
            data_directory.join("updates"),
            Some(data_directory.join("logs").join("updates.log")),
            version::current(),
        )
    }

    pub fn new(
        config: UpdateConfig,
        transport: Arc<dyn Transport>,
        staging: PathBuf,
        log_path: Option<PathBuf>,
        current: semver::Version,
    ) -> Self {
        let phase = match config.unavailable_reason() {
            Some(reason) => Phase::Unavailable {
                reason: reason.to_string(),
            },
            None => Phase::Idle,
        };
        Self {
            config,
            transport,
            staging,
            log: UpdateLog::new(log_path),
            channel: Channel::Stable,
            platform: channel::current_platform_key(),
            current,
            phase: Mutex::new(phase),
            offer: Mutex::new(None),
            ready_package: Mutex::new(None),
            last_install: Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        UpdateStatus {
            phase: self.phase.lock().clone(),
            current_version: self.current.to_string(),
            channel: self.channel,
            platform: self.platform.clone(),
            last_install: self.last_install.lock().clone(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.config.unavailable_reason().is_none()
    }

    fn set(&self, phase: Phase) {
        *self.phase.lock() = phase;
    }

    fn fail(&self, kind: FailureKind, message: &str, log_detail: &str) -> UpdateStatus {
        self.log.write(log_detail);
        self.set(Phase::Failed {
            kind,
            message: message.to_string(),
        });
        self.status()
    }

    /// Run `work` unless another update operation is in progress.
    fn exclusive<T>(&self, work: impl FnOnce() -> T, busy: impl FnOnce() -> T) -> T {
        if self.busy.swap(true, Ordering::AcqRel) {
            return busy();
        }
        struct Release<'a>(&'a AtomicBool);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _release = Release(&self.busy);
        work()
    }

    /// Look at what the previous install left behind. Called once at startup.
    pub fn reconcile_previous_install(&self) {
        let report = install::reconcile(&self.staging, &self.current.to_string());
        match &report {
            Some(InstallReport::Completed { version }) => {
                self.log.write(&format!("Update to {version} completed"));
            }
            Some(InstallReport::NotCompleted { attempted, running }) => {
                self.log.write(&format!(
                    "Update to {attempted} did not complete; {running} is still installed and unchanged"
                ));
            }
            None => {
                // Partial downloads from a session that ended mid-download.
                remove_partials(&self.staging);
            }
        }
        *self.last_install.lock() = report;
    }

    /// Ask the release server what the newest version is.
    ///
    /// Returns the new status. `Ok` means the server answered with a document
    /// that verified, whatever it said; the caller records that as the last
    /// successful check.
    pub fn check(&self) -> Result<UpdateStatus, UpdateStatus> {
        if let Some(reason) = self.config.unavailable_reason() {
            self.set(Phase::Unavailable {
                reason: reason.to_string(),
            });
            return Err(self.status());
        }
        // Busy is not a successful check: the server was not contacted, so
        // the caller must not record it as one.
        self.exclusive(|| self.check_inner(), || Err(self.status()))
    }

    fn check_inner(&self) -> Result<UpdateStatus, UpdateStatus> {
        self.set(Phase::Checking);
        self.log.write("Update check started");

        let metadata_url = self.config.metadata_url(self.channel);
        let signature_url = self.config.signature_url(self.channel);

        let document = match self.transport.fetch(&metadata_url, MAX_METADATA_BYTES) {
            Ok(bytes) => bytes,
            Err(e) => return Err(self.network_failure(&e, "metadata")),
        };
        let signature = match self.transport.fetch(&signature_url, MAX_SIGNATURE_BYTES) {
            Ok(bytes) => bytes,
            Err(e) => return Err(self.network_failure(&e, "signature")),
        };
        self.log.write("Metadata received");

        let signature = String::from_utf8_lossy(&signature);
        if let Err(e) = verify::verify_signature(&self.config.public_key, &document, &signature) {
            return Err(self.fail(
                FailureKind::Verification,
                "The update information could not be verified, so it was ignored.",
                &format!("Metadata signature rejected ({e:?}); nothing was used"),
            ));
        }
        self.log.write("Metadata signature verified");

        let release = match metadata::parse(&document, self.channel) {
            Ok(r) => r,
            Err(e) => {
                return Err(self.fail(
                    FailureKind::Verification,
                    &e.to_string(),
                    &format!("Signed metadata refused: {e}"),
                ))
            }
        };

        let latest = release.parsed_version();
        self.log
            .write(&format!("Current version: {}", self.current));
        self.log.write(&format!("Latest version: {latest}"));

        match version::relation(&self.current, &latest) {
            Relation::Same | Relation::Older => {
                self.log.write("Up to date");
                *self.offer.lock() = None;
                self.set(Phase::UpToDate {
                    latest: latest.to_string(),
                });
                Ok(self.status())
            }
            Relation::Newer => match self.build_offer(&release) {
                Ok(offer) => {
                    self.log.write("Update available");
                    let info = offer.info.clone();
                    *self.offer.lock() = Some(offer);
                    // A package that was already downloaded and verified for
                    // this version stays ready.
                    if let Some(ready) = self.ready_for(&info) {
                        self.set(ready);
                    } else {
                        self.set(Phase::Available { release: info });
                    }
                    Ok(self.status())
                }
                Err(e) => Err(self.fail(
                    FailureKind::Verification,
                    &e.to_string(),
                    &format!("Release {latest} refused: {e}"),
                )),
            },
        }
    }

    fn ready_for(&self, info: &ReleaseInfo) -> Option<Phase> {
        let path = self.ready_package.lock().clone()?;
        if path.exists() && path.to_string_lossy().contains(&info.version) {
            Some(Phase::Ready {
                release: info.clone(),
                manual_install: false,
            })
        } else {
            None
        }
    }

    fn build_offer(&self, release: &ReleaseMetadata) -> Result<Offer, metadata::MetadataError> {
        let package = release.installer_for(&self.platform, &self.config.allowed_hosts)?;
        let mut note = None;
        if !release.kind.installable() {
            note = Some(
                "This update is a kind this version of AllInsight cannot install itself."
                    .to_string(),
            );
        } else if package.is_none() {
            note = Some(format!(
                "There is no automatic update for this platform ({}) in this release. Download it from the AllInsight website.",
                self.platform
            ));
        } else if !release.supports_upgrade_from(&self.current) {
            note = Some(format!(
                "This version cannot update directly to {}. Download the full installer from the AllInsight website.",
                release.version
            ));
        }
        Ok(Offer {
            info: ReleaseInfo {
                version: release.parsed_version().to_string(),
                release_date: release.release_date.clone(),
                notes: release.release_notes.iter().take(20).cloned().collect(),
                security: release.security || release.kind == UpdateKind::Security,
                kind: release.kind,
                size: package.as_ref().map(|p| p.size).unwrap_or(0),
                installable: note.is_none(),
                note,
            },
            package,
        })
    }

    fn network_failure(&self, error: &NetError, what: &str) -> UpdateStatus {
        if error.is_connectivity() {
            self.fail(
                FailureKind::Offline,
                "Unable to check for updates. AllInsight is still fully functional offline.",
                &format!("Update check could not connect while fetching the {what} ({error:?})"),
            )
        } else if matches!(error, NetError::Refused(_)) {
            self.fail(
                FailureKind::Verification,
                "The update server address was refused because it is not a trusted HTTPS address.",
                &format!("Request for the {what} refused before connecting ({error:?})"),
            )
        } else {
            self.fail(
                FailureKind::Network,
                "Couldn't check for updates. Check your Internet connection and try again.",
                &format!("Update server error while fetching the {what} ({error:?})"),
            )
        }
    }

    /// Download and verify the offered package. Only on the user's request.
    pub fn download(&self, progress: &mut dyn FnMut(&UpdateStatus)) -> UpdateStatus {
        self.exclusive(|| self.download_inner(progress), || self.status())
    }

    fn download_inner(&self, progress: &mut dyn FnMut(&UpdateStatus)) -> UpdateStatus {
        let Some(offer) = self.offer.lock().clone() else {
            return self.fail(
                FailureKind::Network,
                "Check for updates first.",
                "Download requested with no update on offer",
            );
        };
        let (Some(package), true) = (offer.package.clone(), offer.info.installable) else {
            return self.fail(
                FailureKind::Install,
                offer
                    .info
                    .note
                    .as_deref()
                    .unwrap_or("This update cannot be installed automatically."),
                "Download requested for an update that cannot be installed here",
            );
        };

        if let Err(e) = std::fs::create_dir_all(&self.staging) {
            return self.fail(
                FailureKind::Install,
                "The update could not be saved on this computer.",
                &format!("Could not create the update folder: {}", e.kind()),
            );
        }
        let final_path = self.staging.join(package_file_name(
            &offer.info.version,
            &self.platform,
            &package,
        ));
        let part_path = final_path.with_extension("part");

        self.log
            .write(&format!("Downloading {}", file_name(&final_path)));
        self.set(Phase::Downloading {
            release: offer.info.clone(),
            downloaded: 0,
            total: none_if_zero(package.size),
        });
        progress(&self.status());

        let result = (|| {
            let mut file = std::fs::File::create(&part_path)
                .map_err(|e| NetError::Interrupted(e.to_string()))?;
            let mut last_reported = 0u64;
            let written = self.transport.download(
                &package.url,
                &mut file,
                MAX_PACKAGE_BYTES,
                &mut |done, total| {
                    // Report roughly every 256 KB, plus the final byte.
                    if done == 0 || done - last_reported >= 256 * 1024 || Some(done) == total {
                        last_reported = done;
                        self.set(Phase::Downloading {
                            release: offer.info.clone(),
                            downloaded: done,
                            total: total.or(none_if_zero(package.size)),
                        });
                        progress(&self.status());
                    }
                },
            )?;
            file.sync_all()
                .map_err(|e| NetError::Interrupted(e.to_string()))?;
            Ok::<u64, NetError>(written)
        })();

        if let Err(e) = result {
            let _ = std::fs::remove_file(&part_path);
            return if e.is_connectivity() {
                self.fail(
                    FailureKind::Offline,
                    "The download stopped because the connection was lost. Nothing was installed.",
                    &format!("Download interrupted ({e:?}); partial file removed"),
                )
            } else {
                self.fail(
                    FailureKind::Network,
                    "The update could not be downloaded. Nothing was installed.",
                    &format!("Download failed ({e:?}); partial file removed"),
                )
            };
        }

        self.set(Phase::Verifying {
            release: offer.info.clone(),
        });
        progress(&self.status());
        self.log.write("Verifying update");

        if let Err(reason) = self.verify_package(&part_path, &package) {
            let _ = std::fs::remove_file(&part_path);
            return self.fail(
                FailureKind::Verification,
                "Update verification failed. For your security, the update was not installed.",
                &format!("Verification failed: {reason}; package deleted"),
            );
        }

        let _ = std::fs::remove_file(&final_path);
        if let Err(e) = std::fs::rename(&part_path, &final_path) {
            let _ = std::fs::remove_file(&part_path);
            return self.fail(
                FailureKind::Install,
                "The update could not be saved on this computer.",
                &format!("Could not keep the verified package: {}", e.kind()),
            );
        }

        self.log.write("Update verified and ready");
        *self.ready_package.lock() = Some(final_path);
        let manual = !matches!(
            package.format,
            metadata::PackageFormat::Nsis | metadata::PackageFormat::Appimage
        ) || (package.format == metadata::PackageFormat::Appimage
            && std::env::var_os("APPIMAGE").is_none());
        self.set(Phase::Ready {
            release: offer.info,
            manual_install: manual,
        });
        let status = self.status();
        progress(&status);
        status
    }

    /// The checks a package must pass before it may run. Called after
    /// download and again immediately before handing over.
    fn verify_package(&self, path: &Path, package: &InstallerEntry) -> Result<(), String> {
        match verify::checksum_matches(path, &package.sha256) {
            Ok(true) => {}
            Ok(false) => return Err("SHA-256 mismatch".into()),
            Err(e) => return Err(format!("could not read the package ({})", e.kind())),
        }
        if package.format == metadata::PackageFormat::Nsis {
            let signature = verify::code_signature(path);
            self.log.write(&format!("Code signature: {signature:?}"));
            if !verify::code_signature_acceptable(
                &signature,
                self.config.required_publisher.as_deref(),
            ) {
                return Err(format!("code signature not acceptable ({signature:?})"));
            }
        }
        Ok(())
    }

    /// Install the verified package. `before` runs first (the database
    /// backup) and a failure there stops the install.
    pub fn install(
        &self,
        before: impl FnOnce() -> Result<(), String>,
    ) -> Result<Handover, UpdateStatus> {
        self.exclusive(|| self.install_inner(before), || Err(self.status()))
    }

    fn install_inner(
        &self,
        before: impl FnOnce() -> Result<(), String>,
    ) -> Result<Handover, UpdateStatus> {
        let offer = self.offer.lock().clone();
        let path = self.ready_package.lock().clone();
        let (Some(offer), Some(path)) = (offer, path) else {
            return Err(self.fail(
                FailureKind::Install,
                "There is no verified update to install.",
                "Install requested with nothing ready",
            ));
        };
        let Some(package) = offer.package.clone() else {
            return Err(self.fail(
                FailureKind::Install,
                "There is no verified update to install.",
                "Install requested with no package",
            ));
        };

        // The file sat on disk between download and now; check it again.
        if let Err(reason) = self.verify_package(&path, &package) {
            let _ = std::fs::remove_file(&path);
            *self.ready_package.lock() = None;
            return Err(self.fail(
                FailureKind::Verification,
                "Update verification failed. For your security, the update was not installed.",
                &format!("Pre-install verification failed: {reason}; package deleted"),
            ));
        }

        if let Err(e) = before() {
            return Err(self.fail(
                FailureKind::Install,
                "Your data could not be backed up first, so the update was not installed.",
                &format!("Pre-install backup failed: {e}"),
            ));
        }
        self.log.write("Database backed up before installing");

        let pending = PendingInstall {
            from_version: self.current.to_string(),
            to_version: offer.info.version.clone(),
            package: file_name(&path),
            started_at: chrono::Utc::now().timestamp(),
        };
        if let Err(e) = install::write_pending(&self.staging, &pending) {
            return Err(self.fail(
                FailureKind::Install,
                "The update could not be started.",
                &format!("Could not record the pending install: {}", e.kind()),
            ));
        }

        match install::hand_over(package.format, &path) {
            Ok(handover) => {
                self.log.write(&format!(
                    "Installing {} -> {} ({handover:?})",
                    pending.from_version, pending.to_version
                ));
                Ok(handover)
            }
            Err(e) => {
                let _ = std::fs::remove_file(install::pending_path(&self.staging));
                Err(self.fail(
                    FailureKind::Install,
                    "The installer could not be started. AllInsight was not changed.",
                    &format!("Installer did not start: {}", e.kind()),
                ))
            }
        }
    }

    /// The user chose "Later". The offer stays; the dialog goes away.
    pub fn later(&self) -> UpdateStatus {
        let phase = self.phase.lock().clone();
        if let Phase::Failed { .. } = phase {
            let offer = self.offer.lock().clone();
            self.set(match offer {
                Some(o) => Phase::Available { release: o.info },
                None => Phase::Idle,
            });
        }
        self.status()
    }
}

fn package_file_name(version: &str, platform: &str, package: &InstallerEntry) -> String {
    let extension = match package.format {
        metadata::PackageFormat::Nsis => "exe",
        metadata::PackageFormat::Appimage => "AppImage",
        metadata::PackageFormat::Deb => "deb",
        metadata::PackageFormat::Rpm => "rpm",
        metadata::PackageFormat::Dmg => "dmg",
    };
    // Built from validated parts only, never from the URL.
    let version: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect();
    format!("AllInsight-{version}-{platform}.{extension}")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn none_if_zero(n: u64) -> Option<u64> {
    (n > 0).then_some(n)
}

fn remove_partials(staging: &Path) {
    if let Ok(entries) = std::fs::read_dir(staging) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "part") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests;

//! End-to-end tests of the update manager against a stand-in server.
//!
//! The metadata fixture in `testdata/` is signed with a throwaway key made
//! for these tests; its public half is `testdata/test.pub`. The package the
//! fixture describes is the four bytes `test`.

use super::*;
use transport::tests::FakeServer;

const KEY: &str = include_str!("testdata/test.pub");
const DOC: &[u8] = include_bytes!("testdata/latest.json");
const SIG: &str = include_str!("testdata/latest.json.sig");
const BASE: &str = "https://github.com/Sk1750-alt/AllInsight/releases/latest/download";

fn config() -> UpdateConfig {
    UpdateConfig {
        enabled: true,
        base_url: BASE.into(),
        public_key: KEY.into(),
        required_publisher: None,
        allowed_hosts: vec!["github.com".into()],
    }
}

fn package_url() -> String {
    let platform = channel::current_platform_key();
    let file = if platform.starts_with("windows") {
        "AllInsight_9.0.0_x64-setup.exe"
    } else {
        "AllInsight_9.0.0_amd64.AppImage"
    };
    format!("https://github.com/Sk1750-alt/AllInsight/releases/download/v9.0.0/{file}")
}

fn has_package() -> bool {
    matches!(
        channel::current_platform_key().as_str(),
        "windows-x64" | "linux-x64"
    )
}

struct Harness {
    server: Arc<FakeServer>,
    manager: UpdateManager,
    dir: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn harness(tag: &str, installed: &str, config: UpdateConfig) -> Harness {
    let dir = std::env::temp_dir().join(format!("allinsight-upd-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let server = Arc::new(FakeServer::default());
    let manager = UpdateManager::new(
        config,
        server.clone(),
        dir.join("updates"),
        Some(dir.join("logs").join("updates.log")),
        version::parse(installed).unwrap(),
    );
    Harness {
        server,
        manager,
        dir,
    }
}

fn serve_genuine(server: &FakeServer) {
    server.answer(&format!("{BASE}/latest.json"), Ok(DOC.to_vec()));
    server.answer(
        &format!("{BASE}/latest.json.sig"),
        Ok(SIG.as_bytes().to_vec()),
    );
    server.answer(&package_url(), Ok(b"test".to_vec()));
}

fn phase(h: &Harness) -> Phase {
    h.manager.status().phase
}

// ------------------------------------------------------------ versions

#[test]
fn an_older_installation_is_offered_the_update() {
    let h = harness("older", "1.0.0", config());
    serve_genuine(&h.server);
    let status = h.manager.check().unwrap();
    let Phase::Available { release } = status.phase else {
        panic!("expected an update, got {:?}", status.phase);
    };
    assert_eq!(release.version, "9.0.0");
    assert_eq!(
        release.notes,
        vec!["Test fixture signed with a throwaway key"]
    );
    assert_eq!(release.installable, has_package());
}

#[test]
fn the_same_version_is_up_to_date() {
    let h = harness("same", "9.0.0", config());
    serve_genuine(&h.server);
    assert_eq!(
        h.manager.check().unwrap().phase,
        Phase::UpToDate {
            latest: "9.0.0".into()
        }
    );
}

#[test]
fn a_newer_installation_is_up_to_date_and_never_downgraded() {
    let h = harness("newer", "10.0.0", config());
    serve_genuine(&h.server);
    assert_eq!(
        h.manager.check().unwrap().phase,
        Phase::UpToDate {
            latest: "9.0.0".into()
        }
    );
}

#[test]
fn a_version_below_the_minimum_is_told_to_reinstall() {
    let h = harness("minimum", "0.9.0", config());
    serve_genuine(&h.server);
    let Phase::Available { release } = h.manager.check().unwrap().phase else {
        panic!()
    };
    assert!(!release.installable);
    assert!(release.note.is_some());
}

// ------------------------------------------------------------ network failures

fn failure(status: Result<UpdateStatus, UpdateStatus>) -> FailureKind {
    match status.expect_err("expected a failure").phase {
        Phase::Failed { kind, .. } => kind,
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn no_internet_is_reported_as_offline_not_as_an_error() {
    let h = harness("offline", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Err(NetError::Offline));
    assert_eq!(failure(h.manager.check()), FailureKind::Offline);
    let Phase::Failed { message, .. } = phase(&h) else {
        panic!()
    };
    assert!(message.contains("fully functional offline"));
}

#[test]
fn a_timeout_is_reported_as_offline() {
    let h = harness("timeout", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Err(NetError::Timeout));
    assert_eq!(failure(h.manager.check()), FailureKind::Offline);
}

#[test]
fn a_dns_failure_is_reported_as_offline() {
    // The real transport maps ureq's DNS error kind to Offline.
    let h = harness("dns", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Err(NetError::Offline));
    assert_eq!(failure(h.manager.check()), FailureKind::Offline);
}

#[test]
fn an_unavailable_server_is_a_network_failure() {
    let h = harness("503", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Err(NetError::Status(503)));
    assert_eq!(failure(h.manager.check()), FailureKind::Network);
}

#[test]
fn missing_metadata_is_a_network_failure() {
    let h = harness("404", "1.0.0", config());
    assert_eq!(failure(h.manager.check()), FailureKind::Network);
}

#[test]
fn a_missing_signature_is_a_network_failure_and_nothing_is_trusted() {
    let h = harness("nosig", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Ok(DOC.to_vec()));
    assert_eq!(failure(h.manager.check()), FailureKind::Network);
    assert!(h.manager.offer.lock().is_none());
}

#[test]
fn invalid_json_is_refused_by_the_signature_first() {
    let h = harness("badjson", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Ok(b"{ not json".to_vec()));
    h.server.answer(
        &format!("{BASE}/latest.json.sig"),
        Ok(SIG.as_bytes().to_vec()),
    );
    assert_eq!(failure(h.manager.check()), FailureKind::Verification);
}

#[test]
fn a_bad_signature_is_a_verification_failure() {
    let h = harness("badsig", "1.0.0", config());
    h.server
        .answer(&format!("{BASE}/latest.json"), Ok(DOC.to_vec()));
    h.server
        .answer(&format!("{BASE}/latest.json.sig"), Ok(b"garbage".to_vec()));
    assert_eq!(failure(h.manager.check()), FailureKind::Verification);
}

#[test]
fn tampered_metadata_is_refused() {
    let h = harness("tamper", "1.0.0", config());
    let tampered = String::from_utf8(DOC.to_vec())
        .unwrap()
        .replace("9f86d081", "00000000");
    h.server
        .answer(&format!("{BASE}/latest.json"), Ok(tampered.into_bytes()));
    h.server.answer(
        &format!("{BASE}/latest.json.sig"),
        Ok(SIG.as_bytes().to_vec()),
    );
    assert_eq!(failure(h.manager.check()), FailureKind::Verification);
    assert!(h.manager.offer.lock().is_none());
}

#[test]
fn a_build_without_a_key_does_not_check_at_all() {
    let mut config = config();
    config.public_key = String::new();
    let h = harness("nokey", "1.0.0", config);
    serve_genuine(&h.server);
    assert!(h.manager.check().is_err());
    assert!(matches!(phase(&h), Phase::Unavailable { .. }));
    assert!(
        h.server.requests.lock().is_empty(),
        "no request may be made"
    );
}

#[test]
fn an_http_update_server_is_refused_without_a_request() {
    let mut config = config();
    config.base_url = "http://github.com/Sk1750-alt/AllInsight/releases/latest/download".into();
    let h = harness("http", "1.0.0", config);
    assert!(h.manager.check().is_err());
    assert!(matches!(phase(&h), Phase::Unavailable { .. }));
    assert!(h.server.requests.lock().is_empty());
}

#[test]
fn an_untrusted_update_server_is_refused_without_a_request() {
    let mut config = config();
    config.base_url = "https://evil.example/download".into();
    let h = harness("untrusted", "1.0.0", config);
    assert!(h.manager.check().is_err());
    assert!(h.server.requests.lock().is_empty());
}

#[test]
fn a_disabled_build_never_checks() {
    let mut config = config();
    config.enabled = false;
    let h = harness("disabled", "1.0.0", config);
    serve_genuine(&h.server);
    assert!(h.manager.check().is_err());
    assert!(h.server.requests.lock().is_empty());
}

// ------------------------------------------------------------ privacy

#[test]
fn a_check_sends_only_the_fixed_requests() {
    let h = harness("privacy", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();

    let requests = h.server.requests.lock().clone();
    let urls: Vec<&str> = requests.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            format!("{BASE}/latest.json").as_str(),
            format!("{BASE}/latest.json.sig").as_str()
        ]
    );
    for (url, headers) in &requests {
        assert!(
            !url.contains('?') && !url.contains('#'),
            "no query string: {url}"
        );
        assert_eq!(
            headers,
            &vec![("User-Agent".to_string(), "AllInsight-Updater".to_string())],
            "only the approved header"
        );
    }
}

#[test]
fn requests_contain_nothing_about_the_user_or_machine() {
    let h = harness("pii", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();
    if has_package() {
        h.manager.download(&mut |_| {});
    }

    let mut forbidden: Vec<String> = Vec::new();
    for var in ["USERNAME", "USER", "COMPUTERNAME", "HOSTNAME", "USERDOMAIN"] {
        if let Ok(v) = std::env::var(var) {
            if v.len() >= 3 {
                forbidden.push(v.to_ascii_lowercase());
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        forbidden.push(home.to_string_lossy().to_ascii_lowercase());
    }
    forbidden.push(h.dir.to_string_lossy().to_ascii_lowercase());
    forbidden.push(channel::current_platform_key());
    forbidden.push("1.0.0".into());

    for (url, headers) in h.server.requests.lock().iter() {
        let mut sent = url.to_ascii_lowercase();
        for (k, v) in headers {
            sent.push_str(&format!(" {k}: {v}").to_ascii_lowercase());
        }
        // The project URL names its owner; that is the server, not the user.
        let sent = sent.replace("sk1750-alt", "");
        for item in &forbidden {
            assert!(
                !sent.contains(item.as_str()),
                "request leaked {item:?}: {sent}"
            );
        }
    }
}

// ------------------------------------------------------------ download and verify

#[test]
fn a_genuine_package_downloads_and_verifies() {
    if !has_package() {
        return;
    }
    let h = harness("download", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();

    let mut seen = Vec::new();
    let status = h.manager.download(&mut |s| seen.push(s.phase.clone()));
    assert!(
        matches!(status.phase, Phase::Ready { .. }),
        "{:?}",
        status.phase
    );
    assert!(seen.iter().any(|p| matches!(p, Phase::Downloading { .. })));
    assert!(seen.iter().any(|p| matches!(p, Phase::Verifying { .. })));

    let ready = h.manager.ready_package.lock().clone().unwrap();
    assert_eq!(std::fs::read(&ready).unwrap(), b"test");
    assert!(ready
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("AllInsight-9.0.0-"));
}

#[test]
fn a_package_with_the_wrong_checksum_is_deleted_and_not_installed() {
    if !has_package() {
        return;
    }
    let h = harness("wrongsha", "1.0.0", config());
    serve_genuine(&h.server);
    h.server.answers.lock().retain(|(u, _)| u != &package_url());
    h.server.answer(&package_url(), Ok(b"TEST".to_vec()));
    h.manager.check().unwrap();

    let status = h.manager.download(&mut |_| {});
    assert_eq!(
        status.phase,
        Phase::Failed {
            kind: FailureKind::Verification,
            message: "Update verification failed. For your security, the update was not installed."
                .into()
        }
    );
    assert!(h.manager.ready_package.lock().is_none());
    let leftovers: Vec<_> = std::fs::read_dir(h.dir.join("updates"))
        .unwrap()
        .flatten()
        .collect();
    assert!(leftovers.is_empty(), "the rejected package must be deleted");

    // And there is nothing to install.
    assert!(h.manager.install(|| Ok(())).is_err());
}

#[test]
fn an_interrupted_download_leaves_nothing_behind() {
    if !has_package() {
        return;
    }
    let h = harness("dropped", "1.0.0", config());
    serve_genuine(&h.server);
    h.server.answers.lock().retain(|(u, _)| u != &package_url());
    h.server.answer(
        &package_url(),
        Err(NetError::Interrupted("connection reset".into())),
    );
    h.manager.check().unwrap();

    let status = h.manager.download(&mut |_| {});
    assert!(matches!(
        status.phase,
        Phase::Failed {
            kind: FailureKind::Network,
            ..
        }
    ));
    let leftovers: Vec<_> = std::fs::read_dir(h.dir.join("updates"))
        .unwrap()
        .flatten()
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_package_altered_after_download_is_refused_at_install() {
    if !has_package() {
        return;
    }
    let h = harness("swapped", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();
    h.manager.download(&mut |_| {});
    let ready = h.manager.ready_package.lock().clone().unwrap();
    std::fs::write(&ready, b"evil").unwrap();

    let mut backed_up = false;
    let result = h.manager.install(|| {
        backed_up = true;
        Ok(())
    });
    assert!(result.is_err());
    assert!(!backed_up, "verification happens before anything else");
    assert!(!ready.exists());
    assert!(!install::pending_path(&h.dir.join("updates")).exists());
}

#[test]
fn a_failed_backup_stops_the_install() {
    if !has_package() {
        return;
    }
    let h = harness("nobackup", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();
    h.manager.download(&mut |_| {});
    let result = h.manager.install(|| Err("disk full".into()));
    assert!(result.is_err());
    assert!(!install::pending_path(&h.dir.join("updates")).exists());
    assert!(
        h.manager.ready_package.lock().clone().unwrap().exists(),
        "the package is kept"
    );
}

#[test]
fn nothing_downloads_without_a_check_first() {
    let h = harness("nocheck", "1.0.0", config());
    serve_genuine(&h.server);
    let status = h.manager.download(&mut |_| {});
    assert!(matches!(status.phase, Phase::Failed { .. }));
    assert!(h.server.requests.lock().is_empty());
}

// ------------------------------------------------------------ after a restart

#[test]
fn a_completed_update_is_reported_once_after_restart() {
    let h = harness("reconcile", "9.0.0", config());
    let staging = h.dir.join("updates");
    install::write_pending(
        &staging,
        &PendingInstall {
            from_version: "1.0.0".into(),
            to_version: "9.0.0".into(),
            package: "AllInsight-9.0.0-windows-x64.exe".into(),
            started_at: 0,
        },
    )
    .unwrap();
    h.manager.reconcile_previous_install();
    assert_eq!(
        h.manager.status().last_install,
        Some(InstallReport::Completed {
            version: "9.0.0".into()
        })
    );
    let log = std::fs::read_to_string(h.dir.join("logs").join("updates.log")).unwrap();
    assert!(log.contains("Update to 9.0.0 completed"));
}

#[test]
fn the_log_holds_only_updater_messages() {
    let h = harness("log", "1.0.0", config());
    serve_genuine(&h.server);
    h.manager.check().unwrap();
    let log = std::fs::read_to_string(h.dir.join("logs").join("updates.log")).unwrap();
    for expected in [
        "Update check started",
        "Metadata received",
        "Current version: 1.0.0",
        "Latest version: 9.0.0",
        "Update available",
    ] {
        assert!(log.contains(expected), "missing {expected:?} in\n{log}");
    }
    if let Some(home) = dirs::home_dir() {
        assert!(
            !log.contains(&*home.to_string_lossy()),
            "no user paths in the log"
        );
    }
}

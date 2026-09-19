//! End-to-end safety tests.
//!
//! The unit tests inside each module check that module. These check the
//! properties that only hold when the pieces are put together, through the
//! same public API the application uses, in a real temporary directory on a
//! real filesystem.
//!
//! If any test in this file fails, the safety model is broken and the build
//! should not ship.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use allinsight_lib::services::cleanup::{self, definitions, CleanupCategory, CleanupRequest};
use allinsight_lib::services::security::guard::EntryKind;
use allinsight_lib::services::security::{paths, DeletionGuard, GuardRejection, ProtectedPaths};
use allinsight_lib::services::storage::{duplicates, large_files, scanner};

fn sandbox(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "allinsight-e2e-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("create sandbox");
    paths::canonicalize(&base).unwrap_or(base)
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

// ---------------------------------------------------------------------------
// The protected list
// ---------------------------------------------------------------------------

/// The folders the product brief calls PROTECTED must be refused, all of them,
/// on this machine as it is actually configured.
#[test]
fn every_protected_user_folder_is_refused() {
    let engine = ProtectedPaths::new(&[]);

    let folders = [
        dirs::document_dir(),
        dirs::desktop_dir(),
        dirs::picture_dir(),
        dirs::video_dir(),
        dirs::audio_dir(),
        dirs::download_dir(),
    ];

    for folder in folders.into_iter().flatten() {
        assert!(
            engine.is_protected(&folder),
            "{} must be protected",
            folder.display()
        );
        assert!(
            engine.is_protected(&folder.join("anything.tmp")),
            "contents of {} must be protected",
            folder.display()
        );
        assert!(
            engine.is_protected(&folder.join("nested").join("deep").join("file.bin")),
            "deeply nested contents of {} must be protected",
            folder.display()
        );
    }
}

#[test]
fn windows_and_program_files_are_refused() {
    let engine = ProtectedPaths::new(&[]);
    for template in ["%SystemRoot%", "%ProgramFiles%", "%ProgramData%"] {
        let Some(root) = paths::expand_env(template) else {
            continue;
        };
        assert!(engine.is_protected(&root), "{} must be protected", root.display());
        assert!(engine.is_protected(&root.join("subfolder").join("file.dll")));
    }
}

/// The single most important property in the application: no cleanup category
/// can be pointed at protected storage unless that exact folder is one of the
/// compiled-in carve-outs.
#[test]
fn no_cleanup_category_can_reach_protected_storage() {
    let engine = ProtectedPaths::new(&[]);
    for definition in definitions() {
        for root in &definition.roots {
            let verdict = engine.classify(root);
            if verdict.protected {
                assert!(
                    engine.is_carve_out(root),
                    "{:?} declares protected root {} which is not a carve-out",
                    definition.id,
                    root.display()
                );
            }
        }
    }
}

/// A user-added protected folder outranks everything, including a category
/// allow-list that happens to contain it.
#[test]
fn a_user_protected_folder_cannot_be_cleaned_by_any_category() {
    let root = sandbox("userprotected");
    let file = root.join("scratch.tmp");
    fs::write(&file, vec![0u8; 4096]).unwrap();

    let mut engine = ProtectedPaths::new(&[]);
    engine.set_user_roots(&[root.clone()]);

    // Point a guard at the folder anyway, as a hostile caller would.
    let guard = DeletionGuard::new(&engine, &[root.clone()]);
    let rejection = guard
        .validate(&file, EntryKind::File)
        .expect_err("a user-protected folder must never be cleanable");
    assert!(matches!(rejection, GuardRejection::Protected { .. }));
    assert!(file.exists());

    cleanup(&root);
}

// ---------------------------------------------------------------------------
// The deletion guard
// ---------------------------------------------------------------------------

#[test]
fn traversal_out_of_an_allowed_root_is_refused() {
    let root = sandbox("traversal");
    let engine = ProtectedPaths::new(&[]);
    let guard = DeletionGuard::new(&engine, &[root.clone()]);

    for payload in [
        "..\\..\\Windows\\System32\\drivers\\etc\\hosts",
        "..\\escape.txt",
        "sub\\..\\..\\escape.txt",
    ] {
        let hostile = root.join(payload);
        assert!(
            guard.validate(&hostile, EntryKind::File).is_err(),
            "{payload} must be refused"
        );
    }

    cleanup(&root);
}

#[cfg(windows)]
#[test]
fn a_junction_cannot_be_used_to_reach_protected_data() {
    let root = sandbox("junction-e2e");
    let outside = sandbox("junction-target-e2e");
    let treasure = outside.join("important.txt");
    fs::write(&treasure, b"user data").unwrap();

    let link = root.join("link");
    let created = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if created {
        let engine = ProtectedPaths::new(&[]);
        let guard = DeletionGuard::new(&engine, &[root.clone()]);

        assert!(
            guard.validate(&link, EntryKind::Directory).is_err(),
            "the junction itself must be refused"
        );
        assert!(
            guard
                .validate(&link.join("important.txt"), EntryKind::File)
                .is_err(),
            "a path through the junction must be refused"
        );
        assert!(treasure.exists(), "the target must be untouched");
    }

    cleanup(&root);
    cleanup(&outside);
}

// ---------------------------------------------------------------------------
// The cleanup engine
// ---------------------------------------------------------------------------

#[test]
fn a_dry_run_removes_nothing() {
    let engine = ProtectedPaths::new(&[]);
    let cancelled = AtomicBool::new(false);

    let (preview, scan) = cleanup::discover(&engine, None, &cancelled);

    let outcome = cleanup::execute(
        &engine,
        &scan,
        &CleanupRequest {
            scan_id: preview.scan_id,
            categories: vec![CleanupCategory::ThumbnailCache],
            candidate_ids: Vec::new(),
            confirmed: false,
        },
        &cancelled,
    )
    .expect("a dry run must succeed");

    assert!(outcome.dry_run);
    // Everything the dry run claimed is still on disk.
    for candidate in scan.by_category(CleanupCategory::ThumbnailCache) {
        assert!(
            candidate.path.exists(),
            "{} must survive a dry run",
            candidate.path.display()
        );
    }
}

#[test]
fn an_unconfirmed_request_is_always_a_dry_run() {
    let engine = ProtectedPaths::new(&[]);
    let cancelled = AtomicBool::new(false);
    let (preview, scan) = cleanup::discover(&engine, Some(&[CleanupCategory::UserTemp]), &cancelled);

    let outcome = cleanup::execute(
        &engine,
        &scan,
        &CleanupRequest {
            scan_id: preview.scan_id,
            categories: vec![CleanupCategory::UserTemp],
            candidate_ids: Vec::new(),
            confirmed: false,
        },
        &cancelled,
    )
    .unwrap();

    assert!(outcome.dry_run);
}

#[test]
fn a_candidate_id_from_another_scan_is_refused() {
    let engine = ProtectedPaths::new(&[]);
    let cancelled = AtomicBool::new(false);

    let (first, _) = cleanup::discover(&engine, Some(&[CleanupCategory::IconCache]), &cancelled);
    let (_, second_scan) =
        cleanup::discover(&engine, Some(&[CleanupCategory::IconCache]), &cancelled);

    let error = cleanup::execute(
        &engine,
        &second_scan,
        &CleanupRequest {
            scan_id: first.scan_id,
            categories: vec![CleanupCategory::IconCache],
            candidate_ids: Vec::new(),
            confirmed: true,
        },
        &cancelled,
    )
    .expect_err("a stale scan id must be refused");

    assert!(error.to_string().contains("out of date"));
}

/// Discovery must never offer a protected file as a cleanup candidate, except
/// through one of the two narrow, compiled-in exemptions.
///
/// Both exemptions are checked here rather than waved through, because the
/// value of this test is that it fails the moment a third one appears.
#[test]
fn discovery_never_offers_protected_paths() {
    use allinsight_lib::services::security::ProtectionReason;

    let engine = ProtectedPaths::new(&[]);
    let cancelled = AtomicBool::new(false);
    let (_, scan) = cleanup::discover(&engine, None, &cancelled);

    for id in &scan.order {
        let candidate = scan.candidate(id).expect("candidate present");
        let verdict = engine.classify(&candidate.path);
        if !verdict.protected {
            continue;
        }

        let definition = cleanup::definition_for(candidate.category).expect("definition");
        let reason = verdict.reason.expect("a refusal always carries a reason");

        // Exemption one: the candidate is inside a directory that is both one
        // of this category's declared roots and a compiled-in carve-out, and
        // the refusal came from that enclosing root rather than from the file
        // itself.
        let via_carve_out = ProtectedPaths::reason_is_system_root(reason)
            && definition.roots.iter().any(|root| {
                paths::is_strictly_within(&candidate.path, root) && engine.is_carve_out(root)
            });

        // Exemption two: the file is a declared cache file refused only by the
        // blanket database-extension rule, e.g. `thumbcache_1280.db`.
        let name = paths::file_name_lower(&candidate.path);
        let via_name_exemption = reason == ProtectionReason::Database
            && definition
                .name_exemptions
                .iter()
                .any(|prefix| name.starts_with(&prefix.to_lowercase()));

        assert!(
            via_carve_out || via_name_exemption,
            "{} was offered as a {:?} candidate but is protected ({reason:?})",
            candidate.path.display(),
            candidate.category
        );

        // Whichever exemption applied, the candidate must still be inside one
        // of that category's own roots. Neither exemption widens location.
        assert!(
            definition
                .roots
                .iter()
                .any(|root| paths::is_strictly_within(&candidate.path, root)),
            "{} is outside every root declared by {:?}",
            candidate.path.display(),
            candidate.category
        );
    }
}

#[test]
fn auto_clean_never_includes_an_irreversible_or_elevated_category() {
    let auto = cleanup::auto_clean_categories();
    assert!(!auto.contains(&CleanupCategory::RecycleBin));
    assert!(!auto.contains(&CleanupCategory::WindowsUpdateCache));
    assert!(!auto.contains(&CleanupCategory::DeliveryOptimizationCache));
    assert!(!auto.contains(&CleanupCategory::ComponentStoreLogs));
    assert!(!auto.contains(&CleanupCategory::BrowserCache));
    assert!(!auto.is_empty());
}

/// The one place a real removal is exercised, in a sandbox the guard has been
/// explicitly pointed at. It proves the whole chain works, not just that it
/// refuses things.
#[test]
fn a_validated_file_in_an_allowed_root_is_actually_removed() {
    let root = sandbox("realremoval");
    let file = root.join("disposable.tmp");
    fs::write(&file, vec![0u8; 8192]).unwrap();

    let engine = ProtectedPaths::new(&[]);
    let guard = DeletionGuard::new(&engine, &[root.clone()]);
    let validated = guard.validate(&file, EntryKind::File).expect("approved");
    assert_eq!(validated.size_bytes(), 8192);

    cleanup::remove::remove(&validated, cleanup::DeletionMode::Permanent).expect("removed");
    assert!(!file.exists());

    cleanup(&root);
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[test]
fn scan_totals_match_what_was_written() {
    let root = sandbox("totals");
    let mut expected = 0u64;
    for (index, size) in [512usize, 4096, 16384, 65536].into_iter().enumerate() {
        let dir = root.join(format!("dir{index}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("payload.bin"), vec![0u8; size]).unwrap();
        expected += size as u64;
    }

    let result = scanner::scan(
        scanner::ScanOptions::for_root(&root),
        std::sync::Arc::new(scanner::ScanProgress::default()),
    );

    assert_eq!(result.total_bytes, expected);
    assert_eq!(result.total_files, 4);
    assert!(!result.cancelled);

    cleanup(&root);
}

#[test]
fn large_file_search_respects_the_threshold_and_marks_protection() {
    let root = sandbox("largefiles");
    fs::write(root.join("small.bin"), vec![0u8; 1024]).unwrap();
    fs::write(root.join("big.bin"), vec![0u8; 200_000]).unwrap();
    fs::write(root.join("vault.kdbx"), vec![0u8; 200_000]).unwrap();

    let engine = ProtectedPaths::new(&[]);
    let report = large_files::find(
        large_files::LargeFileQuery {
            roots: vec![root.clone()],
            min_bytes: 100_000,
            limit: 50,
        },
        &engine,
        std::sync::Arc::new(scanner::ScanProgress::default()),
    );

    assert_eq!(report.entries.len(), 2);
    let vault = report
        .entries
        .iter()
        .find(|e| e.name.ends_with(".kdbx"))
        .expect("the key store must be listed");
    assert_eq!(vault.risk, large_files::RiskLevel::Protected);
    assert!(vault.risk_note.is_some());

    cleanup(&root);
}

#[test]
fn duplicate_detection_is_by_content_not_by_name() {
    let root = sandbox("dupes");
    let payload = vec![42u8; 300_000];

    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    // Same content, different names.
    fs::write(root.join("a").join("holiday.jpg"), &payload).unwrap();
    fs::write(root.join("b").join("copy-of-something.jpg"), &payload).unwrap();
    // Same name, different content.
    let mut different = payload.clone();
    different[150_000] = 7;
    fs::write(root.join("a").join("report.bin"), &payload).unwrap();
    fs::write(root.join("b").join("report.bin"), &different).unwrap();

    let engine = ProtectedPaths::new(&[]);
    let report = duplicates::find(
        duplicates::DuplicateQuery {
            roots: vec![root.clone()],
            min_bytes: 1000,
            max_groups: 50,
        },
        &engine,
        std::sync::Arc::new(scanner::ScanProgress::default()),
    );

    // `report.bin` in `a` matches the two images, because it holds the same
    // bytes; the differing `report.bin` in `b` must not be in any group.
    let grouped: Vec<&str> = report
        .groups
        .iter()
        .flat_map(|g| g.files.iter())
        .map(|f| f.path.to_str().unwrap())
        .collect();

    assert!(grouped.iter().any(|p| p.ends_with("holiday.jpg")));
    assert!(grouped.iter().any(|p| p.ends_with("copy-of-something.jpg")));
    assert!(
        !grouped.iter().any(|p| p.ends_with(&format!("b{}report.bin", std::path::MAIN_SEPARATOR))),
        "a file differing in the middle must not be grouped"
    );

    cleanup(&root);
}

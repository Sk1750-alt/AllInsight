//! Updates: the IPC surface over `services::update`.
//!
//! This is the only place the updater meets the rest of AllInsight, and what
//! crosses is small and fixed: the time of the last successful check (an
//! integer in the settings table), the user's two update preferences, and a
//! closure that backs up the database before an install. The updater never
//! sees the database itself.

// See services::update: the status doubles as the error type.
#![allow(clippy::result_large_err)]

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use once_cell::sync::Lazy;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::{Alert, EVENT_ALERT};
use crate::error::{AllInsightError, Result};
use crate::services::update::install::Handover;
use crate::services::update::{Phase, UpdateManager, UpdateStatus};
use crate::state::AppState;

/// Every change of update state is pushed on this event as an [`UpdateView`].
pub const EVENT_UPDATE: &str = "allinsight://update";

/// Unix seconds of the last check that reached the server and verified.
const LAST_CHECK_KEY: &str = "update.last_check";

/// A failed automatic check waits at least this long before the next try,
/// so being offline does not mean a connection attempt every few minutes.
const RETRY_AFTER_FAILURE_SECS: i64 = 6 * 60 * 60;

/// Automatic checks never run in the first minutes after launch: starting
/// AllInsight is not a reason to contact anyone.
const QUIET_AFTER_LAUNCH_SECS: u64 = 10 * 60;

static LAUNCHED: Lazy<Instant> = Lazy::new(Instant::now);
static LAST_AUTO_ATTEMPT: AtomicI64 = AtomicI64::new(0);

pub type Updates = Arc<UpdateManager>;

/// What the Settings screen shows.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateView {
    #[serde(flatten)]
    pub status: UpdateStatus,
    pub last_checked: Option<i64>,
    pub auto_check: bool,
    pub check_interval_hours: u32,
    /// True when the event came from an automatic check that found an
    /// update, so the interface should ask the user.
    pub prompt: bool,
}

/// Build the manager and settle the previous install. Called from setup.
pub fn create() -> Updates {
    Lazy::force(&LAUNCHED);
    let manager = UpdateManager::for_application(&crate::state::data_directory());
    manager.reconcile_previous_install();
    Arc::new(manager)
}

fn view(state: &AppState, status: UpdateStatus, prompt: bool) -> UpdateView {
    let settings = state.settings();
    UpdateView {
        status,
        last_checked: last_checked(state),
        auto_check: settings.update_auto_check,
        check_interval_hours: settings.update_check_interval_hours,
        prompt,
    }
}

fn last_checked(state: &AppState) -> Option<i64> {
    state
        .db
        .get_setting(LAST_CHECK_KEY)
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
}

fn record_check(state: &AppState) {
    let _ = state
        .db
        .set_setting(LAST_CHECK_KEY, &chrono::Utc::now().timestamp().to_string());
}

#[tauri::command]
pub async fn get_update_status(
    state: State<'_, AppState>,
    updates: State<'_, Updates>,
) -> Result<UpdateView> {
    Ok(view(&state, updates.status(), false))
}

#[tauri::command]
pub async fn check_for_updates(app: AppHandle, updates: State<'_, Updates>) -> Result<UpdateView> {
    let manager = updates.inner().clone();
    let emitter = app.clone();
    let _ = app.emit(
        EVENT_UPDATE,
        view(
            &app.state::<AppState>(),
            {
                let mut s = manager.status();
                s.phase = Phase::Checking;
                s
            },
            false,
        ),
    );
    let result = tauri::async_runtime::spawn_blocking(move || manager.check())
        .await
        .map_err(|e| {
            AllInsightError::Other(format!("The update check stopped unexpectedly: {e}"))
        })?;
    let state = emitter.state::<AppState>();
    let status = match result {
        Ok(status) => {
            record_check(&state);
            status
        }
        Err(status) => status,
    };
    let out = view(&state, status, false);
    let _ = emitter.emit(EVENT_UPDATE, out.clone());
    Ok(out)
}

#[tauri::command]
pub async fn download_update(app: AppHandle, updates: State<'_, Updates>) -> Result<UpdateView> {
    let manager = updates.inner().clone();
    let emitter = app.clone();
    let status = tauri::async_runtime::spawn_blocking(move || {
        manager.download(&mut |status| {
            let state = emitter.state::<AppState>();
            let _ = emitter.emit(EVENT_UPDATE, view(&state, status.clone(), false));
        })
    })
    .await
    .map_err(|e| AllInsightError::Other(format!("The download stopped unexpectedly: {e}")))?;
    let out = view(&app.state::<AppState>(), status, false);
    let _ = app.emit(EVENT_UPDATE, out.clone());
    Ok(out)
}

/// Install the verified update. On Windows this starts the installer and
/// closes AllInsight so it can replace the program files; the installer
/// starts AllInsight again when it is done.
#[tauri::command]
pub async fn install_update(app: AppHandle, updates: State<'_, Updates>) -> Result<UpdateView> {
    let manager = updates.inner().clone();
    let backup_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        manager.install(|| {
            let state = backup_app.state::<AppState>();
            let from = env!("CARGO_PKG_VERSION");
            let directory = crate::state::data_directory().join("backups");
            let destination = directory.join(format!("pre-update-{from}.db"));
            state
                .db
                .backup_to(&destination)
                .map_err(|e| e.to_string())?;
            prune_update_backups(&directory, PRE_UPDATE_KEEP);
            Ok(())
        })
    })
    .await
    .map_err(|e| AllInsightError::Other(format!("The update stopped unexpectedly: {e}")))?;

    let state = app.state::<AppState>();
    match result {
        Ok(Handover::ExitForInstaller) => {
            let exiting = app.clone();
            // Give the interface a moment to say what is happening.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(800));
                exiting.exit(0);
            });
            Ok(view(&state, updates.status(), false))
        }
        Ok(Handover::RestartInto(path)) => {
            relaunch_after_exit(&path).map_err(|e| {
                AllInsightError::Other(format!(
                    "The update is installed, but AllInsight could not restart itself ({e}). Start it again to finish."
                ))
            })?;
            let exiting = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                exiting.exit(0);
            });
            Ok(view(&state, updates.status(), false))
        }
        Ok(Handover::InstallManually(path)) => {
            use tauri_plugin_opener::OpenerExt;
            let _ = app.opener().reveal_item_in_dir(&path);
            Ok(view(&state, updates.status(), false))
        }
        Err(status) => {
            let out = view(&state, status, false);
            let _ = app.emit(EVENT_UPDATE, out.clone());
            Ok(out)
        }
    }
}

/// Database copies taken before updates, kept. Each is a full copy, so only
/// the most recent few stay.
const PRE_UPDATE_KEEP: usize = 3;

fn prune_update_backups(directory: &std::path::Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut copies: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("pre-update-") && n.ends_with(".db"))
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    copies.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (_, path) in copies.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

/// Start the replaced AppImage once this process has gone.
///
/// Starting it directly would race the single-instance guard: the new copy
/// would find this one still running, hand over to it, and quit, and then
/// this one would exit too, leaving nothing open. A shell that waits for this
/// process id to disappear avoids that.
#[cfg(unix)]
fn relaunch_after_exit(path: &std::path::Path) -> std::io::Result<()> {
    let pid = std::process::id().to_string();
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; exec "$2""#)
        .arg("allinsight-relaunch")
        .arg(pid)
        .arg(path)
        .spawn()
        .map(|_| ())
}

#[cfg(not(unix))]
fn relaunch_after_exit(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new(path).spawn().map(|_| ())
}

/// The user chose "Later".
#[tauri::command]
pub async fn dismiss_update(
    state: State<'_, AppState>,
    updates: State<'_, Updates>,
) -> Result<UpdateView> {
    Ok(view(&state, updates.later(), false))
}

/// Called by the background monitor on every wake. Checks only when the user
/// turned automatic checks on, the configured interval has passed since the
/// last successful check, and AllInsight has been running for a while. Never
/// downloads or installs: it only tells the user.
pub fn auto_check_if_due(app: &AppHandle) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let state = app.state::<AppState>();
    let settings = state.settings();
    if !settings.update_auto_check || !updates.is_available() {
        return;
    }
    if LAUNCHED.elapsed().as_secs() < QUIET_AFTER_LAUNCH_SECS {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let interval = i64::from(settings.update_check_interval_hours) * 3600;
    if let Some(last) = last_checked(&state) {
        if now - last < interval {
            return;
        }
    }
    if now - LAST_AUTO_ATTEMPT.load(Ordering::Relaxed) < RETRY_AFTER_FAILURE_SECS {
        return;
    }
    // Do not interrupt something the user started.
    if matches!(
        updates.status().phase,
        Phase::Checking | Phase::Downloading { .. } | Phase::Verifying { .. } | Phase::Ready { .. }
    ) {
        return;
    }
    LAST_AUTO_ATTEMPT.store(now, Ordering::Relaxed);

    let manager = updates.inner().clone();
    let app = app.clone();
    std::thread::Builder::new()
        .name("allinsight-update-check".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            match manager.check() {
                Ok(status) => {
                    record_check(&state);
                    if let Phase::Available { release } = &status.phase {
                        let _ = app.emit(EVENT_UPDATE, view(&state, status.clone(), true));
                        let _ = app.emit(
                            EVENT_ALERT,
                            Alert {
                                id: format!("update-{}", release.version),
                                severity: "info".into(),
                                title: format!("AllInsight {} is available", release.version),
                                body: "Open Settings, then Updates, to see what's new. Nothing is installed until you choose to."
                                    .into(),
                            },
                        );
                    }
                }
                // Quietly: an automatic check that fails is not the user's
                // problem, and AllInsight works the same offline.
                Err(_) => {
                    let _ = manager.later();
                }
            }
        })
        .ok();
}

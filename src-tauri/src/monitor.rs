//! The background monitor.
//!
//! One thread, waking on an interval the user controls, doing the smallest
//! amount of work that keeps the alerts honest: read volume capacity, record a
//! sample, check thresholds, and check drive health. It never walks the
//! filesystem, so its cost stays flat regardless of how many files exist.
//!
//! Auto-Clean runs from here too, but only within the boundaries set
//! elsewhere: the compiled-in eligible category list, intersected with what the
//! user opted into, and only when free space has actually fallen below their
//! threshold.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::commands::{Alert, EVENT_ALERT};
use crate::services::cleanup::{self, CleanupRequest};
use crate::services::health::HealthState;
use crate::services::storage::format_bytes;
use crate::state::{AppState, CleanupOwner};

/// Start the monitor. Returns immediately; the work happens on its own thread.
pub fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("allinsight-monitor".into())
        .spawn(move || run(app))
        .ok();
}

fn run(app: AppHandle) {
    // A short delay so the first tick does not compete with the window
    // appearing.
    std::thread::sleep(Duration::from_secs(5));

    loop {
        let interval = {
            let state = app.state::<AppState>();
            let settings = state.settings();
            // A daily copy of the settings, kept for a week, whether or not
            // background monitoring is on: it is one small file a day.
            if let Err(e) = crate::services::config::ensure_daily_backup(
                &crate::commands::settings::backup_directory(),
                &settings,
            ) {
                tracing::warn!("daily settings backup failed: {e}");
            }
            if settings.background_monitoring {
                tick(&app, &state);
            }
            settings.monitor_interval_seconds.max(60)
        };
        std::thread::sleep(Duration::from_secs(interval as u64));
    }
}

fn tick(app: &AppHandle, state: &AppState) {
    let settings = state.settings();
    let overview = crate::services::storage::overview();

    for volume in &overview.volumes {
        if !volume.is_ready || !volume.kind.is_scannable() {
            continue;
        }
        let _ = state
            .db
            .record_volume_sample(&volume.mount_point, volume.total_bytes, volume.free_bytes);

        if !settings.notifications_enabled {
            continue;
        }

        let name = volume.mount_point.trim_end_matches('\\');
        let used = volume.used_percent.round() as u8;

        // Only the highest threshold that has been crossed fires, so passing
        // 80 and 90 in one step produces one message, not two.
        if let Some(threshold) = settings
            .alert_at_percent
            .iter()
            .rev()
            .find(|t| used >= **t)
            .copied()
        {
            let key = format!("storage-{name}-{threshold}");
            if state.should_alert(&key, settings.notification_quiet_minutes) {
                let severity = if threshold >= 95 {
                    "critical"
                } else if threshold >= 90 {
                    "warning"
                } else {
                    "advice"
                };
                let title = match severity {
                    "critical" => format!("Critical storage shortage on {name}"),
                    "warning" => format!("Only {}% of {name} remains free", 100 - used),
                    _ => format!("Storage usage on {name} is getting high"),
                };
                let body = format!(
                    "{} free of {}.",
                    format_bytes(volume.free_bytes),
                    format_bytes(volume.total_bytes)
                );
                notify(app, state, &key, severity, &title, &body);
            }
        }
    }

    if settings.notify_drive_health {
        if let Ok(report) = crate::services::health::report() {
            for drive in report.drives {
                let severity = match drive.state {
                    HealthState::Critical => "critical",
                    HealthState::Warning => "warning",
                    _ => continue,
                };
                let key = format!("drive-{}-{severity}", drive.device_id);
                if state.should_alert(&key, settings.notification_quiet_minutes.max(720)) {
                    notify(
                        app,
                        state,
                        &key,
                        severity,
                        &format!("{} reported a health warning", drive.model),
                        &drive
                            .notes
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "Open Drive Health for details.".into()),
                    );
                }
            }
        }
    }

    if settings.auto_clean_enabled {
        maybe_auto_clean(app, state);
    }
}

/// Run Auto-Clean when free space has fallen below the user's threshold.
fn maybe_auto_clean(app: &AppHandle, state: &AppState) {
    let settings = state.settings();
    let Some(system) = crate::services::storage::volumes::system_volume() else {
        return;
    };
    if system.total_bytes == 0 {
        return;
    }

    let free_percent = (system.free_bytes as f64 / system.total_bytes as f64 * 100.0) as u8;
    if free_percent >= settings.auto_clean_free_space_percent {
        return;
    }

    // The categories are the intersection of the compiled-in eligible list and
    // what the user chose. An empty result means there is nothing to do, not
    // that everything is fair game.
    let categories = settings.effective_auto_clean_categories();
    if categories.is_empty() {
        return;
    }

    // Do not fight a scan the user started.
    if state.scan_running.load(Ordering::SeqCst) {
        return;
    }

    // Never contend with a cleanup the user started, and stand down at once
    // if one begins while this is running.
    let Ok(_lease) = state.begin_cleanup(CleanupOwner::Automatic) else {
        return;
    };
    state.cleanup_cancel.store(false, Ordering::SeqCst);

    let cancel = Arc::clone(&state.cleanup_cancel);
    let protected = state.protected();
    let (preview, scan) = cleanup::discover(&protected, Some(&categories), &cancel);

    // Discovery stops early when the user preempts, and a partial list is not
    // something to act on unasked.
    if cancel.load(Ordering::SeqCst) {
        return;
    }

    // Not worth interrupting anyone for less than a quarter of a gigabyte.
    if preview.total_bytes < 256 * 1024 * 1024 {
        return;
    }

    let outcome = {
        cleanup::execute(
            &protected,
            &scan,
            &CleanupRequest {
                scan_id: preview.scan_id,
                categories,
                candidate_ids: Vec::new(),
                confirmed: true,
            },
            &cancel,
        )
    };

    match outcome {
        Ok(outcome) if outcome.reclaimed_bytes > 0 => {
            crate::commands::cleanup::record(state, &outcome, "auto");
            if state.settings().notifications_enabled {
                notify(
                    app,
                    state,
                    "auto-clean",
                    "advice",
                    "Automatic cleanup finished",
                    &format!(
                        "{} was reclaimed from temporary and cached data.",
                        format_bytes(outcome.reclaimed_bytes)
                    ),
                );
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(target: "allinsight::monitor", "automatic cleanup failed: {e}");
        }
    }
}

/// Send one alert to the interface and to the Windows notification centre.
fn notify(app: &AppHandle, state: &AppState, id: &str, severity: &str, title: &str, body: &str) {
    let _ = app.emit(
        EVENT_ALERT,
        Alert {
            id: id.to_string(),
            severity: severity.to_string(),
            title: title.to_string(),
            body: body.to_string(),
        },
    );

    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }

    let _ = state.db.log_activity("alert", title, Some(body));
}

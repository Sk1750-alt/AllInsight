//! AllInsight - local device intelligence and storage manager.
//!
//! The backend is split in three:
//!
//! * `services` does the work and owns every decision that matters
//! * `commands` is a thin, typed IPC surface over those services
//! * `state` holds what must outlive a single request
//!
//! The safety model lives in `services::security`. Nothing in this crate
//! removes a file except through `services::cleanup::remove`, which requires a
//! token that only the deletion guard can produce.

pub mod commands;
pub mod error;
pub mod logging;
pub mod monitor;
pub mod platform;
pub mod services;
pub mod state;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

use state::AppState;

/// Build the notification-area icon.
///
/// It exists so that "keep running when the window is closed" is true rather
/// than a claim: without a tray icon, closing the last window ends the process
/// on Windows and the background monitor goes with it.
fn install_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open AllInsight", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit AllInsight", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("allinsight")
        .tooltip("AllInsight - watching storage and drive health")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    // Whatever is on screen was measured before the window was hidden, which
    // may have been a long time ago. Drop the caches so the first refresh
    // after it reappears shows the machine as it is now.
    if let Some(state) = app.try_state::<AppState>() {
        state.invalidate_caches();
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = logging::init();

    tauri::Builder::default()
        // Must be registered first: it decides whether this process should
        // live at all. Closing the window only hides it to the notification
        // area, so launching AllInsight again would otherwise start a second copy
        // that competes with the hidden one for the database and the tray.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::info!(
                target: "allinsight",
                "a second launch was folded into the running window"
            );
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            // Closing the window hides it when the user asked AllInsight to keep
            // watching, and exits otherwise. The tray icon is the only way back
            // in, so it must exist before this can be honoured.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let keep_running = app
                    .try_state::<AppState>()
                    .map(|state| state.settings().minimise_to_tray)
                    .unwrap_or(false);
                if keep_running && app.tray_by_id("allinsight").is_some() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let state = AppState::new().map_err(|e| {
                tracing::error!(target: "allinsight", "could not start: {e}");
                e
            })?;

            let settings = state.settings();

            // Every parallel walk in AllInsight runs on the global rayon pool, so
            // this is the one place the worker count is decided. Zero means
            // "match the machine", which is rayon's own default.
            if settings.scan_threads > 0 {
                let requested = settings.scan_threads as usize;
                if let Err(e) = rayon::ThreadPoolBuilder::new()
                    .num_threads(requested)
                    .build_global()
                {
                    tracing::warn!(
                        target: "allinsight",
                        "could not set the scan worker count to {requested}: {e}"
                    );
                }
            }

            app.manage(state);

            // Load the model in the background when the user asked for that,
            // so a large GGUF never delays the first frame.
            if settings.ai_enabled && settings.ai_load_automatically && settings.ai_model_path.is_some()
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let state = handle.state::<AppState>();
                    let settings = state.settings();

                    let Some(model) = settings.ai_model_path.clone() else {
                        tracing::warn!(
                            target: "allinsight::ai",
                            "automatic load skipped: no model is selected"
                        );
                        return;
                    };
                    let Some(engine) =
                        services::ai::models::find_engine(settings.ai_engine_path.as_deref())
                    else {
                        tracing::warn!(
                            target: "allinsight::ai",
                            "automatic load skipped: llama-server.exe was not found in {}",
                            services::ai::models::engine_directory().display()
                        );
                        return;
                    };

                    tracing::info!(
                        target: "allinsight::ai",
                        "loading {} with {}",
                        model.display(),
                        engine.display()
                    );

                    // The outcome is logged either way. Discarding it left a
                    // failed automatic load with no trace at all, which made
                    // the feature impossible to diagnose from the outside.
                    match state.engine.start(services::ai::EngineConfig {
                        engine_path: engine,
                        model_path: model,
                        context_size: settings.ai_context_size,
                        threads: settings.ai_threads,
                        gpu_layers: settings.ai_gpu_layers,
                    }) {
                        Ok(status) => {
                            tracing::info!(target: "allinsight::ai", "{}", status.message)
                        }
                        Err(e) => {
                            tracing::error!(target: "allinsight::ai", "automatic load failed: {e}")
                        }
                    }
                });
            }

            // On Linux the tray needs libayatana-appindicator at run time, and
            // the binding panics rather than erroring when it is missing, so the
            // panic is caught and treated like any other tray failure.
            let tray = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                install_tray(app.handle())
            }))
            .unwrap_or_else(|_| {
                Err(tauri::Error::AssetNotFound(
                    "the system tray library (libayatana-appindicator3)".into(),
                ))
            });
            if let Err(e) = tray {
                // A missing tray is a degraded experience, not a reason to
                // refuse to start. The close handler checks for it before
                // hiding the window, so the application stays coherent.
                tracing::warn!(target: "allinsight", "the tray icon could not be created: {e}");
            }

            monitor::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Storage
            commands::storage::get_storage_overview,
            commands::storage::scan_directory,
            commands::storage::get_scan_summary,
            commands::storage::get_scan_progress,
            commands::storage::cancel_scan,
            commands::storage::get_treemap_level,
            commands::storage::find_large_files,
            commands::storage::get_large_files,
            commands::storage::find_duplicates,
            commands::storage::get_duplicates,
            commands::storage::get_volume_trend,
            commands::storage::show_in_explorer,
            // Cleanup
            commands::cleanup::get_cleanup_candidates,
            commands::cleanup::preview_cleanup,
            commands::cleanup::execute_cleanup,
            commands::cleanup::get_cleanup_categories,
            commands::cleanup::get_cleanup_history,
            commands::cleanup::get_cleanup_totals,
            commands::cleanup::get_recycle_bin_state,
            commands::cleanup::recycle_reviewed_file,
            // Device
            commands::device::get_system_summary,
            commands::device::get_metrics_history,
            commands::device::get_processes,
            commands::device::end_process,
            commands::device::get_process_location,
            commands::device::get_drive_health,
            commands::device::get_battery_status,
            commands::device::get_installed_applications,
            commands::device::uninstall_application,
            commands::device::get_startup_items,
            commands::device::set_startup_enabled,
            commands::device::get_dashboard,
            commands::device::get_environment,
            commands::device::get_activity,
            // AI
            commands::ai::get_ai_status,
            commands::ai::get_local_models,
            commands::ai::import_local_model,
            commands::ai::remove_local_model,
            commands::ai::load_ai_model,
            commands::ai::unload_ai_model,
            commands::ai::get_ai_insight,
            commands::ai::ask_ai,
            commands::ai::get_ai_context,
            commands::ai::get_insights,
            commands::ai::get_device_score,
            // Settings
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::complete_first_run,
            commands::settings::get_last_route,
            commands::settings::set_last_route,
            commands::settings::get_protected_paths,
            commands::settings::get_cleanup_exceptions,
            commands::settings::add_protected_path,
            commands::settings::remove_protected_path,
            commands::settings::export_diagnostics,
            commands::settings::restart_elevated,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AllInsight");
}

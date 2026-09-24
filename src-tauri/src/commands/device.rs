//! Device commands: metrics, processes, drives, battery, applications and
//! startup entries.

use serde::Serialize;
use tauri::State;

use crate::error::Result;
use crate::services::apps::{self, AppList};
use crate::services::battery::BatteryStatus;
use crate::services::health::DriveHealthReport;
use crate::services::process::{self, ProcessList};
use crate::services::startup::{self, StartupList};
use crate::services::system::{MetricsHistory, SystemSnapshot};
use crate::state::AppState;

#[tauri::command]
pub async fn get_system_summary(state: State<'_, AppState>) -> Result<SystemSnapshot> {
    Ok({
    state.monitor.sample()
})
}

#[tauri::command]
pub async fn get_metrics_history(state: State<'_, AppState>) -> Result<MetricsHistory> {
    Ok({
    state.monitor.history()
})
}

#[tauri::command]
pub async fn get_processes(
    state: State<'_, AppState>,
    limit: Option<usize>,
    resolve_publishers: Option<bool>,
) -> Result<ProcessList> {
    Ok({
    process::list(
        &state.monitor,
        limit.unwrap_or(80).min(500),
        resolve_publishers.unwrap_or(true),
    )
})
}

#[tauri::command]
pub async fn end_process(state: State<'_, AppState>, pid: u32, confirmed: bool) -> Result<()> {
    // The setting only ever tightens the rule. With it on, every process needs
    // an explicit confirmation; with it off, Windows components still do, and
    // the critical list is refused inside the service either way.
    if state.settings().require_confirmation_for_processes && !confirmed {
        return Err(crate::error::AllInsightError::InvalidInput(
            "Confirm before ending a process, or turn that requirement off in Settings.".into(),
        ));
    }
    process::terminate(&state.monitor, pid, confirmed)?;
    let _ = state
        .db
        .log_activity("process", &format!("Ended process {pid}"), None);
    Ok(())
}

#[tauri::command]
pub async fn get_process_location(
    state: State<'_, AppState>,
    pid: u32,
) -> Result<Option<String>> {
    Ok(process::location_of(&state.monitor, pid).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn get_drive_health(state: State<'_, AppState>) -> Result<DriveHealthReport> {
    Ok(state.drive_report())
}

#[tauri::command]
pub async fn get_battery_status(state: State<'_, AppState>) -> Result<BatteryStatus> {
    Ok(state.battery_status())
}

#[tauri::command]
pub async fn get_installed_applications(measure_sizes: Option<bool>) -> AppList {
    apps::list(measure_sizes.unwrap_or(false))
}

#[tauri::command]
pub async fn uninstall_application(state: State<'_, AppState>, id: String) -> Result<()> {
    apps::uninstall(&id)?;
    let _ = state.db.log_activity(
        "application",
        "Started the vendor uninstaller for an application",
        None,
    );
    Ok(())
}

#[tauri::command]
pub async fn get_startup_items() -> StartupList {
    startup::list()
}

#[tauri::command]
pub async fn set_startup_enabled(state: State<'_, AppState>, id: String, enabled: bool) -> Result<()> {
    startup::set_enabled(&id, enabled)?;
    let _ = state.db.log_activity(
        "startup",
        if enabled {
            "Enabled a startup item"
        } else {
            "Disabled a startup item"
        },
        None,
    );
    Ok(())
}

/// Everything the Overview needs, in one round trip, so the dashboard renders
/// in a single frame rather than assembling itself from eight requests.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    pub storage: crate::services::storage::StorageOverview,
    pub system: SystemSnapshot,
    pub drives: DriveHealthReport,
    pub battery: BatteryStatus,
    pub score: crate::services::ai::DeviceScore,
    pub insights: Vec<crate::services::ai::Insight>,
    pub summary: crate::services::ai::AiAnswer,
    pub reclaimable_bytes: u64,
    pub scanned: bool,
    pub elevated: bool,
}

/// One gather, reused for every field.
///
/// Assembling the fact set queries WMI and enumerates the registry, so calling
/// it once and reading the pieces out is the difference between one round of
/// that work per Overview load and three.
#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardSnapshot> {
    Ok({
    let inputs = state.dashboard_inputs();
    DashboardSnapshot {
        storage: crate::services::storage::overview(),
        score: crate::services::ai::device_score(&inputs.facts),
        insights: crate::services::ai::insights::generate(&inputs.facts),
        summary: crate::services::ai::overview_insight(&state.engine, &inputs.facts),
        reclaimable_bytes: inputs.facts.reclaimable_bytes,
        scanned: inputs.facts.storage_scanned,
        elevated: crate::services::security::is_elevated(),
        system: inputs.system,
        drives: inputs.drives,
        battery: inputs.battery,
    }
})
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentInfo {
    /// The operating system family, which the interface uses to pick its
    /// wording (Recycle Bin or Trash, File Explorer or the file manager).
    pub platform: crate::platform::Platform,
    /// The Linux desktop environment, when there is one.
    pub desktop: Option<String>,
    pub elevated: bool,
    pub os_name: String,
    pub host_name: String,
    pub app_version: String,
    pub data_directory: String,
    pub log_directory: String,
    /// Always true, and stated so the Privacy screen is not merely a claim.
    pub offline_only: bool,
}

#[tauri::command]
pub async fn get_environment() -> EnvironmentInfo {
    let snapshot_os = sysinfo::System::long_os_version()
        .unwrap_or_else(|| crate::platform::os_name().into());
    EnvironmentInfo {
        platform: crate::platform::current(),
        desktop: crate::platform::desktop(),
        elevated: crate::services::security::is_elevated(),
        os_name: snapshot_os,
        host_name: sysinfo::System::host_name().unwrap_or_default(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        data_directory: crate::state::data_directory().to_string_lossy().into_owned(),
        log_directory: crate::state::data_directory()
            .join("logs")
            .to_string_lossy()
            .into_owned(),
        offline_only: true,
    }
}

#[tauri::command]
pub async fn get_activity(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<crate::services::db::ActivityEntry>> {
    state.db.activity(limit.unwrap_or(100).min(500))
}

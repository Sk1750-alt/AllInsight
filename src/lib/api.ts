/**
 * The typed client for the Rust backend.
 *
 * Every call goes through `call`, which turns a backend error into a thrown
 * `AllInsightError` carrying the message the backend wrote. Those messages are
 * written for people, so the interface shows them directly rather than
 * inventing its own wording.
 *
 * Note what is missing: there is no function here that sends a path to a
 * destructive command. Cleanup takes category names and backend-issued ids.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "./types";

export class AllInsightError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AllInsightError";
  }
}

async function call<R>(command: string, args?: Record<string, unknown>): Promise<R> {
  try {
    return await invoke<R>(command, args);
  } catch (error) {
    const message =
      typeof error === "string"
        ? error
        : error instanceof Error
          ? error.message
          : "Something went wrong and AllInsight could not complete that.";
    throw new AllInsightError(message);
  }
}

// ---------------------------------------------------------------- events

export const events = {
  SCAN_PROGRESS: "allinsight://scan-progress",
  SCAN_COMPLETE: "allinsight://scan-complete",
  ALERT: "allinsight://alert",
} as const;

export function onScanProgress(handler: (p: T.ScanProgressSnapshot) => void): Promise<UnlistenFn> {
  return listen<T.ScanProgressSnapshot>(events.SCAN_PROGRESS, (e) => handler(e.payload));
}

export function onScanComplete(handler: (payload: unknown) => void): Promise<UnlistenFn> {
  return listen(events.SCAN_COMPLETE, (e) => handler(e.payload));
}

export function onAlert(handler: (a: T.Alert) => void): Promise<UnlistenFn> {
  return listen<T.Alert>(events.ALERT, (e) => handler(e.payload));
}

// --------------------------------------------------------------- storage

export const api = {
  getStorageOverview: () => call<T.StorageOverview>("get_storage_overview"),
  scanDirectory: (root: string) => call<void>("scan_directory", { root }),
  getScanSummary: (root: string) => call<T.ScanSummary | null>("get_scan_summary", { root }),
  getScanProgress: () => call<T.ScanProgressSnapshot | null>("get_scan_progress"),
  cancelScan: () => call<void>("cancel_scan"),
  getTreemapLevel: (root: string, path: string) =>
    call<T.TreemapNode[]>("get_treemap_level", { root, path }),
  findLargeFiles: (roots: string[], minBytes: number) =>
    call<void>("find_large_files", { roots, minBytes }),
  getLargeFiles: () => call<T.LargeFileReport | null>("get_large_files"),
  findDuplicates: (roots: string[], minBytes: number) =>
    call<void>("find_duplicates", { roots, minBytes }),
  getDuplicates: () => call<T.DuplicateReport | null>("get_duplicates"),
  getVolumeTrend: (mountPoint: string) => call<T.VolumeTrend>("get_volume_trend", { mountPoint }),
  showInExplorer: (path: string) => call<void>("show_in_explorer", { path }),

  // --------------------------------------------------------------- cleanup
  getCleanupCandidates: (categories?: T.CleanupCategory[]) =>
    call<T.CleanupPreview>("get_cleanup_candidates", { categories: categories ?? null }),
  previewCleanup: (scanId: number, categories: T.CleanupCategory[], candidateIds: string[] = []) =>
    call<T.CleanupOutcome>("preview_cleanup", { scanId, categories, candidateIds }),
  executeCleanup: (args: {
    scan_id: number;
    categories: T.CleanupCategory[];
    candidate_ids: string[];
    confirmed: boolean;
  }) => call<T.CleanupOutcome>("execute_cleanup", { args }),
  getCleanupCategories: () => call<T.CategoryDescription[]>("get_cleanup_categories"),
  getCleanupHistory: (limit = 50) => call<T.CleanupHistoryEntry[]>("get_cleanup_history", { limit }),
  getCleanupTotals: () => call<T.CleanupTotals>("get_cleanup_totals"),
  getRecycleBinState: () => call<T.RecycleBinState>("get_recycle_bin_state"),
  recycleReviewedFile: (path: string) => call<void>("recycle_reviewed_file", { path }),

  // ---------------------------------------------------------------- device
  getSystemSummary: () => call<T.SystemSnapshot>("get_system_summary"),
  getMetricsHistory: () => call<T.MetricsHistory>("get_metrics_history"),
  getProcesses: (limit = 80, resolvePublishers = true) =>
    call<T.ProcessList>("get_processes", { limit, resolvePublishers }),
  endProcess: (pid: number, confirmed: boolean) => call<void>("end_process", { pid, confirmed }),
  getProcessLocation: (pid: number) => call<string | null>("get_process_location", { pid }),
  getDriveHealth: () => call<T.DriveHealthReport>("get_drive_health"),
  getBatteryStatus: () => call<T.BatteryStatus>("get_battery_status"),
  getInstalledApplications: (measureSizes = false) =>
    call<T.AppList>("get_installed_applications", { measureSizes }),
  uninstallApplication: (id: string) => call<void>("uninstall_application", { id }),
  getStartupItems: () => call<T.StartupList>("get_startup_items"),
  setStartupEnabled: (id: string, enabled: boolean) =>
    call<void>("set_startup_enabled", { id, enabled }),
  getDashboard: () => call<T.DashboardSnapshot>("get_dashboard"),
  getEnvironment: () => call<T.EnvironmentInfo>("get_environment"),
  getActivity: (limit = 100) => call<T.ActivityEntry[]>("get_activity", { limit }),

  // -------------------------------------------------------------------- ai
  getAiStatus: () => call<T.EngineStatus>("get_ai_status"),
  getLocalModels: () => call<T.ModelInventory>("get_local_models"),
  importLocalModel: (path: string) => call<T.LocalModel>("import_local_model", { path }),
  removeLocalModel: (path: string) => call<void>("remove_local_model", { path }),
  loadAiModel: (modelPath?: string) =>
    call<T.EngineStatus>("load_ai_model", { modelPath: modelPath ?? null }),
  unloadAiModel: () => call<T.EngineStatus>("unload_ai_model"),
  getAiInsight: () => call<T.AiAnswer>("get_ai_insight"),
  askAi: (question: string) => call<T.AiAnswer>("ask_ai", { question }),
  getAiContext: () => call<string>("get_ai_context"),
  getInsights: () => call<T.Insight[]>("get_insights"),
  getDeviceScore: () => call<T.DeviceScore>("get_device_score"),

  // -------------------------------------------------------------- settings
  getSettings: () => call<T.Settings>("get_settings"),
  saveSettings: (settings: T.Settings) => call<T.Settings>("save_settings", { settings }),
  completeFirstRun: () => call<T.Settings>("complete_first_run"),
  getLastRoute: () => call<string | null>("get_last_route"),
  setLastRoute: (route: string) => call<void>("set_last_route", { route }),
  getProtectedPaths: () => call<T.ProtectedPathView[]>("get_protected_paths"),
  getCleanupExceptions: () => call<string[]>("get_cleanup_exceptions"),
  addProtectedPath: (path: string) => call<T.Settings>("add_protected_path", { path }),
  removeProtectedPath: (path: string) => call<T.Settings>("remove_protected_path", { path }),
  exportDiagnostics: (destination: string) =>
    call<string>("export_diagnostics", { destination }),
  restartElevated: () => call<void>("restart_elevated"),
};

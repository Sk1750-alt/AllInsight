/**
 * TypeScript mirrors of the Rust types that cross the IPC boundary.
 *
 * These are written by hand rather than generated, and kept in the same order
 * as the Rust definitions so a change on one side is easy to find on the
 * other. Anything optional in Rust is `| null` here, because serde emits
 * `null` rather than omitting the key.
 */

// ---------------------------------------------------------------- storage

export type DriveKind = "fixed" | "removable" | "network" | "optical" | "ram_disk" | "unknown";

export interface VolumeInfo {
  mount_point: string;
  letter: string;
  label: string | null;
  filesystem: string | null;
  kind: DriveKind;
  total_bytes: number;
  free_bytes: number;
  used_bytes: number;
  used_percent: number;
  is_system: boolean;
  is_ready: boolean;
}

export interface StorageOverview {
  volumes: VolumeInfo[];
  total_bytes: number;
  used_bytes: number;
  free_bytes: number;
  system_volume: string | null;
}

export type StorageCategory =
  | "applications"
  | "windows"
  | "downloads"
  | "desktop"
  | "documents"
  | "pictures"
  | "videos"
  | "music"
  | "temporary_files"
  | "cache"
  | "games"
  | "development"
  | "virtual_machines"
  | "archives"
  | "user_files"
  | "other";

export interface CategoryTotal {
  category: StorageCategory;
  label: string;
  bytes: number;
  files: number;
}

export interface TreemapNode {
  path: string;
  name: string;
  size_bytes: number;
  share: number;
  file_count: number;
  has_children: boolean;
  is_file_bucket: boolean;
}

export interface ScanSummary {
  root: string;
  total_bytes: number;
  total_files: number;
  total_dirs: number;
  skipped_dirs: number;
  skipped_links: number;
  duration_ms: number;
  completed_at: number;
  cancelled: boolean;
  categories: CategoryTotal[];
  top_folders: TreemapNode[];
}

export interface ScanProgressSnapshot {
  bytes: number;
  files: number;
  dirs: number;
  errors: number;
  cancelled: boolean;
  current: string;
  finished: boolean;
}

export type RiskLevel = "low" | "review" | "protected";

export interface LargeFileEntry {
  path: string;
  name: string;
  directory: string;
  size_bytes: number;
  modified: number | null;
  accessed: number | null;
  extension: string;
  category: StorageCategory;
  risk: RiskLevel;
  risk_note: string | null;
}

export interface LargeFileReport {
  entries: LargeFileEntry[];
  total_bytes: number;
  scanned_files: number;
  skipped_dirs: number;
  truncated: boolean;
  cancelled: boolean;
}

export interface DuplicateFile {
  path: string;
  name: string;
  directory: string;
  size_bytes: number;
  modified: number | null;
  protected: boolean;
  protection_note: string | null;
}

export interface DuplicateGroup {
  id: string;
  size_bytes: number;
  files: DuplicateFile[];
  reclaimable_bytes: number;
}

export interface DuplicateReport {
  groups: DuplicateGroup[];
  total_reclaimable_bytes: number;
  files_compared: number;
  files_hashed: number;
  cancelled: boolean;
  truncated: boolean;
}

export interface VolumeSample {
  sampled_at: number;
  total_bytes: number;
  free_bytes: number;
}

export interface VolumeTrend {
  mount_point: string;
  samples: VolumeSample[];
}

// ---------------------------------------------------------------- cleanup

export type CleanupCategory =
  | "windows_temp"
  | "user_temp"
  | "crash_dumps"
  | "windows_error_reporting"
  | "thumbnail_cache"
  | "icon_cache"
  | "shader_cache"
  | "browser_cache"
  | "windows_update_cache"
  | "delivery_optimization_cache"
  | "component_store_logs"
  | "font_cache"
  | "recycle_bin";

export type DeletionMode = "recycle" | "permanent" | "shell_api";

export interface CleanupCandidate {
  id: string;
  category: CleanupCategory;
  path: string;
  name: string;
  size_bytes: number;
  modified: number | null;
  kind: "file" | "directory";
}

export interface CategoryReport {
  category: CleanupCategory;
  name: string;
  description: string;
  what_happens: string;
  what_is_untouched: string;
  bytes: number;
  items: number;
  requires_elevation: boolean;
  auto_clean_eligible: boolean;
  deletion: DeletionMode;
  available: boolean;
  note: string | null;
  samples: CleanupCandidate[];
}

export interface CleanupPreview {
  scan_id: number;
  categories: CategoryReport[];
  total_bytes: number;
  total_items: number;
  protected_items: number;
  skipped_items: number;
  generated_at: number;
  elevated: boolean;
}

export interface CategoryOutcome {
  category: CleanupCategory;
  name: string;
  reclaimed_bytes: number;
  removed_items: number;
  skipped_items: number;
}

export interface CleanupOutcome {
  reclaimed_bytes: number;
  removed_items: number;
  skipped_items: number;
  protected_items: number;
  failed_items: number;
  categories: CategoryOutcome[];
  notes: string[];
  finished_at: number;
  dry_run: boolean;
}

export interface CategoryDescription {
  id: CleanupCategory;
  name: string;
  description: string;
  what_happens: string;
  what_is_untouched: string;
  requires_elevation: boolean;
  auto_clean_eligible: boolean;
  present: boolean;
  roots: string[];
}

export interface CleanupHistoryEntry {
  ran_at: number;
  trigger: string;
  reclaimed_bytes: number;
  removed_items: number;
  skipped_items: number;
  categories: string[];
}

export interface CleanupTotals {
  runs: number;
  reclaimed_bytes: number;
  removed_items: number;
}

export interface RecycleBinState {
  bytes: number;
  items: number;
  available: boolean;
}

// ----------------------------------------------------------------- system

export interface CpuStatus {
  usage_percent: number;
  core_usage: number[];
  physical_cores: number | null;
  logical_cores: number;
  brand: string;
  frequency_mhz: number;
}

export interface MemoryStatus {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  used_percent: number;
  swap_total_bytes: number;
  swap_used_bytes: number;
}

export interface NetworkStatus {
  download_bytes_per_sec: number;
  upload_bytes_per_sec: number;
  total_received_bytes: number;
  total_transmitted_bytes: number;
  interfaces: number;
}

export interface DiskActivity {
  read_bytes_per_sec: number;
  write_bytes_per_sec: number;
}

export interface GpuAdapter {
  name: string;
  driver_version: string | null;
  video_memory_bytes: number | null;
}

export interface GpuStatus {
  utilization_percent: number | null;
  adapters: GpuAdapter[];
  dedicated_memory_bytes: number | null;
  available: boolean;
  note: string | null;
}

export interface SystemSnapshot {
  cpu: CpuStatus;
  memory: MemoryStatus;
  network: NetworkStatus;
  disk: DiskActivity;
  gpu: GpuStatus;
  uptime_seconds: number;
  process_count: number;
  os_name: string;
  host_name: string;
  timestamp: number;
}

export interface MetricSample {
  timestamp: number;
  cpu_percent: number;
  memory_percent: number;
  gpu_percent: number | null;
  disk_read_bps: number;
  disk_write_bps: number;
  net_down_bps: number;
  net_up_bps: number;
}

export interface MetricsHistory {
  samples: MetricSample[];
  cpu_average: number;
  cpu_peak: number;
  memory_average: number;
  memory_peak: number;
}

// ---------------------------------------------------------------- process

export type ProcessRisk = "critical" | "system_component" | "normal";

export interface ProcessInfo {
  pid: number;
  parent_pid: number | null;
  name: string;
  executable: string | null;
  publisher: string | null;
  cpu_percent: number;
  memory_bytes: number;
  memory_percent: number;
  disk_read_bytes: number;
  disk_write_bytes: number;
  run_time_seconds: number;
  risk: ProcessRisk;
  is_top_cpu: boolean;
  is_top_memory: boolean;
}

export interface ProcessList {
  processes: ProcessInfo[];
  total: number;
  sampled_at: number;
}

// ----------------------------------------------------------------- health

export type HealthState = "healthy" | "warning" | "critical" | "unknown";
export type MediaKind = "hdd" | "ssd" | "storage_class_memory" | "unspecified";

export interface DriveHealth {
  device_id: string;
  model: string;
  serial_number: string | null;
  firmware: string | null;
  media: MediaKind;
  bus: string;
  size_bytes: number | null;
  spindle_speed_rpm: number | null;
  state: HealthState;
  windows_health: string | null;
  operational_status: string[];
  temperature_celsius: number | null;
  temperature_max_celsius: number | null;
  wear_percent: number | null;
  estimated_life_remaining_percent: number | null;
  power_on_hours: number | null;
  read_errors_total: number | null;
  read_errors_uncorrected: number | null;
  write_errors_total: number | null;
  write_errors_uncorrected: number | null;
  start_stop_cycles: number | null;
  volumes: string[];
  reliability_unavailable: boolean;
  elevation_would_help: boolean;
  notes: string[];
}

export interface DriveHealthReport {
  drives: DriveHealth[];
  elevated: boolean;
  error: string | null;
}

// ---------------------------------------------------------------- battery

export type PowerSource = "battery" | "ac_power" | "unknown";

export interface BatteryStatus {
  present: boolean;
  power_source: PowerSource;
  charging: boolean;
  charge_percent: number | null;
  runtime_seconds: number | null;
  design_capacity_mwh: number | null;
  full_charge_capacity_mwh: number | null;
  remaining_capacity_mwh: number | null;
  cycle_count: number | null;
  voltage_mv: number | null;
  chemistry: string | null;
  manufacturer: string | null;
  health_percent: number | null;
  notes: string[];
}

// ------------------------------------------------------------------- apps

export type AppScope = "all_users" | "current_user";

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string | null;
  version: string | null;
  estimated_size_bytes: number | null;
  measured_size_bytes: number | null;
  install_date: string | null;
  install_location: string | null;
  scope: AppScope;
  has_uninstaller: boolean;
  is_windows_component: boolean;
  source: AppSource;
  /** When removal needs an administrator, the command to run in a terminal. */
  uninstall_hint: string | null;
}

export type AppSource = "registry" | "dpkg" | "rpm" | "pacman" | "flatpak" | "snap" | "manual";

export interface AppList {
  apps: InstalledApp[];
  total: number;
}

// ---------------------------------------------------------------- startup

export type StartupImpact = "low" | "medium" | "high" | "unknown";
export type StartupLocation =
  | "user_run"
  | "machine_run"
  | "machine_run32"
  | "user_startup_folder"
  | "common_startup_folder"
  | "user_autostart"
  | "system_autostart";

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  executable: string | null;
  publisher: string | null;
  location: StartupLocation;
  location_label: string;
  enabled: boolean;
  impact: StartupImpact;
  impact_is_estimated: boolean;
  can_toggle: boolean;
}

export interface StartupList {
  items: StartupItem[];
  enabled_count: number;
  elevated: boolean;
}

// --------------------------------------------------------------------- ai

export type Severity = "critical" | "warning" | "advice" | "positive" | "neutral";

export type InsightAction =
  | "open_cleanup"
  | "open_large_files"
  | "open_duplicates"
  | "open_storage_map"
  | "open_drive_health"
  | "open_startup"
  | "open_processes"
  | "open_battery"
  | "run_scan"
  | "none";

export interface Insight {
  id: string;
  severity: Severity;
  title: string;
  body: string;
  action: InsightAction;
  action_label: string | null;
  value: string | null;
}

export interface DeviceScore {
  score: number;
  label: string;
  reasons: string[];
  partial: boolean;
}

export interface SuggestedAction {
  action: InsightAction;
  label: string;
}

export interface AiAnswer {
  text: string;
  from_model: boolean;
  model_name: string | null;
  actions: SuggestedAction[];
}

export type EngineState = "idle" | "starting" | "ready" | "failed";

export interface EngineStatus {
  state: EngineState;
  model_name: string | null;
  engine_path: string | null;
  port: number | null;
  message: string;
  offline_only: boolean;
}

export interface LocalModel {
  path: string;
  name: string;
  size_bytes: number;
  size_label: string;
  quantisation: string | null;
  parameter_label: string | null;
  estimated_ram_bytes: number;
  estimated_ram_label: string;
  fits_in_memory: boolean;
  is_valid_gguf: boolean;
}

export interface ModelTier {
  parameters: string;
  quantisation: string;
  disk_bytes: number;
  memory_bytes: number;
  note: string;
}

export interface ModelInventory {
  directory: string;
  models: LocalModel[];
  engine_path: string | null;
  engine_present: boolean;
  total_memory_bytes: number;
  free_disk_bytes: number;
  recommendation: string;
  suggested_tier: ModelTier | null;
}

// --------------------------------------------------------------- settings

export type Theme = "system" | "dark" | "light" | "midnight" | "contrast" | "paper";

export interface Settings {
  first_run_complete: boolean;
  launch_at_startup: boolean;
  minimise_to_tray: boolean;

  theme: Theme;
  ui_scale: number;
  reduce_motion: boolean;

  scan_on_launch: boolean;
  large_file_threshold_bytes: number;
  duplicate_min_bytes: number;

  auto_clean_enabled: boolean;
  auto_clean_free_space_percent: number;
  auto_clean_categories: string[];

  notifications_enabled: boolean;
  alert_at_percent: number[];
  notify_drive_health: boolean;
  notification_quiet_minutes: number;

  ai_enabled: boolean;
  ai_model_path: string | null;
  ai_engine_path: string | null;
  ai_context_size: number;
  ai_threads: number;
  ai_gpu_layers: number;
  ai_load_automatically: boolean;
  ai_keep_loaded: boolean;

  telemetry_enabled: boolean;
  crash_reporting_enabled: boolean;
  cloud_services_enabled: boolean;

  background_monitoring: boolean;
  monitor_interval_seconds: number;
  scan_threads: number;

  protected_paths: string[];
  require_confirmation_for_processes: boolean;

  /** Off by default; nothing contacts the network until the user opts in. */
  update_auto_check: boolean;
  update_check_interval_hours: number;
}

export interface ProtectedPathView {
  path: string;
  reason: string;
  explanation: string;
  user_added: boolean;
}

/** One setting an import would change, as described by the backend. */
export interface SettingChange {
  key: string;
  label: string;
  from: string;
  to: string;
  /** True when the change makes AllInsight less careful than it is now. */
  weakens_protection: boolean;
}

export interface ImportPreview {
  /** Binds the apply step to the exact file contents that were reviewed. */
  token: string;
  application_version: string;
  exported_at: string;
  changes: SettingChange[];
  protected_added: string[];
  protected_removed: string[];
  weakens_protection: boolean;
}

export interface ImportResult {
  settings: Settings;
  backup: string;
}

export interface BackupEntry {
  path: string;
  file_name: string;
  kind: "daily" | "before_import";
  /** The timestamp from the file name: YYYY-MM-DD, or YYYY-MM-DD-HHMMSS. */
  created: string;
}

export type Platform = "windows" | "linux" | "macos" | "other";

export interface EnvironmentInfo {
  platform: Platform;
  desktop: string | null;
  elevated: boolean;
  os_name: string;
  host_name: string;
  app_version: string;
  data_directory: string;
  log_directory: string;
  offline_only: boolean;
}

export interface ActivityEntry {
  happened_at: number;
  kind: string;
  summary: string;
  detail: string | null;
}

export interface Alert {
  id: string;
  severity: string;
  title: string;
  body: string;
}

// --------------------------------------------------------------- dashboard

export interface DashboardSnapshot {
  storage: StorageOverview;
  system: SystemSnapshot;
  drives: DriveHealthReport;
  battery: BatteryStatus;
  score: DeviceScore;
  insights: Insight[];
  summary: AiAnswer;
  reclaimable_bytes: number;
  scanned: boolean;
  elevated: boolean;
}

// ---------------------------------------------------------------- updates

export interface ReleaseInfo {
  version: string;
  release_date: string | null;
  notes: string[];
  security: boolean;
  kind: "application" | "model" | "configuration" | "security";
  size: number;
  installable: boolean;
  note: string | null;
}

export type UpdateFailure = "offline" | "network" | "verification" | "install";

export type UpdatePhase =
  | { state: "idle" }
  | { state: "unavailable"; reason: string }
  | { state: "checking" }
  | { state: "up_to_date"; latest: string }
  | { state: "available"; release: ReleaseInfo }
  | { state: "downloading"; release: ReleaseInfo; downloaded: number; total: number | null }
  | { state: "verifying"; release: ReleaseInfo }
  | { state: "ready"; release: ReleaseInfo; manual_install: boolean }
  | { state: "failed"; kind: UpdateFailure; message: string };

export type InstallReport =
  | { outcome: "completed"; version: string }
  | { outcome: "not_completed"; attempted: string; running: string };

export interface UpdateView {
  phase: UpdatePhase;
  current_version: string;
  channel: "stable" | "beta" | "dev";
  platform: string;
  last_install: InstallReport | null;
  last_checked: number | null;
  auto_check: boolean;
  check_interval_hours: number;
  /** Set when an automatic check found an update and the user should be asked. */
  prompt: boolean;
}

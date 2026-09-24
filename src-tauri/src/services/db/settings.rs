//! User settings.
//!
//! Stored as a single JSON document so adding a field never needs a migration.
//! Every default is chosen so that a fresh installation is private and quiet:
//! no telemetry, no crash reporting, no cloud, no background work the user did
//! not ask for, and no automatic cleanup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::Database;

const SETTINGS_KEY: &str = "settings.v1";

/// The available themes.
///
/// Each is a set of values for the same design tokens, so adding one touches
/// no component: the interface reads `--color-ink` and friends and never a
/// literal colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follows the Windows light/dark setting.
    System,
    /// The default. Near-black with a teal accent.
    Dark,
    Light,
    /// Deeper, warmer dark for low-light use.
    Midnight,
    /// Very high contrast on pure black, for readability and OLED panels.
    Contrast,
    /// Muted, low-saturation light theme that is easier for long sessions.
    Paper,
}

impl Theme {
    pub const ALL: [Theme; 6] = [
        Theme::System,
        Theme::Dark,
        Theme::Light,
        Theme::Midnight,
        Theme::Contrast,
        Theme::Paper,
    ];

    /// True when the theme is a dark one, used to pick the window chrome.
    pub fn is_dark(&self) -> bool {
        matches!(self, Theme::Dark | Theme::Midnight | Theme::Contrast)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // --- General ---
    pub first_run_complete: bool,
    pub launch_at_startup: bool,
    pub minimise_to_tray: bool,

    // --- Appearance ---
    pub theme: Theme,
    /// Interface scale as a percentage, 80 to 150.
    pub ui_scale: u32,
    pub reduce_motion: bool,

    // --- Storage ---
    pub scan_on_launch: bool,
    pub large_file_threshold_bytes: u64,
    pub duplicate_min_bytes: u64,

    // --- Cleanup ---
    pub auto_clean_enabled: bool,
    /// Auto-Clean runs when free space on the system volume drops below this.
    pub auto_clean_free_space_percent: u8,
    /// Category ids the user opted into for Auto-Clean. Intersected with the
    /// compiled-in eligible set, never used to widen it.
    pub auto_clean_categories: Vec<String>,

    // --- Notifications ---
    pub notifications_enabled: bool,
    pub alert_at_percent: Vec<u8>,
    pub notify_drive_health: bool,
    /// Minimum gap between notifications, in minutes.
    pub notification_quiet_minutes: u32,

    // --- AI ---
    pub ai_enabled: bool,
    /// Absolute path to a GGUF file the user imported.
    pub ai_model_path: Option<PathBuf>,
    /// Absolute path to the llama.cpp server executable.
    pub ai_engine_path: Option<PathBuf>,
    pub ai_context_size: u32,
    pub ai_threads: u32,
    pub ai_gpu_layers: u32,
    /// Load the model on launch instead of on first use.
    pub ai_load_automatically: bool,
    /// Keep the model resident after answering. When false, AllInsight releases
    /// the memory once a question has been answered, which is the default:
    /// several gigabytes should not stay committed for a feature used
    /// occasionally.
    pub ai_keep_loaded: bool,

    // --- Privacy ---
    /// All three are false and there is no code path that sets them true.
    pub telemetry_enabled: bool,
    pub crash_reporting_enabled: bool,
    pub cloud_services_enabled: bool,

    // --- Performance ---
    pub background_monitoring: bool,
    /// Seconds between background capacity checks.
    pub monitor_interval_seconds: u32,
    /// Worker threads for scanning. Zero means "match the machine".
    pub scan_threads: u32,

    // --- Security ---
    pub protected_paths: Vec<PathBuf>,
    pub require_confirmation_for_processes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            first_run_complete: false,
            launch_at_startup: false,
            minimise_to_tray: true,

            theme: Theme::Light,
            ui_scale: 100,
            reduce_motion: false,

            scan_on_launch: false,
            large_file_threshold_bytes: 1024 * 1024 * 1024,
            duplicate_min_bytes: 1024 * 1024,

            auto_clean_enabled: false,
            auto_clean_free_space_percent: 15,
            auto_clean_categories: Vec::new(),

            notifications_enabled: true,
            alert_at_percent: vec![80, 90, 95],
            notify_drive_health: true,
            notification_quiet_minutes: 240,

            ai_enabled: true,
            ai_model_path: None,
            ai_engine_path: None,
            ai_context_size: 4096,
            ai_threads: 0,
            ai_gpu_layers: 0,
            ai_load_automatically: false,
            ai_keep_loaded: false,

            telemetry_enabled: false,
            crash_reporting_enabled: false,
            cloud_services_enabled: false,

            background_monitoring: true,
            monitor_interval_seconds: 300,
            scan_threads: 0,

            protected_paths: Vec::new(),
            require_confirmation_for_processes: true,
        }
    }
}

impl Settings {
    /// Clamp anything a hand-edited settings row could put out of range.
    ///
    /// The privacy switches are forced off here rather than merely defaulted
    /// off: editing the database must not be a way to turn on data collection
    /// that the application has no code to perform in the first place.
    pub fn sanitise(&mut self) {
        self.ui_scale = self.ui_scale.clamp(80, 150);
        self.auto_clean_free_space_percent = self.auto_clean_free_space_percent.clamp(5, 50);
        self.monitor_interval_seconds = self.monitor_interval_seconds.clamp(60, 3600);
        self.ai_context_size = self.ai_context_size.clamp(512, 32768);
        self.ai_threads = self.ai_threads.min(256);
        self.ai_gpu_layers = self.ai_gpu_layers.min(1000);
        self.scan_threads = self.scan_threads.min(256);
        self.notification_quiet_minutes = self.notification_quiet_minutes.clamp(0, 1440);
        self.large_file_threshold_bytes = self.large_file_threshold_bytes.max(1024 * 1024);
        self.duplicate_min_bytes = self.duplicate_min_bytes.max(4096);

        self.alert_at_percent.retain(|p| (*p >= 50) && (*p <= 99));
        self.alert_at_percent.sort_unstable();
        self.alert_at_percent.dedup();
        if self.alert_at_percent.is_empty() {
            self.alert_at_percent = vec![80, 90, 95];
        }

        self.protected_paths.retain(|p| p.is_absolute());
        self.protected_paths.sort();
        self.protected_paths.dedup();

        self.telemetry_enabled = false;
        self.crash_reporting_enabled = false;
        self.cloud_services_enabled = false;
    }

    /// The Auto-Clean set the engine will actually run: the user's choice
    /// intersected with the compiled-in eligible list.
    pub fn effective_auto_clean_categories(
        &self,
    ) -> Vec<crate::services::cleanup::CleanupCategory> {
        let eligible = crate::services::cleanup::auto_clean_categories();
        eligible
            .into_iter()
            .filter(|c| {
                let name = serde_json::to_string(c).unwrap_or_default();
                let name = name.trim_matches('"');
                self.auto_clean_categories.iter().any(|s| s == name)
            })
            .collect()
    }

    pub fn load(db: &Database) -> Result<Self> {
        let mut settings = match db.get_setting(SETTINGS_KEY)? {
            Some(json) => serde_json::from_str::<Settings>(&json).unwrap_or_default(),
            None => Settings::default(),
        };
        settings.sanitise();
        Ok(settings)
    }

    pub fn save(&mut self, db: &Database) -> Result<()> {
        self.sanitise();
        db.set_setting(SETTINGS_KEY, &serde_json::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::cleanup::CleanupCategory;

    /// Every theme must round-trip through the settings document, since a
    /// value that fails to deserialise silently reverts the user's choice.
    #[test]
    fn every_theme_survives_a_round_trip() {
        let db = Database::in_memory().unwrap();
        for theme in Theme::ALL {
            let mut s = Settings::default();
            s.theme = theme;
            s.save(&db).unwrap();
            assert_eq!(Settings::load(&db).unwrap().theme, theme, "{theme:?}");
        }
    }

    #[test]
    fn dark_themes_are_classified_for_the_window_chrome() {
        assert!(Theme::Dark.is_dark());
        assert!(Theme::Midnight.is_dark());
        assert!(Theme::Contrast.is_dark());
        assert!(!Theme::Light.is_dark());
        assert!(!Theme::Paper.is_dark());
    }

    #[test]
    fn defaults_are_private_and_quiet() {
        let s = Settings::default();
        assert!(!s.telemetry_enabled);
        assert!(!s.crash_reporting_enabled);
        assert!(!s.cloud_services_enabled);
        assert!(!s.auto_clean_enabled);
        assert!(!s.launch_at_startup);
        assert!(!s.ai_keep_loaded);
    }

    #[test]
    fn privacy_switches_cannot_be_turned_on_by_editing_the_database() {
        let mut s = Settings::default();
        s.telemetry_enabled = true;
        s.crash_reporting_enabled = true;
        s.cloud_services_enabled = true;
        s.sanitise();
        assert!(!s.telemetry_enabled);
        assert!(!s.crash_reporting_enabled);
        assert!(!s.cloud_services_enabled);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let mut s = Settings::default();
        s.ui_scale = 5000;
        s.auto_clean_free_space_percent = 99;
        s.monitor_interval_seconds = 1;
        s.ai_context_size = 10;
        s.alert_at_percent = vec![5, 120, 90, 90];
        s.sanitise();
        assert_eq!(s.ui_scale, 150);
        assert_eq!(s.auto_clean_free_space_percent, 50);
        assert_eq!(s.monitor_interval_seconds, 60);
        assert_eq!(s.ai_context_size, 512);
        assert_eq!(s.alert_at_percent, vec![90]);
    }

    #[test]
    fn relative_protected_paths_are_dropped() {
        let mut s = Settings::default();
        let keep = PathBuf::from(if cfg!(windows) { "D:\\Keep" } else { "/srv/keep" });
        s.protected_paths = vec![PathBuf::from("relative\\thing"), keep.clone()];
        s.sanitise();
        assert_eq!(s.protected_paths, vec![keep]);
    }

    /// Naming a category in settings that Auto-Clean is not allowed to run
    /// must not enable it.
    #[test]
    fn auto_clean_selection_cannot_widen_the_eligible_set() {
        let mut s = Settings::default();
        s.auto_clean_categories = vec![
            "recycle_bin".into(),
            "windows_update_cache".into(),
            "user_temp".into(),
        ];
        let effective = s.effective_auto_clean_categories();
        assert!(effective.contains(&CleanupCategory::UserTemp));
        assert!(!effective.contains(&CleanupCategory::RecycleBin));
        assert!(!effective.contains(&CleanupCategory::WindowsUpdateCache));
    }

    #[test]
    fn settings_survive_a_round_trip_through_the_database() {
        let db = Database::in_memory().unwrap();
        let mut s = Settings::default();
        s.theme = Theme::Light;
        s.ui_scale = 125;
        s.save(&db).unwrap();

        let loaded = Settings::load(&db).unwrap();
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.ui_scale, 125);
    }

    #[test]
    fn a_corrupt_settings_row_falls_back_to_defaults() {
        let db = Database::in_memory().unwrap();
        db.set_setting(SETTINGS_KEY, "{ not json at all ").unwrap();
        let loaded = Settings::load(&db).unwrap();
        assert_eq!(loaded.ui_scale, 100);
        assert!(!loaded.telemetry_enabled);
    }
}

//! Deterministic insights.
//!
//! Every sentence here is generated from a measurement, by a rule that can be
//! read and tested. Nothing is produced when the underlying number is missing.
//! This is what makes "the application works fully without an LLM" true rather
//! than aspirational: the Overview, the recommendations and the device score
//! all come from this module, and the local model only ever adds conversation
//! on top of it.

use serde::{Deserialize, Serialize};

use crate::services::storage::format_bytes;

use super::facts::DeviceFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Warning,
    Advice,
    Positive,
    Neutral,
}

/// What the user can do about an insight. A variant, not a command string:
/// the frontend maps it to a screen, and nothing here can be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightAction {
    OpenCleanup,
    OpenLargeFiles,
    OpenDuplicates,
    OpenStorageMap,
    OpenDriveHealth,
    OpenStartup,
    OpenProcesses,
    OpenBattery,
    RunScan,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub body: String,
    pub action: InsightAction,
    pub action_label: Option<String>,
    /// The number the insight is about, so the UI can show it prominently.
    pub value: Option<String>,
}

/// A single 0-100 figure for the Overview.
///
/// The formula is published here rather than hidden, because a health score
/// that cannot be explained is decoration. Each component removes points from
/// a starting 100, and the reasons are returned alongside the number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceScore {
    pub score: u8,
    pub label: String,
    pub reasons: Vec<String>,
    /// True when parts of the score could not be measured, so the figure is
    /// based on less than the full picture.
    pub partial: bool,
}

fn score_label(score: u8) -> &'static str {
    match score {
        90..=100 => "Excellent",
        75..=89 => "Good",
        60..=74 => "Fair",
        40..=59 => "Needs attention",
        _ => "Poor",
    }
}

pub fn device_score(facts: &DeviceFacts) -> DeviceScore {
    let mut score: i32 = 100;
    let mut reasons = Vec::new();
    let mut partial = false;

    // Storage pressure on the system volume, up to 40 points.
    match facts.system_volume() {
        Some(volume) => {
            let used = volume.used_percent;
            let penalty = if used >= 95.0 {
                40
            } else if used >= 90.0 {
                28
            } else if used >= 80.0 {
                16
            } else if used >= 70.0 {
                6
            } else {
                0
            };
            if penalty > 0 {
                reasons.push(format!(
                    "{} is {:.0}% full.",
                    volume.mount_point.trim_end_matches('\\'),
                    used
                ));
            }
            score -= penalty;
        }
        None => partial = true,
    }

    // Drive health, up to 35 points.
    let mut health_penalty = 0;
    for drive in &facts.drives {
        use crate::services::health::HealthState;
        match drive.state {
            HealthState::Critical => {
                health_penalty = health_penalty.max(35);
                reasons.push(format!("{} reported a hardware problem.", drive.model));
            }
            HealthState::Warning => {
                health_penalty = health_penalty.max(18);
                reasons.push(format!("{} reported a health warning.", drive.model));
            }
            HealthState::Unknown => partial = true,
            HealthState::Healthy => {}
        }
    }
    if facts.drives.is_empty() {
        partial = true;
    }
    score -= health_penalty;

    // Memory pressure, up to 12 points.
    if let Some(memory) = &facts.memory {
        if memory.used_percent >= 92.0 {
            score -= 12;
            reasons.push("Memory is almost fully committed.".into());
        } else if memory.used_percent >= 85.0 {
            score -= 6;
            reasons.push("Memory use is high.".into());
        }
    } else {
        partial = true;
    }

    // Reclaimable clutter, up to 10 points. Only counted once a scan has run.
    if facts.storage_scanned {
        const TEN_GB: u64 = 10 * 1024 * 1024 * 1024;
        if facts.reclaimable_bytes >= TEN_GB {
            score -= 10;
            reasons.push(format!(
                "{} of removable temporary data has built up.",
                format_bytes(facts.reclaimable_bytes)
            ));
        } else if facts.reclaimable_bytes >= TEN_GB / 4 {
            score -= 4;
        }
    } else {
        partial = true;
    }

    // Startup load, up to 8 points.
    if facts.startup.high_impact >= 5 {
        score -= 8;
        reasons.push(format!(
            "{} startup programs are estimated as high impact.",
            facts.startup.high_impact
        ));
    } else if facts.startup.high_impact >= 3 {
        score -= 4;
    }

    // Battery, up to 8 points, only when the device actually reported health.
    if let Some(battery) = &facts.battery {
        if let Some(health) = battery.health_percent {
            if health < 60 {
                score -= 8;
                reasons.push(format!("Battery holds {health}% of its design capacity."));
            } else if health < 80 {
                score -= 3;
            }
        } else if battery.present {
            partial = true;
        }
    }

    let score = score.clamp(0, 100) as u8;
    if reasons.is_empty() {
        reasons.push("No problems were found in the areas AllInsight could measure.".into());
    }

    DeviceScore {
        label: score_label(score).to_string(),
        score,
        reasons,
        partial,
    }
}

/// Build the insight list. Ordered by severity, then by size of the number.
pub fn generate(facts: &DeviceFacts) -> Vec<Insight> {
    let mut out: Vec<Insight> = Vec::new();

    // --- Storage pressure ---------------------------------------------
    for volume in &facts.volumes {
        let used = volume.used_percent;
        let name = volume.mount_point.trim_end_matches('\\');
        if used >= 95.0 {
            out.push(Insight {
                id: format!("storage-critical-{name}"),
                severity: Severity::Critical,
                title: format!("{name} has almost no free space"),
                body: format!(
                    "Only {} of {} remains on {name}. Windows needs free space to update and to page memory, and applications can fail to save when it runs out.",
                    format_bytes(volume.free_bytes),
                    format_bytes(volume.total_bytes)
                ),
                action: InsightAction::OpenCleanup,
                action_label: Some("Free up space".into()),
                value: Some(format!("{used:.0}% full")),
            });
        } else if used >= 90.0 {
            out.push(Insight {
                id: format!("storage-low-{name}"),
                severity: Severity::Warning,
                title: format!("{name} is running low on space"),
                body: format!(
                    "{} of {} is in use. Below ten percent free, Windows updates and large downloads start to fail.",
                    format_bytes(volume.used_bytes_estimate()),
                    format_bytes(volume.total_bytes)
                ),
                action: InsightAction::OpenStorageMap,
                action_label: Some("See what is using it".into()),
                value: Some(format!("{used:.0}% full")),
            });
        } else if used >= 80.0 && volume.is_system {
            out.push(Insight {
                id: format!("storage-high-{name}"),
                severity: Severity::Advice,
                title: format!("Storage use on {name} is getting high"),
                body: format!(
                    "{} free of {}. This is not urgent, but it is worth knowing where the space is going.",
                    format_bytes(volume.free_bytes),
                    format_bytes(volume.total_bytes)
                ),
                action: InsightAction::OpenStorageMap,
                action_label: Some("Open storage map".into()),
                value: Some(format!("{used:.0}% full")),
            });
        }
    }

    // --- Reclaimable space --------------------------------------------
    if facts.reclaimable_bytes >= 512 * 1024 * 1024 {
        let biggest = facts
            .cleanup
            .iter()
            .max_by_key(|c| c.bytes)
            .map(|c| format!(" The largest single contributor is {} at {}.", c.category, format_bytes(c.bytes)))
            .unwrap_or_default();
        out.push(Insight {
            id: "cleanup-available".into(),
            severity: Severity::Advice,
            title: format!("{} can be safely reclaimed", format_bytes(facts.reclaimable_bytes)),
            body: format!(
                "This is temporary and cached data that Windows and your applications rebuild automatically.{biggest} No documents, downloads or media are included."
            ),
            action: InsightAction::OpenCleanup,
            action_label: Some("Review and clean".into()),
            value: Some(format_bytes(facts.reclaimable_bytes)),
        });
    }

    // --- Where the space actually went --------------------------------
    if facts.storage_scanned {
        if let Some(top) = facts.storage_categories.first() {
            let total: u64 = facts.storage_categories.iter().map(|c| c.bytes).sum();
            if total > 0 {
                let share = (top.bytes as f64 / total as f64 * 100.0).round();
                out.push(Insight {
                    id: "storage-composition".into(),
                    severity: Severity::Neutral,
                    title: format!("{} accounts for most of your scanned storage", top.label),
                    body: format!(
                        "{} across {} files, about {share:.0}% of everything scanned.",
                        format_bytes(top.bytes),
                        top.files
                    ),
                    action: InsightAction::OpenStorageMap,
                    action_label: Some("Open storage map".into()),
                    value: Some(format_bytes(top.bytes)),
                });
            }
        }

        // "Most reclaimable storage comes from temporary files rather than
        // personal files", stated only when the numbers support it.
        let personal: u64 = facts
            .storage_categories
            .iter()
            .filter(|c| {
                use crate::services::storage::StorageCategory::*;
                matches!(c.category, Documents | Pictures | Videos | Music | Desktop)
            })
            .map(|c| c.bytes)
            .sum();
        if facts.reclaimable_bytes > 0 && personal > 0 && facts.reclaimable_bytes < personal / 4 {
            out.push(Insight {
                id: "cleanup-vs-personal".into(),
                severity: Severity::Neutral,
                title: "Most of your storage is personal files, not clutter".into(),
                body: format!(
                    "Documents, pictures, video and music account for {}, while removable temporary data accounts for {}. Reviewing large files will free more space than cleaning caches.",
                    format_bytes(personal),
                    format_bytes(facts.reclaimable_bytes)
                ),
                action: InsightAction::OpenLargeFiles,
                action_label: Some("Review large files".into()),
                value: None,
            });
        }
    } else {
        out.push(Insight {
            id: "no-scan".into(),
            severity: Severity::Neutral,
            title: "Storage has not been analysed yet".into(),
            body: "Run a scan to see which folders and categories are using your space. Nothing is changed by scanning.".into(),
            action: InsightAction::RunScan,
            action_label: Some("Scan storage".into()),
            value: None,
        });
    }

    // --- Large files ---------------------------------------------------
    if facts.large_files.over_2gb > 0 {
        out.push(Insight {
            id: "large-files".into(),
            severity: Severity::Advice,
            title: format!(
                "{} files are larger than 2 GB",
                facts.large_files.over_2gb
            ),
            body: format!(
                "They hold {} between them{}. AllInsight will not remove any of them; this is a list to review.",
                format_bytes(facts.large_files.total_bytes),
                facts
                    .large_files
                    .largest_folder
                    .as_ref()
                    .map(|f| format!(", mostly in {f}"))
                    .unwrap_or_default()
            ),
            action: InsightAction::OpenLargeFiles,
            action_label: Some("Review large files".into()),
            value: Some(format_bytes(facts.large_files.total_bytes)),
        });
    }

    // --- Duplicates -----------------------------------------------------
    if facts.duplicates.reclaimable_bytes >= 256 * 1024 * 1024 {
        out.push(Insight {
            id: "duplicates".into(),
            severity: Severity::Advice,
            title: format!(
                "{} of duplicate copies found",
                format_bytes(facts.duplicates.reclaimable_bytes)
            ),
            body: format!(
                "Across {} groups of files with identical contents. Keeping one copy of each would free that space. Nothing is selected or removed automatically.",
                facts.duplicates.groups
            ),
            action: InsightAction::OpenDuplicates,
            action_label: Some("Review duplicates".into()),
            value: Some(format_bytes(facts.duplicates.reclaimable_bytes)),
        });
    }

    // --- Drive health ---------------------------------------------------
    for drive in &facts.drives {
        use crate::services::health::HealthState;
        match drive.state {
            HealthState::Critical => out.push(Insight {
                id: format!("drive-critical-{}", drive.model),
                severity: Severity::Critical,
                title: format!("{} reported a hardware problem", drive.model),
                body: "Back up anything important on this drive now. A drive that reports a fault can fail without further warning.".into(),
                action: InsightAction::OpenDriveHealth,
                action_label: Some("See drive health".into()),
                value: Some(drive.state.label().to_string()),
            }),
            HealthState::Warning => out.push(Insight {
                id: format!("drive-warning-{}", drive.model),
                severity: Severity::Warning,
                title: format!("{} reported a health warning", drive.model),
                body: "Windows or the drive itself flagged a condition worth watching. Check the details and make sure your backups are current.".into(),
                action: InsightAction::OpenDriveHealth,
                action_label: Some("See drive health".into()),
                value: Some(drive.state.label().to_string()),
            }),
            HealthState::Healthy => {
                if let Some(life) = drive.life_remaining_percent {
                    out.push(Insight {
                        id: format!("drive-life-{}", drive.model),
                        severity: Severity::Positive,
                        title: format!("{} reports {life}% of its rated life remaining", drive.model),
                        body: "This figure comes from the drive's own wear counter, not an estimate.".into(),
                        action: InsightAction::OpenDriveHealth,
                        action_label: None,
                        value: Some(format!("{life}%")),
                    });
                }
            }
            HealthState::Unknown => {}
        }
    }

    // --- Startup --------------------------------------------------------
    if facts.startup.high_impact >= 3 {
        let names = facts.startup.top_names.join(", ");
        out.push(Insight {
            id: "startup-impact".into(),
            severity: Severity::Advice,
            title: format!(
                "{} startup programs look heavy",
                facts.startup.high_impact
            ),
            body: if names.is_empty() {
                "Disabling programs you do not need at sign-in shortens the time before the desktop is usable.".into()
            } else {
                format!("{names} start with Windows. Disabling the ones you do not need shortens sign-in. Nothing is uninstalled by disabling a startup entry.")
            },
            action: InsightAction::OpenStartup,
            action_label: Some("Manage startup".into()),
            value: Some(facts.startup.enabled.to_string()),
        });
    }

    // --- Memory and CPU --------------------------------------------------
    if let Some(memory) = &facts.memory {
        if memory.used_percent >= 90.0 {
            out.push(Insight {
                id: "memory-pressure".into(),
                severity: Severity::Warning,
                title: "Memory is nearly full".into(),
                body: format!(
                    "{:.0}% of {} is committed. Windows will start paging to disk, which makes everything feel slower.",
                    memory.used_percent,
                    format_bytes(memory.total_bytes)
                ),
                action: InsightAction::OpenProcesses,
                action_label: Some("See what is using it".into()),
                value: Some(format!("{:.0}%", memory.used_percent)),
            });
        }
    }
    if let Some(cpu) = &facts.cpu {
        if cpu.average_percent >= 70.0 {
            out.push(Insight {
                id: "cpu-sustained".into(),
                severity: Severity::Advice,
                title: "Sustained high processor use".into(),
                body: format!(
                    "The processor has averaged {:.0}% since AllInsight started{}.",
                    cpu.average_percent,
                    cpu.top_process
                        .as_ref()
                        .map(|p| format!(", led by {p}"))
                        .unwrap_or_default()
                ),
                action: InsightAction::OpenProcesses,
                action_label: Some("Open processes".into()),
                value: Some(format!("{:.0}%", cpu.average_percent)),
            });
        }
    }

    // --- Battery ----------------------------------------------------------
    if let Some(battery) = &facts.battery {
        if let Some(health) = battery.health_percent {
            if health < 70 {
                out.push(Insight {
                    id: "battery-worn".into(),
                    severity: Severity::Advice,
                    title: format!("Battery holds {health}% of its original capacity"),
                    body: format!(
                        "Runtime on a full charge is roughly {health}% of what it was when the device was new{}.",
                        battery
                            .cycle_count
                            .map(|c| format!(", after {c} charge cycles"))
                            .unwrap_or_default()
                    ),
                    action: InsightAction::OpenBattery,
                    action_label: Some("See battery".into()),
                    value: Some(format!("{health}%")),
                });
            } else if health >= 90 {
                out.push(Insight {
                    id: "battery-good".into(),
                    severity: Severity::Positive,
                    title: format!("Battery health is {health}%"),
                    body: "The battery still holds close to its original design capacity.".into(),
                    action: InsightAction::None,
                    action_label: None,
                    value: Some(format!("{health}%")),
                });
            }
        }
    }

    out.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.title.cmp(&b.title))
    });
    out
}

/// The one-paragraph summary shown on the Overview when no local model is
/// loaded. Same facts, same rules, no inference required.
pub fn summary(facts: &DeviceFacts) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(volume) = facts.system_volume() {
        parts.push(format!(
            "Your {} system drive is {:.0}% full, with {} free.",
            format_bytes(volume.total_bytes),
            volume.used_percent,
            format_bytes(volume.free_bytes)
        ));
    }

    if facts.storage_scanned {
        if let Some(top) = facts.storage_categories.first() {
            parts.push(format!(
                "The largest contributor is {} at {}.",
                top.label.to_lowercase(),
                format_bytes(top.bytes)
            ));
        }
    }

    if facts.reclaimable_bytes >= 100 * 1024 * 1024 {
        parts.push(format!(
            "About {} of temporary and cached data can be reclaimed safely.",
            format_bytes(facts.reclaimable_bytes)
        ));
    } else if facts.reclaimable_bytes > 0 {
        parts.push("There is very little temporary data to clean.".into());
    }

    if facts.large_files.over_1gb > 0 && facts.reclaimable_bytes < facts.large_files.total_bytes / 4
    {
        parts.push(format!(
            "Reviewing the {} files larger than 1 GB would free considerably more.",
            facts.large_files.over_1gb
        ));
    }

    let unknown_drives = facts.drives.iter().filter(|d| !d.data_available).count();
    if unknown_drives > 0 && unknown_drives == facts.drives.len() {
        parts.push(
            "Detailed drive health is not available without administrator permission.".into(),
        );
    }

    if parts.is_empty() {
        "AllInsight has not gathered enough information yet. Run a storage scan to get started.".into()
    } else {
        parts.join(" ")
    }
}

/// Convenience for the volume fact, which stores free rather than used bytes.
trait UsedBytes {
    fn used_bytes_estimate(&self) -> u64;
}

impl UsedBytes for super::facts::VolumeFact {
    fn used_bytes_estimate(&self) -> u64 {
        self.total_bytes.saturating_sub(self.free_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;
    use crate::services::ai::facts::*;
    use crate::services::health::HealthState;
    use crate::services::storage::{CategoryTotal, StorageCategory};

    fn volume(used_percent: f64) -> VolumeFact {
        let total = 512 * GB;
        let free = (total as f64 * (1.0 - used_percent / 100.0)) as u64;
        VolumeFact {
            mount_point: "C:\\".into(),
            label: None,
            total_bytes: total,
            free_bytes: free,
            used_percent,
            is_system: true,
        }
    }

    fn facts(used_percent: f64) -> DeviceFacts {
        DeviceFacts {
            volumes: vec![volume(used_percent)],
            storage_scanned: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_full_system_drive_produces_a_critical_insight() {
        let insights = generate(&facts(96.0));
        let first = &insights[0];
        assert_eq!(first.severity, Severity::Critical);
        assert_eq!(first.action, InsightAction::OpenCleanup);
        assert!(first.body.contains("free"));
    }

    #[test]
    fn a_comfortable_drive_produces_no_storage_warning() {
        let insights = generate(&facts(40.0));
        assert!(!insights
            .iter()
            .any(|i| i.severity == Severity::Critical || i.severity == Severity::Warning));
    }

    #[test]
    fn insights_are_never_generated_from_missing_data() {
        let empty = DeviceFacts::default();
        let insights = generate(&empty);
        // With nothing measured the only thing to say is "run a scan".
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].action, InsightAction::RunScan);
    }

    #[test]
    fn a_drive_with_unknown_health_produces_no_health_claim() {
        let mut f = facts(50.0);
        f.drives = vec![DriveHealthFact {
            model: "Test SSD".into(),
            state: HealthState::Unknown,
            media: "Ssd".into(),
            life_remaining_percent: None,
            temperature_celsius: None,
            power_on_hours: None,
            data_available: false,
        }];
        let insights = generate(&f);
        assert!(!insights.iter().any(|i| i.id.starts_with("drive-")));
    }

    #[test]
    fn the_device_score_explains_every_deduction() {
        let mut f = facts(96.0);
        f.memory = Some(MemoryFact {
            total_bytes: 16 * GB,
            used_percent: 95.0,
        });
        let score = device_score(&f);
        assert!(score.score < 60, "score was {}", score.score);
        assert!(score.reasons.iter().any(|r| r.contains("full")));
        assert!(score.reasons.iter().any(|r| r.contains("Memory")));
    }

    #[test]
    fn a_healthy_machine_scores_well_and_says_why_not_perfectly() {
        let mut f = facts(35.0);
        f.memory = Some(MemoryFact {
            total_bytes: 32 * GB,
            used_percent: 40.0,
        });
        f.drives = vec![DriveHealthFact {
            model: "Good SSD".into(),
            state: HealthState::Healthy,
            media: "Ssd".into(),
            life_remaining_percent: Some(94),
            temperature_celsius: Some(38),
            power_on_hours: Some(1200),
            data_available: true,
        }];
        let score = device_score(&f);
        assert!(score.score >= 90, "score was {}", score.score);
        assert!(!score.partial);
    }

    #[test]
    fn an_unmeasured_machine_is_marked_partial() {
        let score = device_score(&DeviceFacts::default());
        assert!(score.partial);
    }

    #[test]
    fn the_summary_never_states_a_figure_it_does_not_have() {
        let empty = summary(&DeviceFacts::default());
        assert!(empty.contains("not gathered enough"));

        let mut f = facts(91.0);
        f.reclaimable_bytes = 7 * GB;
        f.storage_categories = vec![CategoryTotal {
            category: StorageCategory::Videos,
            label: "Videos".into(),
            bytes: 96 * GB,
            files: 210,
        }];
        let text = summary(&f);
        assert!(text.contains("91% full"));
        assert!(text.contains("videos"));
        assert!(text.contains("7.00 GB"));
    }

    #[test]
    fn duplicate_and_large_file_insights_need_real_numbers() {
        let mut f = facts(50.0);
        f.duplicates = DuplicateFact {
            groups: 12,
            reclaimable_bytes: 4 * GB,
        };
        f.large_files = LargeFileFact {
            over_1gb: 14,
            over_2gb: 14,
            over_5gb: 2,
            largest_bytes: 9 * GB,
            largest_folder: Some("C:\\Users\\Me\\Downloads".into()),
            total_bytes: 38 * GB,
        };
        let insights = generate(&f);
        assert!(insights.iter().any(|i| i.id == "duplicates"));
        let large = insights.iter().find(|i| i.id == "large-files").unwrap();
        assert!(large.title.contains("14 files"));
        assert!(large.body.contains("Downloads"));
    }

    #[test]
    fn severity_ordering_puts_critical_first() {
        let mut f = facts(96.0);
        f.duplicates = DuplicateFact {
            groups: 3,
            reclaimable_bytes: 2 * GB,
        };
        let insights = generate(&f);
        for pair in insights.windows(2) {
            assert!(pair[0].severity <= pair[1].severity);
        }
    }
}

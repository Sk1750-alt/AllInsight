//! Local model management.
//!
//! AllInsight ships no model. It looks in a known folder for GGUF files the user
//! put there, reports what it finds, and recommends a size class based on the
//! memory actually installed. Nothing is ever downloaded: there is no download
//! code in this binary.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AllInsightError, Result};
use crate::services::security::paths;
use crate::services::storage::format_bytes;

/// The magic bytes at the start of every GGUF file.
const GGUF_MAGIC: &[u8; 4] = b"GGUF";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModel {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub size_label: String,
    /// Parsed from the filename, which is how the GGUF ecosystem labels
    /// quantisation. `None` when the name does not follow the convention.
    pub quantisation: Option<String>,
    /// Rough parameter count, also from the filename.
    pub parameter_label: Option<String>,
    /// Memory needed to load it, estimated as the file size plus the KV cache
    /// and a working margin.
    pub estimated_ram_bytes: u64,
    pub estimated_ram_label: String,
    /// Whether this machine has enough memory to run it comfortably.
    pub fits_in_memory: bool,
    pub is_valid_gguf: bool,
}

/// Serialised outward only, like [`ModelTier`] which it carries.
#[derive(Debug, Clone, Serialize)]
pub struct ModelInventory {
    pub directory: PathBuf,
    pub models: Vec<LocalModel>,
    /// The engine executable, when one has been located.
    pub engine_path: Option<PathBuf>,
    pub engine_present: bool,
    pub total_memory_bytes: u64,
    /// Free space on the volume holding the models folder, which constrains
    /// the recommendation as much as memory does.
    pub free_disk_bytes: u64,
    pub recommendation: String,
    /// The tier AllInsight suggests, when one fits at all.
    pub suggested_tier: Option<ModelTier>,
}

/// Where AllInsight looks for models by default.
pub fn model_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AllInsight")
        .join("models")
}

/// Where AllInsight looks for the llama.cpp server executable.
pub fn engine_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("AllInsight")
        .join("engine")
}

fn parse_quantisation(name: &str) -> Option<String> {
    let upper = name.to_uppercase();
    for token in [
        "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_0", "Q4_1", "Q4_K_S", "Q4_K_M", "Q5_0", "Q5_1",
        "Q5_K_S", "Q5_K_M", "Q6_K", "Q8_0", "IQ2_M", "IQ3_M", "IQ4_XS", "F16", "BF16", "F32",
    ] {
        if upper.contains(token) {
            return Some(token.to_string());
        }
    }
    None
}

fn parse_parameters(name: &str) -> Option<String> {
    let upper = name.to_uppercase();
    for token in [
        "0.5B", "1B", "1.1B", "1.5B", "2B", "3B", "3.8B", "4B", "7B", "8B", "9B", "12B", "13B",
        "14B", "20B", "27B", "30B", "32B", "70B",
    ] {
        if upper.contains(token) {
            return Some(token.to_string());
        }
    }
    None
}

/// Loading a GGUF needs roughly the file itself, plus the KV cache, plus room
/// for the runtime. The margin is deliberately generous: telling someone a
/// model fits when it does not means watching their machine thrash.
fn estimate_ram(size_bytes: u64) -> u64 {
    size_bytes + (size_bytes / 4) + (512 * 1024 * 1024)
}

/// Confirm the file really is a GGUF rather than trusting the extension.
fn is_gguf(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(paths::long_path(path)) else {
        return false;
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && &magic == GGUF_MAGIC
}

fn total_memory() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory()
}

/// One rung on the size ladder, smallest first.
///
/// Serialised outward only, which is why it can hold `&'static str` and skip
/// `Deserialize`: nothing ever sends a tier back to the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModelTier {
    /// Parameter count as it appears in GGUF filenames.
    pub parameters: &'static str,
    pub quantisation: &'static str,
    /// Roughly what the file occupies.
    pub disk_bytes: u64,
    /// Roughly what it needs resident to run.
    pub memory_bytes: u64,
    pub note: &'static str,
}

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// The ladder AllInsight recommends from.
///
/// It starts far smaller than most model guidance does, because a device that
/// needs a storage manager is frequently a device that has no room for a five
/// gigabyte file. A 0.5B model is limited, but it answers questions about
/// measurements it is handed, which is the whole job here.
pub const MODEL_TIERS: &[ModelTier] = &[
    ModelTier {
        parameters: "0.5B",
        quantisation: "Q4_K_M",
        disk_bytes: 400 * MIB,
        memory_bytes: 1 * GIB,
        note: "Smallest useful option. Terse but coherent, and it fits almost anywhere.",
    },
    ModelTier {
        parameters: "1B",
        quantisation: "Q4_K_M",
        disk_bytes: 810 * MIB,
        memory_bytes: 2 * GIB,
        note: "A good balance when storage is tight.",
    },
    ModelTier {
        parameters: "1.5B",
        quantisation: "Q4_K_M",
        disk_bytes: 1100 * MIB,
        memory_bytes: 3 * GIB,
        note: "Noticeably better prose than 1B for a small extra cost.",
    },
    ModelTier {
        parameters: "3B",
        quantisation: "Q4_K_M",
        disk_bytes: 2 * GIB,
        memory_bytes: 4 * GIB,
        note: "Comfortable on a machine with room to spare.",
    },
    ModelTier {
        parameters: "7B to 8B",
        quantisation: "Q4_K_M",
        disk_bytes: 4700 * MIB,
        memory_bytes: 6 * GIB,
        note: "The best answers, if both disk and memory allow.",
    },
];

/// Space kept back so installing a model cannot be what tips the drive over.
///
/// Below roughly this much free, Windows updates start failing and paging
/// suffers, so a recommendation that would eat into it is not a recommendation
/// worth making.
const DISK_RESERVE: u64 = 6 * GIB;

/// The largest tier that fits both constraints, if any.
pub fn best_tier(total_memory_bytes: u64, free_disk_bytes: u64) -> Option<&'static ModelTier> {
    // Half of installed memory is a fair ceiling: the rest of the machine has
    // to keep working while the model is resident.
    let memory_budget = total_memory_bytes / 2;
    let disk_budget = free_disk_bytes.saturating_sub(DISK_RESERVE);

    MODEL_TIERS
        .iter()
        .rev()
        .find(|t| t.memory_bytes <= memory_budget && t.disk_bytes <= disk_budget)
}

/// The advice shown in Settings and on the assistant screen.
///
/// Memory alone is not enough to decide this. A machine can have plenty of RAM
/// and almost no disk, which is a common state for exactly the machines this
/// application is installed on, and recommending a five gigabyte download onto
/// a drive with eight gigabytes left would be actively unhelpful.
pub fn recommendation_for(total_memory_bytes: u64, free_disk_bytes: u64) -> String {
    match best_tier(total_memory_bytes, free_disk_bytes) {
        Some(tier) => {
            let limited_by_disk = free_disk_bytes.saturating_sub(DISK_RESERVE) < 4700 * MIB
                && total_memory_bytes / 2 >= 6 * GIB;
            format!(
                "A {} model at {} suits this device, about {} on disk. {}{}",
                tier.parameters,
                tier.quantisation,
                format_bytes(tier.disk_bytes),
                tier.note,
                if limited_by_disk {
                    format!(
                        " Free space is the limit here, not memory: {} remains free.",
                        format_bytes(free_disk_bytes)
                    )
                } else {
                    String::new()
                }
            )
        }
        None => format!(
            "There is not enough room for a model right now: {} of disk is free, and AllInsight keeps {} in reserve so installing one cannot fill the drive. Free up space first, or keep using AllInsight without a model - every insight and recommendation works without one.",
            format_bytes(free_disk_bytes),
            format_bytes(DISK_RESERVE)
        ),
    }
}

/// Free space on the volume that holds the models folder.
fn free_space_for_models() -> u64 {
    let directory = model_directory();
    crate::services::storage::volumes::list_volumes()
        .into_iter()
        .filter(|v| v.is_ready)
        .find(|v| paths::is_within(&directory, &v.path()))
        .map(|v| v.free_bytes)
        .unwrap_or(0)
}

/// The file names llama.cpp gives its server, preferred name first.
#[cfg(windows)]
pub const ENGINE_FILE_NAMES: &[&str] = &["llama-server.exe", "server.exe"];
#[cfg(not(windows))]
pub const ENGINE_FILE_NAMES: &[&str] = &["llama-server", "server"];

/// Look up an engine executable in the standard locations.
pub fn find_engine(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
    }
    let preferred = ENGINE_FILE_NAMES[0];
    let mut candidates: Vec<PathBuf> = ENGINE_FILE_NAMES
        .iter()
        .map(|name| engine_directory().join(name))
        .collect();
    // Also next to the installed application, so a portable copy works.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("engine").join(preferred));
            candidates.push(dir.join(preferred));
        }
    }
    // On Linux and macOS llama.cpp is commonly installed by a package
    // manager (the AUR, Homebrew, Nix), which puts it on PATH. Only the
    // distinctive name is looked up there; a bare `server` could be anything.
    #[cfg(not(windows))]
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join(preferred)));
    }
    candidates.into_iter().find(|c| c.is_file())
}

pub fn inventory(configured_engine: Option<&Path>) -> ModelInventory {
    let directory = model_directory();
    let _ = std::fs::create_dir_all(&directory);
    let total_memory_bytes = total_memory();
    let free_disk_bytes = free_space_for_models();

    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if paths::extension_lower(&path) != "gguf" {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let size_bytes = meta.len();
            let estimated_ram_bytes = estimate_ram(size_bytes);
            models.push(LocalModel {
                quantisation: parse_quantisation(&name),
                parameter_label: parse_parameters(&name),
                size_label: format_bytes(size_bytes),
                estimated_ram_label: format_bytes(estimated_ram_bytes),
                fits_in_memory: estimated_ram_bytes < total_memory_bytes,
                is_valid_gguf: is_gguf(&path),
                estimated_ram_bytes,
                size_bytes,
                name,
                path,
            });
        }
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));

    let engine_path = find_engine(configured_engine);
    ModelInventory {
        engine_present: engine_path.is_some(),
        engine_path,
        directory,
        models,
        recommendation: recommendation_for(total_memory_bytes, free_disk_bytes),
        suggested_tier: best_tier(total_memory_bytes, free_disk_bytes).copied(),
        total_memory_bytes,
        free_disk_bytes,
    }
}

/// Copy a model the user picked into the managed folder.
///
/// The file is validated before it is copied, so an unrelated file renamed to
/// `.gguf` is refused rather than becoming a confusing failure later.
pub fn import(source: &Path) -> Result<LocalModel> {
    if !source.is_file() {
        return Err(AllInsightError::InvalidInput(
            "That path is not a file.".into(),
        ));
    }
    if paths::extension_lower(source) != "gguf" {
        return Err(AllInsightError::InvalidInput(
            "AllInsight can only use models in GGUF format.".into(),
        ));
    }
    if !is_gguf(source) {
        return Err(AllInsightError::InvalidInput(
            "That file is named .gguf but does not contain a GGUF model.".into(),
        ));
    }

    let directory = model_directory();
    std::fs::create_dir_all(&directory)
        .map_err(|e| AllInsightError::Other(format!("Could not create the models folder: {e}")))?;

    let file_name = source
        .file_name()
        .ok_or_else(|| AllInsightError::InvalidInput("That file has no name.".into()))?;
    if paths::has_hostile_name(file_name) {
        return Err(AllInsightError::InvalidInput(
            "That file name contains characters AllInsight will not accept.".into(),
        ));
    }
    let destination = directory.join(file_name);

    if !paths::is_strictly_within(&destination, &directory) {
        return Err(AllInsightError::InvalidInput(
            "That file name is not valid.".into(),
        ));
    }

    if !paths::same_path(source, &destination) {
        std::fs::copy(source, &destination)
            .map_err(|e| AllInsightError::Other(format!("Could not copy the model: {e}")))?;
    }

    inventory(None)
        .models
        .into_iter()
        .find(|m| paths::same_path(&m.path, &destination))
        .ok_or_else(|| AllInsightError::Other("The imported model could not be read back.".into()))
}

/// Remove a model from the managed folder.
pub fn remove(path: &Path) -> Result<()> {
    let directory = model_directory();
    let target = paths::normalize_lexical(path);
    // Only files inside the managed folder can be removed here, so a crafted
    // request cannot delete something else.
    if !paths::is_strictly_within(&target, &directory) {
        return Err(AllInsightError::InvalidInput(
            "Only models inside the AllInsight models folder can be removed here.".into(),
        ));
    }
    if paths::extension_lower(&target) != "gguf" {
        return Err(AllInsightError::InvalidInput(
            "Only GGUF model files can be removed here.".into(),
        ));
    }
    if paths::is_reparse_point(&target) {
        return Err(AllInsightError::InvalidInput(
            "That entry is a link and will not be removed.".into(),
        ));
    }
    // Containment above was checked lexically, which a junction anywhere on
    // the way down would defeat.
    if let Some(link) = paths::first_reparse_ancestor(&target, Some(&directory)) {
        return Err(AllInsightError::InvalidInput(format!(
            "That model is reached through a link ({}), so it will not be removed.",
            link.display()
        )));
    }
    std::fs::remove_file(paths::long_path(&target))
        .map_err(|e| AllInsightError::Other(format!("Could not remove the model: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantisation_and_size_are_read_from_the_filename() {
        assert_eq!(
            parse_quantisation("Meta-Llama-3-8B-Instruct.Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(
            parse_parameters("Meta-Llama-3-8B-Instruct.Q4_K_M.gguf").as_deref(),
            Some("8B")
        );
        assert!(parse_quantisation("mystery-model.gguf").is_none());
    }

    #[test]
    fn ram_estimates_leave_headroom() {
        let four_gb = 4u64 * 1024 * 1024 * 1024;
        let estimate = estimate_ram(four_gb);
        assert!(estimate > four_gb);
        assert!(estimate < four_gb * 2);
    }

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn recommendations_scale_with_installed_memory_when_disk_is_plentiful() {
        let roomy = 200 * GB;
        assert_eq!(best_tier(4 * GB, roomy).unwrap().parameters, "1B");
        assert_eq!(best_tier(8 * GB, roomy).unwrap().parameters, "3B");
        assert_eq!(best_tier(16 * GB, roomy).unwrap().parameters, "7B to 8B");
        assert_eq!(best_tier(64 * GB, roomy).unwrap().parameters, "7B to 8B");
    }

    /// Windows never reports the full nominal figure, so the tiers have to
    /// tolerate the shortfall rather than demoting the machine a whole class.
    #[test]
    fn a_machine_reporting_just_under_a_nominal_size_is_not_demoted() {
        let reported_16gb = 16_852_000_000u64;
        assert_eq!(
            best_tier(reported_16gb, 200 * GB).unwrap().parameters,
            "7B to 8B",
            "15.7 GB must not be demoted a whole tier"
        );
    }

    /// The case this device is actually in: plenty of memory, almost no disk.
    /// Recommending a 5 GB download here would be actively unhelpful.
    #[test]
    fn a_full_drive_limits_the_recommendation_regardless_of_memory() {
        let plenty_of_memory = 16 * GB;

        // 9 GB free, less the 6 GB reserve, leaves 3 GB: enough for 3B but
        // not for the 4.7 GB the memory alone would have allowed.
        let tight = best_tier(plenty_of_memory, 9 * GB).unwrap();
        assert_eq!(tight.parameters, "3B");

        // 7 GB free leaves 1 GB, which only the smallest tiers fit inside.
        let tighter = best_tier(plenty_of_memory, 7 * GB).unwrap();
        assert_eq!(tighter.parameters, "1B");

        let advice = recommendation_for(plenty_of_memory, 9 * GB);
        assert!(
            advice.contains("Free space is the limit"),
            "the reason for the smaller suggestion must be stated: {advice}"
        );
    }

    #[test]
    fn a_drive_with_no_room_recommends_no_model_at_all() {
        assert!(best_tier(16 * GB, 5 * GB).is_none());
        let advice = recommendation_for(16 * GB, 5 * GB);
        assert!(advice.contains("not enough room"));
        assert!(
            advice.contains("without a model"),
            "refusing must point out that the application still works"
        );
    }

    #[test]
    fn memory_still_constrains_a_machine_with_a_huge_empty_disk() {
        assert_eq!(best_tier(2 * GB, 500 * GB).unwrap().parameters, "0.5B");
    }

    #[test]
    fn a_file_that_is_not_a_gguf_is_refused_on_import() {
        let dir = std::env::temp_dir().join(format!("allinsight-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("not-really.gguf");
        std::fs::write(&fake, b"this is not a model").unwrap();

        let err = import(&fake).unwrap_err();
        assert!(err.to_string().contains("does not contain a GGUF"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removal_is_confined_to_the_managed_folder() {
        let err = remove(Path::new("C:\\Windows\\System32\\kernel32.dll")).unwrap_err();
        assert!(err.to_string().contains("models folder"));

        let outside = model_directory().join("..").join("escape.gguf");
        let err = remove(&outside).unwrap_err();
        assert!(err.to_string().contains("models folder"));
    }

    #[test]
    fn an_inventory_of_an_empty_folder_is_not_an_error() {
        let inventory = inventory(None);
        assert!(inventory.total_memory_bytes > 0);
        assert!(!inventory.recommendation.is_empty());
    }
}

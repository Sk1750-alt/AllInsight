//! The cleanup subsystem.
//!
//! Reading order: `categories` declares what may be cleaned, `engine` decides
//! what is actually eligible right now, `remove` performs removal on entries
//! the deletion guard has approved, and `recycle_bin` wraps the one category
//! Windows owns.

pub mod categories;
pub mod engine;
pub mod recycle_bin;
pub mod remove;

pub use categories::{
    definition_for, definitions, CategoryDefinition, CleanupCategory, DeletionMode,
    ALL_CLEANUP_CATEGORIES,
};
pub use engine::{
    auto_clean_categories, discover, execute, CategoryReport, CleanupCandidate, CleanupOutcome,
    CleanupPreview, CleanupRequest, CleanupScan,
};
pub use recycle_bin::RecycleBinState;

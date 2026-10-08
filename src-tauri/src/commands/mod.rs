//! The IPC surface.
//!
//! Every function here is thin on purpose. It validates its arguments, calls
//! one service, and returns plain data. No decision that matters is made in
//! this layer, and in particular no destructive operation takes a path from
//! the frontend: cleanup accepts category variants and backend-issued
//! candidate ids, and nothing else.

pub mod ai;
pub mod cleanup;
pub mod device;
pub mod settings;
pub mod storage;
pub mod update;

use serde::Serialize;

/// Long-running work reports progress through these events rather than
/// blocking a command.
pub const EVENT_SCAN_PROGRESS: &str = "allinsight://scan-progress";
pub const EVENT_SCAN_COMPLETE: &str = "allinsight://scan-complete";
pub const EVENT_CLEANUP_PROGRESS: &str = "allinsight://cleanup-progress";
pub const EVENT_ALERT: &str = "allinsight://alert";

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub body: String,
}

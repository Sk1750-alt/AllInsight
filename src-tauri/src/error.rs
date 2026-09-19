//! Central error type for every AllInsight backend service.
//!
//! Everything that crosses the Tauri IPC boundary is serialised as a plain
//! string so the frontend never sees a Rust-specific shape. Internally we keep
//! the variants so callers can branch on the failure class - in particular on
//! `AccessDenied`, which is an entirely normal outcome when walking a Windows
//! filesystem without elevation and must never be treated as fatal.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AllInsightError {
    /// A filesystem entry could not be read. Expected during unelevated scans.
    AccessDenied(PathBuf),
    /// The path does not exist any more (races with the user deleting things).
    NotFound(PathBuf),
    /// The path is on the protected list and must never be touched.
    Protected { path: PathBuf, reason: String },
    /// The requested operation needs administrator rights.
    ElevationRequired(String),
    /// A candidate id supplied by the frontend does not match backend state.
    UnknownCandidate(String),
    /// Input from the frontend failed validation.
    InvalidInput(String),
    /// The underlying platform API failed.
    Platform(String),
    /// A database operation failed.
    Database(String),
    /// The local AI engine is unavailable or errored.
    Ai(String),
    /// A long running job was cancelled by the user.
    Cancelled,
    /// Anything else, with context.
    Other(String),
}

impl fmt::Display for AllInsightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllInsightError::AccessDenied(p) => write!(f, "Access denied: {}", p.display()),
            AllInsightError::NotFound(p) => write!(f, "Not found: {}", p.display()),
            AllInsightError::Protected { path, reason } => {
                write!(f, "Protected path refused ({}): {}", reason, path.display())
            }
            AllInsightError::ElevationRequired(what) => {
                write!(f, "Administrator permission is required to {what}")
            }
            AllInsightError::UnknownCandidate(id) => {
                write!(f, "Unknown or expired cleanup candidate: {id}")
            }
            AllInsightError::InvalidInput(m) => write!(f, "Invalid input: {m}"),
            AllInsightError::Platform(m) => write!(f, "Windows API error: {m}"),
            AllInsightError::Database(m) => write!(f, "Database error: {m}"),
            AllInsightError::Ai(m) => write!(f, "Local AI error: {m}"),
            AllInsightError::Cancelled => write!(f, "Operation cancelled"),
            AllInsightError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AllInsightError {}

impl serde::Serialize for AllInsightError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<std::io::Error> for AllInsightError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => AllInsightError::AccessDenied(PathBuf::new()),
            std::io::ErrorKind::NotFound => AllInsightError::NotFound(PathBuf::new()),
            _ => AllInsightError::Other(e.to_string()),
        }
    }
}

impl From<rusqlite::Error> for AllInsightError {
    fn from(e: rusqlite::Error) -> Self {
        AllInsightError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for AllInsightError {
    fn from(e: serde_json::Error) -> Self {
        AllInsightError::Other(format!("serialisation failed: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, AllInsightError>;

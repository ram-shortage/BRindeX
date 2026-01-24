//! FastFileIndex (FFI) - Instant file/folder name lookups for Windows.
//!
//! This library provides the core functionality for the FFI Windows service,
//! including database management, file indexing, and search capabilities.

pub mod service;
pub mod db;
pub mod indexer;
pub mod ipc;
pub mod search;
pub mod ui;

use thiserror::Error;

/// Volume state for lifecycle management.
///
/// Tracks whether a volume is actively monitored, temporarily offline,
/// undergoing indexing, or disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeState {
    /// Volume mounted and actively monitored.
    Online,
    /// Volume unmounted, data preserved. Contains timestamp when it went offline.
    Offline { since: i64 },
    /// Initial indexing in progress.
    Indexing,
    /// Background rescan in progress (after journal wrap or reconnect).
    Rescanning,
    /// Configured but not enabled for indexing.
    Disabled,
}

impl VolumeState {
    /// Parse from database string representation.
    pub fn from_db(state_str: &str, offline_since: Option<i64>) -> Self {
        match state_str {
            "online" => VolumeState::Online,
            "offline" => VolumeState::Offline { since: offline_since.unwrap_or(0) },
            "indexing" => VolumeState::Indexing,
            "rescanning" => VolumeState::Rescanning,
            "disabled" => VolumeState::Disabled,
            _ => VolumeState::Online, // Default fallback
        }
    }

    /// Convert to database string representation.
    pub fn to_db_str(&self) -> &'static str {
        match self {
            VolumeState::Online => "online",
            VolumeState::Offline { .. } => "offline",
            VolumeState::Indexing => "indexing",
            VolumeState::Rescanning => "rescanning",
            VolumeState::Disabled => "disabled",
        }
    }

    /// Get the offline_since timestamp if state is Offline.
    pub fn offline_since(&self) -> Option<i64> {
        match self {
            VolumeState::Offline { since } => Some(*since),
            _ => None,
        }
    }
}

/// FFI error types covering all failure modes.
#[derive(Error, Debug)]
pub enum FFIError {
    /// Windows service-related errors
    #[error("Service error: {0}")]
    Service(String),

    /// Database errors (SQLite operations)
    #[error("Database error: {0}")]
    Database(String),

    /// Indexer errors (MFT/filesystem scanning)
    #[error("Indexer error: {0}")]
    Indexer(String),

    /// Configuration errors (TOML parsing, file access)
    #[error("Config error: {0}")]
    Config(String),

    /// I/O errors (file/network operations)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// IPC errors (named pipe communication)
    #[error("IPC error: {0}")]
    Ipc(String),

    /// Search/parsing errors
    #[error("Search error: {0}")]
    Search(String),
}

/// Result type alias using FFIError
pub type Result<T> = std::result::Result<T, FFIError>;

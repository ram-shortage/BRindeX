//! FastFileIndex (FFI) - Instant file/folder name lookups for Windows.
//!
//! This library provides the core functionality for the FFI Windows service,
//! including database management, file indexing, and search capabilities.

pub mod service;
pub mod db;
// pub mod indexer; // Coming in Plan 01-03

use thiserror::Error;

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

    /// I/O errors (file/network operations)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias using FFIError
pub type Result<T> = std::result::Result<T, FFIError>;

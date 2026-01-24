//! Database module for FFI - SQLite with WAL mode for crash-safe persistence.
//!
//! This module provides database connection management with optimized PRAGMAs
//! for high-performance file indexing operations.

mod schema;
mod ops;

pub use ops::*;

use rusqlite::Connection;
use std::path::Path;

use crate::{FFIError, Result};

/// Database wrapper providing connection management.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get a mutable reference to the underlying connection.
    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

/// Open a database connection with WAL mode and optimized PRAGMAs.
///
/// This function:
/// 1. Creates parent directory if it doesn't exist
/// 2. Opens connection with rusqlite
/// 3. Configures WAL mode for crash safety
/// 4. Sets performance-optimized PRAGMAs
/// 5. Initializes schema (creates tables if needed)
///
/// # Arguments
/// * `path` - Path to the SQLite database file
///
/// # Returns
/// * `Result<Database>` - Wrapped database connection
pub fn open_database(path: &Path) -> Result<Database> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // Open the database connection
    let conn = Connection::open(path).map_err(|e| FFIError::Database(e.to_string()))?;

    // Configure WAL mode - persists to the database file
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| FFIError::Database(format!("Failed to set journal_mode: {}", e)))?;

    // NORMAL synchronous is safe in WAL mode, faster than FULL
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| FFIError::Database(format!("Failed to set synchronous: {}", e)))?;

    // Store temp tables in memory
    conn.pragma_update(None, "temp_store", "MEMORY")
        .map_err(|e| FFIError::Database(format!("Failed to set temp_store: {}", e)))?;

    // Enable memory-mapped I/O (256MB)
    conn.pragma_update(None, "mmap_size", 268435456i64)
        .map_err(|e| FFIError::Database(format!("Failed to set mmap_size: {}", e)))?;

    // 64MB page cache (negative value = KB)
    conn.pragma_update(None, "cache_size", -64000i32)
        .map_err(|e| FFIError::Database(format!("Failed to set cache_size: {}", e)))?;

    // 5 second busy timeout for concurrent access
    conn.pragma_update(None, "busy_timeout", 5000i32)
        .map_err(|e| FFIError::Database(format!("Failed to set busy_timeout: {}", e)))?;

    // Initialize schema (creates tables if needed)
    schema::init(&conn)?;

    Ok(Database { conn })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_open_database_creates_parent_dir() {
        let temp_dir = std::env::temp_dir().join("ffi_test_db");
        let db_path = temp_dir.join("subdir").join("test.db");

        // Ensure clean state
        let _ = fs::remove_dir_all(&temp_dir);

        // Open database should create parent directories
        let result = open_database(&db_path);
        assert!(result.is_ok());

        // Verify file was created
        assert!(db_path.exists());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let temp_dir = std::env::temp_dir().join("ffi_test_wal");
        let db_path = temp_dir.join("test.db");

        // Ensure clean state
        let _ = fs::remove_dir_all(&temp_dir);

        let db = open_database(&db_path).unwrap();

        // Verify WAL mode is enabled
        let journal_mode: String = db
            .conn()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}

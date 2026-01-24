//! Database schema module - table definitions and migrations.
//!
//! This module contains the SQL schema for the FFI database, including
//! the volumes and files tables with appropriate indexes.

use rusqlite::Connection;
use crate::{FFIError, Result};

/// Initialize the database schema.
///
/// Creates the volumes and files tables with appropriate indexes if they
/// don't already exist. This is called on every database open to ensure
/// the schema is up to date.
///
/// # Schema
///
/// ## volumes table
/// - `id`: Primary key
/// - `drive_letter`: Drive letter (e.g., "C:", "D:")
/// - `volume_serial`: Volume serial number for identity
/// - `fs_type`: Filesystem type ("NTFS", "FAT32", "exFAT")
/// - `last_usn`: Last processed USN (NTFS only)
/// - `usn_journal_id`: USN Journal ID (NTFS only)
/// - `last_scan_time`: Unix timestamp of last scan
///
/// ## files table
/// - `id`: Primary key
/// - `volume_id`: Foreign key to volumes
/// - `file_ref`: MFT file reference number (NTFS)
/// - `parent_ref`: Parent MFT reference (NTFS) or parent file id (FAT)
/// - `name`: Filename only (not full path)
/// - `size`: File size in bytes
/// - `modified`: Last modified time (Unix timestamp)
/// - `is_dir`: Whether this is a directory
///
/// ## Indexes
/// - `idx_files_name`: Fast case-insensitive filename search
/// - `idx_files_parent`: Path reconstruction (parent lookups)
/// - `idx_files_volume`: Volume-based operations
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS volumes (
            id INTEGER PRIMARY KEY,
            drive_letter TEXT NOT NULL UNIQUE,
            volume_serial TEXT NOT NULL,
            fs_type TEXT NOT NULL,
            last_usn INTEGER,
            usn_journal_id INTEGER,
            last_scan_time INTEGER
        );

        CREATE TABLE IF NOT EXISTS files (
            id INTEGER PRIMARY KEY,
            volume_id INTEGER NOT NULL REFERENCES volumes(id),
            file_ref INTEGER,
            parent_ref INTEGER,
            name TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            modified INTEGER,
            is_dir INTEGER NOT NULL DEFAULT 0,
            UNIQUE(volume_id, file_ref)
        );

        -- Index for fast filename search (case-insensitive)
        CREATE INDEX IF NOT EXISTS idx_files_name ON files(name COLLATE NOCASE);

        -- Index for path reconstruction (parent lookups)
        CREATE INDEX IF NOT EXISTS idx_files_parent ON files(volume_id, parent_ref);

        -- Index for volume-based operations
        CREATE INDEX IF NOT EXISTS idx_files_volume ON files(volume_id);
        "#,
    )
    .map_err(|e| FFIError::Database(format!("Failed to initialize schema: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_init_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();

        // Verify volumes table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='volumes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify files table exists
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='files'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_init_creates_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn).unwrap();

        // Verify all three indexes exist
        let indexes: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_files_%'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(indexes.contains(&"idx_files_name".to_string()));
        assert!(indexes.contains(&"idx_files_parent".to_string()));
        assert!(indexes.contains(&"idx_files_volume".to_string()));
    }

    #[test]
    fn test_schema_init_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Initialize twice - should not error
        init(&conn).unwrap();
        init(&conn).unwrap();
    }
}

//! Database operations module - CRUD operations and batch inserts.
//!
//! This module provides all database operations for the FFI index,
//! including volume management, file operations, and path reconstruction.

use rusqlite::{params, Connection};
use std::path::PathBuf;

use crate::{FFIError, Result};

/// Batch size for bulk inserts - 100,000 records per transaction.
/// This is optimal for SQLite per research benchmarks.
pub const BATCH_SIZE: usize = 100_000;

/// Information about an indexed volume.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// Database ID
    pub id: i64,
    /// Drive letter (e.g., "C:", "D:")
    pub drive_letter: String,
    /// Volume serial number
    pub volume_serial: String,
    /// Filesystem type ("NTFS", "FAT32", "exFAT")
    pub fs_type: String,
}

/// A file entry for insertion into the database.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Volume this file belongs to
    pub volume_id: i64,
    /// MFT file reference number (NTFS) or None for FAT
    pub file_ref: Option<i64>,
    /// Parent MFT reference (NTFS) or parent file ID (FAT)
    pub parent_ref: Option<i64>,
    /// Filename only (not full path)
    pub name: String,
    /// File size in bytes
    pub size: i64,
    /// Last modified time as Unix timestamp
    pub modified: Option<i64>,
    /// Whether this is a directory
    pub is_dir: bool,
}

// Volume operations will be implemented in Task 3
// File operations will be implemented in Task 3
// Path reconstruction will be implemented in Task 3

/// Insert or update a volume, returning its ID.
///
/// If a volume with the same drive letter exists, it will be updated.
/// Otherwise, a new volume record will be created.
pub fn insert_volume(
    conn: &Connection,
    drive_letter: &str,
    serial: &str,
    fs_type: &str,
) -> Result<i64> {
    // Use INSERT OR REPLACE to handle both insert and update
    conn.execute(
        "INSERT OR REPLACE INTO volumes (drive_letter, volume_serial, fs_type, last_scan_time)
         VALUES (?1, ?2, ?3, strftime('%s', 'now'))",
        params![drive_letter, serial, fs_type],
    )
    .map_err(|e| FFIError::Database(format!("Failed to insert volume: {}", e)))?;

    // Get the ID of the inserted/updated row
    let id = conn.last_insert_rowid();

    // If last_insert_rowid returns 0, the row already existed and was replaced
    // In that case, query for the actual ID
    if id == 0 {
        let id: i64 = conn
            .query_row(
                "SELECT id FROM volumes WHERE drive_letter = ?1",
                params![drive_letter],
                |row| row.get(0),
            )
            .map_err(|e| FFIError::Database(format!("Failed to get volume ID: {}", e)))?;
        Ok(id)
    } else {
        Ok(id)
    }
}

/// Get volume information by drive letter.
pub fn get_volume(conn: &Connection, drive_letter: &str) -> Result<Option<VolumeInfo>> {
    let result = conn.query_row(
        "SELECT id, drive_letter, volume_serial, fs_type FROM volumes WHERE drive_letter = ?1",
        params![drive_letter],
        |row| {
            Ok(VolumeInfo {
                id: row.get(0)?,
                drive_letter: row.get(1)?,
                volume_serial: row.get(2)?,
                fs_type: row.get(3)?,
            })
        },
    );

    match result {
        Ok(volume) => Ok(Some(volume)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(FFIError::Database(format!("Failed to get volume: {}", e))),
    }
}

/// Update USN tracking information for a volume.
pub fn update_volume_usn(
    conn: &Connection,
    volume_id: i64,
    last_usn: i64,
    journal_id: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE volumes SET last_usn = ?1, usn_journal_id = ?2 WHERE id = ?3",
        params![last_usn, journal_id, volume_id],
    )
    .map_err(|e| FFIError::Database(format!("Failed to update volume USN: {}", e)))?;

    Ok(())
}

/// Batch insert files with transactions for optimal performance.
///
/// Uses BATCH_SIZE (100,000) records per transaction as recommended
/// by SQLite benchmarks. Uses prepared cached statements for efficiency.
///
/// # Returns
/// The number of files successfully inserted.
pub fn batch_insert_files(conn: &mut Connection, files: &[FileEntry]) -> Result<usize> {
    let mut total_inserted = 0;

    for chunk in files.chunks(BATCH_SIZE) {
        let tx = conn
            .transaction()
            .map_err(|e| FFIError::Database(format!("Failed to start transaction: {}", e)))?;

        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO files (volume_id, file_ref, parent_ref, name, size, modified, is_dir)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| FFIError::Database(format!("Failed to prepare statement: {}", e)))?;

            for file in chunk {
                stmt.execute(params![
                    file.volume_id,
                    file.file_ref,
                    file.parent_ref,
                    file.name,
                    file.size,
                    file.modified,
                    file.is_dir as i32,
                ])
                .map_err(|e| FFIError::Database(format!("Failed to insert file: {}", e)))?;

                total_inserted += 1;
            }
        }

        tx.commit()
            .map_err(|e| FFIError::Database(format!("Failed to commit transaction: {}", e)))?;
    }

    Ok(total_inserted)
}

/// Delete all files for a volume.
///
/// # Returns
/// The number of files deleted.
pub fn delete_volume_files(conn: &Connection, volume_id: i64) -> Result<usize> {
    let deleted = conn
        .execute("DELETE FROM files WHERE volume_id = ?1", params![volume_id])
        .map_err(|e| FFIError::Database(format!("Failed to delete files: {}", e)))?;

    Ok(deleted)
}

/// Search files by name (case-insensitive LIKE search).
///
/// # Arguments
/// * `conn` - Database connection
/// * `query` - Search query (will be wrapped in %...%)
/// * `limit` - Maximum number of results to return
pub fn search_files(conn: &Connection, query: &str, limit: usize) -> Result<Vec<FileEntry>> {
    let pattern = format!("%{}%", query);

    let mut stmt = conn
        .prepare_cached(
            "SELECT volume_id, file_ref, parent_ref, name, size, modified, is_dir
             FROM files
             WHERE name LIKE ?1
             LIMIT ?2",
        )
        .map_err(|e| FFIError::Database(format!("Failed to prepare search: {}", e)))?;

    let rows = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(FileEntry {
                volume_id: row.get(0)?,
                file_ref: row.get(1)?,
                parent_ref: row.get(2)?,
                name: row.get(3)?,
                size: row.get(4)?,
                modified: row.get(5)?,
                is_dir: row.get::<_, i32>(6)? != 0,
            })
        })
        .map_err(|e| FFIError::Database(format!("Failed to execute search: {}", e)))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| FFIError::Database(format!("Failed to read row: {}", e)))?);
    }

    Ok(results)
}

/// Get the total count of files in the database.
///
/// # Arguments
/// * `conn` - Database connection
/// * `volume_id` - Optional volume ID to filter by
pub fn get_file_count(conn: &Connection, volume_id: Option<i64>) -> Result<i64> {
    let count = if let Some(vid) = volume_id {
        conn.query_row(
            "SELECT COUNT(*) FROM files WHERE volume_id = ?1",
            params![vid],
            |row| row.get(0),
        )
    } else {
        conn.query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
    };

    count.map_err(|e| FFIError::Database(format!("Failed to count files: {}", e)))
}

/// Reconstruct the full path for a file by walking the parent_ref chain.
///
/// # Arguments
/// * `conn` - Database connection
/// * `volume_id` - Volume the file belongs to
/// * `file_ref` - File reference to reconstruct path for
///
/// # Returns
/// The reconstructed path starting from root
pub fn reconstruct_path(conn: &Connection, volume_id: i64, file_ref: i64) -> Result<PathBuf> {
    let mut components: Vec<String> = Vec::new();
    let mut current_ref = Some(file_ref);

    // Walk up the parent chain
    while let Some(ref_num) = current_ref {
        let result = conn.query_row(
            "SELECT name, parent_ref FROM files WHERE volume_id = ?1 AND file_ref = ?2",
            params![volume_id, ref_num],
            |row| {
                let name: String = row.get(0)?;
                let parent: Option<i64> = row.get(1)?;
                Ok((name, parent))
            },
        );

        match result {
            Ok((name, parent)) => {
                // Don't add root node name (usually empty or ".")
                if !name.is_empty() && name != "." {
                    components.push(name);
                }
                current_ref = parent;
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Reached root or broken chain
                break;
            }
            Err(e) => {
                return Err(FFIError::Database(format!("Failed to reconstruct path: {}", e)));
            }
        }
    }

    // Reverse to get path from root to file
    components.reverse();

    // Build path
    let mut path = PathBuf::new();
    for component in components {
        path.push(component);
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::init(&conn).unwrap();
        conn
    }

    #[test]
    fn test_insert_volume() {
        let conn = setup_test_db();
        let id = insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_volume() {
        let conn = setup_test_db();
        insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();

        let volume = get_volume(&conn, "C:").unwrap();
        assert!(volume.is_some());
        let volume = volume.unwrap();
        assert_eq!(volume.drive_letter, "C:");
        assert_eq!(volume.volume_serial, "1234-ABCD");
        assert_eq!(volume.fs_type, "NTFS");
    }

    #[test]
    fn test_get_volume_not_found() {
        let conn = setup_test_db();
        let volume = get_volume(&conn, "Z:").unwrap();
        assert!(volume.is_none());
    }

    #[test]
    fn test_batch_insert_files() {
        let mut conn = setup_test_db();
        let volume_id = insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();

        let files: Vec<FileEntry> = (0..1000)
            .map(|i| FileEntry {
                volume_id,
                file_ref: Some(i),
                parent_ref: Some(0),
                name: format!("file_{}.txt", i),
                size: 1024,
                modified: Some(1700000000),
                is_dir: false,
            })
            .collect();

        let inserted = batch_insert_files(&mut conn, &files).unwrap();
        assert_eq!(inserted, 1000);

        // Verify count
        let count = get_file_count(&conn, Some(volume_id)).unwrap();
        assert_eq!(count, 1000);
    }

    #[test]
    fn test_search_files() {
        let mut conn = setup_test_db();
        let volume_id = insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();

        let files = vec![
            FileEntry {
                volume_id,
                file_ref: Some(1),
                parent_ref: Some(0),
                name: "document.txt".to_string(),
                size: 1024,
                modified: Some(1700000000),
                is_dir: false,
            },
            FileEntry {
                volume_id,
                file_ref: Some(2),
                parent_ref: Some(0),
                name: "Document.pdf".to_string(),
                size: 2048,
                modified: Some(1700000000),
                is_dir: false,
            },
            FileEntry {
                volume_id,
                file_ref: Some(3),
                parent_ref: Some(0),
                name: "image.png".to_string(),
                size: 4096,
                modified: Some(1700000000),
                is_dir: false,
            },
        ];

        batch_insert_files(&mut conn, &files).unwrap();

        // Case-insensitive search should find both document files
        let results = search_files(&conn, "document", 100).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_delete_volume_files() {
        let mut conn = setup_test_db();
        let volume_id = insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();

        let files: Vec<FileEntry> = (0..100)
            .map(|i| FileEntry {
                volume_id,
                file_ref: Some(i),
                parent_ref: Some(0),
                name: format!("file_{}.txt", i),
                size: 1024,
                modified: Some(1700000000),
                is_dir: false,
            })
            .collect();

        batch_insert_files(&mut conn, &files).unwrap();

        let deleted = delete_volume_files(&conn, volume_id).unwrap();
        assert_eq!(deleted, 100);

        let count = get_file_count(&conn, Some(volume_id)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_reconstruct_path() {
        let mut conn = setup_test_db();
        let volume_id = insert_volume(&conn, "C:", "1234-ABCD", "NTFS").unwrap();

        // Create a directory hierarchy: root -> Users -> John -> Documents -> file.txt
        let files = vec![
            FileEntry {
                volume_id,
                file_ref: Some(5),
                parent_ref: None, // Root
                name: "".to_string(),
                size: 0,
                modified: None,
                is_dir: true,
            },
            FileEntry {
                volume_id,
                file_ref: Some(100),
                parent_ref: Some(5),
                name: "Users".to_string(),
                size: 0,
                modified: None,
                is_dir: true,
            },
            FileEntry {
                volume_id,
                file_ref: Some(200),
                parent_ref: Some(100),
                name: "John".to_string(),
                size: 0,
                modified: None,
                is_dir: true,
            },
            FileEntry {
                volume_id,
                file_ref: Some(300),
                parent_ref: Some(200),
                name: "Documents".to_string(),
                size: 0,
                modified: None,
                is_dir: true,
            },
            FileEntry {
                volume_id,
                file_ref: Some(400),
                parent_ref: Some(300),
                name: "file.txt".to_string(),
                size: 1024,
                modified: Some(1700000000),
                is_dir: false,
            },
        ];

        batch_insert_files(&mut conn, &files).unwrap();

        let path = reconstruct_path(&conn, volume_id, 400).unwrap();
        assert_eq!(path, PathBuf::from("Users/John/Documents/file.txt"));
    }
}

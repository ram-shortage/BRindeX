//! FAT volume directory walker using walkdir.
//!
//! This module provides indexing for FAT32/exFAT volumes that don't have
//! an MFT. Uses directory traversal which is slower but works universally.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;

use crate::db::{batch_insert_files, insert_volume, Database, FileEntry};
use crate::Result;

/// Batch size for database inserts
const BATCH_SIZE: usize = 100_000;

/// Progress logging interval
const PROGRESS_INTERVAL: usize = 50_000;

/// Shutdown check interval
const SHUTDOWN_CHECK_INTERVAL: usize = 10_000;

/// Scan a FAT volume using directory walking.
///
/// This function:
/// 1. Walks the directory tree starting from root
/// 2. Generates synthetic file references for FAT (no MFT refs)
/// 3. Tracks parent-child relationships for path reconstruction
/// 4. Batches entries for database insertion
/// 5. Checks for shutdown signal periodically
///
/// # Arguments
/// * `drive_letter` - The drive letter to scan (e.g., 'D')
/// * `db` - Database instance for persisting indexed files
/// * `shutdown_rx` - Channel receiver for shutdown signals
///
/// # Returns
/// The total number of files indexed.
pub fn scan_fat_volume(
    drive_letter: char,
    db: &mut Database,
    shutdown_rx: &Receiver<()>,
) -> Result<usize> {
    // Construct root path
    #[cfg(windows)]
    let root_path = format!("{}:\\", drive_letter);
    #[cfg(not(windows))]
    let root_path = format!("/mnt/{}", drive_letter.to_lowercase());

    tracing::info!("Starting FAT volume scan for {}", root_path);

    // Insert or update volume record
    let volume_id = insert_volume(
        db.conn(),
        &format!("{}:", drive_letter),
        "", // Serial from volume detection
        "FAT", // Could be FAT32 or exFAT, generic label
    )?;

    // Synthetic file reference counter
    // FAT doesn't have MFT references, so we generate sequential IDs
    let mut next_file_ref: i64 = 1;

    // Track path -> file_ref mapping for parent reference lookups
    let mut path_to_ref: HashMap<PathBuf, i64> = HashMap::new();

    // Root directory gets ref 0 (like MFT root entry 5)
    let root = PathBuf::from(&root_path);
    path_to_ref.insert(root.clone(), 0);

    let mut batch: Vec<FileEntry> = Vec::with_capacity(BATCH_SIZE);
    let mut total_indexed = 0;
    let mut errors = 0;
    let mut count = 0;

    // Walk the directory tree
    for entry_result in WalkDir::new(&root_path)
        .follow_links(false)
        .into_iter()
    {
        count += 1;

        // Check for shutdown periodically
        if count % SHUTDOWN_CHECK_INTERVAL == 0 {
            if shutdown_rx.try_recv().is_ok() {
                tracing::info!("Shutdown signal received during FAT scan");
                // Flush any remaining entries
                if !batch.is_empty() {
                    let inserted = batch_insert_files(db.conn_mut(), &batch)?;
                    total_indexed += inserted;
                }
                return Ok(total_indexed);
            }
        }

        // Log progress
        if count % PROGRESS_INTERVAL == 0 {
            tracing::info!("FAT scan progress: {} entries processed", count);
        }

        // Handle entry
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                errors += 1;
                if errors <= 10 {
                    tracing::debug!("Error walking directory: {}", e);
                }
                continue;
            }
        };

        let path = entry.path().to_path_buf();

        // Skip root directory itself (already tracked)
        if path == root {
            continue;
        }

        // Get metadata
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                errors += 1;
                if errors <= 10 {
                    tracing::debug!("Cannot get metadata for {:?}: {}", path, e);
                }
                continue;
            }
        };

        // Extract file information
        let name = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        let is_dir = metadata.is_dir();
        let size = if is_dir { 0 } else { metadata.len() as i64 };

        // Get modified time
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        // Assign synthetic file reference
        let file_ref = next_file_ref;
        next_file_ref += 1;

        // Store path -> ref mapping
        path_to_ref.insert(path.clone(), file_ref);

        // Get parent reference
        let parent_ref = path
            .parent()
            .and_then(|p| path_to_ref.get(p))
            .copied();

        // Add to batch
        batch.push(FileEntry {
            volume_id,
            file_ref: Some(file_ref),
            parent_ref,
            name,
            size,
            modified,
            is_dir,
        });

        // Flush batch when full
        if batch.len() >= BATCH_SIZE {
            let inserted = batch_insert_files(db.conn_mut(), &batch)?;
            total_indexed += inserted;
            batch.clear();

            // Clear path_to_ref to limit memory usage
            // Keep only entries that might still have children
            // (directories in the last level of the batch)
            // For simplicity, we'll keep all - this may use more memory
            // but ensures accurate parent refs
        }
    }

    // Insert remaining entries
    if !batch.is_empty() {
        let inserted = batch_insert_files(db.conn_mut(), &batch)?;
        total_indexed += inserted;
    }

    if errors > 0 {
        tracing::warn!("Encountered {} errors during FAT scan", errors);
    }

    tracing::info!(
        "FAT volume scan complete for {}: {} files indexed",
        root_path,
        total_indexed
    );

    Ok(total_indexed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_size() {
        assert_eq!(BATCH_SIZE, 100_000);
    }

    #[test]
    fn test_progress_interval() {
        assert_eq!(PROGRESS_INTERVAL, 50_000);
    }

    #[test]
    fn test_shutdown_check_interval() {
        assert_eq!(SHUTDOWN_CHECK_INTERVAL, 10_000);
    }
}

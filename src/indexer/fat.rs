//! FAT volume directory walker using walkdir.
//!
//! This module provides indexing for FAT32/exFAT volumes that don't have
//! an MFT. Uses directory traversal which is slower but works universally.

use std::sync::mpsc::Receiver;

use crate::db::Database;
use crate::Result;

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
    _drive_letter: char,
    _db: &mut Database,
    _shutdown_rx: &Receiver<()>,
) -> Result<usize> {
    // Implementation in Task 3
    tracing::warn!("FAT volume scanning not yet implemented");
    Ok(0)
}

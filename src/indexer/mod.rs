//! File indexing orchestration and background task management.
//!
//! This module coordinates volume detection and file scanning,
//! dispatching to the appropriate scanner (MFT for NTFS, walkdir for FAT).
//! Also provides USN Journal monitoring for real-time NTFS updates.

mod volume;
mod mft;
mod fat;
pub mod usn_monitor;

pub use volume::*;
pub use mft::*;
pub use fat::*;
pub use usn_monitor::{ChangeType, UsnChange, UsnError, UsnMonitor, deduplicate_changes, apply_changes_batch};

use std::sync::mpsc::Receiver;
use std::thread::{self, JoinHandle};

use crate::db::Database;

/// Background indexer that scans volumes and populates the database.
pub struct Indexer {
    handle: Option<JoinHandle<()>>,
}

impl Indexer {
    /// Signal the indexer to stop.
    pub fn stop(&mut self) {
        // The indexer checks shutdown_rx; dropping it will cause try_recv to return Disconnected
        // which we interpret as a shutdown signal. The actual signaling is done via the channel.
        // Here we just wait for the thread to finish.
        if let Some(handle) = self.handle.take() {
            // Wait for the indexer thread to finish (it checks shutdown_rx)
            match handle.join() {
                Ok(()) => tracing::info!("Indexer thread stopped gracefully"),
                Err(_) => tracing::error!("Indexer thread panicked"),
            }
        }
    }
}

/// Start a background indexer that scans all detected volumes.
///
/// The indexer runs in a separate thread and:
/// 1. Detects available volumes
/// 2. For each volume, chooses the appropriate scanner (MFT for NTFS, walkdir for FAT)
/// 3. Streams file entries to the database in batches
/// 4. Checks for shutdown signal periodically
///
/// # Arguments
/// * `db` - Database instance for persisting indexed files
/// * `shutdown_rx` - Channel receiver for shutdown signals
///
/// # Returns
/// An `Indexer` instance that can be used to stop the background task.
pub fn start_background_indexer(db: Database, shutdown_rx: Receiver<()>) -> Indexer {
    let handle = thread::spawn(move || {
        run_indexer(db, shutdown_rx);
    });

    Indexer {
        handle: Some(handle),
    }
}

/// Internal function that runs the indexing loop.
fn run_indexer(mut db: Database, shutdown_rx: Receiver<()>) {
    tracing::info!("Background indexer started");

    // Detect available volumes
    let volumes = detect_volumes();
    tracing::info!("Detected {} volumes", volumes.len());

    for volume in &volumes {
        // Check for shutdown before processing each volume
        if shutdown_rx.try_recv().is_ok() {
            tracing::info!("Shutdown signal received, stopping indexer");
            return;
        }

        tracing::info!(
            "Indexing volume {}: {:?} ({:?})",
            volume.drive_letter,
            volume.fs_type,
            volume.volume_serial
        );

        let result = match volume.fs_type {
            VolumeType::NTFS => scan_ntfs_volume(volume.drive_letter, &mut db, &shutdown_rx),
            VolumeType::FAT32 | VolumeType::ExFAT => {
                scan_fat_volume(volume.drive_letter, &mut db, &shutdown_rx)
            }
            VolumeType::Unknown => {
                tracing::warn!(
                    "Skipping volume {} with unknown filesystem type",
                    volume.drive_letter
                );
                continue;
            }
        };

        match result {
            Ok(count) => {
                tracing::info!(
                    "Volume {} indexing complete: {} files",
                    volume.drive_letter,
                    count
                );
            }
            Err(e) => {
                tracing::error!("Failed to index volume {}: {}", volume.drive_letter, e);
            }
        }
    }

    tracing::info!("Background indexer finished");
}

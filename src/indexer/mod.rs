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
pub use usn_monitor::{
    ChangeType, UsnChange, UsnError, UsnMonitor,
    AdaptiveThrottle, UsnMonitorHandle,
    deduplicate_changes, apply_changes_batch, usn_monitor_loop, trigger_background_rescan,
};

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

/// Collection of active USN monitor handles for managing lifecycle.
pub struct UsnMonitors {
    handles: Vec<UsnMonitorHandle>,
    shutdown_txs: Vec<std::sync::mpsc::Sender<()>>,
}

impl UsnMonitors {
    /// Create an empty monitors collection.
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
            shutdown_txs: Vec::new(),
        }
    }

    /// Stop all monitors gracefully.
    pub fn stop_all(&mut self) {
        tracing::info!("Stopping all USN monitors...");

        // Signal all monitors to stop
        for tx in self.shutdown_txs.drain(..) {
            let _ = tx.send(());
        }

        // Wait for all threads to finish
        for mut handle in self.handles.drain(..) {
            handle.stop();
        }

        tracing::info!("All USN monitors stopped");
    }
}

impl Default for UsnMonitors {
    fn default() -> Self {
        Self::new()
    }
}

/// Start USN monitors for all configured NTFS volumes.
///
/// This function should be called after initial indexing completes.
/// It spawns a monitoring thread for each NTFS volume that has
/// USN journal support.
///
/// # Arguments
/// * `db_path` - Path to the database (each monitor opens its own connection)
/// * `poll_interval_secs` - Polling interval in seconds (from config)
///
/// # Returns
/// A `UsnMonitors` instance for managing the monitor lifecycle.
pub fn start_usn_monitors(
    db_path: &std::path::Path,
    poll_interval_secs: u64,
) -> UsnMonitors {
    use crate::db::{open_database, get_volume_usn, get_volume};

    let mut monitors = UsnMonitors::new();

    // Detect NTFS volumes
    let volumes = detect_volumes();
    let ntfs_volumes: Vec<_> = volumes
        .iter()
        .filter(|v| v.fs_type == VolumeType::NTFS)
        .collect();

    if ntfs_volumes.is_empty() {
        tracing::info!("No NTFS volumes found, skipping USN monitoring");
        return monitors;
    }

    tracing::info!("Starting USN monitors for {} NTFS volumes", ntfs_volumes.len());

    for volume in ntfs_volumes {
        let drive_letter = volume.drive_letter;

        // Each monitor needs its own database connection
        let db = match open_database(db_path) {
            Ok(db) => db,
            Err(e) => {
                tracing::error!("Failed to open database for USN monitor {}: {}", drive_letter, e);
                continue;
            }
        };

        // Check for saved USN state to resume from
        let drive_str = format!("{}:", drive_letter);
        let resume_usn = match get_volume(db.conn(), &drive_str) {
            Ok(Some(vol)) => {
                match get_volume_usn(db.conn(), vol.id) {
                    Ok(usn_state) => usn_state,
                    Err(e) => {
                        tracing::warn!("Failed to get USN state for {}: {}", drive_letter, e);
                        None
                    }
                }
            }
            _ => None,
        };

        // Create shutdown channel for this monitor
        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

        // Start the monitor
        let handle = usn_monitor_loop(
            drive_letter,
            db,
            poll_interval_secs,
            shutdown_rx,
            resume_usn,
        );

        monitors.handles.push(handle);
        monitors.shutdown_txs.push(shutdown_tx);

        tracing::info!("Started USN monitor for volume {}", drive_letter);
    }

    monitors
}

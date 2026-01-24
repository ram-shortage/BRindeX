//! File indexing orchestration and background task management.
//!
//! This module coordinates volume detection and file scanning,
//! dispatching to the appropriate scanner (MFT for NTFS, walkdir for FAT).
//! Also provides USN Journal monitoring for real-time NTFS updates,
//! and FAT volume periodic reconciliation.

mod volume;
mod mft;
mod fat;
pub mod usn_monitor;
pub mod fat_reconciler;

pub use volume::*;
pub use mft::*;
pub use fat::*;
pub use usn_monitor::{
    ChangeType, UsnChange, UsnError, UsnMonitor,
    AdaptiveThrottle, UsnMonitorHandle,
    deduplicate_changes, apply_changes_batch, usn_monitor_loop, trigger_background_rescan,
};
pub use fat_reconciler::{FatReconciler, FatReconcilerHandle, start_fat_reconciler};

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

/// Handle a volume mount event.
///
/// This function:
/// 1. Checks if volume is configured for indexing
/// 2. Compares volume serial to detect volume swaps
/// 3. Sets volume state to Online
/// 4. Triggers appropriate indexing (NTFS USN monitor or FAT reconciliation)
///
/// # Arguments
/// * `drive_letter` - The mounted drive letter
/// * `config` - Service configuration
/// * `db_path` - Path to the database
///
/// # Returns
/// Ok if handled successfully, error otherwise.
pub fn handle_volume_mount(
    drive_letter: char,
    config: &crate::service::config::Config,
    db_path: &std::path::Path,
) -> crate::Result<()> {
    use crate::db::{open_database, get_volume, update_volume_state, get_volume_serial};
    use crate::VolumeState;

    // Check if volume is configured for indexing
    if !config.is_volume_enabled(drive_letter) {
        tracing::debug!("Volume {} mounted but not configured for indexing", drive_letter);
        return Ok(());
    }

    tracing::info!("Handling mount event for configured volume {}", drive_letter);

    // Get volume serial number
    let serial = get_volume_serial(drive_letter);
    let serial_str = serial.map(|s| format!("{:08X}", s)).unwrap_or_default();

    // Open database
    let db = open_database(db_path)?;

    // Get existing volume record
    let drive_str = format!("{}:", drive_letter);
    let existing = get_volume(db.conn(), &drive_str)?;

    if let Some(vol) = existing {
        // Check if serial matches (same physical volume)
        if vol.volume_serial != serial_str && !serial_str.is_empty() {
            // Different volume at same drive letter!
            tracing::warn!(
                "Volume swap detected at {}: {} -> {}",
                drive_letter,
                vol.volume_serial,
                serial_str
            );

            // Mark old data as offline
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            update_volume_state(db.conn(), vol.id, VolumeState::Offline { since: now })?;

            tracing::info!("Old volume {} marked offline, fresh index needed", vol.volume_serial);

            // Note: A fresh index will be triggered by the next indexing run
            // The FAT reconciler or USN monitor setup will create a new volume record
        } else {
            // Same volume reconnected - set to Online and trigger reconciliation
            tracing::info!("Volume {} reconnected (serial: {})", drive_letter, serial_str);
            update_volume_state(db.conn(), vol.id, VolumeState::Online)?;

            // Note: Quick reconciliation will happen on next FAT reconciler cycle
            // or USN monitor will catch up from stored last_usn
        }
    } else {
        tracing::info!("New volume {} detected (serial: {}), will be indexed", drive_letter, serial_str);
        // New volume - will be picked up by indexer
    }

    Ok(())
}

/// Handle a volume unmount event.
///
/// Sets the volume state to Offline with current timestamp.
/// The volume data is preserved for quick reconnection, and will be
/// automatically cleaned up after the configured retention period.
///
/// # Arguments
/// * `drive_letter` - The unmounted drive letter
/// * `db_path` - Path to the database
///
/// # Returns
/// Ok if handled successfully, error otherwise.
pub fn handle_volume_unmount(
    drive_letter: char,
    db_path: &std::path::Path,
) -> crate::Result<()> {
    use crate::db::{open_database, get_volume, update_volume_state};
    use crate::VolumeState;

    tracing::info!("Handling unmount event for volume {}", drive_letter);

    // Open database
    let db = open_database(db_path)?;

    // Get volume record
    let drive_str = format!("{}:", drive_letter);
    if let Some(vol) = get_volume(db.conn(), &drive_str)? {
        // Set to offline with current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        update_volume_state(db.conn(), vol.id, VolumeState::Offline { since: now })?;

        tracing::info!(
            "Volume {} marked offline, data preserved for 7 days",
            drive_letter
        );
    } else {
        tracing::debug!("Volume {} unmounted but not in database", drive_letter);
    }

    Ok(())
}

/// Start the volume event handler thread.
///
/// Spawns a thread that receives VolumeEvents and dispatches them
/// to handle_volume_mount/handle_volume_unmount.
///
/// Debounces mount events with a 100ms window to handle boot-time floods.
///
/// # Arguments
/// * `event_rx` - Receiver for volume events
/// * `config` - Service configuration
/// * `db_path` - Path to the database
/// * `shutdown_rx` - Shutdown signal receiver
///
/// # Returns
/// Thread handle for the event handler.
pub fn start_volume_event_handler(
    event_rx: std::sync::mpsc::Receiver<crate::service::VolumeEvent>,
    config: crate::service::config::Config,
    db_path: std::path::PathBuf,
    shutdown_rx: std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    use crate::service::VolumeEvent;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    std::thread::spawn(move || {
        const DEBOUNCE_WINDOW: Duration = Duration::from_millis(100);
        let mut pending_mounts: HashMap<char, Instant> = HashMap::new();

        loop {
            // Check for shutdown
            match shutdown_rx.try_recv() {
                Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::info!("Volume event handler shutting down");
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }

            // Process any ready debounced mounts
            let now = Instant::now();
            let ready: Vec<_> = pending_mounts
                .iter()
                .filter(|(_, &time)| now.duration_since(time) >= DEBOUNCE_WINDOW)
                .map(|(&drive, _)| drive)
                .collect();

            for drive in ready {
                pending_mounts.remove(&drive);
                if let Err(e) = handle_volume_mount(drive, &config, &db_path) {
                    tracing::error!("Failed to handle mount for {}: {}", drive, e);
                }
            }

            // Receive events with timeout
            match event_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(VolumeEvent::Mounted(drive)) => {
                    // Add to pending with current time (debounce)
                    pending_mounts.insert(drive, Instant::now());
                }
                Ok(VolumeEvent::Unmounted(drive)) => {
                    // Remove from pending if was queued
                    pending_mounts.remove(&drive);
                    // Handle unmount immediately
                    if let Err(e) = handle_volume_unmount(drive, &db_path) {
                        tracing::error!("Failed to handle unmount for {}: {}", drive, e);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::info!("Volume event channel disconnected");
                    return;
                }
            }
        }
    })
}

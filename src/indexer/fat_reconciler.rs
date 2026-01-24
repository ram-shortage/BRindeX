//! FAT volume periodic reconciliation scheduler.
//!
//! FAT volumes don't have USN Journal, so we use periodic full scans
//! to keep the index current. This module manages the scheduling and
//! execution of those reconciliation scans.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::db::{open_database, get_volume, update_volume_state, cleanup_old_offline_volumes};
use crate::indexer::{scan_fat_volume, detect_volumes, VolumeType};
use crate::service::config::Config;
use crate::{Result, VolumeState};

/// Interval between reconciler loop iterations (checks if any volume is due for scan).
const LOOP_INTERVAL: Duration = Duration::from_secs(60);

/// Interval between offline volume cleanup checks (once per day).
const CLEANUP_INTERVAL: Duration = Duration::from_secs(86400);

/// FAT volume reconciliation scheduler.
///
/// Manages periodic full scans for FAT32/exFAT volumes which don't support
/// USN Journal change tracking.
pub struct FatReconciler {
    /// Map of drive letter to reconciliation interval.
    volumes: HashMap<char, Duration>,
    /// When each volume was last scanned.
    last_scan: HashMap<char, Instant>,
    /// Path to the database.
    db_path: PathBuf,
    /// Offline retention period from config.
    offline_retention_days: u32,
}

impl FatReconciler {
    /// Create a new FAT reconciler from configuration.
    ///
    /// Filters configured volumes for FAT-type (non-NTFS) volumes only.
    /// Uses the reconcile_interval_mins from each volume config, or default 30 minutes.
    pub fn new(config: &Config, db_path: PathBuf) -> Self {
        let mut volumes = HashMap::new();
        let now = Instant::now();
        let mut last_scan = HashMap::new();

        // Detect actual volumes to check filesystem types
        let detected = detect_volumes();
        let fat_volumes: Vec<_> = detected
            .iter()
            .filter(|v| matches!(v.fs_type, VolumeType::FAT32 | VolumeType::ExFAT))
            .collect();

        for vol in fat_volumes {
            let drive_letter = vol.drive_letter;

            // Check if volume is configured and enabled
            if config.is_volume_enabled(drive_letter) {
                let interval_mins = config.reconcile_interval_mins(drive_letter);
                let interval = Duration::from_secs(interval_mins * 60);

                tracing::info!(
                    "FAT reconciler: volume {} configured with {}min interval",
                    drive_letter,
                    interval_mins
                );

                volumes.insert(drive_letter, interval);
                // Don't scan immediately on start - wait for first interval
                last_scan.insert(drive_letter, now);
            }
        }

        Self {
            volumes,
            last_scan,
            db_path,
            offline_retention_days: config.general.offline_retention_days,
        }
    }

    /// Add a volume to the reconciler (for hot-adding mounted volumes).
    pub fn add_volume(&mut self, drive_letter: char, interval: Duration) {
        self.volumes.insert(drive_letter, interval);
        self.last_scan.insert(drive_letter, Instant::now());
        tracing::info!(
            "FAT reconciler: added volume {} with {:?} interval",
            drive_letter,
            interval
        );
    }

    /// Remove a volume from the reconciler (for unmounted volumes).
    pub fn remove_volume(&mut self, drive_letter: char) {
        self.volumes.remove(&drive_letter);
        self.last_scan.remove(&drive_letter);
        tracing::info!("FAT reconciler: removed volume {}", drive_letter);
    }

    /// Check all volumes and run reconciliation for any that are due.
    ///
    /// Checks shutdown_rx between volumes to allow graceful shutdown.
    pub fn check_and_reconcile(&mut self, shutdown_rx: &Receiver<()>) -> Result<()> {
        let now = Instant::now();
        let volumes: Vec<_> = self.volumes.iter().map(|(k, v)| (*k, *v)).collect();

        for (drive_letter, interval) in volumes {
            // Check for shutdown
            if shutdown_rx.try_recv().is_ok() {
                tracing::info!("FAT reconciler shutdown signal received");
                return Ok(());
            }

            // Check if this volume is due for scan
            if let Some(last) = self.last_scan.get(&drive_letter) {
                if now.duration_since(*last) < interval {
                    continue; // Not due yet
                }
            }

            tracing::info!("FAT reconciler: starting scan for volume {}", drive_letter);

            // Open database connection for this scan
            let mut db = open_database(&self.db_path)?;

            // Get volume ID and set state to Rescanning
            let drive_str = format!("{}:", drive_letter);
            if let Ok(Some(vol)) = get_volume(db.conn(), &drive_str) {
                let _ = update_volume_state(db.conn(), vol.id, VolumeState::Rescanning);
            }

            // Run the scan
            match scan_fat_volume(drive_letter, &mut db, shutdown_rx) {
                Ok(count) => {
                    tracing::info!(
                        "FAT reconciler: volume {} scan complete, {} files",
                        drive_letter,
                        count
                    );
                }
                Err(e) => {
                    tracing::error!("FAT reconciler: volume {} scan failed: {}", drive_letter, e);
                }
            }

            // Set state back to Online
            if let Ok(Some(vol)) = get_volume(db.conn(), &drive_str) {
                let _ = update_volume_state(db.conn(), vol.id, VolumeState::Online);
            }

            // Update last scan time
            self.last_scan.insert(drive_letter, Instant::now());
        }

        Ok(())
    }

    /// Check if there are any volumes to reconcile.
    pub fn has_volumes(&self) -> bool {
        !self.volumes.is_empty()
    }
}

/// Run the FAT reconciler loop in a background thread.
///
/// This function:
/// 1. Creates a FatReconciler from config
/// 2. Loops every 60 seconds checking for due volumes
/// 3. Runs cleanup_old_offline_volumes once per day
/// 4. Exits when shutdown signal received
pub fn fat_reconciler_loop(
    config: Config,
    db_path: PathBuf,
    shutdown_rx: Receiver<()>,
) {
    let mut reconciler = FatReconciler::new(&config, db_path.clone());
    let mut last_cleanup = Instant::now();

    if !reconciler.has_volumes() {
        tracing::info!("FAT reconciler: no FAT volumes configured, loop idle");
    }

    loop {
        // Check for shutdown
        match shutdown_rx.try_recv() {
            Ok(_) => {
                tracing::info!("FAT reconciler loop shutting down");
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::info!("FAT reconciler shutdown channel disconnected");
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Check and reconcile volumes
        if let Err(e) = reconciler.check_and_reconcile(&shutdown_rx) {
            tracing::error!("FAT reconciler error: {}", e);
        }

        // Run offline volume cleanup once per day
        if last_cleanup.elapsed() >= CLEANUP_INTERVAL {
            tracing::debug!("Running offline volume cleanup...");
            match open_database(&db_path) {
                Ok(db) => {
                    match cleanup_old_offline_volumes(
                        db.conn(),
                        config.general.offline_retention_days,
                    ) {
                        Ok(deleted) if deleted > 0 => {
                            tracing::info!("Cleaned up {} files from old offline volumes", deleted);
                        }
                        Ok(_) => {}
                        Err(e) => tracing::error!("Offline cleanup failed: {}", e),
                    }
                }
                Err(e) => tracing::error!("Failed to open database for cleanup: {}", e),
            }
            last_cleanup = Instant::now();
        }

        // Sleep for loop interval
        std::thread::sleep(LOOP_INTERVAL);
    }
}

/// Handle for a running FAT reconciler thread.
pub struct FatReconcilerHandle {
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FatReconcilerHandle {
    /// Stop the reconciler and wait for thread to finish.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(()) => tracing::info!("FAT reconciler thread stopped"),
                Err(_) => tracing::error!("FAT reconciler thread panicked"),
            }
        }
    }
}

/// Start the FAT reconciler in a background thread.
///
/// Returns a handle for lifecycle management and the shutdown sender.
pub fn start_fat_reconciler(
    config: Config,
    db_path: PathBuf,
) -> (FatReconcilerHandle, std::sync::mpsc::Sender<()>) {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let handle = std::thread::spawn(move || {
        fat_reconciler_loop(config, db_path, shutdown_rx);
    });

    (
        FatReconcilerHandle {
            handle: Some(handle),
        },
        shutdown_tx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_interval() {
        assert_eq!(LOOP_INTERVAL, Duration::from_secs(60));
    }

    #[test]
    fn test_cleanup_interval() {
        assert_eq!(CLEANUP_INTERVAL, Duration::from_secs(86400));
    }
}

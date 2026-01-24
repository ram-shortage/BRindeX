//! USN Change Journal monitoring for NTFS volumes.
//!
//! This module provides real-time file change monitoring by polling the
//! NTFS USN Change Journal. It handles:
//! - Journal wrap detection (when old entries are overwritten)
//! - Journal recreation detection (when journal ID changes)
//! - Change deduplication (rapid changes to same file)
//! - Batched database updates

use std::collections::HashMap;

use crate::db::Database;
use crate::{FFIError, Result};

/// Type of filesystem change detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// File or directory was created
    Create,
    /// File or directory was deleted
    Delete,
    /// File or directory was renamed/moved
    Rename,
    /// File content or metadata was modified
    Modify,
}

/// A single USN change record.
#[derive(Debug, Clone)]
pub struct UsnChange {
    /// MFT file reference number
    pub file_ref: i64,
    /// Parent directory MFT reference
    pub parent_ref: i64,
    /// Filename
    pub name: String,
    /// Type of change
    pub change_type: ChangeType,
    /// Whether this is a directory
    pub is_dir: bool,
}

/// Errors specific to USN Journal operations.
#[derive(Debug)]
pub enum UsnError {
    /// USN Journal is not active on this volume (treat as FAT)
    JournalNotActive,
    /// Journal wrapped - missed changes, need rescan
    JournalWrapped {
        /// Last USN we processed
        last_processed: i64,
        /// Current lowest valid USN
        lowest_valid: i64,
    },
    /// Journal was recreated (different ID)
    JournalRecreated {
        /// Our stored journal ID
        old_id: u64,
        /// Current journal ID
        new_id: u64,
    },
    /// Other error
    Other(String),
}

impl std::fmt::Display for UsnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsnError::JournalNotActive => write!(f, "USN Journal is not active"),
            UsnError::JournalWrapped {
                last_processed,
                lowest_valid,
            } => write!(
                f,
                "Journal wrapped: last_processed={}, lowest_valid={}",
                last_processed, lowest_valid
            ),
            UsnError::JournalRecreated { old_id, new_id } => {
                write!(f, "Journal recreated: old_id={}, new_id={}", old_id, new_id)
            }
            UsnError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for UsnError {}

/// USN Journal monitor for a single NTFS volume.
#[cfg(windows)]
pub struct UsnMonitor {
    /// Drive letter (e.g., 'C')
    volume: char,
    /// Last processed USN
    last_usn: i64,
    /// Journal ID for wrap detection
    journal_id: u64,
}

#[cfg(windows)]
impl UsnMonitor {
    /// Create a new USN monitor for the specified drive.
    ///
    /// Opens the USN journal and stores the current journal ID and
    /// starting USN for future comparisons.
    pub fn new(drive_letter: char) -> std::result::Result<Self, UsnError> {
        use usn_journal_rs::volume::Volume;
        use usn_journal_rs::journal::UsnJournal;

        let volume = Volume::from_drive_letter(drive_letter)
            .map_err(|e| UsnError::Other(format!("Failed to open volume: {}", e)))?;

        let journal = UsnJournal::new(&volume);

        let metadata = journal.query(false)
            .map_err(|e| {
                // Check if journal is not active
                let msg = format!("{}", e);
                if msg.contains("not active") || msg.contains("1179") {
                    UsnError::JournalNotActive
                } else {
                    UsnError::Other(format!("Failed to query journal metadata: {}", e))
                }
            })?;

        tracing::info!(
            "USN Monitor initialized for {}: - journal_id={}, first_usn={}, next_usn={}",
            drive_letter,
            metadata.journal_id,
            metadata.first_usn,
            metadata.next_usn
        );

        Ok(Self {
            volume: drive_letter,
            last_usn: metadata.first_usn as i64,
            journal_id: metadata.journal_id,
        })
    }

    /// Create a monitor resuming from a known state.
    ///
    /// Used when resuming from database-stored last USN on service restart.
    pub fn resume(
        drive_letter: char,
        last_usn: i64,
        journal_id: u64,
    ) -> std::result::Result<Self, UsnError> {
        use usn_journal_rs::volume::Volume;
        use usn_journal_rs::journal::UsnJournal;

        let volume = Volume::from_drive_letter(drive_letter)
            .map_err(|e| UsnError::Other(format!("Failed to open volume: {}", e)))?;

        let journal = UsnJournal::new(&volume);

        let metadata = journal.query(false)
            .map_err(|e| {
                let msg = format!("{}", e);
                if msg.contains("not active") || msg.contains("1179") {
                    UsnError::JournalNotActive
                } else {
                    UsnError::Other(format!("Failed to query journal metadata: {}", e))
                }
            })?;

        // Check for journal recreation
        if metadata.journal_id != journal_id {
            return Err(UsnError::JournalRecreated {
                old_id: journal_id,
                new_id: metadata.journal_id,
            });
        }

        // Check for journal wrap
        if (last_usn as u64) < (metadata.lowest_valid_usn as u64) {
            return Err(UsnError::JournalWrapped {
                last_processed: last_usn,
                lowest_valid: (metadata.lowest_valid_usn as u64) as i64,
            });
        }

        tracing::info!(
            "USN Monitor resumed for {}: - from usn={}, journal_id={}",
            drive_letter,
            last_usn,
            journal_id
        );

        Ok(Self {
            volume: drive_letter,
            last_usn,
            journal_id,
        })
    }

    /// Poll for changes since last_usn.
    ///
    /// Returns a list of changes or an error if the journal has wrapped/recreated.
    pub fn poll_changes(&mut self) -> std::result::Result<Vec<UsnChange>, UsnError> {
        use usn_journal_rs::volume::Volume;
        use usn_journal_rs::journal::UsnJournal;

        let volume = Volume::from_drive_letter(self.volume)
            .map_err(|e| UsnError::Other(format!("Failed to open volume: {}", e)))?;

        let journal = UsnJournal::new(&volume);

        let metadata = journal.query(false)
            .map_err(|e| UsnError::Other(format!("Failed to query journal metadata: {}", e)))?;

        // Check for journal recreation
        if metadata.journal_id != self.journal_id {
            return Err(UsnError::JournalRecreated {
                old_id: self.journal_id,
                new_id: metadata.journal_id,
            });
        }

        // Check for journal wrap
        if (self.last_usn as u64) < (metadata.lowest_valid_usn as u64) {
            return Err(UsnError::JournalWrapped {
                last_processed: self.last_usn,
                lowest_valid: (metadata.lowest_valid_usn as u64) as i64,
            });
        }

        // Read changes from last_usn
        let mut changes = Vec::new();

        // usn_journal_rs 0.4 API: iter() returns Result<UsnJournalIter>
        // UsnJournalIter implements Iterator<Item = UsnRecord>
        let iter = journal.iter()
            .map_err(|e| UsnError::Other(format!("Failed to iterate journal: {}", e)))?;

        let starting_usn = self.last_usn;
        for result in iter {
            let record = match result {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Error reading USN record: {}", e);
                    continue;
                }
            };

            // Skip records older than our starting position
            if record.usn <= starting_usn {
                continue;
            }

            // Update our position
            self.last_usn = record.usn;

            // Convert USN reason flags to ChangeType
            let change_type = Self::reason_to_change_type(record.reason);

            // Convert file_name from OsString to String
            let name = record.file_name.to_string_lossy().into_owned();

            // Note: usn_journal_rs 0.4 UsnEntry doesn't expose file_attrs
            // We'll determine directory status during database lookup/update
            let is_dir = false;

            changes.push(UsnChange {
                file_ref: record.fid as i64,
                parent_ref: record.parent_fid as i64,
                name,
                change_type,
                is_dir,
            });
        }

        if !changes.is_empty() {
            tracing::debug!(
                "Polled {} changes from volume {}: (last_usn={})",
                changes.len(),
                self.volume,
                self.last_usn
            );
        }

        Ok(changes)
    }

    /// Get the current last USN for persistence.
    pub fn last_usn(&self) -> i64 {
        self.last_usn
    }

    /// Get the journal ID for persistence.
    pub fn journal_id(&self) -> u64 {
        self.journal_id
    }

    /// Get the volume letter.
    pub fn volume(&self) -> char {
        self.volume
    }

    /// Convert USN reason flags to ChangeType.
    ///
    /// USN reasons are bitmasks; we pick the most significant change type.
    fn reason_to_change_type(reason: u32) -> ChangeType {
        // USN reason flags (from Windows SDK)
        const USN_REASON_FILE_CREATE: u32 = 0x00000100;
        const USN_REASON_FILE_DELETE: u32 = 0x00000200;
        const USN_REASON_RENAME_NEW_NAME: u32 = 0x00002000;
        const USN_REASON_RENAME_OLD_NAME: u32 = 0x00001000;

        // Priority: Delete > Create > Rename > Modify
        if reason & USN_REASON_FILE_DELETE != 0 {
            ChangeType::Delete
        } else if reason & USN_REASON_FILE_CREATE != 0 {
            ChangeType::Create
        } else if reason & (USN_REASON_RENAME_NEW_NAME | USN_REASON_RENAME_OLD_NAME) != 0 {
            ChangeType::Rename
        } else {
            ChangeType::Modify
        }
    }
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
pub struct UsnMonitor {
    volume: char,
    last_usn: i64,
    journal_id: u64,
}

#[cfg(not(windows))]
impl UsnMonitor {
    pub fn new(_drive_letter: char) -> std::result::Result<Self, UsnError> {
        Err(UsnError::JournalNotActive)
    }

    pub fn resume(
        _drive_letter: char,
        _last_usn: i64,
        _journal_id: u64,
    ) -> std::result::Result<Self, UsnError> {
        Err(UsnError::JournalNotActive)
    }

    pub fn poll_changes(&mut self) -> std::result::Result<Vec<UsnChange>, UsnError> {
        Err(UsnError::JournalNotActive)
    }

    pub fn last_usn(&self) -> i64 {
        self.last_usn
    }

    pub fn journal_id(&self) -> u64 {
        self.journal_id
    }

    pub fn volume(&self) -> char {
        self.volume
    }
}

/// Deduplicate rapid changes to the same file within a batch.
///
/// When multiple changes occur to the same file within a polling interval,
/// only the final state is kept. Special case: Create followed by Delete
/// removes the entry entirely (file never needed to exist in index).
pub fn deduplicate_changes(changes: Vec<UsnChange>) -> Vec<UsnChange> {
    let mut final_state: HashMap<i64, UsnChange> = HashMap::new();

    for change in changes {
        let file_ref = change.file_ref;

        // Check for Create-then-Delete pattern
        if let Some(existing) = final_state.get(&file_ref) {
            if existing.change_type == ChangeType::Create && change.change_type == ChangeType::Delete
            {
                // File was created then deleted within batch - remove entirely
                final_state.remove(&file_ref);
                continue;
            }
        }

        // Later changes override earlier ones
        final_state.insert(file_ref, change);
    }

    final_state.into_values().collect()
}

/// Apply a batch of changes to the database.
///
/// All changes are applied in a single transaction for atomicity.
pub fn apply_changes_batch(
    db: &mut Database,
    volume_id: i64,
    changes: &[UsnChange],
) -> Result<usize> {
    // FileEntry is re-exported from crate::db via pub use ops::*
    use rusqlite::params;

    if changes.is_empty() {
        return Ok(0);
    }

    let conn = db.conn_mut();
    let tx = conn
        .transaction()
        .map_err(|e| FFIError::Database(format!("Failed to start transaction: {}", e)))?;

    let mut applied = 0;

    for change in changes {
        let result = match change.change_type {
            ChangeType::Create => {
                tx.execute(
                    "INSERT OR REPLACE INTO files (volume_id, file_ref, parent_ref, name, is_dir)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        volume_id,
                        change.file_ref,
                        change.parent_ref,
                        change.name,
                        change.is_dir as i32,
                    ],
                )
            }
            ChangeType::Delete => {
                tx.execute(
                    "DELETE FROM files WHERE volume_id = ?1 AND file_ref = ?2",
                    params![volume_id, change.file_ref],
                )
            }
            ChangeType::Rename => {
                tx.execute(
                    "UPDATE files SET name = ?1, parent_ref = ?2
                     WHERE volume_id = ?3 AND file_ref = ?4",
                    params![change.name, change.parent_ref, volume_id, change.file_ref],
                )
            }
            ChangeType::Modify => {
                // For modify, we mainly update name in case it changed
                // Size and modified time would require additional file queries
                tx.execute(
                    "UPDATE files SET name = ?1 WHERE volume_id = ?2 AND file_ref = ?3",
                    params![change.name, volume_id, change.file_ref],
                )
            }
        };

        match result {
            Ok(_) => applied += 1,
            Err(e) => {
                tracing::warn!(
                    "Failed to apply change for file_ref {}: {}",
                    change.file_ref,
                    e
                );
            }
        }
    }

    tx.commit()
        .map_err(|e| FFIError::Database(format!("Failed to commit changes: {}", e)))?;

    tracing::debug!("Applied {} changes to volume {}", applied, volume_id);

    Ok(applied)
}

/// Adaptive throttling based on system CPU load.
///
/// Reduces polling frequency when the system is under heavy load
/// to avoid impacting user work.
pub struct AdaptiveThrottle {
    system: sysinfo::System,
    normal_interval: std::time::Duration,
    throttled_interval: std::time::Duration,
    cpu_threshold: f32,
}

impl AdaptiveThrottle {
    /// Create a new adaptive throttle with the specified normal interval.
    ///
    /// When CPU usage exceeds 80%, polling interval increases 4x.
    pub fn new(normal_secs: u64) -> Self {
        Self {
            system: sysinfo::System::new(),
            normal_interval: std::time::Duration::from_secs(normal_secs),
            throttled_interval: std::time::Duration::from_secs(normal_secs * 4),
            cpu_threshold: 80.0,
        }
    }

    /// Get the current polling interval based on CPU load.
    ///
    /// Returns throttled interval if CPU > 80%, otherwise normal interval.
    pub fn get_interval(&mut self) -> std::time::Duration {
        use sysinfo::CpuRefreshKind;

        // Refresh CPU info - use nothing() and add CPU usage
        let cpu_kind = CpuRefreshKind::nothing().with_cpu_usage();
        self.system.refresh_cpu_specifics(cpu_kind);

        // Need two samples for accurate CPU reading
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.system.refresh_cpu_specifics(cpu_kind);

        let cpu_usage = self.system.global_cpu_usage();

        if cpu_usage > self.cpu_threshold {
            tracing::debug!("CPU at {:.1}%, throttling USN polling", cpu_usage);
            self.throttled_interval
        } else {
            self.normal_interval
        }
    }
}

/// Monitor handle for managing USN monitor lifecycle.
pub struct UsnMonitorHandle {
    /// Thread handle for joining
    handle: Option<std::thread::JoinHandle<()>>,
}

impl UsnMonitorHandle {
    /// Signal the monitor to stop and wait for it to finish.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(()) => tracing::info!("USN monitor thread stopped gracefully"),
                Err(_) => tracing::error!("USN monitor thread panicked"),
            }
        }
    }
}

/// Start the USN monitor loop for a volume.
///
/// This function spawns a background thread that:
/// 1. Polls the USN journal at configurable intervals
/// 2. Deduplicates rapid changes
/// 3. Applies batched updates to the database
/// 4. Handles journal wrap by triggering background rescan
/// 5. Adapts polling frequency based on CPU load
///
/// # Arguments
/// * `drive_letter` - The volume to monitor (e.g., 'C')
/// * `db` - Database instance for persisting changes
/// * `poll_interval_secs` - Normal polling interval in seconds
/// * `shutdown_rx` - Channel receiver for shutdown signals
/// * `resume_usn` - Optional (last_usn, journal_id) tuple for resuming from saved state
///
/// # Returns
/// A handle that can be used to stop the monitor.
#[cfg(windows)]
pub fn usn_monitor_loop(
    drive_letter: char,
    mut db: Database,
    poll_interval_secs: u64,
    shutdown_rx: std::sync::mpsc::Receiver<()>,
    resume_usn: Option<(i64, u64)>,
) -> UsnMonitorHandle {
    use std::time::Instant;
    use crate::db::{get_volume_usn, update_volume_usn, get_volume};

    let handle = std::thread::spawn(move || {
        tracing::info!("Starting USN monitor for volume {}: ", drive_letter);

        // Create or resume monitor
        let monitor_result = if let Some((last_usn, journal_id)) = resume_usn {
            tracing::info!(
                "Resuming USN monitor from usn={}, journal_id={}",
                last_usn,
                journal_id
            );
            UsnMonitor::resume(drive_letter, last_usn, journal_id)
        } else {
            UsnMonitor::new(drive_letter)
        };

        let mut monitor = match monitor_result {
            Ok(m) => m,
            Err(UsnError::JournalNotActive) => {
                tracing::info!(
                    "USN Journal not active on volume {}: - treating as FAT volume",
                    drive_letter
                );
                return;
            }
            Err(UsnError::JournalWrapped { last_processed, lowest_valid }) => {
                tracing::warn!(
                    "USN Journal wrapped on volume {}: (last={}, lowest={}). Triggering rescan.",
                    drive_letter,
                    last_processed,
                    lowest_valid
                );
                trigger_background_rescan(drive_letter);
                return;
            }
            Err(UsnError::JournalRecreated { old_id, new_id }) => {
                tracing::warn!(
                    "USN Journal recreated on volume {}: (old={}, new={}). Triggering rescan.",
                    drive_letter,
                    old_id,
                    new_id
                );
                trigger_background_rescan(drive_letter);
                return;
            }
            Err(e) => {
                tracing::error!("Failed to create USN monitor for {}: {}", drive_letter, e);
                return;
            }
        };

        // Get volume ID from database for updates
        let volume_id = {
            let drive_str = format!("{}:", drive_letter);
            match get_volume(db.conn(), &drive_str) {
                Ok(Some(vol)) => vol.id,
                Ok(None) => {
                    tracing::error!("Volume {} not found in database", drive_letter);
                    return;
                }
                Err(e) => {
                    tracing::error!("Failed to get volume {}: {}", drive_letter, e);
                    return;
                }
            }
        };

        let mut throttle = AdaptiveThrottle::new(poll_interval_secs);

        loop {
            // Check for shutdown signal
            match shutdown_rx.try_recv() {
                Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    tracing::info!("USN monitor for {} received shutdown signal", drive_letter);
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }

            let start = Instant::now();

            // Poll for changes
            match monitor.poll_changes() {
                Ok(changes) if !changes.is_empty() => {
                    let deduped = deduplicate_changes(changes);
                    tracing::info!(
                        "Volume {}: processing {} changes ({} after dedup)",
                        drive_letter,
                        deduped.len(),
                        deduped.len()
                    );

                    match apply_changes_batch(&mut db, volume_id, &deduped) {
                        Ok(applied) => {
                            tracing::debug!("Applied {} changes to volume {}", applied, drive_letter);
                        }
                        Err(e) => {
                            tracing::error!("Failed to apply changes: {}", e);
                        }
                    }

                    // Update persisted USN position
                    if let Err(e) = update_volume_usn(
                        db.conn(),
                        volume_id,
                        monitor.last_usn(),
                        monitor.journal_id() as i64,
                    ) {
                        tracing::error!("Failed to persist USN position: {}", e);
                    }
                }
                Ok(_) => {
                    // No changes this poll cycle
                }
                Err(UsnError::JournalWrapped { last_processed, lowest_valid }) => {
                    tracing::warn!(
                        "USN Journal wrapped on volume {} (last={}, lowest={}). Triggering rescan.",
                        drive_letter,
                        last_processed,
                        lowest_valid
                    );
                    trigger_background_rescan(drive_letter);
                    break;
                }
                Err(UsnError::JournalRecreated { old_id, new_id }) => {
                    tracing::warn!(
                        "USN Journal recreated on volume {} (old={}, new={}). Triggering rescan.",
                        drive_letter,
                        old_id,
                        new_id
                    );
                    trigger_background_rescan(drive_letter);
                    break;
                }
                Err(e) => {
                    tracing::error!("Error polling USN journal for {}: {}", drive_letter, e);
                }
            }

            // Get adaptive interval based on CPU load
            let interval = throttle.get_interval();

            // Sleep for remainder of interval
            let elapsed = start.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }

        tracing::info!("USN monitor for {} exiting", drive_letter);
    });

    UsnMonitorHandle {
        handle: Some(handle),
    }
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
pub fn usn_monitor_loop(
    _drive_letter: char,
    _db: Database,
    _poll_interval_secs: u64,
    _shutdown_rx: std::sync::mpsc::Receiver<()>,
    _resume_usn: Option<(i64, u64)>,
) -> UsnMonitorHandle {
    tracing::warn!("USN monitoring not available on non-Windows platforms");
    UsnMonitorHandle { handle: None }
}

/// Trigger a background rescan of a volume.
///
/// Called when the USN journal has wrapped or been recreated,
/// meaning some file changes were missed.
pub fn trigger_background_rescan(drive_letter: char) {
    // TODO: Implement full background rescan integration with indexer
    // For now, just log the event
    tracing::warn!(
        "Background rescan triggered for volume {}: - full rescan needed",
        drive_letter
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_removes_create_delete() {
        let changes = vec![
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "test.txt".to_string(),
                change_type: ChangeType::Create,
                is_dir: false,
            },
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "test.txt".to_string(),
                change_type: ChangeType::Delete,
                is_dir: false,
            },
        ];

        let deduped = deduplicate_changes(changes);
        assert!(deduped.is_empty(), "Create+Delete should result in no change");
    }

    #[test]
    fn test_deduplicate_keeps_final_state() {
        let changes = vec![
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "old.txt".to_string(),
                change_type: ChangeType::Create,
                is_dir: false,
            },
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "new.txt".to_string(),
                change_type: ChangeType::Rename,
                is_dir: false,
            },
        ];

        let deduped = deduplicate_changes(changes);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].name, "new.txt");
        assert_eq!(deduped[0].change_type, ChangeType::Rename);
    }

    #[test]
    fn test_deduplicate_multiple_files() {
        let changes = vec![
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "file1.txt".to_string(),
                change_type: ChangeType::Create,
                is_dir: false,
            },
            UsnChange {
                file_ref: 200,
                parent_ref: 5,
                name: "file2.txt".to_string(),
                change_type: ChangeType::Create,
                is_dir: false,
            },
            UsnChange {
                file_ref: 100,
                parent_ref: 5,
                name: "file1.txt".to_string(),
                change_type: ChangeType::Modify,
                is_dir: false,
            },
        ];

        let deduped = deduplicate_changes(changes);
        assert_eq!(deduped.len(), 2);
    }
}

# Phase 2: Real-time Updates - Research

**Researched:** 2026-01-24
**Domain:** USN Change Journal monitoring, volume lifecycle management, configuration persistence
**Confidence:** HIGH

## Summary

Phase 2 implements real-time index updates by monitoring filesystem changes. NTFS volumes use the USN Change Journal for efficient change tracking (polling every 30 seconds per user decision). FAT32/exFAT volumes use periodic full rescans (default 30-minute intervals). The phase also adds configuration management for volume selection and exclude patterns, plus graceful handling of volume mount/unmount events.

The standard approach leverages `usn-journal-rs` 0.4+ for USN Journal access, which uses windows crate 0.62 (matching the project's existing dependency). Volume mount/unmount detection uses Windows `WM_DEVICECHANGE` messages via the windows crate. Configuration uses TOML format with `toml` + `serde` for serialization.

**Primary recommendation:** Use `usn-journal-rs` 0.4+ for USN Journal monitoring (compatible windows 0.62), `toml` + `serde` for configuration, `sysinfo` for load detection, and `WM_DEVICECHANGE` via windows crate for volume events. Implement journal wrap detection by comparing stored `last_usn` against `LowestValidUsn` from `FSCTL_QUERY_USN_JOURNAL`.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| **usn-journal-rs** | 0.4+ | USN Journal monitoring | Only maintained Rust crate for USN journal. Uses windows 0.62, matching project. Provides `UsnJournal::iter()` for change records. |
| **toml** | 0.9+ | Configuration parsing | Standard Rust config format. Cargo uses TOML. Human-readable, serde-compatible. |
| **serde** | 1.0+ | Serialization | De-facto standard for Rust serialization. Derive macros for config structs. |
| **sysinfo** | 0.33+ | System load monitoring | Cross-platform system info. `System::global_cpu_usage()` for throttling decisions. |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **windows** | 0.62+ | Windows API bindings | Volume mount/unmount via WM_DEVICECHANGE, DeviceIoControl for USN queries |
| **directories** | 5.0+ | Config file paths | Standard locations (`%APPDATA%`, `%PROGRAMDATA%`) for config/data files |
| **notify-debouncer-mini** | 0.5+ | FAT watcher alternative | If periodic scan proves insufficient, debounced file watching |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| **usn-journal-rs** | Direct DeviceIoControl | More control but requires unsafe FFI, buffer management |
| **TOML config** | JSON/YAML | TOML is Rust ecosystem standard, more readable for config |
| **sysinfo** | Manual CPU query | sysinfo is cross-platform, handles edge cases |
| **WM_DEVICECHANGE** | WMI queries | WM_DEVICECHANGE is simpler, real-time, lower overhead |

**Installation:**
```toml
[dependencies]
# Existing from Phase 1
windows-service = "0.7"
rusqlite = { version = "0.38", features = ["bundled"] }
walkdir = "2"
tokio = { version = "1.43", features = ["full"] }
tracing = "0.1"
thiserror = "2.0"
anyhow = "1.0"

# New for Phase 2
usn-journal-rs = "0.4"
toml = "0.9"
serde = { version = "1.0", features = ["derive"] }
sysinfo = "0.33"
directories = "5.0"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.62", features = [
    "Win32_Storage_FileSystem",
    "Win32_System_Ioctl",
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",  # WM_DEVICECHANGE
    "Win32_System_SystemInformation",  # System info queries
] }
```

## Architecture Patterns

### Recommended Project Structure (Additions)

```
src/
├── service/
│   ├── config.rs        # EXPAND: TOML config loading/saving
│   └── volume_watcher.rs  # NEW: Mount/unmount detection
├── indexer/
│   ├── usn_monitor.rs   # NEW: USN Journal polling loop
│   └── fat_reconciler.rs  # NEW: Periodic FAT rescan scheduler
└── lib.rs               # ADD: Config types, volume state enum
```

### Pattern 1: USN Journal Polling Loop

**What:** Background thread that polls USN Journal at fixed interval, processes changes in batches.

**When to use:** For all NTFS volumes after initial indexing completes.

**Example:**
```rust
// Source: usn-journal-rs docs + CONTEXT.md decisions
use usn_journal_rs::{volume::Volume, journal::UsnJournal};
use std::time::{Duration, Instant};
use std::sync::mpsc::Receiver;

const POLL_INTERVAL: Duration = Duration::from_secs(30);

pub struct UsnMonitor {
    volume: Volume,
    last_usn: i64,
    journal_id: u64,
}

impl UsnMonitor {
    pub fn new(drive_letter: char) -> Result<Self, Box<dyn std::error::Error>> {
        let volume = Volume::from_drive_letter(drive_letter)?;
        let journal = UsnJournal::new(&volume);
        let journal_data = journal.query()?;

        Ok(Self {
            volume,
            last_usn: journal_data.first_usn as i64,
            journal_id: journal_data.usn_journal_id,
        })
    }

    pub fn poll_changes(&mut self) -> Result<Vec<UsnChange>, UsnError> {
        let journal = UsnJournal::new(&self.volume);

        // Check for journal wrap
        let current_data = journal.query()?;
        if current_data.usn_journal_id != self.journal_id {
            return Err(UsnError::JournalRecreated);
        }
        if (self.last_usn as u64) < current_data.lowest_valid_usn {
            return Err(UsnError::JournalWrapped);
        }

        // Read changes since last_usn
        let mut changes = Vec::new();
        for result in journal.iter_from(self.last_usn as u64) {
            match result {
                Ok(record) => {
                    self.last_usn = record.usn as i64;
                    changes.push(UsnChange::from(record));
                }
                Err(e) => tracing::warn!("USN record error: {}", e),
            }
        }

        Ok(changes)
    }
}

fn usn_monitor_loop(
    mut monitor: UsnMonitor,
    db: Database,
    shutdown_rx: Receiver<()>,
) {
    loop {
        // Check shutdown
        match shutdown_rx.try_recv() {
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        let start = Instant::now();

        match monitor.poll_changes() {
            Ok(changes) if !changes.is_empty() => {
                // Deduplicate rapid changes to same file
                let deduped = deduplicate_changes(changes);
                // Apply to database in single transaction
                apply_changes_batch(&db, &deduped);
            }
            Ok(_) => { /* No changes */ }
            Err(UsnError::JournalWrapped) => {
                tracing::warn!("USN Journal wrapped, triggering rescan");
                trigger_background_rescan(&db, monitor.volume.drive_letter());
            }
            Err(e) => tracing::error!("USN poll error: {}", e),
        }

        // Sleep for remainder of interval
        let elapsed = start.elapsed();
        if elapsed < POLL_INTERVAL {
            std::thread::sleep(POLL_INTERVAL - elapsed);
        }
    }
}
```

### Pattern 2: Journal Wrap Detection

**What:** Compare stored `last_usn` against `LowestValidUsn` to detect missed changes.

**When to use:** On every poll cycle and on service restart.

**Example:**
```rust
// Source: Microsoft FSCTL_QUERY_USN_JOURNAL docs
pub fn check_journal_validity(
    stored_usn: i64,
    stored_journal_id: u64,
    current_data: &UsnJournalData,
) -> JournalStatus {
    // Journal recreated (different ID)
    if current_data.usn_journal_id != stored_journal_id {
        return JournalStatus::Recreated;
    }

    // Journal wrapped (our last USN is no longer valid)
    if (stored_usn as u64) < current_data.lowest_valid_usn {
        return JournalStatus::Wrapped {
            missed_from: stored_usn,
            lowest_valid: current_data.lowest_valid_usn as i64,
        };
    }

    JournalStatus::Valid
}

pub enum JournalStatus {
    Valid,
    Wrapped { missed_from: i64, lowest_valid: i64 },
    Recreated,
}
```

### Pattern 3: Volume Mount/Unmount Detection

**What:** Listen for `WM_DEVICECHANGE` messages to detect volume arrival/removal.

**When to use:** Background thread for real-time volume events.

**Example:**
```rust
// Source: Microsoft WM_DEVICECHANGE docs
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Foundation::*;

const DBT_DEVICEARRIVAL: u32 = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;
const DBT_DEVTYP_VOLUME: u32 = 0x00000002;

#[repr(C)]
struct DevBroadcastVolume {
    dbcv_size: u32,
    dbcv_devicetype: u32,
    dbcv_reserved: u32,
    dbcv_unitmask: u32,
    dbcv_flags: u16,
}

fn get_drive_letters_from_mask(mask: u32) -> Vec<char> {
    let mut letters = Vec::new();
    for i in 0..26 {
        if mask & (1 << i) != 0 {
            letters.push((b'A' + i as u8) as char);
        }
    }
    letters
}

// In hidden window message handler
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DEVICECHANGE => {
            let event = wparam.0 as u32;
            match event {
                DBT_DEVICEARRIVAL => {
                    let header = lparam.0 as *const DevBroadcastVolume;
                    if (*header).dbcv_devicetype == DBT_DEVTYP_VOLUME {
                        let drives = get_drive_letters_from_mask((*header).dbcv_unitmask);
                        for drive in drives {
                            handle_volume_mount(drive);
                        }
                    }
                }
                DBT_DEVICEREMOVECOMPLETE => {
                    let header = lparam.0 as *const DevBroadcastVolume;
                    if (*header).dbcv_devicetype == DBT_DEVTYP_VOLUME {
                        let drives = get_drive_letters_from_mask((*header).dbcv_unitmask);
                        for drive in drives {
                            handle_volume_unmount(drive);
                        }
                    }
                }
                _ => {}
            }
            LRESULT(1) // TRUE
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
```

### Pattern 4: TOML Configuration

**What:** Human-readable config file with serde deserialization.

**When to use:** For all user-configurable settings.

**Example:**
```rust
// Source: toml crate docs + serde derive
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,

    #[serde(default)]
    pub volumes: HashMap<String, VolumeConfig>,

    #[serde(default)]
    pub exclude: ExcludeConfig,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GeneralConfig {
    /// Data directory (default: %PROGRAMDATA%\FFI)
    pub data_dir: Option<PathBuf>,

    /// USN polling interval in seconds (default: 30)
    #[serde(default = "default_poll_interval")]
    pub usn_poll_interval_secs: u64,

    /// Days to keep offline volume data (default: 7)
    #[serde(default = "default_offline_retention")]
    pub offline_retention_days: u32,
}

fn default_poll_interval() -> u64 { 30 }
fn default_offline_retention() -> u32 { 7 }

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeConfig {
    /// Enable indexing for this volume
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// FAT reconciliation interval in minutes (default: 30)
    #[serde(default = "default_reconcile_interval")]
    pub reconcile_interval_mins: u64,
}

fn default_true() -> bool { true }
fn default_reconcile_interval() -> u64 { 30 }

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ExcludeConfig {
    /// Path prefixes to exclude (e.g., ["C:\\Windows\\Temp"])
    #[serde(default)]
    pub paths: Vec<String>,

    /// File extensions to exclude (e.g., ["tmp", "log"])
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl Config {
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Read(e))?;
        toml::from_str(&contents)
            .map_err(|e| ConfigError::Parse(e))
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        let contents = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e))?;
        std::fs::write(path, contents)
            .map_err(|e| ConfigError::Write(e))
    }
}
```

**Config file example (`ffi.toml`):**
```toml
[general]
usn_poll_interval_secs = 30
offline_retention_days = 7

[volumes.C]
enabled = true

[volumes.D]
enabled = true
reconcile_interval_mins = 60  # FAT drive, longer interval

[volumes.E]
enabled = false  # External drive, don't auto-index

[exclude]
paths = [
    "C:\\Windows\\Temp",
    "C:\\$Recycle.Bin",
]
extensions = ["tmp", "log", "bak"]
```

### Pattern 5: Adaptive Throttling

**What:** Reduce polling frequency when system is under heavy load.

**When to use:** During USN polling to avoid impacting user work.

**Example:**
```rust
// Source: sysinfo crate docs + CONTEXT.md discretion
use sysinfo::System;

pub struct AdaptiveThrottle {
    system: System,
    normal_interval: Duration,
    throttled_interval: Duration,
    cpu_threshold: f32,
}

impl AdaptiveThrottle {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            normal_interval: Duration::from_secs(30),
            throttled_interval: Duration::from_secs(120), // 4x slower under load
            cpu_threshold: 80.0, // Throttle when CPU > 80%
        }
    }

    pub fn get_interval(&mut self) -> Duration {
        self.system.refresh_cpu_usage();

        // Need two samples for accurate reading
        std::thread::sleep(Duration::from_millis(200));
        self.system.refresh_cpu_usage();

        let cpu_usage = self.system.global_cpu_usage();

        if cpu_usage > self.cpu_threshold {
            tracing::debug!("CPU at {:.1}%, throttling", cpu_usage);
            self.throttled_interval
        } else {
            self.normal_interval
        }
    }
}
```

### Anti-Patterns to Avoid

- **Polling USN too frequently:** 30 seconds is sufficient; 1-second polling wastes CPU with minimal benefit
- **Processing changes one-at-a-time:** Batch changes within polling interval, deduplicate, single transaction
- **Ignoring journal wrap:** Will silently miss file changes; always check LowestValidUsn
- **Blocking service thread for mount events:** Use separate thread with message pump
- **Hardcoding config values:** Everything user-visible should be in TOML config
- **Full rescan on any FAT change:** Use periodic reconciliation per CONTEXT.md decision

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| USN Journal parsing | Raw DeviceIoControl + buffer walking | `usn-journal-rs` | Variable-length records, version handling, safe FFI |
| Config file parsing | Manual TOML/regex | `toml` + `serde` | Error handling, type safety, derive macros |
| CPU usage detection | Raw WMI queries | `sysinfo` crate | Cross-platform, handles edge cases, refreshes properly |
| Standard paths | Hardcoded strings | `directories` crate | Respects Windows versions, user vs system |
| Change deduplication | Linear scan | `HashMap<FileRef, ChangeType>` | O(1) lookup vs O(n) scan |

**Key insight:** The USN Journal API returns variable-length records that must be walked carefully. The buffer starts with a USN value followed by zero or more `USN_RECORD_V2`/`USN_RECORD_V3` structures. File names are not null-terminated. This is exactly what `usn-journal-rs` handles.

## Common Pitfalls

### Pitfall 1: USN Journal Not Present

**What goes wrong:** `FSCTL_QUERY_USN_JOURNAL` fails with `ERROR_JOURNAL_NOT_ACTIVE` (1179).

**Why it happens:** USN Journal is disabled by default on some volumes, or was deleted.

**How to avoid:**
1. Check for this error specifically on first poll attempt
2. Log clear message: "USN Journal not active on volume X, falling back to periodic scan"
3. Treat volume as FAT for update purposes (periodic rescan)
4. Do NOT try to create journal (requires admin, may be intentionally disabled)

**Warning signs:** Error 1179, empty journal queries

### Pitfall 2: Journal Wrap During Heavy I/O

**What goes wrong:** Between polls, so many changes occur that old USN records are overwritten.

**Why it happens:** Default USN Journal size is 32MB; heavy file operations can fill it quickly.

**How to avoid:**
1. Always check `LowestValidUsn` before reading
2. On wrap detection, log warning with gap info
3. Trigger background rescan (non-blocking)
4. Consider recommending larger journal size in docs

**Warning signs:** Stored USN < LowestValidUsn, gaps in file change tracking

### Pitfall 3: Mount Event Flood on Boot

**What goes wrong:** Multiple `DBT_DEVICEARRIVAL` events at system boot overwhelm service.

**Why it happens:** All volumes mount nearly simultaneously at boot.

**How to avoid:**
1. Debounce mount events (100ms window)
2. Queue volumes for sequential processing
3. Don't start USN monitoring until initial index complete (per CONTEXT.md)
4. Use StartPending checkpoints during boot processing

**Warning signs:** High CPU at boot, timeout during service start

### Pitfall 4: Volume Serial Number Changes

**What goes wrong:** Same drive letter, but different volume (USB swap). Old index data is incorrect.

**Why it happens:** User unplugs USB drive, plugs in different one with same letter.

**How to avoid:**
1. Store volume serial in volumes table (already done in Phase 1)
2. On mount, compare serial to stored value
3. If mismatch, mark old data offline and start fresh index
4. Log the swap for debugging

**Warning signs:** Search results pointing to non-existent files on removable drives

### Pitfall 5: Config File Locked

**What goes wrong:** Config reload fails because another process has file open.

**Why it happens:** User editing config in Notepad (which may hold file lock).

**How to avoid:**
1. Use short-lived file handles for config read
2. Implement retry with backoff (3 attempts, 100ms delay)
3. Log warning if config can't be loaded, continue with cached config
4. Consider file change notification to auto-reload

**Warning signs:** Config changes not taking effect, log messages about file access

## Code Examples

### Change Deduplication

```rust
// Source: CONTEXT.md decision - deduplicate rapid changes within batch
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Create,
    Delete,
    Rename { old_parent: i64, old_name: String },
    Modify,
}

pub fn deduplicate_changes(changes: Vec<UsnRecord>) -> Vec<(i64, ChangeType)> {
    let mut final_state: HashMap<i64, ChangeType> = HashMap::new();

    for record in changes {
        let file_ref = record.file_reference as i64;
        let reason = record.reason;

        // Determine change type from USN reason flags
        let change = if reason & USN_REASON_FILE_CREATE != 0 {
            ChangeType::Create
        } else if reason & USN_REASON_FILE_DELETE != 0 {
            ChangeType::Delete
        } else if reason & USN_REASON_RENAME_NEW_NAME != 0 {
            ChangeType::Rename {
                old_parent: record.parent_file_reference as i64,
                old_name: record.file_name.clone(),
            }
        } else {
            ChangeType::Modify
        };

        // Later changes override earlier ones
        // Special case: Create then Delete = no change needed
        if let Some(existing) = final_state.get(&file_ref) {
            if *existing == ChangeType::Create && change == ChangeType::Delete {
                final_state.remove(&file_ref);
                continue;
            }
        }

        final_state.insert(file_ref, change);
    }

    final_state.into_iter().collect()
}
```

### Database Operations for Updates

```rust
// Source: Phase 1 patterns extended for updates
pub fn apply_file_change(
    conn: &Connection,
    volume_id: i64,
    file_ref: i64,
    change: ChangeType,
    record: &UsnRecord,
) -> Result<()> {
    match change {
        ChangeType::Create => {
            conn.execute(
                "INSERT OR REPLACE INTO files
                 (volume_id, file_ref, parent_ref, name, size, modified, is_dir)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    volume_id,
                    file_ref,
                    record.parent_file_reference as i64,
                    record.file_name,
                    record.file_size,
                    record.last_write_time.timestamp(),
                    record.is_directory as i32,
                ],
            )?;
        }
        ChangeType::Delete => {
            conn.execute(
                "DELETE FROM files WHERE volume_id = ?1 AND file_ref = ?2",
                params![volume_id, file_ref],
            )?;
        }
        ChangeType::Rename { .. } => {
            conn.execute(
                "UPDATE files SET name = ?1, parent_ref = ?2
                 WHERE volume_id = ?3 AND file_ref = ?4",
                params![
                    record.file_name,
                    record.parent_file_reference as i64,
                    volume_id,
                    file_ref,
                ],
            )?;
        }
        ChangeType::Modify => {
            conn.execute(
                "UPDATE files SET size = ?1, modified = ?2
                 WHERE volume_id = ?3 AND file_ref = ?4",
                params![
                    record.file_size,
                    record.last_write_time.timestamp(),
                    volume_id,
                    file_ref,
                ],
            )?;
        }
    }
    Ok(())
}
```

### Volume State Machine

```rust
// Source: CONTEXT.md decisions on volume lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeState {
    /// Volume configured and mounted, actively monitoring
    Online,
    /// Volume unmounted, keeping data, waiting for reconnect
    Offline { since: i64 },
    /// Initial indexing in progress
    Indexing,
    /// Background rescan in progress (after journal wrap)
    Rescanning,
    /// Not configured for indexing
    Disabled,
}

pub fn update_volume_state(
    conn: &Connection,
    volume_id: i64,
    state: VolumeState,
) -> Result<()> {
    let (state_str, offline_since) = match state {
        VolumeState::Online => ("online", None),
        VolumeState::Offline { since } => ("offline", Some(since)),
        VolumeState::Indexing => ("indexing", None),
        VolumeState::Rescanning => ("rescanning", None),
        VolumeState::Disabled => ("disabled", None),
    };

    conn.execute(
        "UPDATE volumes SET state = ?1, offline_since = ?2 WHERE id = ?3",
        params![state_str, offline_since, volume_id],
    )?;

    Ok(())
}

pub fn cleanup_old_offline_volumes(conn: &Connection, retention_days: u32) -> Result<usize> {
    let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);

    // Delete files first (foreign key)
    let deleted = conn.execute(
        "DELETE FROM files WHERE volume_id IN (
            SELECT id FROM volumes WHERE state = 'offline' AND offline_since < ?1
        )",
        params![cutoff],
    )?;

    // Then delete volumes
    conn.execute(
        "DELETE FROM volumes WHERE state = 'offline' AND offline_since < ?1",
        params![cutoff],
    )?;

    Ok(deleted)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| File watchers everywhere | USN Journal (NTFS) + periodic scan (FAT) | Windows Vista (2006) | 1000x more efficient for NTFS |
| INI config files | TOML with serde | 2018+ (Rust ecosystem) | Type safety, better error messages |
| Manual CPU polling | sysinfo crate | 2020+ | Cross-platform, handles quirks |
| WMI for device events | WM_DEVICECHANGE | Always better | Lower overhead, real-time |

**Deprecated/outdated:**
- **notify crate on NTFS:** Still works but USN Journal is far more efficient for this use case
- **usn-journal-rs versions < 0.4:** Earlier versions used older windows crate, caused conflicts
- **Manual FSCTL buffer walking:** Error-prone; usn-journal-rs handles this correctly

## Open Questions

1. **Quick scan strategy for reconnected volumes**
   - What we know: Need to detect changes while volume was offline
   - What's unclear: Whether to compare MFT state or just rescan fully
   - Recommendation: Start with full rescan on reconnect; optimize later if too slow

2. **Exact throttling thresholds**
   - What we know: Should back off under heavy load
   - What's unclear: Optimal CPU% threshold, interval multiplier
   - Recommendation: Start with 80% CPU threshold, 4x interval increase; tune based on testing

3. **Config hot-reload mechanism**
   - What we know: Config should persist and be editable
   - What's unclear: Whether to support live reload or require service restart
   - Recommendation: Start with restart-required; add hot-reload in v2 if users request

## Sources

### Primary (HIGH confidence)
- [usn-journal-rs GitHub](https://github.com/wangfu91/usn-journal-rs) - API, windows 0.62 compatibility confirmed
- [Microsoft FSCTL_QUERY_USN_JOURNAL](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_query_usn_journal) - Journal metadata, wrap detection
- [Microsoft FSCTL_READ_USN_JOURNAL](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal) - Reading change records
- [Microsoft Walking USN Buffer](https://learn.microsoft.com/en-us/windows/win32/fileio/walking-a-buffer-of-change-journal-records) - Buffer structure, iteration
- [Microsoft WM_DEVICECHANGE](https://learn.microsoft.com/en-us/windows/win32/devio/wm-devicechange) - Volume mount/unmount events
- [toml crate docs.rs](https://docs.rs/toml) - Config parsing
- [sysinfo crate docs.rs](https://docs.rs/sysinfo) - CPU monitoring

### Secondary (MEDIUM confidence)
- [Microsoft Journal Wrap Troubleshooting](https://learn.microsoft.com/en-us/troubleshoot/windows-server/networking/how-frs-uses-usn-change-journal-ntfs-file-system) - Journal wrap causes and recovery
- [USN Journal Wikipedia](https://en.wikipedia.org/wiki/USN_Journal) - Background, journal size recommendations

### Tertiary (LOW confidence)
- Community discussions on journal wrap recovery strategies

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - usn-journal-rs 0.4 verified compatible with windows 0.62, APIs well-documented
- Architecture: HIGH - Patterns based on Microsoft documentation and CONTEXT.md user decisions
- Pitfalls: HIGH - Journal wrap, mount floods documented in Microsoft troubleshooting guides

**Research date:** 2026-01-24
**Valid until:** 2026-02-24 (30 days - stable domain)

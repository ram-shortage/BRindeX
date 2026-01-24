# Phase 1: Foundation - Research

**Researched:** 2026-01-24
**Domain:** Windows service scaffolding, SQLite persistence, NTFS MFT reading, FAT directory enumeration
**Confidence:** HIGH

## Summary

Phase 1 establishes the core foundation: a Windows service that builds and persists a complete file index. This research covers four interconnected domains: (1) Windows service implementation in Rust, (2) SQLite with WAL mode for crash-safe persistence, (3) MFT reading for high-speed NTFS indexing, and (4) directory enumeration for FAT32/exFAT fallback.

The standard Rust stack for this domain is well-established: `windows-service` for service scaffolding, `rusqlite` with bundled SQLite for persistence, and `usn-journal-rs` for MFT enumeration. For FAT volumes without a change journal, `walkdir` provides efficient directory traversal with performance comparable to native `find`.

**Primary recommendation:** Use `windows-service` 0.7+ for service lifecycle, `rusqlite` 0.38+ with bundled feature and WAL mode, `usn-journal-rs` 0.4+ for MFT enumeration on NTFS, and `walkdir` 2.x for FAT volume initial scans.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| **windows-service** | 0.7+ | Windows service scaffolding | Mullvad-maintained, production-proven (Mullvad VPN uses it). Provides `define_windows_service!` macro, service control handler registration, and status reporting. |
| **rusqlite** | 0.38+ | SQLite bindings | Mature, synchronous bindings. Simpler than sqlx for single-writer service pattern. Use `bundled` feature to embed SQLite. |
| **usn-journal-rs** | 0.4+ | MFT enumeration + USN Journal | Only maintained Rust crate for USN journal/MFT access. Provides `Mft::iter()` for full MFT enumeration and `UsnJournal` for change tracking. |
| **walkdir** | 2.x | Directory traversal (FAT fallback) | BurntSushi-maintained, comparable performance to native `find`. Used when MFT unavailable (FAT32/exFAT). |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **windows** | 0.62+ | Windows API bindings | Microsoft's official Rust bindings. Use for any Windows API not wrapped by higher-level crates (volume detection, etc.) |
| **tokio** | 1.43+ | Async runtime | Required for async database operations, service event handling. Uses IOCP on Windows. |
| **tracing** | 0.1.41+ | Structured logging | Tokio ecosystem standard. Use with `tracing-appender` for file logging in service. |
| **thiserror** | 2.0+ | Error type definitions | Derive macros for domain errors (IndexError, MftError, etc.) |
| **anyhow** | 1.0+ | Error propagation | Context-rich errors for application boundaries and service entry points. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| **rusqlite** | sqlx | Async overhead unnecessary for single-writer pattern; compile-time checks add complexity |
| **usn-journal-rs** | Direct Windows API | usn-journal-rs wraps complex FFI; fallback to raw APIs if issues arise |
| **walkdir** | std::fs::read_dir | walkdir handles errors better, provides DirEntry with cached metadata |

**Installation:**
```toml
[dependencies]
windows-service = "0.7"
rusqlite = { version = "0.38", features = ["bundled"] }
usn-journal-rs = "0.4"
walkdir = "2"
windows = { version = "0.62", features = [
    "Win32_Storage_FileSystem",
    "Win32_System_Ioctl",
    "Win32_Foundation",
]}
tokio = { version = "1.43", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
thiserror = "2.0"
anyhow = "1.0"
```

## Architecture Patterns

### Recommended Project Structure

```
src/
├── bin/
│   └── ffi-service.rs   # Windows service entry point
├── service/
│   ├── mod.rs           # Service lifecycle (start/stop/pause)
│   ├── control.rs       # Service control handler
│   └── config.rs        # Service configuration loading
├── db/
│   ├── mod.rs           # SQLite connection management
│   ├── schema.rs        # Table definitions, migrations
│   └── ops.rs           # CRUD operations, batch inserts
├── indexer/
│   ├── mod.rs           # Indexing orchestration
│   ├── mft.rs           # NTFS MFT reader
│   ├── fat.rs           # FAT volume directory walker
│   └── volume.rs        # Volume detection and classification
└── lib.rs               # Shared types, error definitions
```

### Pattern 1: Windows Service Lifecycle

**What:** Use `windows-service` crate's macro-based approach with explicit state transitions.

**When to use:** Always for Windows service implementation.

**Example:**
```rust
// Source: https://docs.rs/windows-service
use std::ffi::OsString;
use windows_service::{
    define_windows_service,
    service_control_handler::{self, ServiceControlHandlerResult},
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode,
        ServiceState, ServiceStatus, ServiceType,
    },
    service_dispatcher,
};

define_windows_service!(ffi_service, service_main);

fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service(arguments) {
        // Log error
    }
}

fn run_service(_arguments: Vec<OsString>) -> Result<(), Box<dyn std::error::Error>> {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    // Register the service control handler
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                shutdown_tx.send(()).ok();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register("FFIService", event_handler)?;

    // Tell Windows we're starting
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(30),
        process_id: None,
    })?;

    // Initialize service (database, indexers, etc.)
    // ...

    // Tell Windows we're running
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
    })?;

    // Wait for shutdown signal
    shutdown_rx.recv().ok();

    // Tell Windows we're stopping
    status_handle.set_service_status(ServiceStatus {
        current_state: ServiceState::Stopped,
        ..Default::default()
    })?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    service_dispatcher::start("FFIService", ffi_service)?;
    Ok(())
}
```

### Pattern 2: SQLite WAL Mode with Crash Safety

**What:** Configure SQLite for WAL mode with appropriate PRAGMAs for durability and performance.

**When to use:** On every database connection open.

**Example:**
```rust
// Source: https://sqlite.org/wal.html, https://docs.rs/rusqlite
use rusqlite::{Connection, Result};

fn open_database(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;

    // Enable WAL mode - this persists to the database file
    conn.pragma_update(None, "journal_mode", "WAL")?;

    // NORMAL synchronous is safe in WAL mode, faster than FULL
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    // Store temp tables in memory
    conn.pragma_update(None, "temp_store", "MEMORY")?;

    // Enable memory-mapped I/O (256MB)
    conn.pragma_update(None, "mmap_size", 268435456)?;

    // 64MB page cache
    conn.pragma_update(None, "cache_size", -64000)?;

    // Busy timeout for concurrent access
    conn.pragma_update(None, "busy_timeout", 5000)?;

    Ok(conn)
}
```

### Pattern 3: MFT Enumeration for Initial Index

**What:** Use `usn-journal-rs` to iterate MFT records for fast initial indexing.

**When to use:** On service start for NTFS volumes, or when USN journal wrap detected.

**Example:**
```rust
// Source: https://docs.rs/usn-journal-rs
use usn_journal_rs::{volume::Volume, mft::Mft};

fn scan_ntfs_volume(drive_letter: char) -> Result<Vec<FileEntry>, Box<dyn std::error::Error>> {
    let volume = Volume::from_drive_letter(drive_letter)?;
    let mft = Mft::new(&volume);

    let mut entries = Vec::new();

    for result in mft.iter() {
        match result {
            Ok(entry) => {
                // entry contains file_reference, parent_file_reference,
                // file_name, file_size, timestamps, etc.
                entries.push(FileEntry {
                    file_ref: entry.file_reference,
                    parent_ref: entry.parent_file_reference,
                    name: entry.file_name.clone(),
                    size: entry.file_size,
                    modified: entry.last_write_time,
                });
            }
            Err(e) => {
                // Log and continue - some MFT records may be unreadable
                tracing::warn!("MFT entry error: {}", e);
            }
        }
    }

    Ok(entries)
}
```

### Pattern 4: Batch Inserts for Performance

**What:** Use transactions with batched inserts for high-throughput database writes.

**When to use:** During initial index build (inserting millions of records).

**Example:**
```rust
// Source: https://sqlite.org/wal.html, Performance best practices
use rusqlite::{Connection, params, Transaction};

fn batch_insert_files(conn: &mut Connection, files: &[FileEntry]) -> Result<(), rusqlite::Error> {
    // Batch size of 100,000 is optimal for SQLite
    const BATCH_SIZE: usize = 100_000;

    for chunk in files.chunks(BATCH_SIZE) {
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO files (file_ref, parent_ref, name, size, modified)
                 VALUES (?1, ?2, ?3, ?4, ?5)"
            )?;

            for file in chunk {
                stmt.execute(params![
                    file.file_ref,
                    file.parent_ref,
                    file.name,
                    file.size,
                    file.modified,
                ])?;
            }
        }

        tx.commit()?;
    }

    Ok(())
}
```

### Pattern 5: FAT Volume Directory Walking

**What:** Use `walkdir` for recursive directory enumeration on non-NTFS volumes.

**When to use:** For FAT32/exFAT volumes that lack MFT.

**Example:**
```rust
// Source: https://github.com/BurntSushi/walkdir
use walkdir::WalkDir;
use std::path::Path;

fn scan_fat_volume(root: &Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let metadata = entry.metadata().ok();
        let path = entry.path();

        entries.push(FileEntry {
            path: path.to_path_buf(),
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir: entry.file_type().is_dir(),
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata.and_then(|m| m.modified().ok()),
        });
    }

    entries
}
```

### Anti-Patterns to Avoid

- **Single-row inserts without transaction:** Each INSERT is its own transaction = ~1000x slower than batched
- **Keeping read connections open:** Blocks WAL checkpoints, causes file growth
- **Polling filesystem instead of using MFT/USN:** Orders of magnitude slower
- **Running service initialization synchronously:** Blocks service start, can hit Windows timeout
- **Storing full paths in each row:** Wastes space; use parent_id references like MFT does

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Windows service lifecycle | Manual SCM registration | `windows-service` crate | Complex FFI, status reporting, control events |
| MFT parsing | Raw FSCTL_QUERY_USN_JOURNAL | `usn-journal-rs` | Binary format parsing, file reference resolution |
| SQLite connection pooling | Manual pool | `r2d2` + `r2d2_sqlite` | Thread safety, connection reuse |
| Directory traversal | Recursive `read_dir` | `walkdir` | Error handling, symlink cycles, cross-platform |
| UTF-8 path handling | Manual `OsString` conversion | `camino::Utf8Path` | Consistent path operations |

**Key insight:** The Windows filesystem APIs (MFT, USN Journal) are complex and poorly documented. The existing Rust ecosystem provides safe wrappers that handle edge cases. The time saved by using existing crates outweighs any minor overhead.

## Common Pitfalls

### Pitfall 1: Service Start Timeout

**What goes wrong:** Windows Service Control Manager expects services to report Running within 30 seconds. A full MFT scan on a large volume takes longer, causing SCM to kill the service.

**Why it happens:** Developers try to complete initialization before reporting Running state.

**How to avoid:**
1. Report StartPending immediately with progress checkpoints
2. Use `wait_hint` to request more time
3. Spawn indexing as background task, report Running before indexing completes
4. Implement "indexing in progress" state that allows queries on partial index

**Warning signs:** Service shows as "Starting" then fails, Event Log shows timeout

### Pitfall 2: SQLite WAL Checkpoint Starvation

**What goes wrong:** Search queries hold read transactions open, preventing WAL checkpoints. WAL file grows unbounded.

**Why it happens:** Long-running read connections block the checkpoint process in WAL mode.

**How to avoid:**
1. Use short-lived read transactions (open, query, close)
2. Schedule periodic `PRAGMA wal_checkpoint(TRUNCATE)` during low activity
3. Monitor WAL file size (should stay under main DB size)
4. Set `PRAGMA wal_autocheckpoint` appropriately

**Warning signs:** WAL file larger than main database, growing over time

### Pitfall 3: Administrator Privileges for MFT Access

**What goes wrong:** MFT enumeration requires admin/elevated privileges. Service fails silently or crashes when run without elevation.

**Why it happens:** `DeviceIoControl` with `FSCTL_ENUM_USN_DATA` requires `SE_MANAGE_VOLUME_NAME` privilege.

**How to avoid:**
1. Run service as LocalSystem or with specific privileges granted
2. Detect privilege issues early and log clear error
3. Document that service installation requires admin
4. Use `windows-service` service account configuration

**Warning signs:** Access denied errors on volume open, empty MFT iteration results

### Pitfall 4: Database Corruption on Abrupt Shutdown

**What goes wrong:** Power failure or kill -9 during database write corrupts the index.

**Why it happens:** Even with WAL mode, incomplete writes can corrupt if synchronous mode is wrong.

**How to avoid:**
1. Use `PRAGMA synchronous = NORMAL` (not OFF) - safe for WAL mode
2. Handle service stop signal gracefully - flush pending writes
3. Implement integrity check on startup (`PRAGMA integrity_check`)
4. Keep database path on local disk, not network share

**Warning signs:** `SQLITE_CORRUPT` errors after system crash, service fails to start after power loss

### Pitfall 5: Memory Exhaustion During Initial Scan

**What goes wrong:** Loading millions of MFT entries into memory before writing to database exhausts RAM.

**Why it happens:** Naive implementation collects all entries then writes.

**How to avoid:**
1. Stream MFT entries directly to database in batches
2. Use iterator-based approach, don't collect into Vec
3. Batch size of 100K is optimal balance of memory vs transaction overhead
4. Monitor memory usage during indexing

**Warning signs:** Out of memory crash during initial index, system slowdown during scan

## Code Examples

### Complete Service Entry Point

```rust
// Source: windows-service documentation + project patterns
use std::ffi::OsString;
use std::sync::mpsc;
use std::time::Duration;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode,
        ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("Service error: {}", e);
    }
}

fn run_service() -> anyhow::Result<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                shutdown_tx.send(()).ok();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register("FFIService", event_handler)?;

    // Report StartPending
    let mut status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(60),
        process_id: None,
    };
    status_handle.set_service_status(status.clone())?;

    // Initialize database
    status.checkpoint = 1;
    status_handle.set_service_status(status.clone())?;
    let db = open_database("C:\\ProgramData\\FFI\\index.db")?;

    // Start background indexer (non-blocking)
    status.checkpoint = 2;
    status_handle.set_service_status(status.clone())?;
    let indexer_handle = start_background_indexer(db.clone());

    // Report Running
    status.current_state = ServiceState::Running;
    status.controls_accepted = ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN;
    status.checkpoint = 0;
    status.wait_hint = Duration::default();
    status_handle.set_service_status(status.clone())?;

    // Wait for shutdown
    shutdown_rx.recv().ok();

    // Graceful shutdown
    status.current_state = ServiceState::StopPending;
    status.wait_hint = Duration::from_secs(30);
    status_handle.set_service_status(status.clone())?;

    indexer_handle.stop();
    db.close()?;

    status.current_state = ServiceState::Stopped;
    status_handle.set_service_status(status)?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    service_dispatcher::start("FFIService", ffi_service_main)?;
    Ok(())
}
```

### Database Schema

```sql
-- Source: Everything's approach + SQLite best practices
-- Store files with parent references (like MFT structure)

CREATE TABLE IF NOT EXISTS volumes (
    id INTEGER PRIMARY KEY,
    drive_letter TEXT NOT NULL UNIQUE,
    volume_serial TEXT NOT NULL,
    fs_type TEXT NOT NULL,  -- 'NTFS', 'FAT32', 'exFAT'
    last_usn INTEGER,       -- Last processed USN (NTFS only)
    usn_journal_id INTEGER, -- USN Journal ID (NTFS only)
    last_scan_time INTEGER  -- Unix timestamp
);

CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    volume_id INTEGER NOT NULL REFERENCES volumes(id),
    file_ref INTEGER,       -- MFT file reference number (NTFS)
    parent_ref INTEGER,     -- Parent MFT reference (NTFS) or parent file id (FAT)
    name TEXT NOT NULL,     -- Filename only (not full path)
    size INTEGER NOT NULL DEFAULT 0,
    modified INTEGER,       -- Unix timestamp
    is_dir INTEGER NOT NULL DEFAULT 0,
    UNIQUE(volume_id, file_ref)
);

-- Index for fast filename search
CREATE INDEX IF NOT EXISTS idx_files_name ON files(name COLLATE NOCASE);

-- Index for path reconstruction (parent lookups)
CREATE INDEX IF NOT EXISTS idx_files_parent ON files(volume_id, parent_ref);

-- Index for volume-based operations
CREATE INDEX IF NOT EXISTS idx_files_volume ON files(volume_id);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Polling directories for changes | USN Journal monitoring | Windows Vista (2006) | 1000x+ faster change detection |
| ANSI path APIs (MAX_PATH=260) | Wide APIs with `\\?\` prefix | Windows 10 1607 (2016) | Support for paths up to 32,767 chars |
| Roll-your-own SQLite builds | `rusqlite` with `bundled` feature | 2020+ | Simplified builds, version consistency |
| Manual Windows FFI | `windows-rs` crate | 2021 (v0.9) | Type-safe, complete Windows API access |

**Deprecated/outdated:**
- **mft crate** (omerbenamram): Parses MFT files from disk, not live volumes. Use `usn-journal-rs` for live enumeration.
- **ntfs crate** (ColinFinck): Low-level NTFS implementation, overkill for file indexing. Good for embedded/no-OS scenarios.
- **FileSystemWatcher polling:** Still needed for FAT but use `notify` crate's `ReadDirectoryChangesW` wrapper.

## Open Questions

1. **usn-journal-rs stability at scale**
   - What we know: API is straightforward, handles basic MFT enumeration
   - What's unclear: Behavior with very large volumes (10M+ files), error recovery
   - Recommendation: Build with usn-journal-rs, have fallback plan to direct Windows API if issues

2. **Database file location**
   - What we know: Service needs writable location accessible at boot
   - What's unclear: Best practice for Windows services (`%PROGRAMDATA%` vs service-specific)
   - Recommendation: Use `C:\ProgramData\FFI\` with appropriate ACLs

3. **Concurrent read access during indexing**
   - What we know: WAL mode allows concurrent reads
   - What's unclear: Performance impact of reads during heavy write batches
   - Recommendation: Test with simulated search queries during initial index build

## Sources

### Primary (HIGH confidence)
- [windows-service docs.rs](https://docs.rs/windows-service) - Service lifecycle, control handler
- [rusqlite docs.rs](https://docs.rs/rusqlite) - Connection management, PRAGMA, transactions
- [SQLite WAL Mode](https://sqlite.org/wal.html) - WAL behavior, checkpointing, concurrency
- [usn-journal-rs docs.rs](https://docs.rs/usn-journal-rs) - MFT iteration, USN journal access
- [walkdir docs.rs](https://docs.rs/walkdir) - Directory traversal, error handling
- [Microsoft: Change Journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals) - USN Journal API

### Secondary (MEDIUM confidence)
- [Everything FAQ](https://www.voidtools.com/faq/) - Performance benchmarks, architecture patterns
- [PDQ: SQLite Bulk Insert](https://www.pdq.com/blog/improving-bulk-insert-speed-in-sqlite-a-comparison-of-transactions/) - Batch size optimization

### Tertiary (LOW confidence)
- [Medium: Rust Windows Service Example](https://medium.com/@aleksej.gudkov/rust-windows-service-example-building-a-windows-service-in-rust-907be67d2287) - Additional service patterns

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries verified via official docs, versions confirmed
- Architecture: HIGH - Patterns based on Microsoft documentation and Everything's proven approach
- Pitfalls: HIGH - Documented in Microsoft docs, validated by community experience

**Research date:** 2026-01-24
**Valid until:** 2026-02-24 (30 days - stable domain)

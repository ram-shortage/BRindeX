---
phase: 01-foundation
plan: 03
subsystem: indexer
tags: [mft, walkdir, ntfs, fat32, exfat, file-indexing, windows-api, background-thread]

# Dependency graph
requires:
  - phase: 01-02
    provides: "SQLite database layer with batch_insert_files and schema"
provides:
  - "Volume detection (NTFS vs FAT32/exFAT)"
  - "NTFS MFT reader using mft crate"
  - "FAT directory walker using walkdir"
  - "Background indexer with graceful shutdown"
  - "Service integration with database and indexer lifecycle"
affects: [02-realtime-updates, 03-search-experience]

# Tech tracking
tech-stack:
  added: [mft (0.7), walkdir (2.x), windows (0.62)]
  patterns: [background-thread-with-shutdown, batch-insert, conditional-compilation]

key-files:
  created:
    - src/indexer/mod.rs
    - src/indexer/volume.rs
    - src/indexer/mft.rs
    - src/indexer/fat.rs
  modified:
    - Cargo.toml
    - src/lib.rs
    - src/service/mod.rs
    - src/bin/ffi-service.rs

key-decisions:
  - "Used mft crate (0.7) instead of usn-journal-rs due to windows crate version conflicts"
  - "Synthetic file references for FAT volumes (incrementing counter) since FAT has no MFT refs"
  - "100K batch size for database inserts, 10K-100K shutdown check intervals"
  - "Path-to-ref HashMap for FAT parent reference tracking"

patterns-established:
  - "Background thread with shutdown channel: spawn thread, pass Receiver<()>, check try_recv() periodically"
  - "Windows/non-Windows conditional compilation: #[cfg(windows)] for Windows-only code"
  - "Batch streaming to database: collect in Vec, flush at threshold, handle shutdown between batches"

# Metrics
duration: 8min
completed: 2026-01-24
---

# Phase 01 Plan 03: Initial File Indexing Summary

**NTFS MFT reader and FAT directory walker with service integration for background indexing**

## Performance

- **Duration:** 8 min
- **Started:** 2026-01-24T13:24:08Z
- **Completed:** 2026-01-24T13:31:59Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- Volume detection identifying NTFS vs FAT32/exFAT filesystems using Windows API
- NTFS MFT reader streaming entries to database in 100K batches
- FAT directory walker with synthetic file refs and parent tracking
- Service integration with database open, indexer start, and graceful shutdown

## Task Commits

Each task was committed atomically:

1. **Task 1: Add indexing dependencies and volume detection** - `ec11839` (feat)
2. **Task 2: Implement NTFS MFT reader** - `f4fbef9` (feat)
3. **Task 3: Implement FAT directory walker and wire up service** - `305543e` (feat)

## Files Created/Modified
- `src/indexer/mod.rs` - Indexing orchestration, Indexer struct, start_background_indexer()
- `src/indexer/volume.rs` - VolumeType enum, VolumeInfo struct, detect_volumes() with Windows API
- `src/indexer/mft.rs` - scan_ntfs_volume() using mft crate for MFT parsing
- `src/indexer/fat.rs` - scan_fat_volume() using walkdir with synthetic refs
- `Cargo.toml` - Added mft, walkdir, windows dependencies
- `src/lib.rs` - Added pub mod indexer
- `src/service/mod.rs` - Integrated database and indexer lifecycle
- `src/bin/ffi-service.rs` - Added version logging on startup

## Decisions Made
- **Used mft crate instead of usn-journal-rs:** The usn-journal-rs and ntfs-reader crates had incompatible windows crate versions (windows-future 0.2.x/0.3.x conflict with windows-core). The mft crate (0.7) has no windows dependency conflicts and provides MFT parsing.
- **Synthetic file references for FAT:** FAT filesystems don't have MFT-style file references, so we generate sequential IDs starting from 1. Root directory gets ref 0 (like MFT entry 5).
- **Path HashMap for parent tracking:** Maintain path -> file_ref mapping during FAT scan to correctly set parent_ref for path reconstruction.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Switched MFT library due to windows crate version conflict**
- **Found during:** Task 1 (dependency setup)
- **Issue:** usn-journal-rs 0.3/0.4 depends on windows 0.61 which has incompatible windows-future 0.2.1 causing compile errors (IMarshal, marshaler not found in windows_core::imp)
- **Fix:** Replaced usn-journal-rs with mft crate (0.7) which parses MFT files and has no windows crate conflicts
- **Files modified:** Cargo.toml
- **Verification:** cargo check passes, cargo build --release succeeds
- **Committed in:** ec11839 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Library change necessary due to Rust ecosystem version conflicts. mft crate provides equivalent MFT parsing functionality. MFT access approach changed from live volume API to parsing $MFT file directly.

## Issues Encountered
- Windows crate ecosystem version conflicts between windows 0.61/0.62 and windows-future 0.2/0.3. Resolved by using mft crate which avoids the problematic windows-future dependency.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- File indexing foundation complete
- Ready for Phase 02 real-time updates via USN Journal monitoring
- Note: MFT scanning requires administrator privileges to access $MFT directly

---
*Phase: 01-foundation*
*Completed: 2026-01-24*

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-24)

**Core value:** Instant file/folder name lookups that actually work - no flakiness, no waiting, no stale results.
**Current focus:** PROJECT COMPLETE

## Current Position

Phase: 3 of 3 (Search Experience)
Plan: 3 of 3 in current phase
Status: Complete
Last activity: 2026-01-24 - Completed 03-02-PLAN.md (final plan)

Progress: [██████████] 100%

## Performance Metrics

**Velocity:**
- Total plans completed: 8
- Average duration: 5.1 min
- Total execution time: 0.68 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 3/3 | 16 min | 5 min |
| 2. Real-time Updates | 2/2 | 10 min | 5 min |
| 3. Search Experience | 3/3 | 15 min | 5 min |

**Recent Trend:**
- Last 5 plans: 6 min, 4 min, 4 min, 5 min, 8 min
- Trend: Stable

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Key decisions from this project:

- SQLite with WAL mode for persistence (simplicity + crash safety)
- NTFS USN Journal for real-time updates (proven pattern from Everything)
- Rust stack with egui for UI (memory safety + fast startup)
- Conditional compilation (#[cfg(windows)]) for cross-platform development
- mft crate (0.7) for MFT parsing instead of usn-journal-rs (windows crate version conflicts)
- Synthetic file references for FAT volumes (sequential counter since FAT has no MFT refs)
- 100K batch size for database inserts
- usn-journal-rs 0.4 for USN monitoring (compatible with windows 0.62)
- 30-second USN polling interval, 4x throttle when CPU > 80%
- Volumes must be explicitly enabled in config (no surprise indexing)
- VolumeState enum stored as TEXT in database for readability
- 100ms debounce for mount events (boot-time flood prevention)
- 7-day offline volume retention with daily cleanup
- Length-prefixed JSON for IPC (4-byte LE prefix + JSON payload)
- Stateless IPC client (connects per request for simplicity)
- Named pipe loop pattern with spawn for server
- pest grammar for search syntax (maintainable DSL parsing)
- Windows path special handling (path_value rule for C:\)
- Path scope deferred to post-filter (SQL would need schema change)
- global-hotkey crate for system-wide hotkey registration
- 100ms search debounce for responsive search-as-you-type
- Virtual scrolling with show_rows for 1M+ result performance

### Pending Todos

None - project complete.

### Known Limitations

- MFT scanning requires administrator privileges to access $MFT directly
- Service integration of USN monitors, FAT reconciler, and IPC server needs wiring in run_service
- Path scope filtering requires path reconstruction - needs post-filter or schema change

## Session Continuity

Last session: 2026-01-24T16:52:13Z
Stopped at: PROJECT COMPLETE - All 8 plans executed
Resume file: None

## What Was Built

FastFileIndex (FFI) - An instant file search tool for Windows:

1. **Windows Service** - Background indexing with SQLite persistence
2. **NTFS MFT Reader** - Fast initial indexing (~1s per 100k files)
3. **USN Journal Monitor** - Real-time NTFS change tracking
4. **FAT Reconciler** - Periodic sync for FAT32/exFAT volumes
5. **Volume Lifecycle** - Mount/unmount detection with 7-day retention
6. **IPC Layer** - Named pipes with length-prefixed JSON protocol
7. **Search Parser** - Pest grammar for filters and wildcards
8. **Search UI** - egui popup with global hotkey and keyboard navigation

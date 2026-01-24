# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-01-24)

**Core value:** Instant file/folder name lookups that actually work - no flakiness, no waiting, no stale results.
**Current focus:** Phase 3 - Search Experience (IN PROGRESS)

## Current Position

Phase: 3 of 3 (Search Experience)
Plan: 1 of 3 in current phase
Status: In progress
Last activity: 2026-01-24 - Completed 03-01-PLAN.md

Progress: [████████░░] 75%

## Performance Metrics

**Velocity:**
- Total plans completed: 6
- Average duration: 5.0 min
- Total execution time: 0.50 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 3/3 | 16 min | 5 min |
| 2. Real-time Updates | 2/2 | 10 min | 5 min |
| 3. Search Experience | 1/3 | 4 min | 4 min |

**Recent Trend:**
- Last 5 plans: 8 min, 6 min, 4 min, 4 min
- Trend: Stable

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

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

### Pending Todos

None yet.

### Blockers/Concerns

- MFT scanning requires administrator privileges to access $MFT directly
- Service integration of USN monitors, FAT reconciler, and IPC server needs wiring in run_service

## Session Continuity

Last session: 2026-01-24T16:39:23Z
Stopped at: Completed 03-01-PLAN.md
Resume file: None

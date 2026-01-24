---
phase: 02-real-time-updates
plan: 02
subsystem: indexer
tags: [fat-reconciler, volume-watcher, volume-state, wm-devicechange, lifecycle, cleanup]

# Dependency graph
requires:
  - phase: 02-01
    provides: "USN Journal monitoring and TOML configuration"
  - phase: 01-03
    provides: "FAT volume scanning via scan_fat_volume"
provides:
  - "Volume state machine (Online/Offline/Indexing/Rescanning/Disabled)"
  - "FAT reconciliation scheduler with per-volume intervals"
  - "Volume mount/unmount detection via WM_DEVICECHANGE"
  - "Volume swap detection via serial comparison"
  - "Offline volume auto-cleanup after 7 days"
affects: [03-search-experience]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "WM_DEVICECHANGE message pump for volume events"
    - "Debounced event handling (100ms) for boot-time floods"
    - "Volume state persistence in SQLite"
    - "Periodic cleanup runs daily via reconciler loop"

key-files:
  created:
    - src/indexer/fat_reconciler.rs
    - src/service/volume_watcher.rs
  modified:
    - src/lib.rs
    - src/db/schema.rs
    - src/db/ops.rs
    - src/indexer/mod.rs
    - src/service/mod.rs

key-decisions:
  - "VolumeState enum stored as TEXT in database for readability"
  - "FAT reconciler checks every 60 seconds but respects per-volume intervals"
  - "Mount events debounced 100ms to handle boot-time volume floods"
  - "Volume serial comparison for detecting USB drive swaps at same letter"
  - "Offline volumes auto-delete after 7 days (cleanup runs daily)"

patterns-established:
  - "VolumeState::from_db/to_db_str for database persistence"
  - "start_fat_reconciler() returns handle and shutdown sender"
  - "start_volume_watcher() returns handle, shutdown sender, and event receiver"
  - "handle_volume_mount/unmount dispatch functions in indexer"

# Metrics
duration: 4min
completed: 2026-01-24
---

# Phase 02 Plan 02: FAT Reconciliation and Volume Lifecycle Summary

**FAT periodic reconciliation scheduler with volume mount/unmount detection, state machine, and offline cleanup**

## Performance

- **Duration:** 4 min
- **Started:** 2026-01-24T14:35:22Z
- **Completed:** 2026-01-24T14:39:35Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Complete volume state machine with Online/Offline/Indexing/Rescanning/Disabled states
- FAT reconciliation scheduler running periodic scans at configured intervals per volume
- WM_DEVICECHANGE-based volume watcher detecting mount/unmount events in real-time
- Volume swap detection comparing serial numbers to catch USB drive changes
- Automatic cleanup of offline volume data after 7-day retention period
- Debounced mount event handling (100ms window) for boot-time stability

## Task Commits

Each task was committed atomically:

1. **Task 1: Volume state machine and database operations** - `3255619` (feat)
2. **Task 2: FAT reconciliation scheduler** - `bbdc98d` (feat)
3. **Task 3: Volume watcher and service integration** - `0facadc` (feat)

## Files Created/Modified

- `src/lib.rs` - Added VolumeState enum with from_db/to_db_str methods
- `src/db/schema.rs` - Added state and offline_since columns to volumes table
- `src/db/ops.rs` - Added update_volume_state, get_volume_state, get_volume_by_serial, cleanup_old_offline_volumes, get_volume_serial
- `src/indexer/fat_reconciler.rs` - New FAT reconciliation scheduler with loop and handle
- `src/indexer/mod.rs` - Added handle_volume_mount, handle_volume_unmount, start_volume_event_handler
- `src/service/volume_watcher.rs` - New WM_DEVICECHANGE-based volume watcher
- `src/service/mod.rs` - Added volume_watcher module export

## Decisions Made

1. **VolumeState as TEXT in database:** More readable in direct SQL queries, easy to add new states without schema migration.

2. **60-second reconciler check loop:** Responsive enough to catch due volumes while minimal CPU overhead.

3. **100ms debounce for mount events:** Per RESEARCH.md Pitfall 3 - prevents boot-time flood overwhelming service.

4. **Serial comparison for volume swaps:** Uses GetVolumeInformationW on Windows to detect when different physical volume is mounted at same letter.

5. **Daily cleanup in reconciler loop:** Piggybacks on existing 60-second loop, tracks last cleanup time to run once per day.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks completed successfully on first attempt.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 2 complete - all real-time update components implemented
- USN monitoring (02-01) + FAT reconciliation (02-02) = complete change tracking
- Volume watcher provides mount/unmount lifecycle handling
- Ready for Phase 3: Search Experience

---
*Phase: 02-real-time-updates*
*Completed: 2026-01-24*

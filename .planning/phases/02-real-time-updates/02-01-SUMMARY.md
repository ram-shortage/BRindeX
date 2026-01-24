---
phase: 02-real-time-updates
plan: 01
subsystem: indexer
tags: [usn-journal, ntfs, polling, sysinfo, toml, serde, real-time, change-detection]

# Dependency graph
requires:
  - phase: 01-02
    provides: "SQLite database layer with batch_insert_files and schema"
  - phase: 01-03
    provides: "Volume detection and indexer infrastructure"
provides:
  - "USN Journal monitoring with wrap detection"
  - "TOML configuration infrastructure"
  - "Adaptive CPU-based throttling"
  - "Change deduplication for rapid file operations"
  - "Volume USN state persistence for resume"
affects: [02-02, 03-search-experience]

# Tech tracking
tech-stack:
  added: [usn-journal-rs, toml, serde, sysinfo, directories]
  patterns:
    - "Background thread with shutdown channel for USN polling"
    - "Adaptive throttling based on CPU usage (80% threshold)"
    - "TOML configuration with sensible defaults"
    - "Journal wrap detection via LowestValidUsn comparison"

key-files:
  created:
    - src/indexer/usn_monitor.rs
  modified:
    - Cargo.toml
    - src/lib.rs
    - src/service/config.rs
    - src/indexer/mod.rs
    - src/db/ops.rs

key-decisions:
  - "Used usn-journal-rs 0.4 for USN Journal access (compatible with windows 0.62)"
  - "30-second polling interval per CONTEXT.md decision"
  - "4x throttled interval (120s) when CPU > 80%"
  - "Volumes must be explicitly enabled in config (no surprise indexing)"
  - "Legacy ServiceConfig deprecated but maintained for backward compatibility"

patterns-established:
  - "Config::load() returns defaults if file missing"
  - "UsnMonitor::resume() for service restart continuation"
  - "deduplicate_changes() removes create-then-delete pairs"
  - "UsnMonitors struct manages lifecycle of all monitor threads"

# Metrics
duration: 6min
completed: 2026-01-24
---

# Phase 02 Plan 01: USN Journal Monitoring Summary

**NTFS USN Journal polling with wrap detection, change deduplication, TOML configuration, and adaptive CPU throttling**

## Performance

- **Duration:** 6 min
- **Started:** 2026-01-24T14:25:30Z
- **Completed:** 2026-01-24T14:31:54Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- Complete TOML configuration system with volume settings, exclude patterns, and sensible defaults
- USN Journal monitor that polls every 30 seconds, detects journal wrap/recreation
- Change deduplication that collapses rapid file changes (create+delete = no-op)
- Adaptive throttling that backs off to 120s polling when CPU > 80%
- Volume USN state persistence for resuming from last position on service restart
- UsnMonitors lifecycle manager for graceful shutdown of all monitor threads

## Task Commits

Each task was committed atomically:

1. **Task 1: Add dependencies and TOML configuration** - `416d98b` (feat)
2. **Task 2: Implement USN Journal monitor with wrap detection** - `99d78ad` (feat)
3. **Task 3: Wire USN monitor loop with adaptive throttling** - `226d6ed` (feat)

## Files Created/Modified

- `Cargo.toml` - Added usn-journal-rs, toml, serde, sysinfo, directories dependencies
- `src/lib.rs` - Added FFIError::Config variant
- `src/service/config.rs` - Expanded with Config, GeneralConfig, VolumeConfig, ExcludeConfig
- `src/indexer/usn_monitor.rs` - New file with UsnMonitor, change types, deduplication, monitor loop
- `src/indexer/mod.rs` - Added exports and start_usn_monitors()
- `src/db/ops.rs` - Added get_volume_usn() for resume functionality

## Decisions Made

1. **usn-journal-rs 0.4 chosen:** RESEARCH.md recommended this version for windows 0.62 compatibility. Initial investigation confirmed it resolves the windows-future version conflicts encountered in Phase 1.

2. **Volumes require explicit enablement:** Per CONTEXT.md, volumes must be listed in config to be indexed - no surprise indexing of plugged-in drives.

3. **Legacy ServiceConfig deprecated:** Maintained for backward compatibility with existing service code while transitioning to new Config system.

4. **CPU throttling threshold 80%:** Per RESEARCH.md guidance, 4x slowdown (30s to 120s) when system is under heavy load.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- sysinfo API changed: `CpuRefreshKind::new()` doesn't exist in 0.33, used `CpuRefreshKind::nothing().with_cpu_usage()` instead. Minor API difference, easily resolved.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- USN monitoring infrastructure complete
- Ready for Plan 02-02: Volume mount/unmount detection and FAT reconciliation
- Service integration with USN monitors will need wiring in next plan
- Background rescan placeholder needs full implementation when integrated

---
*Phase: 02-real-time-updates*
*Completed: 2026-01-24*

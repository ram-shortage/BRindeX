# Phase 2: Real-time Updates - Context

**Gathered:** 2026-01-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Keep the file index current via filesystem monitoring. NTFS volumes use USN Change Journal for near-real-time updates. FAT32/exFAT volumes use periodic full rescans. Handle volume mount/unmount gracefully. Configuration allows users to select volumes and exclude patterns.

</domain>

<decisions>
## Implementation Decisions

### USN Journal behavior
- Resume from last USN on service start (not full rescan)
- If journal has wrapped (missed changes): log warning, then trigger automatic background rescan
- Poll USN Journal every 30 seconds — low performance impact, not real-time
- Start USN monitoring only AFTER initial indexing completes (no parallel)

### FAT reconciliation
- No file watchers — periodic full scan only (simpler, more reliable)
- Reconciliation interval is configurable per-volume in config file
- Default interval: 30 minutes when not configured
- Rescans run in background with low priority — searches not blocked, results may be briefly stale

### Volume lifecycle
- Auto-index only volumes explicitly listed in config (no surprise indexing)
- On volume unmount: keep index data, mark as offline
- Auto-delete offline volume data after 7 days
- On volume reconnect: resume monitoring + quick reconciliation scan to catch changes while offline

### Update priorities
- Process all filesystem changes equally (FIFO, no path prioritization)
- Batch changes within polling interval, write in single transaction
- Deduplicate rapid changes to same file within batch (only apply final state)
- Adaptive throttling when system is under heavy load — be a good citizen

### Claude's Discretion
- Exact throttling algorithm/thresholds
- Quick scan strategy for reconnected volumes
- How to detect system load
- Config file format details

</decisions>

<specifics>
## Specific Ideas

- "Low performance impact" is the priority — 30 second polling is acceptable even though it's not truly real-time
- FAT volumes are typically removable media (USB, SD cards) — periodic scan is fine for these use cases

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-real-time-updates*
*Context gathered: 2026-01-24*

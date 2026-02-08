# FastFileIndex PRD (Windows) — Persistent Local File/Folder Name Cache with Background Updating & Validation

**Status:** Updated to include FAT/FAT32/exFAT support
**Last updated:** 2026-01-23
**Audience:** Engineering, Product, Security/IT

---

## 1. Overview

**Product (working name):** FastFileIndex (FFI)

**Problem:** Apps and power users need instant file/folder name lookups across one or more Windows volumes without repeated full crawls.

**Solution:** A local Windows service that builds and maintains a persistent, high-performance index of file and folder names (and optional lightweight metadata), supporting continuous background updates and periodic validation/self-healing across **NTFS, FAT, FAT32, and exFAT**.

---

## 2. Goals and Non-goals

### Goals
1. **Near-instant search** over file/folder names.
2. **Persistent cache** across reboots.
3. **Background updates** that keep the index consistent with disk changes.
4. **Validation & self-healing** to detect missed updates and drift.
5. **Developer-friendly local API** (IPC) + optional CLI.

### Non-goals (v1)
- Full-text indexing of file contents.
- Cloud sync/cross-device index.
- Advanced semantic classification.
- Perfect real-time fidelity on all non-NTFS filesystems (FAT-family has inherent limitations).

---

## 3. Supported Filesystems

### NTFS
- Preferred: journal-based incremental updates (USN Change Journal) + fast initial build strategies.
- Highest fidelity, lowest background cost.

### FAT / FAT32 / exFAT (Added)
- **No NTFS USN Change Journal equivalent**. Updates must rely on:
  - Directory change notifications (where available) and/or
  - Scheduled incremental scans / reconciliation.
- Time resolution and metadata behavior differ; indexing must tolerate timestamp granularity limits and weaker "stable ID" semantics.

---

## 4. Target Users / Personas
1. **Power user / Creator:** instant "find by name" across drives (incl. SD cards, USB).
2. **Developer embedding:** local filename search service for apps.
3. **IT/Admin:** deterministic resource usage and controllable scope.

---

## 5. Key Use Cases
- Type-ahead search UI ("Everything-like").
- App auto-complete/jump-to-file.
- Background inventory for names (and optional metadata).

---

## 6. Functional Requirements

### 6.1 Index Scope & Configuration
- **Selectable volumes:** Fixed + removable. (Removable media is common for FAT/exFAT.)
- Include/exclude rules:
  - Path prefix exclusions
  - Extension allow/deny list
  - Hidden/system files toggle
- Reparse points:
  - Follow symlinks/junctions default **off**
  - Loop detection if enabled
- Per-volume **filesystem capability detection** (NTFS vs FAT-family) to choose update strategy.

### 6.2 Initial Index Build
- Enumerate all files/folders and persist records.
- Two ingestion modes:
  1. **NTFS optimized path:** journal-assisted build and/or optimized enumeration.
  2. **Universal traversal path (required):** robust directory traversal for FAT/FAT32/exFAT and as fallback for NTFS.
- Throttling:
  - CPU cap / IO priority (background/idle)
  - Pause/resume
- Progress:
  - Per-volume counts and % based on discovered directories/files (best-effort).

### 6.3 Continuous Updates (Background)

#### 6.3.1 NTFS Update Mode
- Consume incremental changes using NTFS journal facilities and persist checkpoints:
  - Per volume: journal identity + last processed position
- Handles create/delete/rename/move with low overhead.

#### 6.3.2 FAT/FAT32/exFAT Update Mode (New)
Because FAT-family volumes lack NTFS-style journaling, FFI must combine **event-driven signals** and **periodic reconciliation**:

**A) Event-driven monitoring (best-effort)**
- Use directory change notifications where supported by Windows APIs.
- Practical design:
  - Maintain watchers on a rolling set of "hot" directories (recently queried, recently changed).
  - Expand watchers for included directories up to a configurable cap (to avoid handle exhaustion).
  - Coalesce events into a work queue (debounce bursts).

**B) Periodic reconciliation (required)**
- Scheduled incremental scans that ensure eventual consistency:
  - Fast pass: scan directory tree metadata to find changed directories (heuristic).
  - Deep pass: rescan file lists under directories flagged as changed.
- Scheduling options:
  - Idle-time scanning
  - Fixed intervals (e.g., every N minutes/hours)
  - On volume mount/unmount events: refresh on mount; mark offline on dismount.

**C) Drift detection triggers**
- If validation detects mismatch above a threshold, trigger targeted rescan.
- If volume was offline, missed notifications, or had heavy churn, run reconciliation.

**Expected behavior note (FAT-family):**
- Changes may be reflected with **seconds-to-minutes** delay depending on scan interval and system load.
- Rename/move detection may require heuristics (see 6.5).

### 6.4 Validation & Self-healing
- Background validator runs when idle:
  - Spot checks: sample directories and compare real listing to index.
  - Drift thresholds trigger partial rebuild.
- Integrity:
  - Transactional writes (WAL) and crash-safe commits.
  - DB checksums / manifests.
- Recovery:
  - Corruption → rebuild affected volumes while serving queries from last good snapshot if possible.

### 6.5 Rename/Move Correlation (Important for FAT-family)
NTFS can use stable identifiers more reliably; FAT-family may not expose stable per-file identifiers consistently. v1 must support **best-effort** rename/move correlation:

- Primary approach:
  - Treat events as delete+create when correlation is uncertain.
- Heuristic correlation (optional, configurable):
  - Same parent directory + close timestamp + same size (if indexed) + same name similarity
- Correctness preference:
  - Prefer **eventual consistency** over potentially wrong correlations.

### 6.6 Query & Access
- Query features:
  - Prefix search (`pho*`)
  - Substring search (`photon` → matches anywhere)
  - Optional fuzzy matching (config)
  - Filters: volume, path prefix, extension, file/folder
  - Sort: relevance, name, path
- Local IPC API:
  - Named pipes or gRPC over localhost
  - Pagination + streaming results
- Optional CLI:
  - `ffi query "report*"`
  - `ffi status`
  - `ffi rebuild --volume E:`

### 6.7 Optional UI
- Minimal tray app:
  - Status, included volumes, rebuild/reconcile controls
  - Show "index freshness" per volume (NTFS live vs FAT-family reconciled)

---

## 7. Non-Functional Requirements (NFRs)

### Performance targets
- Query latency:
  - p50 < 5ms, p95 < 30ms for 1–5M entries on SSD (indicative)
- Background overhead:
  - Configurable CPU and IO caps.
- Update lag:
  - NTFS: seconds-level typical.
  - FAT/FAT32/exFAT: bounded by notification + reconciliation cadence (configurable), typically seconds–minutes.

### Reliability
- Survive abrupt shutdown without index corruption.
- On restart:
  - Resume checkpoints (NTFS journal position or last reconciliation cursor).
  - For removable FAT/exFAT volumes: detect mount, validate, reconcile.

### Security/Privacy
- Local-only by default.
- Access control model (choose one for v1):
  1. **Per-user service** (simpler, avoids cross-user leakage).
  2. **Single system service** with strict caller-based filtering (more complex).

---

## 8. Data Model

### Core record (minimum viable)
- `entry_id` (internal)
- `volume_id`
- `parent_id` (directory node)
- `name`
- `is_dir`
- Optional metadata (config):
  - `size`
  - `mtime`
  - `attributes`

### Directory table
- `dir_id`, `parent_id`, `name`

### Volume table
- `volume_id`
- `filesystem_type` (NTFS/FAT/FAT32/exFAT)
- `volume_serial` (if available via Windows APIs)
- `mount_path(s)`
- `state` (Online/Offline/Building/Live/Reconcile)
- Checkpoints:
  - NTFS: journal identity + position
  - FAT-family: last reconciliation timestamp + last scanned subtree cursor

**Note on FAT-family timestamps:** some FAT variants have coarse timestamp granularity; validation must tolerate this and rely on directory listings rather than timestamp-only "changed" checks.

---

## 9. Storage Engine

**Recommended v1:** SQLite (WAL) for simplicity + tuned indexes
Optional performance layer: auxiliary in-memory/persisted prefix index (radix/trie) or n-gram index for substring acceleration.

Key indexes:
- `name` (collation suitable for case-insensitive search)
- `(volume_id, parent_id, name)`
- Optional: `(volume_id, is_dir, name)`

---

## 10. Architecture

### Components
1. **Service host**
   - Startup, health, policy, IPC.
2. **Ingest pipeline**
   - Initial build workers (per volume).
3. **Update subsystem**
   - NTFS journal consumer (NTFS volumes).
   - FAT-family watcher manager (best-effort).
   - Reconciliation scheduler (required for FAT-family; also useful for NTFS safety net).
4. **DB writer**
   - Single-writer queue, batched commits, WAL.
5. **Query engine**
   - Read-optimized snapshots, minimal locks.
6. **Validator**
   - Spot checks, drift detection, repairs.

### Per-volume State Machine
- `NotIndexed` → `Building` → `Live`
- `Live` + (drift detected / missed events / remount) → `Reconcile` → `Live`
- Removable: `Live` ↔ `Offline` with mount detection.

---

## 11. FAT/FAT32/exFAT Specific Design Notes (New)

### 11.1 Mount/Unmount Handling
- Detect volume arrival/removal.
- On mount:
  - Quick validation pass (sample directories).
  - Reconcile if last index time is older than threshold or drift detected.
- On unmount:
  - Mark offline; keep last known index and show "stale/offline".

### 11.2 Watcher Scaling Strategy
- Avoid placing watchers on every directory (can exhaust resources).
- Strategy:
  - Always watch root + top-level include paths.
  - Add watchers to recently changed directories and "popular" query paths.
  - Periodically prune watchers that are cold.
- Event bursts:
  - Coalesce updates, batch writes, debounce directory rescans.

### 11.3 Reconciliation Cadence
Configurable profiles:
- **Balanced (default):** idle-time plus periodic light pass (e.g., every 15–60 min).
- **Aggressive:** more frequent passes for near-real-time feel (more IO).
- **Battery saver:** suspend or lengthen interval when on battery.

### 11.4 Correctness Guarantees
- NTFS: near-real-time, high-fidelity.
- FAT-family: **eventual consistency** with bounded delay (based on cadence), best-effort near-real-time via notifications.

---

## 12. Telemetry & Observability (Opt-in)
- Local logs:
  - Index size, build rate, reconcile rate
  - Watcher count, queue depth
  - "Freshness" per volume (NTFS lag / FAT reconcile age)
  - Validation mismatch rate and repair actions

---

## 13. Acceptance Criteria (v1)
1. Index persists across reboot.
2. Query returns results interactively for 1M+ entries.
3. NTFS volumes reflect create/delete/rename/move with seconds-level lag under normal load.
4. FAT/FAT32/exFAT volumes:
   - After a change, results become correct within the configured reconciliation bound.
   - Mount/unmount is handled gracefully (offline state + refresh on mount).
5. Service remains stable under abrupt shutdown; DB remains consistent.
6. Validation detects drift and triggers targeted repairs without manual intervention.

---

## 14. Build vs Integrate (Refresher)
- **Integrate existing indexer** when acceptable:
  - Everything (excellent for NTFS; removable/FAT-family behavior may differ).
  - Windows Search (broader, less deterministic for "names-only").
- **Build** if you require:
  - First-class FAT/exFAT reconciliation logic
  - Custom ACL/privacy model
  - Deterministic policy controls and embedded distribution
  - Extensible schema/metadata and custom validation rules

---

## 15. Open Questions / Decisions for v1 Implementation
- Per-user vs system service for access control
- DB choice: SQLite-only vs SQLite + auxiliary index
- Default reconcile cadence for FAT-family (balanced vs aggressive)
- Optional metadata (size/mtime) trade-offs for speed vs utility

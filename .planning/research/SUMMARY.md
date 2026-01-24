# Project Research Summary

**Project:** FastFileIndex (FFI) - Windows File Indexing Service
**Domain:** Desktop file search and system utilities
**Researched:** 2026-01-24
**Confidence:** HIGH

## Executive Summary

FFI is a Windows file indexing service competing with voidtools Everything in the desktop file search domain. The research reveals a well-established architectural pattern: leverage NTFS's built-in USN Change Journal for instant, real-time indexing on NTFS volumes, while using directory watchers with periodic reconciliation for FAT/exFAT volumes. The recommended stack is Rust for memory-safe systems programming with direct Windows API access, SQLite with WAL mode for concurrent read/write operations, and egui for a minimal, fast-launching search UI.

The critical success factor is matching Everything's bar: instant search-as-you-type (<50ms response), real-time index updates, and minimal resource usage (~15MB RAM idle). Your stated differentiator—first-class FAT32/exFAT support—addresses a genuine gap in the market that Everything struggles with. The architecture must separate the privileged indexing service from the unprivileged search UI to minimize security risk.

The highest-risk pitfalls are USN journal wrap (causing silent index desynchronization), FileSystemWatcher unreliability on FAT volumes (requiring reconciliation), and SQLite WAL checkpoint starvation (unbounded database growth). Each has well-documented mitigation strategies that must be implemented from day one.

## Key Findings

### Recommended Stack

Rust is the only practical choice for a high-performance Windows service requiring direct USN Journal access. C++ would work but lacks memory safety; C# adds .NET runtime overhead and complicates low-level NTFS operations. The Rust ecosystem provides mature Windows support through microsoft/windows-rs, proven async capabilities via Tokio (uses IOCP on Windows), and well-maintained service scaffolding through windows-service-rs (Mullvad VPN production-proven).

**Core technologies:**
- **Rust 1.84+** with Tokio async runtime — Memory safety without GC, zero-cost abstractions, excellent Windows API access
- **windows-service** crate (0.7+) — Service lifecycle management with status reporting and control handlers
- **usn-journal-rs** (0.4+) — Safe abstractions for USN Journal and MFT enumeration (fallback: direct Windows API via windows crate)
- **notify** (8.2+) — Directory watching for FAT volumes using ReadDirectoryChangesW
- **rusqlite** (0.38+) with bundled SQLite — Synchronous database access, simpler than sqlx for single-writer scenario
- **egui/eframe** (0.32+) — Immediate-mode GUI with <100ms startup time, minimal boilerplate for popup UI
- **tray-icon** (0.21+) — System tray integration (Tauri-maintained)
- **win-hotkeys** (0.5+) — Global hotkey capture with WIN key modifier support

**Critical configuration:** SQLite must use WAL mode with proper PRAGMA settings (synchronous=normal, mmap_size=256MB, cache_size=64MB) to enable concurrent reads during writes.

### Expected Features

**Must have (table stakes):**
- Instant search-as-you-type (<50ms latency) — Everything set the bar; users won't tolerate slower
- Real-time NTFS index updates via USN Journal — Changes appear in results immediately
- Global hotkey to summon UI — All competitors have this (Ctrl+Ctrl, Alt+Space, etc.)
- Basic result actions — Open file, open containing folder, copy path to clipboard
- Minimal resource usage when idle — Everything uses ~15MB RAM; Windows Search's 300MB+ is universally hated
- Fast initial indexing — Everything indexes 120k files in ~1 second via direct MFT reading
- Wildcard search (*, ?) — Standard expectation from power users
- Keyboard-first navigation — Up/down through results, Enter to open, never touch mouse

**Should have (competitive):**
- **FAT32/exFAT volume support** — Your stated differentiator; WizFile added recently but still a gap
- Reliable filesystem watching — "It just works" is Everything's killer feature
- Search filters (ext:, size:, type:) — Power users love targeted searches
- Fuzzy matching — Listary's fuzzy matching predicts desired results
- Dark mode — Modern expectation (Everything 1.5 added this)
- Date filters (modified:, created:) — Find files from specific time periods
- Exclude patterns/folders — Critical for developers (node_modules, .git)
- Search history — Quick access to recent searches

**Defer (v2+):**
- Preview pane — Significant UI complexity
- Quick Switch integration — Requires deep Windows hooks (Listary's killer feature)
- Regex support — Niche power user feature
- Export results to CSV/TXT — Low priority utility
- Network share indexing — Complex, different use case
- Portable mode — Configuration complexity

**Anti-features (explicitly avoid):**
- Content indexing — Massively increases complexity, changes product scope
- Web search integration — Universally hated in Windows Search
- AI/Copilot features — Users explicitly reject "AI shoved down throats"
- Telemetry/data collection — Privacy-conscious users choose Everything for offline-only

### Architecture Approach

Windows file indexing follows a well-established pattern separating change detection, index management, and query services. The service runs elevated (requires admin for MFT access), while the client UI runs as standard user, communicating via named pipes for security isolation.

**Major components:**

1. **Volume Monitors** — NTFS Monitor reads MFT for initial index and polls USN Journal every 100-1000ms for changes; FAT Monitor uses ReadDirectoryChangesW with periodic reconciliation scans to catch missed events

2. **Producer-Consumer Queue** — In-memory buffer decouples change detection from database writes, allowing burst tolerance and batch operations during high disk activity

3. **Index Writer** — Single-writer worker processes queue asynchronously, writes to SQLite with WAL mode enabling concurrent reads during writes

4. **IPC Layer** — Named pipe server in service accepts queries from client, executes SQLite searches, returns results

5. **Search UI** — Borderless, topmost, always-transparent egui window; global hotkey shows/hides; sends queries via IPC client; displays results with keyboard navigation

**Data flow patterns:**
- Initial NTFS index: Read MFT directly via DeviceIoControl (FSCTL_ENUM_USN_DATA) → Batch insert to SQLite → Record journal position
- Real-time NTFS updates: USN Journal monitor polls → Change queue → Index writer processes batches → SQLite UPDATE/INSERT/DELETE
- FAT volumes: ReadDirectoryChangesW real-time + periodic full reconciliation (hourly/daily) to catch buffer overflow misses
- Query flow: Hotkey → Show popup → User types (debounced ~50ms) → IPC query → SQLite search → Results displayed

**Scalability targets (based on Everything benchmarks):**
- 100K files: ~1s initial index, ~14MB RAM, <10ms query latency
- 1M files: ~10s initial index, ~75MB RAM, <50ms query latency
- 10M files: ~2min initial index, ~500MB RAM, <200ms query latency

### Critical Pitfalls

1. **USN Journal Wrap (Data Loss)** — The circular USN Change Journal has fixed size (~32-64MB default). When the service falls behind during high disk activity or after downtime, the journal wraps and discards old entries, causing silent index desynchronization. Prevention: Detect ERROR_JOURNAL_ENTRY_DELETED, check journal ID on startup, trigger full rescan if ID changed, recommend users increase journal size to 4GB via fsutil. Address in core NTFS indexing phase.

2. **FileSystemWatcher Unreliability (FAT Volumes)** — ReadDirectoryChangesW has inherent limitations: internal buffer overflow silently discards events, no recovery mechanism, network/removable drives may not support notifications. Index gradually diverges from reality with no indication. Prevention: Large internal buffer (default 8KB too small), periodic reconciliation scans every 15-30 minutes, use watcher as trigger not source of truth, log and alert on buffer overflow. Address in FAT filesystem monitoring phase.

3. **SQLite WAL Checkpoint Starvation** — Search queries keep long-running read transactions open, preventing WAL checkpoints from completing. WAL file grows unbounded until disk fills or performance degrades. Prevention: Use short-lived read transactions (open, read, close immediately), implement connection pooling with idle timeout, schedule explicit PRAGMA wal_checkpoint(TRUNCATE) during low-activity, monitor WAL size and alert on threshold. Address in database layer implementation.

4. **Running as SYSTEM Without Necessity** — Service defaults to NT AUTHORITY\SYSTEM for MFT access, creating privilege escalation attack surface. Any exploit grants SYSTEM access. Prevention: Separate service into privileged indexer + unprivileged search components, use named pipes for IPC, drop privileges after initial MFT read if possible, audit all file operations for path vulnerabilities. Address in service architecture phase before implementation.

5. **Long Path and Unicode Filename Handling** — Using ANSI APIs (*A functions) instead of Wide APIs (*W functions) or not using \\?\ prefix causes files with paths >260 characters or Unicode filenames to be invisible. Prevention: Use Wide API functions exclusively (CreateFileW, FindFirstFileW), prepend \\?\ to all absolute paths, enable long path awareness in manifest, store paths as UTF-8 in SQLite. Test with 300+ character paths, Japanese/Chinese/Arabic filenames, emoji. Address in core filesystem enumeration + database schema.

## Implications for Roadmap

Based on research, suggested phase structure follows component dependencies and risk mitigation:

### Phase 1: Core Database Layer
**Rationale:** Everything else depends on persistent storage; must establish solid foundation with proper WAL configuration and schema design before building on top.

**Delivers:**
- SQLite database with optimized schema (files table with indexes on filename, path, parent_id)
- WAL mode configuration with checkpoint management
- Basic CRUD operations for file records
- Long path and Unicode support (UTF-8 storage, Wide API preparation)

**Addresses:**
- Table stakes: Persistence for instant search
- Pitfall #3: WAL checkpoint starvation (implement short-lived transactions from start)
- Pitfall #5: Long path handling (design schema for UTF-8, plan for \\?\ prefix)

**Research flag:** Standard patterns (SQLite is well-documented), skip /gsd:research-phase

### Phase 2: NTFS MFT Reader + Initial Indexing
**Rationale:** Provides initial index population; needed before real-time monitoring makes sense. Validates usn-journal-rs maturity early.

**Delivers:**
- MFT enumeration via usn-journal-rs or direct DeviceIoControl
- Fast initial scan (target: 120k files in ~1 second)
- Batch insertion to database
- Volume serial number tracking

**Addresses:**
- Table stakes: Fast initial indexing
- Pitfall #5: Long path and Unicode (use Wide APIs, \\?\ prefix)
- Stack validation: Test usn-journal-rs in production (fallback to direct Windows API if needed)

**Research flag:** May need /gsd:research-phase if usn-journal-rs proves insufficient (fallback to direct Windows API patterns)

### Phase 3: USN Journal Real-Time Monitoring
**Rationale:** Builds on Phase 1 (database) and Phase 2 (MFT reading); enables the killer feature of real-time updates.

**Delivers:**
- USN Journal polling (100-1000ms adaptive interval)
- Change queue (producer-consumer pattern)
- Index writer worker (batch processing)
- Journal wrap detection and recovery

**Addresses:**
- Table stakes: Real-time NTFS index updates
- Pitfall #1: USN journal wrap (implement wrap detection, journal ID verification, rescan trigger)
- Architecture: Producer-consumer queue for burst tolerance

**Research flag:** Standard patterns (well-documented by Microsoft), skip /gsd:research-phase

### Phase 4: Windows Service Infrastructure
**Rationale:** Wraps indexing components into deployable service; must establish privilege separation architecture early.

**Delivers:**
- windows-service scaffolding with lifecycle handlers
- Service installation/uninstallation
- Configuration file handling (TOML in %PROGRAMDATA%\FFI\)
- Logging to file with rotation (tracing + tracing-appender)
- Power event handling (suspend/resume)

**Addresses:**
- Pitfall #4: Running as SYSTEM (separate privileged indexer from unprivileged UI, IPC boundaries)
- Pitfall #10: Ignoring power events (register for WM_POWERBROADCAST, checkpoint on suspend, rescan on resume)
- Table stakes: Minimal resource usage (measure and optimize)

**Research flag:** Standard patterns (windows-service-rs well-documented), skip /gsd:research-phase

### Phase 5: IPC Layer (Named Pipes)
**Rationale:** Service must be running to test IPC; enables client-service separation for security.

**Delivers:**
- Named pipe server in service (\\.\pipe\LOCAL\FFI)
- Query protocol (JSON over pipe)
- IPC client library for UI
- Async query handling with cancellation

**Addresses:**
- Architecture: Service/client separation for security
- Pitfall #4: Privilege separation (unprivileged client communicates via IPC)

**Research flag:** May need /gsd:research-phase for Windows named pipe best practices in Rust

### Phase 6: Search UI + Global Hotkey
**Rationale:** Requires IPC client to fetch results; delivers end-user value (visible search interface).

**Delivers:**
- egui/eframe borderless popup window
- Global hotkey registration with win-hotkeys
- Search-as-you-type with debouncing (~50ms)
- Results display with keyboard navigation
- System tray icon with tray-icon

**Addresses:**
- Table stakes: Instant search-as-you-type, global hotkey, keyboard navigation
- Pitfall #6: Global hotkey conflicts (user-configurable, conflict detection)
- Differentiator: Fast startup (<100ms egui launch time)

**Research flag:** Standard patterns (egui well-documented), skip /gsd:research-phase

### Phase 7: Search Features + Filters
**Rationale:** Core infrastructure complete; now add power user features that differentiate product.

**Delivers:**
- Wildcard search (*, ?)
- Search filters (ext:, size:, type:, path:)
- Boolean operators (AND, OR, NOT)
- Result ranking (recency, match quality, location)
- Search history and bookmarks

**Addresses:**
- Table stakes: Wildcard search
- Differentiators: Filters, ranking
- Pitfall #11: Poor result ranking (implement recency + location + match quality)

**Research flag:** Standard patterns (search parser well-established), skip /gsd:research-phase

### Phase 8: FAT/exFAT Volume Support
**Rationale:** Your stated differentiator; optional enhancement after NTFS works perfectly.

**Delivers:**
- ReadDirectoryChangesW watcher with large buffer
- Periodic reconciliation scanner (configurable interval)
- Filesystem type detection
- Volume mount/unmount handling

**Addresses:**
- Differentiator: First-class FAT32/exFAT support (Everything struggles here)
- Pitfall #2: FileSystemWatcher unreliability (large buffer + reconciliation)
- Pitfall #9: Removable drive handling (volume serial tracking, mount detection)

**Research flag:** May need /gsd:research-phase for ReadDirectoryChangesW reliability patterns and reconciliation strategies

### Phase 9: Polish + Resilience
**Rationale:** Core functionality complete; now harden for production.

**Delivers:**
- Database corruption detection and recovery
- Index rebuild UI option
- Exclude patterns configuration (node_modules, .git)
- Dark mode theme
- Column sorting
- Throttled initial indexing with progress indication

**Addresses:**
- Pitfall #7: Indexing too aggressively (throttle, low-priority I/O, progress UI)
- Pitfall #12: No corruption recovery (integrity_check, rebuild option)
- Pitfall #13: Excluding important locations (smart defaults, user configuration)
- Differentiators: Dark mode, exclude patterns

**Research flag:** Standard patterns (well-documented), skip /gsd:research-phase

### Phase Ordering Rationale

- **Phases 1-3** build the indexing core bottom-up: database → initial scan → real-time updates. Each phase depends on the previous.
- **Phase 4** wraps the core into a deployable service, establishing privilege boundaries early to avoid security rework.
- **Phase 5-6** add the client-facing layer: IPC for security separation, then the UI that users interact with.
- **Phase 7** adds power user features once the baseline works.
- **Phase 8** tackles your differentiator (FAT support) after proving NTFS works flawlessly.
- **Phase 9** hardens for production after core functionality validated.

This ordering avoids:
- **USN journal wrap** by implementing detection in Phase 3 when journal monitoring is first built
- **SYSTEM privilege overexposure** by separating service/client in Phase 4-5 before UI exists
- **WAL checkpoint starvation** by establishing short-lived transaction patterns in Phase 1
- **FileSystemWatcher unreliability** by implementing reconciliation in Phase 8 when FAT support added

### Research Flags

**Needs deeper research during planning:**
- **Phase 2:** usn-journal-rs maturity validation (may need fallback to direct Windows API)
- **Phase 5:** Windows named pipe best practices in Rust (security, performance, error handling)
- **Phase 8:** ReadDirectoryChangesW reliability patterns and reconciliation strategies

**Standard patterns (skip /gsd:research-phase):**
- **Phase 1:** SQLite configuration and schema design (well-documented)
- **Phase 3:** USN Journal monitoring (Microsoft documentation complete)
- **Phase 4:** windows-service-rs usage (Mullvad VPN reference implementation)
- **Phase 6:** egui/eframe UI patterns (excellent documentation)
- **Phase 7:** Search parser implementation (established patterns)
- **Phase 9:** General resilience patterns (standard practices)

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All core recommendations verified via official documentation (windows-rs, Tokio, rusqlite, egui). Only uncertainty is usn-journal-rs maturity, but fallback to direct Windows API is documented. |
| Features | HIGH | Based on direct analysis of Everything, Listary, WizFile, Flow Launcher, Windows Search. Table stakes and differentiators clearly established by competitive research. |
| Architecture | HIGH | Verified against Microsoft documentation for USN Journal, ReadDirectoryChangesW, and IPC. Everything's approach validated via FAQ and forum discussions. SQLite WAL mode official documentation. |
| Pitfalls | HIGH | Critical pitfalls (journal wrap, FileSystemWatcher unreliability, WAL checkpoint) verified via Microsoft documentation and community incident reports. Security risks validated against CISA advisories. |

**Overall confidence:** HIGH

The domain is mature with well-established patterns (Everything has proven the approach for 15+ years). Microsoft documentation for USN Journal and Windows APIs is comprehensive. The main uncertainties are Rust-specific crate maturity, but all have fallback options to direct Windows API calls.

### Gaps to Address

**During Phase 2 (MFT Reader):**
- **usn-journal-rs maturity:** Crate has fewer downloads than mainstream crates. Plan: Test thoroughly in Phase 2; if insufficient, implement direct Windows API calls via windows crate (FSCTL_ENUM_USN_DATA, FSCTL_QUERY_USN_JOURNAL documented).

**During Phase 5 (IPC Layer):**
- **Named pipe security:** Research best practices for named pipe ACLs to ensure only authorized clients connect. Plan: Review Mullvad VPN implementation and Microsoft security guidance.

**During Phase 8 (FAT Support):**
- **Reconciliation strategy:** Determine optimal interval for full scans on FAT volumes (hourly? daily? configurable?). Plan: Start with configurable default (hourly), monitor performance impact, add adaptive logic.

**During Phase 6 (UI):**
- **Hotkey conflicts:** No standard conflict detection API exists. Plan: Implement try-register-fallback pattern, provide clear UI for user configuration.

**Throughout (Memory/Handle Leaks - Pitfall #8):**
- **Long-running service stability:** Rust's ownership system helps but not foolproof. Plan: Continuous monitoring, stress testing with weeks of simulated operation, Application Verifier during development.

## Sources

### Primary (HIGH confidence)
- [Microsoft Learn: Change Journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals) — USN Journal architecture and API
- [Microsoft Learn: ReadDirectoryChangesW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw) — FAT volume monitoring
- [Microsoft Learn: Maximum Path Length Limitation](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation) — Long path handling
- [windows-rs GitHub](https://github.com/microsoft/windows-rs) — Rust Windows API bindings
- [Tokio Documentation](https://tokio.rs/) — Async runtime
- [SQLite WAL Mode](https://sqlite.org/wal.html) — Write-ahead logging
- [egui Documentation](https://docs.rs/egui/latest/egui/) — Immediate-mode GUI
- [windows-service GitHub](https://github.com/mullvad/windows-service-rs) — Service scaffolding
- [rusqlite GitHub](https://github.com/rusqlite/rusqlite) — SQLite bindings

### Secondary (MEDIUM confidence)
- [voidtools Everything FAQ](https://www.voidtools.com/faq/) — Architecture reference for file indexing
- [Everything Forum: USN Journal](https://www.voidtools.com/forum/viewtopic.php?t=12779) — Technical implementation details
- [Listary features](https://www.listary.com/) — Competitive feature analysis
- [WizFile](https://antibody-software.com/wizfile/) — FAT/exFAT support patterns
- [usn-journal-rs GitHub](https://github.com/wangfu91/usn-journal-rs) — Crate API and usage
- [NTFS USN Journal Wrap](https://www.linkedin.com/pulse/ntfs-usn-journal-wrap-hell-how-get-out-todd-maxey) — Journal wrap incident report
- [Jellyfin SQLite Locking](https://jellyfin.org/posts/SQLite-locking/) — WAL concurrency patterns

### Tertiary (LOW confidence, needs validation)
- [SQLite Performance Tuning](https://phiresky.github.io/blog/2020/sqlite-performance-tuning/) — PRAGMA optimization (verify in production)
- [Windows Privilege Escalation via Services](https://medium.com/@indigoshadowwashere/windows-privilege-escalation-through-exploiting-services-d99cb7c485a8) — Security patterns
- Community reports on FileSystemWatcher buffer overflow (multiple sources, needs production validation)

---
*Research completed: 2026-01-24*
*Ready for roadmap: yes*

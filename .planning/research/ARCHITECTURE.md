# Architecture Patterns

**Domain:** Windows file indexing service (desktop search)
**Researched:** 2026-01-24
**Confidence:** HIGH (verified against Microsoft documentation and proven implementations)

## Executive Summary

Windows file indexing systems follow a well-established architecture pattern separating **change detection**, **index management**, and **query services**. The key insight from studying Windows Search and voidtools Everything is that NTFS provides a built-in change journal (USN Journal) that eliminates the need for expensive polling or file system watchers on NTFS volumes. FAT-family volumes require traditional directory watching with periodic reconciliation.

## Recommended Architecture

```
                    ┌─────────────────────────────────────────────────────┐
                    │                   FFI SERVICE                        │
                    │                 (Elevated/Admin)                     │
                    │                                                      │
                    │  ┌─────────────┐     ┌─────────────┐                │
                    │  │ NTFS Volume │     │ FAT Volume  │                │
                    │  │   Monitor   │     │   Monitor   │                │
                    │  └──────┬──────┘     └──────┬──────┘                │
                    │         │                   │                        │
                    │         │  ┌────────────────┘                        │
                    │         │  │                                         │
                    │         ▼  ▼                                         │
                    │  ┌─────────────────┐                                 │
                    │  │  Change Queue   │ ← Producer-Consumer Pattern     │
                    │  │  (In-Memory)    │                                 │
                    │  └────────┬────────┘                                 │
                    │           │                                          │
                    │           ▼                                          │
                    │  ┌─────────────────┐                                 │
                    │  │  Index Writer   │                                 │
                    │  │   (Worker)      │                                 │
                    │  └────────┬────────┘                                 │
                    │           │                                          │
                    │           ▼                                          │
                    │  ┌─────────────────┐     ┌─────────────────┐        │
                    │  │  SQLite + WAL   │     │   IPC Server    │        │
                    │  │   (Database)    │     │ (Named Pipe)    │        │
                    │  └─────────────────┘     └────────┬────────┘        │
                    │                                   │                  │
                    └───────────────────────────────────┼──────────────────┘
                                                        │
                                    ┌───────────────────┘
                                    │ Named Pipe / IPC
                                    ▼
                    ┌─────────────────────────────────────────────────────┐
                    │                   FFI CLIENT                         │
                    │              (Standard User Process)                 │
                    │                                                      │
                    │  ┌─────────────────┐     ┌─────────────────┐        │
                    │  │ Global Hotkey   │────▶│  Search Popup   │        │
                    │  │   Handler       │     │      UI         │        │
                    │  └─────────────────┘     └────────┬────────┘        │
                    │                                   │                  │
                    │                                   ▼                  │
                    │                          ┌─────────────────┐        │
                    │                          │   IPC Client    │        │
                    │                          │ (Query Service) │        │
                    │                          └─────────────────┘        │
                    └─────────────────────────────────────────────────────┘
```

## Component Boundaries

| Component | Responsibility | Process | Communicates With |
|-----------|---------------|---------|-------------------|
| **NTFS Volume Monitor** | Reads MFT for initial index, monitors USN Journal for changes | Service | Change Queue |
| **FAT Volume Monitor** | ReadDirectoryChangesW watcher + periodic reconciliation scans | Service | Change Queue |
| **Change Queue** | In-memory buffer for file change events | Service | Volume Monitors (producers), Index Writer (consumer) |
| **Index Writer** | Processes queue, writes to SQLite database | Service | Change Queue, SQLite Database |
| **SQLite Database** | Persistent index storage (filenames, paths, sizes, dates) | Service | Index Writer, IPC Server |
| **IPC Server** | Handles query requests from client over named pipe | Service | SQLite Database, IPC Client |
| **Global Hotkey Handler** | Registers system-wide hotkey, shows/hides popup | Client | Search Popup UI |
| **Search Popup UI** | Minimal overlay window for search input and results | Client | Global Hotkey Handler, IPC Client |
| **IPC Client** | Sends queries to service, receives results | Client | IPC Server, Search Popup UI |

## Data Flow

### 1. Initial Index Build (NTFS)

```
Volume Mount Detected
        │
        ▼
┌───────────────────┐
│ Read MFT Directly │  ← DeviceIoControl with FSCTL_GET_NTFS_VOLUME_DATA
│ (Full Scan Once)  │     then enumerate MFT entries
└─────────┬─────────┘
          │
          ▼
    Batch Insert to SQLite
          │
          ▼
    Record USN Journal Position
```

**Key insight from Everything:** Reading the MFT directly is orders of magnitude faster than directory traversal. A fresh Windows 10 (~120,000 files) indexes in ~1 second.

### 2. Real-Time Updates (NTFS)

```
File System Change (create/delete/rename/modify)
        │
        ▼
┌───────────────────┐
│ NTFS Driver Writes│  ← Automatic, managed by Windows
│ to USN Journal    │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ USN Journal       │  ← FSCTL_READ_USN_JOURNAL via DeviceIoControl
│ Monitor (polling) │     Poll every 100-1000ms
└─────────┬─────────┘
          │
          ▼
    Change Queue (add/delete/rename events)
          │
          ▼
    Index Writer processes queue
          │
          ▼
    SQLite UPDATE/INSERT/DELETE
```

### 3. FAT Volume Monitoring (No Journal Available)

```
┌────────────────────────────────────────────────┐
│           FAT Volume Monitor                    │
│                                                 │
│  ┌─────────────────┐    ┌─────────────────┐   │
│  │ ReadDirectory   │    │ Periodic Full   │   │
│  │ ChangesW        │    │ Reconciliation  │   │
│  │ (Real-time)     │    │ (Hourly/Daily)  │   │
│  └────────┬────────┘    └────────┬────────┘   │
│           │                      │             │
│           └──────────┬───────────┘             │
│                      │                         │
│                      ▼                         │
│              Change Queue                      │
└────────────────────────────────────────────────┘
```

**Why dual approach for FAT:**
- `ReadDirectoryChangesW` can miss events (buffer overflow, service restart)
- Periodic reconciliation catches drift
- Everything uses this pattern for non-NTFS volumes

### 4. Query Flow

```
User presses global hotkey (e.g., Ctrl+Space)
        │
        ▼
┌───────────────────┐
│ Show Search Popup │  ← WPF Window with AllowsTransparency, Topmost
└─────────┬─────────┘
          │
          ▼
User types query (debounced ~50ms)
          │
          ▼
┌───────────────────┐
│ IPC Client sends  │  ← Named pipe: \\.\pipe\LOCAL\FFI
│ query to service  │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Service executes  │  ← SQLite FTS5 or LIKE query
│ search query      │     Concurrent reads via WAL mode
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Return results    │  ← Filename, path, size, date
│ over named pipe   │
└─────────┬─────────┘
          │
          ▼
UI displays results with keyboard navigation
```

## Patterns to Follow

### Pattern 1: Service/Client Separation

**What:** Separate the indexing service (elevated) from the UI (standard user).

**Why:**
- USN Journal requires elevated privileges or admin rights
- UI should run as standard user for security
- Service can run at boot before user login

**Implementation:**
```
Service: Runs as LocalSystem or custom service account
  - Has access to raw NTFS structures
  - Manages index database
  - Exposes IPC endpoint

Client: Runs as current user
  - No elevation required
  - Lightweight tray application
  - Popup appears on hotkey
```

### Pattern 2: Producer-Consumer Queue for Changes

**What:** Buffer file changes in a queue, process asynchronously.

**Why:**
- Burst changes (e.g., unzip operation) don't overwhelm database
- Allows batching of database writes
- Decouples detection from processing

**Implementation:**
```csharp
// Producer (Volume Monitor)
ConcurrentQueue<FileChange> _changeQueue;

void OnUSNRecordReceived(USN_RECORD record)
{
    _changeQueue.Enqueue(new FileChange
    {
        Type = MapReason(record.Reason),
        FileId = record.FileReferenceNumber,
        ParentId = record.ParentFileReferenceNumber,
        Name = record.FileName
    });
}

// Consumer (Index Writer)
async Task ProcessChangesAsync(CancellationToken ct)
{
    while (!ct.IsCancellationRequested)
    {
        var batch = new List<FileChange>();
        while (batch.Count < 100 && _changeQueue.TryDequeue(out var change))
        {
            batch.Add(change);
        }

        if (batch.Any())
            await WriteBatchToDatabase(batch);
        else
            await Task.Delay(50, ct);
    }
}
```

### Pattern 3: SQLite WAL for Concurrent Access

**What:** Use Write-Ahead Logging mode for SQLite.

**Why:**
- Allows concurrent reads while writing
- Readers don't block writer
- Critical for responsive search during indexing

**Implementation:**
```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;  -- Good balance of safety/speed
PRAGMA cache_size=-64000;   -- 64MB cache
```

### Pattern 4: Minimal Popup UI (Overlay Window)

**What:** Borderless, always-on-top window that appears/hides instantly.

**Why:**
- No window decoration overhead
- Feels like system-integrated search
- Fast show/hide animation

**Implementation (WPF):**
```xml
<Window WindowStyle="None"
        AllowsTransparency="True"
        Background="Transparent"
        Topmost="True"
        ShowInTaskbar="False"
        ResizeMode="NoResize">
```

## Anti-Patterns to Avoid

### Anti-Pattern 1: Polling the File System

**What:** Using periodic directory enumeration to detect changes.

**Why bad:**
- Extremely slow for large volumes
- High CPU and I/O usage
- Misses changes between polls
- Everything does this ONLY for FAT volumes as fallback

**Instead:** Use USN Journal for NTFS, ReadDirectoryChangesW for FAT.

### Anti-Pattern 2: Single Process with Elevation

**What:** Running the entire application (UI + indexer) as elevated/admin.

**Why bad:**
- Security risk (UI attack surface with admin privileges)
- UAC prompt on every launch
- Can't auto-start for standard users

**Instead:** Service/Client architecture with IPC.

### Anti-Pattern 3: Indexing File Contents

**What:** Trying to index file contents like Windows Search does.

**Why bad:**
- Enormously slower (reading every file)
- Much larger database
- Complex filter system needed
- Not the goal of a "file name search" tool

**Instead:** Index names only. This is what makes Everything fast.

### Anti-Pattern 4: Synchronous IPC in UI Thread

**What:** Blocking UI thread while waiting for search results.

**Why bad:**
- UI freezes during search
- Poor perceived performance
- Can trigger "not responding" state

**Instead:** Async queries with cancellation on keystroke.

### Anti-Pattern 5: Full Database Rebuild on Change

**What:** Rebuilding entire index when files change.

**Why bad:**
- Wastes resources on incremental changes
- Index unavailable during rebuild
- Everything only rebuilds when USN Journal ID changes

**Instead:** Incremental updates via queue processing.

## Suggested Build Order

Based on component dependencies:

### Phase 1: Core Database Layer
**Components:** SQLite schema, basic CRUD operations
**Why first:** Everything else depends on storage
**Deliverable:** Tested database module that can store/retrieve file records

### Phase 2: NTFS MFT Reader
**Components:** MFT enumeration, initial index population
**Why second:** Needed to populate database with initial data
**Deliverable:** Can scan NTFS volume and populate database

### Phase 3: USN Journal Monitor
**Components:** Journal reader, change event parsing
**Why third:** Builds on Phase 1 (writes to DB), enables real-time updates
**Deliverable:** Can detect and index file changes in real-time

### Phase 4: Windows Service Shell
**Components:** Service installation, lifecycle, configuration
**Why fourth:** Wraps Phase 2-3 components into deployable service
**Deliverable:** Installable service that indexes on startup and monitors changes

### Phase 5: IPC Layer
**Components:** Named pipe server (service), client library
**Why fifth:** Service must be running to test IPC
**Deliverable:** Client can query service and receive results

### Phase 6: Search Popup UI
**Components:** WPF window, global hotkey, results display
**Why sixth:** Requires IPC client to fetch results
**Deliverable:** Hotkey shows popup, typing searches, results displayed

### Phase 7: FAT Volume Support
**Components:** Directory watcher, reconciliation scanner
**Why seventh:** Optional enhancement, NTFS is primary target
**Deliverable:** FAT32/exFAT volumes also indexed

### Phase 8: Polish & Features
**Components:** Keyboard actions, copy path, settings, tray icon
**Why last:** Core functionality must work first
**Deliverable:** Production-ready application

## Build Order Dependency Graph

```
Phase 1 (Database)
    │
    ├───────────────────┬───────────────────┐
    ▼                   ▼                   │
Phase 2 (MFT)      Phase 3 (USN)           │
    │                   │                   │
    └─────────┬─────────┘                   │
              │                             │
              ▼                             │
         Phase 4 (Service)                  │
              │                             │
              ▼                             │
         Phase 5 (IPC) ◄────────────────────┘
              │
              ▼
         Phase 6 (UI)
              │
              ├───────────────────┐
              ▼                   ▼
         Phase 7 (FAT)      Phase 8 (Polish)
```

## Scalability Considerations

| Concern | At 100K files | At 1M files | At 10M files |
|---------|---------------|-------------|--------------|
| Initial index time | ~1 second | ~10 seconds | ~2 minutes |
| RAM usage (index in memory) | ~14 MB | ~75 MB | ~500 MB |
| Database size | ~9 MB | ~45 MB | ~300 MB |
| USN poll frequency | 1000ms | 500ms | 100ms |
| Query latency | <10ms | <50ms | <200ms |

*Estimates based on Everything's published benchmarks*

## Sources

### HIGH Confidence (Official Documentation)
- [Microsoft Learn: Indexing Process in Windows Search](https://learn.microsoft.com/en-us/windows/win32/search/-search-indexing-process-overview) - Windows Search architecture
- [Microsoft Learn: Change Journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals) - USN Journal API
- [Microsoft Learn: Interprocess Communications](https://learn.microsoft.com/en-us/windows/win32/ipc/interprocess-communications) - IPC mechanisms

### MEDIUM Confidence (Verified Against Multiple Sources)
- [voidtools Everything FAQ](https://www.voidtools.com/faq/) - Everything's approach
- [voidtools Forum: Understanding Indexing and USN Journal](https://www.voidtools.com/forum/viewtopic.php?t=12779) - Technical details
- [GitHub: NHotkey](https://github.com/thomaslevesque/NHotkey) - Global hotkey implementation
- [SQLite WAL Documentation](https://sqlite.org/wal.html) - Write-ahead logging

### LOW Confidence (Community/Single Source)
- Producer-consumer pattern details from Medium articles
- Some performance estimates extrapolated from Everything benchmarks

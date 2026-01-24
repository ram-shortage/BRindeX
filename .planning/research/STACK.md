# Technology Stack

**Project:** FastFileIndex (FFI) - Windows File Indexing Service
**Researched:** 2026-01-24

## Executive Summary

Build FFI in **Rust** using the native Windows ecosystem. Rust provides the performance, memory safety, and low-level Windows API access required for a system service that must run continuously, handle filesystem events in real-time, and maintain a responsive UI. The stack leverages mature, well-maintained crates with strong Windows support.

**Overall Confidence:** HIGH - All core recommendations verified via official documentation and current crate versions.

---

## Recommended Stack

### Language & Runtime

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **Rust** | 1.84+ (stable) | Core language | Memory safety without GC, zero-cost abstractions, excellent Windows support via windows-rs. Required for USN journal access and performant indexing. | HIGH |
| **Tokio** | 1.43+ | Async runtime | De facto standard async runtime. Uses IOCP on Windows. Required for concurrent filesystem monitoring, database operations, and service event handling. | HIGH |

**Rationale:** Rust is the only practical choice for a high-performance Windows service that needs direct USN journal access. C++ would work but lacks memory safety guarantees. C# could work but adds .NET runtime overhead and complicates USN journal access.

### Windows Service Infrastructure

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **windows-service** | 0.7+ | Service scaffolding | Mullvad-maintained crate with `define_windows_service!` macro. Handles service lifecycle, status reporting, and control events (stop, pause, interrogate). | HIGH |
| **windows** | 0.62+ | Windows API bindings | Microsoft's official Rust bindings. Required for RegisterHotKey, Shell_NotifyIcon, and any Windows API not wrapped by higher-level crates. | HIGH |

**Rationale:** windows-service is the standard for Rust Windows services, used in production by Mullvad VPN. The windows crate is Microsoft-maintained and provides access to any Windows API.

### NTFS/USN Journal Access

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **usn-journal-rs** | 0.4+ | USN Journal + MFT enumeration | Only maintained Rust crate for USN journal access. Provides safe abstractions for `DeviceIoControl` calls, MFT enumeration, and file ID resolution. | MEDIUM |

**Rationale:** usn-journal-rs wraps the complex Windows FFI for USN journal operations. While it has fewer downloads than mainstream crates, it's the only option and the code is straightforward. If issues arise, the underlying Windows API calls are well-documented and can be implemented directly via the windows crate.

**Fallback:** Direct Windows API calls via `windows` crate if usn-journal-rs proves insufficient:
- `FSCTL_QUERY_USN_JOURNAL` - Query journal state
- `FSCTL_READ_USN_JOURNAL` - Read journal records
- `FSCTL_ENUM_USN_DATA` - Enumerate MFT for initial index

### FAT/exFAT Filesystem Monitoring

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **notify** | 8.2+ | Directory watching | Cross-platform filesystem watcher. Uses ReadDirectoryChangesW on Windows. Provides real-time events for FAT32/exFAT volumes that lack USN journal. | HIGH |

**Rationale:** FAT filesystems have no change journal. notify uses ReadDirectoryChangesW which works on any Windows filesystem. For FAT volumes, combine real-time watching with periodic full scans to catch changes that occur when the service isn't running.

**Architecture for FAT:**
1. **Real-time:** notify watches FAT volumes while service runs
2. **Reconciliation:** Periodic full directory scan (configurable interval)
3. **Startup:** Full scan on service start for FAT volumes (NTFS uses USN journal catchup)

### Database

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **rusqlite** | 0.38+ | SQLite bindings | Mature, synchronous SQLite bindings. Simpler than sqlx for our use case (single-writer service). | HIGH |
| Feature: `bundled` | - | Static SQLite | Embeds SQLite, avoiding system dependency issues on Windows. | HIGH |

**Rationale:** SQLite with WAL mode is ideal for this use case:
- Single writer (indexing service), multiple readers (search UI)
- Persistent storage survives service restarts
- No external database server needed
- WAL mode allows concurrent reads during writes

**Why rusqlite over sqlx:**
- We don't need multi-database support
- Synchronous API is simpler for our indexing workload
- No compile-time query checking overhead needed for simple queries
- Avoids potential libsqlite3-sys version conflicts

**SQLite Configuration (PRAGMA settings):**
```sql
PRAGMA journal_mode = WAL;          -- Write-ahead logging for concurrency
PRAGMA synchronous = normal;        -- Safe in WAL mode, better performance
PRAGMA temp_store = memory;         -- Temp tables in RAM
PRAGMA mmap_size = 268435456;       -- 256MB memory-mapped I/O
PRAGMA cache_size = -64000;         -- 64MB page cache
PRAGMA busy_timeout = 5000;         -- 5s busy wait
```

### Search

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **SQLite LIKE/GLOB** | Built-in | Filename search | For MVP, simple LIKE queries with ESCAPE are sufficient. Index on filename column. | HIGH |
| **SQLite FTS5** | Built-in | Future: path search | Optional future enhancement for searching path segments. Not needed for filename-only search. | MEDIUM |

**Rationale:** Everything (voidtools) proves that simple string matching is sufficient for filename search. FTS5 would only help if searching *within* filenames (partial word matching) or path segments becomes important.

**Why NOT Tantivy:**
- Adds complexity without proportional benefit for filename search
- SQLite is already required for persistence
- Tantivy excels at document content search, which we don't need

### Global Hotkey

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **win-hotkeys** | 0.5+ | Global hotkey capture | Windows-specific, thread-safe, supports WIN key modifier. Uses WH_KEYBOARD_LL hook. | MEDIUM |

**Alternative:** **global-hotkey** (0.6+) - Cross-platform from Tauri team, more actively maintained but requires win32 event loop.

**Rationale:** win-hotkeys is simpler for Windows-only development. global-hotkey is better if cross-platform is ever considered.

### Search UI

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **egui** | 0.32+ | Immediate-mode GUI | Minimal boilerplate, fast startup, simple popup/overlay creation. No HTML/CSS/JS required. | HIGH |
| **eframe** | 0.32+ | egui native wrapper | Handles window creation, event loop, rendering. Uses winit + glow (OpenGL). | HIGH |

**Rationale:** egui/eframe is ideal for a minimal search popup:
- Sub-100ms startup time (critical for hotkey responsiveness)
- Immediate mode = simple state management for search-as-you-type
- Native look optional via egui-winit-viewport
- Small binary size vs Tauri/Electron

**UI Architecture:**
- Service runs headless
- Hotkey spawns/shows search window process
- Window communicates with service via named pipe or shared SQLite
- Window is borderless, positioned near cursor or centered

**Why NOT Tauri:**
- WebView startup time is noticeable
- Overkill for a single search input + results list
- Adds web stack complexity (HTML/CSS/JS)

### System Tray

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **tray-icon** | 0.21+ | System tray icon | Tauri-maintained, cross-platform, well-documented. Shows icon + context menu. | HIGH |

**Rationale:** tray-icon integrates well with egui/eframe event loop and provides standard tray functionality (icon, tooltip, context menu).

### Error Handling

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **thiserror** | 2.0+ | Error type definitions | Derive macros for custom error types. Use for library/crate boundaries. | HIGH |
| **anyhow** | 1.0+ | Error propagation | Context-rich errors for application code. Use in main, service entry points. | HIGH |

**Pattern:**
- Define domain errors with thiserror (IndexError, WatcherError, etc.)
- Wrap/propagate with anyhow at application boundaries
- Log errors with tracing before propagation

### Logging & Diagnostics

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **tracing** | 0.1.41+ | Structured logging | Tokio ecosystem standard. Spans for async context. | HIGH |
| **tracing-subscriber** | 0.3+ | Log output | FmtSubscriber for development, file rotation for production. | HIGH |
| **tracing-appender** | 0.2+ | File logging | Non-blocking file appender with rotation. Essential for Windows service. | HIGH |

**Configuration:**
- Development: stdout with `tracing-subscriber::fmt`
- Production: Rolling file in `%PROGRAMDATA%\FFI\logs\`
- Level: INFO default, DEBUG/TRACE via config

### Serialization & Configuration

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **serde** | 1.0+ | Serialization framework | De facto standard. Required for config, IPC messages. | HIGH |
| **toml** | 0.8+ | Config file format | Human-readable, Windows-friendly. Simpler than YAML. | HIGH |

**Config location:** `%PROGRAMDATA%\FFI\config.toml`

### Inter-Process Communication

| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| **Named Pipes** (windows crate) | - | Service <-> UI communication | Windows-native IPC. Low latency for search queries. | HIGH |

**Alternative:** Shared SQLite database (simpler, higher latency)

**Rationale:** Named pipes provide low-latency request/response for search-as-you-type. The UI sends search queries, service returns results. Protocol can be simple JSON over pipe.

---

## Alternatives Considered

### Language Alternatives

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| **Rust** | C++ | Memory safety concerns, more error-prone Windows API usage |
| **Rust** | C# | .NET runtime overhead, USN journal access requires P/Invoke complexity |
| **Rust** | Go | Poor Windows service support, no USN journal crates, CGO complexity |

### Database Alternatives

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| **rusqlite** | sqlx | Async overhead unnecessary, compile-time checks add build complexity |
| **SQLite** | LMDB | Less tooling, no SQL for ad-hoc queries |
| **SQLite** | RocksDB | Overkill for file index, larger binary |

### GUI Alternatives

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| **egui/eframe** | Tauri | WebView startup latency, larger binary, web stack complexity |
| **egui/eframe** | iced | Steeper learning curve, slower iteration |
| **egui/eframe** | Slint | Commercial license required for some uses |
| **egui/eframe** | WinUI/WPF | Requires C# interop, defeats Rust purpose |

### Search Alternatives

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| **SQLite LIKE** | Tantivy | Overkill for filename search, adds operational complexity |
| **SQLite LIKE** | MeiliSearch | External server, overkill for local search |

---

## Project Dependencies

### Cargo.toml (Service)

```toml
[package]
name = "ffi-service"
version = "0.1.0"
edition = "2024"

[dependencies]
# Async runtime
tokio = { version = "1.43", features = ["full"] }

# Windows service
windows-service = "0.7"

# Windows API
windows = { version = "0.62", features = [
    "Win32_Storage_FileSystem",
    "Win32_System_Ioctl",
    "Win32_Foundation",
] }

# USN Journal
usn-journal-rs = "0.4"

# Filesystem watching (FAT volumes)
notify = { version = "8.2", default-features = false, features = ["macos_fsevent"] }

# Database
rusqlite = { version = "0.38", features = ["bundled"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"

# Error handling
thiserror = "2.0"
anyhow = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
```

### Cargo.toml (UI)

```toml
[package]
name = "ffi-ui"
version = "0.1.0"
edition = "2024"

[dependencies]
# GUI
eframe = "0.32"
egui = "0.32"

# System tray
tray-icon = "0.21"

# Hotkey
win-hotkeys = "0.5"
# or: global-hotkey = "0.6"

# Windows API (for window positioning, etc.)
windows = { version = "0.62", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
] }

# IPC
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Error handling
anyhow = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## Build & Distribution

### Compilation

```bash
# Release build with optimizations
cargo build --release

# Windows-specific: embed manifest for admin privileges
# (Configure in build.rs or .cargo/config.toml)
```

### Binary Size Optimization

```toml
# Cargo.toml
[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

### Installer Considerations

| Tool | Purpose |
|------|---------|
| **WiX Toolset** | MSI installer creation |
| **Inno Setup** | Alternative installer |
| **cargo-wix** | Rust-friendly WiX wrapper |

---

## Sources

### Official Documentation
- [Windows Change Journals - Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals) (HIGH confidence)
- [windows-rs GitHub - Microsoft](https://github.com/microsoft/windows-rs) (HIGH confidence)
- [Tokio Documentation](https://tokio.rs/) (HIGH confidence)
- [SQLite WAL Mode](https://sqlite.org/wal.html) (HIGH confidence)
- [egui Documentation](https://docs.rs/egui/latest/egui/) (HIGH confidence)

### Crate Documentation
- [usn-journal-rs GitHub](https://github.com/wangfu91/usn-journal-rs) (MEDIUM confidence - less mainstream)
- [windows-service GitHub - Mullvad](https://github.com/mullvad/windows-service-rs) (HIGH confidence)
- [rusqlite GitHub](https://github.com/rusqlite/rusqlite) (HIGH confidence)
- [notify GitHub](https://github.com/notify-rs/notify) (HIGH confidence)
- [tray-icon crates.io](https://crates.io/crates/tray-icon) (HIGH confidence)
- [win-hotkeys crates.io](https://crates.io/crates/win-hotkeys) (MEDIUM confidence)
- [tracing GitHub - Tokio](https://github.com/tokio-rs/tracing) (HIGH confidence)

### Reference Implementations
- [Everything FAQ - voidtools](https://www.voidtools.com/faq/) - Architecture reference for file indexing (HIGH confidence)
- [SQLite Performance Tuning](https://phiresky.github.io/blog/2020/sqlite-performance-tuning/) (MEDIUM confidence)

---

## Open Questions

1. **usn-journal-rs maturity:** Need to validate in production. Fallback plan is direct Windows API calls.
2. **UI process architecture:** Single process with hidden window vs separate UI process? Separate is cleaner but adds IPC complexity.
3. **Network drive support:** ReadDirectoryChangesW has limitations on network paths. May need polling fallback.
4. **ReFS support:** usn-journal-rs claims ReFS support but needs verification.

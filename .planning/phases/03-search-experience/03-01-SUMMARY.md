---
phase: 03-search-experience
plan: 01
status: complete
subsystem: ipc
tags: [named-pipes, windows, tokio, serde-json]
dependency-graph:
  requires: [01-03, 02-01]
  provides: [ipc-layer, search-protocol]
  affects: [03-02, 03-03]
tech-stack:
  added: [serde_json]
  patterns: [length-prefixed-json, named-pipe-loop]
key-files:
  created:
    - src/ipc/mod.rs
    - src/ipc/protocol.rs
    - src/ipc/server.rs
    - src/ipc/client.rs
  modified:
    - Cargo.toml
    - src/lib.rs
decisions:
  - id: IPC-01
    choice: "Length-prefixed JSON messages (4-byte LE prefix)"
    rationale: "Prevents partial read issues, simple to implement, debuggable wire format"
  - id: IPC-02
    choice: "Stateless client (connects per request)"
    rationale: "Simplifies error handling and avoids connection state management"
  - id: IPC-03
    choice: "Loop pattern with spawn for server"
    rationale: "Allows multiple sequential clients, handles each connection independently"
metrics:
  duration: "4 min"
  completed: "2026-01-24"
---

# Phase 3 Plan 1: IPC Layer Summary

Named pipes IPC for service-client communication with length-prefixed JSON protocol.

## What Was Built

### IPC Protocol (src/ipc/protocol.rs)
- `PIPE_NAME` constant: `\\.\pipe\FFI_Search`
- `SearchRequest`: query, limit, offset fields
- `SearchResponse`: results, total_count, search_time_ms
- `FileResult`: id, name, path, size, modified, is_dir
- `read_message<T>` / `write_message<T>`: async helpers with 4-byte LE length prefix
- 16MB max message size guard

### IPC Server (src/ipc/server.rs)
- `IpcServer::new(db)`: constructor with shared database reference
- `IpcServer::run(shutdown)`: accepts broadcast shutdown channel
- Loop pattern from RESEARCH.md: create server -> await connection -> spawn handler -> repeat
- `handle_client`: reads request, executes search_files, reconstructs paths, measures time
- Uses `tokio::select!` for clean shutdown handling

### IPC Client (src/ipc/client.rs)
- `IpcClient::new()`: stateless constructor
- `search(query, limit)`: basic search API
- `search_with_offset(query, limit, offset)`: paginated search
- `is_service_available()`: connectivity check
- Helpful error messages when service unavailable

### Module Structure (src/ipc/mod.rs)
- Conditional compilation: server/client behind `#[cfg(windows)]`
- Re-exports protocol types unconditionally for cross-platform usage

## Decisions Made

| ID | Decision | Rationale |
|----|----------|-----------|
| IPC-01 | Length-prefixed JSON (4-byte LE) | Prevents partial reads, debuggable format |
| IPC-02 | Stateless client | Simpler error handling, no connection state |
| IPC-03 | Server loop with spawn | Multiple sequential clients, independent handling |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Search module missing query.rs**
- **Found during:** Task 1 verification
- **Issue:** Parallel Plan 03-03 created search module but was incomplete
- **Fix:** Did not need to fix - file was created by concurrent execution
- **Resolution:** Waited for file system sync, proceeded with compilation

## Technical Details

### Wire Protocol
```
Request/Response format:
+--------+--------------------+
| 4 bytes|   N bytes          |
| length |   JSON payload     |
| (LE)   |                    |
+--------+--------------------+
```

### Server Pattern
```rust
loop {
    let server = ServerOptions::new()
        .first_pipe_instance(false)
        .create(PIPE_NAME)?;

    tokio::select! {
        _ = shutdown.recv() => return Ok(()),
        result = server.connect() => {
            // spawn handler
        }
    }
}
```

## Commits

| Hash | Message |
|------|---------|
| 899b3d5 | feat(03-01): add IPC protocol types for search communication |
| 0fb9869 | feat(03-01): implement named pipe server for search IPC |
| 7b9b85a | feat(03-01): implement named pipe client for search UI |

## Next Phase Readiness

### Completed Prerequisites
- [x] IPC module compiles without errors
- [x] Protocol types serializable with serde_json
- [x] Server uses loop pattern for multiple clients
- [x] Client provides async search API
- [x] All code has #[cfg(windows)] gates

### Ready For
- Plan 03-02: Search UI can use IpcClient for queries
- Plan 03-03: Search syntax parser can be integrated into protocol
- Service integration: IpcServer can be spawned alongside USN monitors

### Integration Points
- Service: `IpcServer::run()` should be spawned in `run_service()`
- UI: `IpcClient` provides the search API for the egui frontend
- Database: Server uses existing `search_files` and `reconstruct_path` from db::ops

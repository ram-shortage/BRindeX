# Requirements: FastFileIndex (FFI)

**Defined:** 2026-01-24
**Core Value:** Instant file/folder name lookups that actually work — no flakiness, no waiting, no stale results.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Service Infrastructure

- [ ] **SERV-01**: Windows service runs at system startup with minimal resource footprint
- [ ] **SERV-02**: Service survives abrupt shutdown without index corruption
- [ ] **SERV-03**: Service exposes IPC endpoint for client queries (named pipes)

### Indexing

- [ ] **INDX-01**: Index all file and folder names on configured NTFS volumes
- [ ] **INDX-02**: Index all file and folder names on configured FAT32/exFAT volumes
- [ ] **INDX-03**: Store file metadata: size, modified date
- [ ] **INDX-04**: Persist index to SQLite with WAL mode (survives reboot)
- [ ] **INDX-05**: Fast initial index build (~1 second per 100k files on NTFS)

### Real-time Updates

- [ ] **UPDT-01**: NTFS volumes update in near-real-time via USN Change Journal
- [ ] **UPDT-02**: FAT32/exFAT volumes update via directory watchers + periodic reconciliation
- [ ] **UPDT-03**: Detect and recover from USN Journal wrap (missed changes)
- [ ] **UPDT-04**: Handle volume mount/unmount gracefully (refresh on mount, mark offline on dismount)

### Configuration

- [ ] **CONF-01**: User can select which volumes to index
- [ ] **CONF-02**: User can define exclude patterns (paths, extensions)
- [ ] **CONF-03**: Configuration persists across service restarts

### Search UI

- [ ] **SRUI-01**: Global hotkey opens search popup instantly
- [ ] **SRUI-02**: Search-as-you-type with results in <50ms for 1M+ entries
- [ ] **SRUI-03**: Keyboard navigation through results (Up/Down/Enter/Esc)
- [ ] **SRUI-04**: Dark mode that matches system theme
- [ ] **SRUI-05**: Search history dropdown (recent searches)

### Result Display

- [ ] **DISP-01**: Show filename prominently
- [ ] **DISP-02**: Show full path (parent directory)
- [ ] **DISP-03**: Show file size in human-readable format (KB, MB, GB)
- [ ] **DISP-04**: Show modified date
- [ ] **DISP-05**: Column sorting (click to sort by name, size, date, path)

### Result Actions

- [ ] **ACTN-01**: Open file with default application (Enter)
- [ ] **ACTN-02**: Open containing folder in Explorer (with file selected)
- [ ] **ACTN-03**: Copy full path to clipboard

### Search Syntax

- [ ] **SYNT-01**: Case-insensitive search by default
- [ ] **SYNT-02**: Wildcard support (* matches any characters, ? matches single character)
- [ ] **SYNT-03**: Extension filter (ext:pdf, ext:docx)
- [ ] **SYNT-04**: Size filter (size:>10mb, size:<1kb)
- [ ] **SYNT-05**: Type filter (type:folder, type:file)
- [ ] **SYNT-06**: Date filter (modified:today, modified:lastweek, modified:>2024-01-01)
- [ ] **SYNT-07**: Path scoping (path:C:\Projects\ limits search to that subtree)

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Enhanced Actions

- **ACTN-04**: Multiple keyboard shortcuts (Ctrl+Enter for folder, Ctrl+C for path, etc.)

### Advanced Search

- **SYNT-08**: Boolean operators (AND, OR, NOT)
- **SYNT-09**: Regex support (toggle on/off)
- **SYNT-10**: Fuzzy matching (forgive typos)

### UI Enhancements

- **SRUI-06**: Bookmarked/saved searches
- **SRUI-07**: Preview pane for files
- **SRUI-08**: Portable mode (run from USB, no installation)

### Extended Features

- **INDX-06**: Network share indexing
- **UPDT-05**: Configurable reconciliation cadence per volume

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Content indexing | Massively increases complexity and scope; filename search is the core value |
| Web search integration | Users want local file search, not Bing results |
| AI/Copilot features | Adds complexity without solving file finding; users hate forced AI |
| Cloud storage indexing | OneDrive sync issues are complex; local files that sync work automatically |
| Telemetry/data collection | Privacy-conscious users value offline-only operation |
| Per-user ACL filtering | Simple model chosen — all local users see full index |
| Full file attributes | Only size and mtime indexed; attributes add bloat without proportional value |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SERV-01 | Phase 1 | Pending |
| SERV-02 | Phase 1 | Pending |
| SERV-03 | Phase 1 | Pending |
| INDX-01 | Phase 1 | Pending |
| INDX-02 | Phase 1 | Pending |
| INDX-03 | Phase 1 | Pending |
| INDX-04 | Phase 1 | Pending |
| INDX-05 | Phase 1 | Pending |
| UPDT-01 | Phase 2 | Pending |
| UPDT-02 | Phase 2 | Pending |
| UPDT-03 | Phase 2 | Pending |
| UPDT-04 | Phase 2 | Pending |
| CONF-01 | Phase 2 | Pending |
| CONF-02 | Phase 2 | Pending |
| CONF-03 | Phase 2 | Pending |
| SRUI-01 | Phase 3 | Pending |
| SRUI-02 | Phase 3 | Pending |
| SRUI-03 | Phase 3 | Pending |
| SRUI-04 | Phase 3 | Pending |
| SRUI-05 | Phase 3 | Pending |
| DISP-01 | Phase 3 | Pending |
| DISP-02 | Phase 3 | Pending |
| DISP-03 | Phase 3 | Pending |
| DISP-04 | Phase 3 | Pending |
| DISP-05 | Phase 3 | Pending |
| ACTN-01 | Phase 3 | Pending |
| ACTN-02 | Phase 3 | Pending |
| ACTN-03 | Phase 3 | Pending |
| SYNT-01 | Phase 3 | Pending |
| SYNT-02 | Phase 3 | Pending |
| SYNT-03 | Phase 3 | Pending |
| SYNT-04 | Phase 3 | Pending |
| SYNT-05 | Phase 3 | Pending |
| SYNT-06 | Phase 3 | Pending |
| SYNT-07 | Phase 3 | Pending |

**Coverage:**
- v1 requirements: 32 total
- Mapped to phases: 32
- Unmapped: 0

---
*Requirements defined: 2026-01-24*
*Last updated: 2026-01-24 after roadmap creation*

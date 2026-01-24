# Roadmap: FastFileIndex (FFI)

## Overview

FFI delivers instant file/folder name lookups via three phases: build the index foundation (Windows service with SQLite persistence), keep it fresh (NTFS USN Journal + FAT watchers for real-time updates), and let users search it (global hotkey popup with filters and keyboard navigation). Each phase delivers complete, verifiable functionality.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [x] **Phase 1: Foundation** - Windows service with persistent file index
- [ ] **Phase 2: Real-time Updates** - NTFS USN Journal and FAT volume monitoring
- [ ] **Phase 3: Search Experience** - Global hotkey popup with filters and actions

## Phase Details

### Phase 1: Foundation
**Goal**: Windows service builds and persists a complete file index across configured volumes
**Depends on**: Nothing (first phase)
**Requirements**: SERV-01, SERV-02, SERV-03, INDX-01, INDX-02, INDX-03, INDX-04, INDX-05
**Success Criteria** (what must be TRUE):
  1. Service starts automatically at Windows boot and appears in Services console
  2. Service survives abrupt shutdown (kill process) without corrupting the database
  3. User can configure which volumes to index and service indexes all files/folders on those volumes
  4. Index persists across reboots (restart service, data still present)
  5. Initial indexing completes at ~1 second per 100k files on NTFS volumes
**Plans**: 3 plans

Plans:
- [x] 01-01: Windows service scaffolding with lifecycle management
- [x] 01-02: SQLite database layer with WAL mode and optimized schema
- [x] 01-03: MFT reader for NTFS + directory enumeration for FAT initial indexing

### Phase 2: Real-time Updates
**Goal**: Index stays current automatically via filesystem monitoring
**Depends on**: Phase 1
**Requirements**: UPDT-01, UPDT-02, UPDT-03, UPDT-04, CONF-01, CONF-02, CONF-03
**Success Criteria** (what must be TRUE):
  1. File created/renamed/deleted on NTFS volume appears in index within 2 seconds
  2. File changes on FAT32/exFAT volumes detected via watchers and periodic reconciliation
  3. Service recovers gracefully from USN Journal wrap (detects and triggers rescan)
  4. Service handles volume mount/unmount (refreshes on mount, marks offline on dismount)
  5. User can configure volumes and exclude patterns via configuration file
**Plans**: 2 plans

Plans:
- [ ] 02-01: USN Journal monitoring with wrap detection, configuration, and adaptive throttling
- [ ] 02-02: FAT periodic reconciliation and volume mount/unmount lifecycle management

### Phase 3: Search Experience
**Goal**: User can instantly search and act on files via global hotkey popup
**Depends on**: Phase 2
**Requirements**: SRUI-01, SRUI-02, SRUI-03, SRUI-04, SRUI-05, DISP-01, DISP-02, DISP-03, DISP-04, DISP-05, ACTN-01, ACTN-02, ACTN-03, SYNT-01, SYNT-02, SYNT-03, SYNT-04, SYNT-05, SYNT-06, SYNT-07
**Success Criteria** (what must be TRUE):
  1. Global hotkey opens search popup instantly from any application
  2. Typing shows results in <50ms with 1M+ indexed files
  3. User navigates results with keyboard (Up/Down/Enter/Esc) and opens files
  4. Results show filename, path, size, and modified date with column sorting
  5. Search supports wildcards, extension filter, size filter, type filter, date filter, and path scoping
**Plans**: 3 plans

Plans:
- [ ] 03-01: IPC layer (named pipes) for client-service communication
- [ ] 03-02: Search UI with global hotkey and keyboard navigation
- [ ] 03-03: Search syntax parser and filter implementation

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 (insert decimal phases as needed)

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation | 3/3 | Complete | 2026-01-24 |
| 2. Real-time Updates | 0/2 | Not started | - |
| 3. Search Experience | 0/3 | Not started | - |

---
*Roadmap created: 2026-01-24*
*Depth: quick (3 phases, 8 plans)*

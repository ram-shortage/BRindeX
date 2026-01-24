# FastFileIndex (FFI)

## What This Is

A Windows service that builds and maintains a persistent, high-performance index of file and folder names across NTFS and FAT-family volumes. Users invoke it via a global hotkey that opens a minimal search popup — type a query, see instant results, and open files, navigate to folders, or copy paths with keyboard shortcuts.

## Core Value

Instant file/folder name lookups that actually work — no flakiness, no waiting, no stale results.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Index all file and folder names across configured volumes (NTFS + FAT/FAT32/exFAT)
- [ ] Persist index across reboots (SQLite with WAL)
- [ ] NTFS: Use USN Change Journal for near-real-time incremental updates
- [ ] FAT-family: Directory watchers + periodic reconciliation (balanced 15-60 min cadence)
- [ ] Validation & self-healing to detect drift and repair automatically
- [ ] System-wide service with simple access model (all local users can query)
- [ ] Global hotkey opens search popup
- [ ] Type-ahead search with prefix and substring matching
- [ ] Results display: filename, path, size
- [ ] Keyboard actions: open file, open containing folder, copy path
- [ ] Minimal functional UI (not polished, just works)

### Out of Scope

- Full-text indexing of file contents — name search only
- Cloud sync / cross-device index — local only
- Per-user access filtering / ACL-based result filtering — simple model, everyone sees everything
- Advanced semantic classification — just names and basic metadata
- Polished UI / visual design — functional is enough for v1
- Attributes metadata — only size and mtime indexed

## Context

**Motivation:** Windows Search is unreliable and flaky. Everything is fast but dated and may not be the right tool. The goal is instant, reliable file lookup via hotkey — similar to Spotlight/Alfred experience but for Windows file search.

**Reference PRD:** See `PRD.md` in repo root for detailed technical specification including filesystem-specific update strategies, data model, architecture components, and acceptance criteria.

**Target user:** Power user / developer who needs instant "find by name" across drives.

## Constraints

- **Platform**: Windows only (uses NTFS USN Change Journal, Windows directory notifications)
- **Storage**: SQLite with WAL for simplicity and crash safety (reassess if performance issues arise)
- **Metadata**: Index size + mtime only (not full attributes) to balance speed vs utility
- **FAT reconciliation**: Balanced cadence (15-60 min idle-time scans) — eventual consistency acceptable

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| System-wide service (not per-user) | Simpler deployment, one shared index, admin installs and all users query | — Pending |
| SQLite-only storage | Start simple, add auxiliary indexes (trie/n-gram) only if perf requires | — Pending |
| Balanced FAT reconciliation | 15-60 min idle-time cadence balances freshness vs resource usage | — Pending |
| Size + mtime metadata only | Enough for useful result display without bloating index | — Pending |
| Simple access model | No ACL filtering — all local users see full index | — Pending |

---
*Last updated: 2026-01-24 after initialization*

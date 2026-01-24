# Feature Landscape: Windows File Indexing Service

**Domain:** Desktop file search / file indexing service for Windows
**Researched:** 2026-01-24
**Confidence:** HIGH (based on analysis of Everything, Listary, WizFile, Flow Launcher, Windows Search)

---

## Table Stakes

Features users expect. Missing = product feels incomplete or users immediately leave.

| Feature | Why Expected | Complexity | Dependencies | Notes |
|---------|--------------|------------|--------------|-------|
| **Instant search-as-you-type** | Everything/Listary set the bar; users expect results in <50ms | Medium | Index infrastructure | Core promise of the product; without this, no reason to exist over Windows Search |
| **Real-time index updates (NTFS)** | Everything does this seamlessly via USN Journal | High | USN Change Journal integration | Users expect file changes to appear in results immediately, not after manual rescans |
| **Global hotkey to summon UI** | All competitors have this (Ctrl+Ctrl, Alt+Space, etc.) | Low | None | Without this, product is just another app to Alt+Tab to |
| **Result actions: Open file** | Basic expectation - click/Enter opens the file | Low | None | Use default application association |
| **Result actions: Open containing folder** | "Open Path" is heavily used in Everything | Low | None | Opens Explorer with file selected |
| **Result actions: Copy path to clipboard** | Common workflow need, especially for developers | Low | None | Should copy full absolute path |
| **File/folder name display** | Basic result information | Low | None | Must be clear, readable, not truncated |
| **Full path display** | Users need to see where files are | Low | None | Show parent path, allow copy |
| **File size display** | Helps distinguish files with same name | Low | Index stores size | Human-readable format (KB, MB, GB) |
| **Keyboard navigation** | Up/Down through results, Enter to open | Low | None | Power users never touch mouse |
| **Minimal resource usage when idle** | Everything uses ~15MB RAM; Windows Search uses 300MB+ | Medium | Efficient index design | Users hate background CPU/memory hogs |
| **Fast initial indexing** | Everything indexes 120k files in ~1 second | High | MFT reading for NTFS | If initial setup takes hours, users abandon |
| **Wildcard search (*, ?)** | Standard expectation from Everything users | Low | Search parser | `*.pdf`, `report*.docx`, etc. |
| **Case-insensitive search by default** | Standard behavior; case-sensitivity as option | Low | Search parser | Match user mental model |

## Differentiators

Features that set product apart. Not universally expected, but valued by target users.

| Feature | Value Proposition | Complexity | Dependencies | Notes |
|---------|-------------------|------------|--------------|-------|
| **FAT32/exFAT volume support** | Everything struggles with non-NTFS; FFI treats this as first-class | High | Directory watchers, periodic reconciliation | WizFile added this recently; still a gap for many tools |
| **Reliable file system watching** | Windows Search is notoriously unreliable; "it just works" is differentiation | High | Robust watcher implementation | Everything's reliability is its killer feature |
| **Search filters (type:, ext:, size:)** | Power users love targeted searches | Medium | Filter parser | `ext:pdf`, `size:>10mb`, etc. |
| **Fuzzy matching** | Listary's fuzzy matching predicts desired result | Medium | Fuzzy search algorithm | Forgives typos, partial matches |
| **Recent/frequent files ranking** | Listary learns usage patterns | Medium | Usage tracking | Show frequently accessed files higher |
| **Dark mode** | Everything 1.5 added this; modern expectation | Low | UI theming | Match system theme or allow toggle |
| **Date filters (modified:, created:)** | Find files from specific time periods | Medium | Date parsing, index dates | `modified:today`, `modified:lastweek` |
| **Exclude patterns/folders** | Don't index node_modules, .git, etc. | Low | Configuration | Critical for developers |
| **Search history** | Quick access to recent searches | Low | Local storage | Dropdown or hotkey to recall |
| **Bookmarked searches** | Save frequently used searches | Low | Local storage | Everything has this as "bookmarks" |
| **Portable mode** | Run from USB, no installation | Low | Self-contained config | Everything Portable is popular |
| **Preview pane** | Quick look at file contents | Medium | File type handlers | Images, text files, PDFs |
| **Regex support** | Power user feature for complex patterns | Medium | Regex engine | Everything has this; toggle on/off |
| **Boolean operators (AND, OR, NOT)** | Combine search terms | Low | Search parser | Everything uses space/pipe/exclamation |
| **Column sorting (name, size, date)** | Organize results differently | Low | UI | Click column headers to sort |
| **Path-scoped search** | Limit search to specific folder tree | Low | Search parser | `path:C:\Projects\ myfile` |
| **Quick Switch integration** | Jump to folder in Save/Open dialogs | High | Windows hook integration | Listary's killer feature |
| **Export results** | Save search results to CSV/TXT | Low | Export function | Everything has this |
| **Multiple result actions via keyboard** | Ctrl+Enter, Ctrl+C, Ctrl+Shift+C, etc. | Low | Keybinding config | Power users want keyboard shortcuts for everything |

## Anti-Features

Features to explicitly NOT build. Common mistakes in this domain.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Content indexing** | Massively increases complexity, index size, and resource usage. Changes the product scope entirely. | Stick to filename/path indexing. Content search is a different product (DocFetcher, Windows Search). If needed, offer slow on-demand content search like Everything's `content:` |
| **Web search integration** | Bing integration in Windows Search is universally hated; users want local file search, not web results | Keep focus on local files. If web search wanted, users use browser |
| **AI/Copilot features** | Users explicitly say "stop shoving AI down our throats" about Windows 11 | Solve the core problem well. AI adds complexity without solving file finding |
| **Upsell/ads** | Everything is beloved because it's completely free with no nags | If monetizing, be upfront (Listary Pro model works: free tier is generous) |
| **Registry data indexing** | Out of scope; different use case | Stay focused on file/folder search |
| **Cloud storage indexing** | OneDrive sync issues plague Windows Search; complex edge cases | Index local filesystem only. Cloud folders that sync locally work automatically |
| **Automatic system integration without permission** | Shell extensions, Explorer integration should be opt-in | Ask before modifying system; portable-first mentality |
| **Heavy startup/background services** | Windows Search's high resource usage is a primary complaint | Service should be lightweight; users notice CPU/memory usage |
| **Telemetry/data collection** | Privacy-conscious users choose Everything because it's offline-only | Process everything locally; don't phone home |
| **Complex installation requirements** | Users want simple, fast setup | Minimize prerequisites; portable option for zero-install |

## Feature Dependencies

```
Core Index Infrastructure
    |
    +-- NTFS USN Journal Integration
    |       |
    |       +-- Real-time updates
    |
    +-- FAT/exFAT Directory Watcher
    |       |
    |       +-- Periodic reconciliation
    |
    +-- SQLite + WAL Storage
            |
            +-- All search features

Search Parser
    |
    +-- Wildcards (*, ?)
    +-- Filters (type:, ext:, size:, date:)
    +-- Boolean operators
    +-- Regex (optional, toggle)
    +-- Path scoping

UI Layer
    |
    +-- Global hotkey
    +-- Results list
    |       |
    |       +-- Column sorting
    |       +-- Keyboard navigation
    |
    +-- Result actions
    |       |
    |       +-- Open file
    |       +-- Open folder
    |       +-- Copy path
    |
    +-- Dark mode
    +-- Search history
```

## MVP Recommendation

For MVP, prioritize all table stakes plus select differentiators:

### Phase 1: Core (Must ship with these)
1. **Instant search** - The core promise
2. **Real-time NTFS updates** - USN Journal integration
3. **Global hotkey + minimal UI** - Launcher UX
4. **Basic result actions** - Open file, open folder, copy path
5. **Minimal resource usage** - Service architecture done right

### Phase 2: Essential Differentiators
6. **FAT32/exFAT support** - Your stated differentiator
7. **Wildcard search** - Expected by power users
8. **Exclude patterns** - Critical for developer workflows
9. **Keyboard-first navigation** - Power user essential

### Phase 3: Quality of Life
10. **Dark mode** - Modern expectation
11. **Filters (ext:, size:)** - Power features
12. **Search history** - Nice to have
13. **Column sorting** - Nice to have

### Defer to Post-MVP
- **Fuzzy matching**: Nice but adds complexity to search
- **Preview pane**: Significant UI complexity
- **Quick Switch integration**: Requires deep Windows hooks
- **Regex support**: Niche power user feature
- **Export results**: Low priority
- **Network share indexing**: Complex, different use case
- **Portable mode**: Configuration complexity

## Complexity Summary

| Complexity | Features |
|------------|----------|
| **Low** | Hotkey, basic actions, copy path, keyboard nav, wildcards, exclude patterns, dark mode, history, bookmarks, export, column sorting |
| **Medium** | Search parser with filters, fuzzy matching, date parsing, preview pane, regex, efficient index design |
| **High** | USN Journal integration, FAT watcher with reconciliation, Fast MFT reading, Quick Switch hooks, reliable watching |

## Sources

- [voidtools Everything documentation](https://www.voidtools.com/support/everything/)
- [Everything search features](https://www.voidtools.com/support/everything/searching/)
- [Listary features](https://www.listary.com/)
- [WizFile](https://antibody-software.com/wizfile/)
- [Flow Launcher](https://www.flowlauncher.com/)
- [PowerToys Run documentation](https://learn.microsoft.com/en-us/windows/powertoys/run)
- [Windows Search frustrations - ResetEra](https://www.resetera.com/threads/how-can-windows-search-still-be-so-goddamn-bad.30749/)
- [USN Journal documentation](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals)
- [XDA: Windows Search vs Everything](https://www.xda-developers.com/reasons-stop-using-windows-search-use-everything-instead/)

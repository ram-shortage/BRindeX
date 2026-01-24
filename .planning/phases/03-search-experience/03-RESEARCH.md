# Phase 3: Search Experience - Research

**Researched:** 2026-01-24
**Domain:** Windows desktop search UI with IPC, global hotkeys, and search syntax parsing
**Confidence:** HIGH

## Summary

Phase 3 implements the user-facing search experience for FastFileIndex. This involves three key technical domains: (1) IPC via Windows named pipes for service-client communication, (2) an egui-based search popup with global hotkey activation, and (3) a search syntax parser for filters like `ext:pdf`, `size:>10mb`, and `modified:today`.

The Rust ecosystem has mature solutions for all three areas. Named pipes are well-supported in Tokio (built-in) and the interprocess crate. egui 0.33+ provides immediate-mode UI suitable for fast popup windows. For hotkeys, the win-hotkeys crate (0.5.1) offers thread-safe Windows-native global hotkey registration. Search syntax parsing is best handled with pest for its grammar-first approach to DSLs.

**Primary recommendation:** Use Tokio's built-in `tokio::net::windows::named_pipe` for IPC, egui/eframe for the borderless popup UI, win-hotkeys for global hotkey capture, and pest for parsing the search filter syntax.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| **tokio** | 1.43+ | Async runtime + named pipes | Built-in Windows named pipe support via `tokio::net::windows::named_pipe`. Already in project. |
| **egui** | 0.33+ | Immediate-mode GUI | Fast startup, simple state management for search-as-you-type. Already chosen for project. |
| **eframe** | 0.33+ | egui native wrapper | Handles window creation, event loop. Supports borderless windows. |
| **win-hotkeys** | 0.5+ | Global hotkey capture | Windows-specific, thread-safe, supports WIN key, uses WH_KEYBOARD_LL hook. |
| **pest** | 2.8+ | Grammar-based parser | PEG parser generator ideal for search syntax DSL. Declarative grammar files. |
| **serde_json** | 1.0+ | IPC message format | JSON serialization for named pipe messages. Simple, debuggable. |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| **opener** | 0.8+ | Open files/reveal in Explorer | ACTN-01 (open file), ACTN-02 (reveal in folder) |
| **arboard** | 3.0+ | Clipboard operations | ACTN-03 (copy path to clipboard) |
| **chrono** | 0.4+ | Date/time handling | SYNT-06 date filter parsing and comparison |
| **pest_derive** | 2.8+ | pest grammar derive macros | Code generation from .pest grammar files |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| **win-hotkeys** | global-hotkey (Tauri team) | Cross-platform but requires win32 event loop setup; win-hotkeys is simpler for Windows-only |
| **tokio named pipes** | interprocess crate | interprocess (2.2.3) provides local_socket abstraction but adds dependency; tokio built-in is sufficient |
| **pest** | nom | nom is faster but requires procedural combinator code; pest's grammar files are more maintainable for DSLs |
| **arboard** | copypasta | Both work; arboard has better Windows support and 1Password backing |

**Installation:**
```bash
# Add to Cargo.toml
cargo add tokio --features net
cargo add egui eframe
cargo add win-hotkeys
cargo add pest pest_derive
cargo add serde_json
cargo add opener arboard chrono
```

## Architecture Patterns

### Recommended Project Structure

```
src/
├── bin/
│   ├── ffi-service.rs       # Service entry point (existing)
│   └── ffi-search.rs        # NEW: Search UI entry point
├── ipc/
│   ├── mod.rs               # IPC module
│   ├── protocol.rs          # Message types (SearchRequest, SearchResponse)
│   ├── server.rs            # Named pipe server (runs in service)
│   └── client.rs            # Named pipe client (used by UI)
├── search/
│   ├── mod.rs               # Search module
│   ├── parser.rs            # pest-based syntax parser
│   ├── query.rs             # Search query builder (SQL generation)
│   └── filters.rs           # Filter types and matching
├── ui/
│   ├── mod.rs               # UI module
│   ├── app.rs               # eframe App implementation
│   ├── hotkey.rs            # Global hotkey handler
│   ├── results.rs           # Results list widget
│   └── actions.rs           # File actions (open, reveal, copy)
└── ...
```

### Pattern 1: Named Pipe Server/Client

**What:** Service runs a named pipe server at `\\.\pipe\FFI_Search`, UI connects as client.
**When to use:** All search queries from UI to service.

**Server (in service):**
```rust
// Source: https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/
use tokio::net::windows::named_pipe::{ServerOptions, NamedPipeServer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PIPE_NAME: &str = r"\\.\pipe\FFI_Search";

async fn run_pipe_server(db: Arc<Database>) -> Result<()> {
    loop {
        // Create new server instance
        let server = ServerOptions::new()
            .first_pipe_instance(false)
            .create(PIPE_NAME)?;

        // Wait for client connection
        server.connect().await?;

        // Spawn handler for this client
        let db_clone = db.clone();
        tokio::spawn(async move {
            handle_client(server, db_clone).await
        });
    }
}

async fn handle_client(mut pipe: NamedPipeServer, db: Arc<Database>) -> Result<()> {
    let mut buf = vec![0u8; 4096];
    loop {
        let n = pipe.read(&mut buf).await?;
        if n == 0 { break; }

        let request: SearchRequest = serde_json::from_slice(&buf[..n])?;
        let results = execute_search(&db, &request).await?;
        let response = serde_json::to_vec(&SearchResponse { results })?;
        pipe.write_all(&response).await?;
    }
    Ok(())
}
```

**Client (in UI):**
```rust
use tokio::net::windows::named_pipe::ClientOptions;

async fn search(query: &str) -> Result<Vec<FileResult>> {
    let mut client = ClientOptions::new().open(PIPE_NAME)?;

    let request = SearchRequest { query: query.to_string(), limit: 100 };
    let request_bytes = serde_json::to_vec(&request)?;
    client.write_all(&request_bytes).await?;

    let mut buf = vec![0u8; 65536];
    let n = client.read(&mut buf).await?;
    let response: SearchResponse = serde_json::from_slice(&buf[..n])?;
    Ok(response.results)
}
```

### Pattern 2: Borderless Always-On-Top Popup

**What:** egui window with no decorations, always on top, transparent background.
**When to use:** The search popup window.

```rust
// Source: https://docs.rs/eframe/latest/eframe/
use eframe::{NativeOptions, egui};

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 400.0])
            .with_decorations(false)          // No title bar
            .with_always_on_top()             // Stay above other windows
            .with_transparent(true)           // Transparent background
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "FFI Search",
        options,
        Box::new(|cc| Box::new(SearchApp::new(cc))),
    )
}
```

### Pattern 3: Global Hotkey with Window Toggle

**What:** Register a global hotkey that shows/hides the search window.
**When to use:** SRUI-01 requirement.

```rust
// Source: https://github.com/iholston/win-hotkeys
use win_hotkeys::{HotkeyManager, VKey, ModKey};
use std::sync::mpsc;

fn setup_hotkey(tx: mpsc::Sender<HotkeyAction>) {
    let mut hkm = HotkeyManager::new();

    // Register Ctrl+Space as global hotkey
    hkm.register_hotkey(
        VKey::Space,
        &[ModKey::Ctrl],
        move || {
            tx.send(HotkeyAction::ToggleWindow).unwrap();
        }
    ).expect("Failed to register hotkey");

    // Start listening (blocking - run in separate thread)
    hkm.event_loop();
}
```

### Pattern 4: Keyboard Navigation in Results List

**What:** Custom keyboard handling for Up/Down/Enter/Esc navigation.
**When to use:** SRUI-03 requirement.

```rust
// Manual keyboard navigation since egui doesn't have built-in list navigation
impl SearchApp {
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                self.selected_index = (self.selected_index + 1).min(self.results.len().saturating_sub(1));
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            if i.key_pressed(egui::Key::Enter) {
                if let Some(result) = self.results.get(self.selected_index) {
                    self.open_file(&result.path);
                }
            }
            if i.key_pressed(egui::Key::Escape) {
                self.hide_window = true;
            }
        });
    }
}
```

### Pattern 5: Search Syntax Grammar (pest)

**What:** Define search syntax as a PEG grammar.
**When to use:** SYNT-01 through SYNT-07 requirements.

```pest
// search.pest
WHITESPACE = _{ " " | "\t" }

query = { SOI ~ term* ~ EOI }
term = { filter | word }

filter = { filter_type ~ ":" ~ filter_value }
filter_type = { "ext" | "size" | "type" | "modified" | "path" }
filter_value = { quoted_string | comparison | word }

comparison = { comparator ~ (size_value | date_value) }
comparator = { ">=" | "<=" | ">" | "<" }
size_value = { number ~ size_unit? }
size_unit = { "kb" | "mb" | "gb" | "KB" | "MB" | "GB" }
date_value = { iso_date | relative_date }
iso_date = { ASCII_DIGIT{4} ~ "-" ~ ASCII_DIGIT{2} ~ "-" ~ ASCII_DIGIT{2} }
relative_date = { "today" | "yesterday" | "lastweek" | "lastmonth" }

word = { (wildcard | char)+ }
wildcard = { "*" | "?" }
char = { !(":" | " " | "\"") ~ ANY }
quoted_string = { "\"" ~ inner ~ "\"" }
inner = { (!("\"") ~ ANY)* }
number = { ASCII_DIGIT+ }
```

### Anti-Patterns to Avoid

- **Synchronous IPC in UI thread:** Always use async/await or spawn search queries on background thread. Blocking UI causes "not responding" state.
- **Polling for hotkey events:** Use the callback-based win-hotkeys API, not a polling loop.
- **Rebuilding entire UI each frame for large result sets:** Use egui's `ScrollArea::show_rows()` for virtual list rendering.
- **Substring search with leading wildcard on SQLite index:** `%query` cannot use the name index. Consider FTS5 trigram for this case.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Named pipe IPC | Raw Windows API calls | tokio::net::windows::named_pipe | Async-aware, handles connection lifecycle, tested |
| Global hotkey registration | Direct RegisterHotKey calls | win-hotkeys crate | Thread-safe, callback-based, handles message loop |
| Search syntax parsing | Regex or manual string parsing | pest grammar | Maintainable grammar file, automatic error messages |
| Clipboard operations | Windows clipboard API | arboard crate | Cross-platform abstraction, handles edge cases |
| File opening/reveal | ShellExecuteW direct | opener crate | Cross-platform, handles paths with spaces |
| Date parsing | Manual string parsing | chrono crate | Handles timezones, ISO formats, relative dates |

**Key insight:** The Rust ecosystem has mature, tested solutions for all these Windows-specific operations. Hand-rolling them leads to edge case bugs (paths with spaces, Unicode, concurrent access).

## Common Pitfalls

### Pitfall 1: Named Pipe Server Only Creates One Instance

**What goes wrong:** Only one client can connect at a time, subsequent connections fail.
**Why it happens:** Named pipe server must create a new instance before or immediately after accepting connection.
**How to avoid:** Use the loop pattern shown above - create new server, await connection, spawn handler, repeat.
**Warning signs:** "Access denied" or "Pipe busy" errors from client.

### Pitfall 2: egui Window Visibility Toggle Bug on Windows

**What goes wrong:** Window cannot be shown again after being hidden with `set_visible(false)`.
**Why it happens:** Windows stops sending repaint events to invisible windows, preventing viewport command processing.
**How to avoid:** Instead of hiding, minimize to tray or move window off-screen. Or use a process-based approach where hotkey spawns new window process.
**Warning signs:** Window never reappears after hiding.

### Pitfall 3: Hotkey Registration Fails Silently

**What goes wrong:** Hotkey doesn't trigger even though registration "succeeded".
**Why it happens:** Another application already registered that hotkey combination.
**How to avoid:** Check registration result, provide fallback hotkey options, allow user configuration.
**Warning signs:** No callback invocation despite hotkey press.

### Pitfall 4: SQLite LIKE with Leading Wildcard is Slow

**What goes wrong:** Search for `%test%` scans entire table, taking seconds with 1M+ files.
**Why it happens:** Index on name column only works for prefix searches, not substring.
**How to avoid:** Use FTS5 with trigram tokenizer for substring search. The trigram index supports `LIKE '%test%'` efficiently.
**Warning signs:** Search latency increases linearly with database size.

### Pitfall 5: UI Blocks During Search

**What goes wrong:** Typing feels laggy, UI freezes during search.
**Why it happens:** IPC or database query running on UI thread.
**How to avoid:** Run search in background task, update results via channel. Cancel previous search on new keystroke.
**Warning signs:** Input lag > 50ms, "not responding" in title bar.

### Pitfall 6: Results Flicker During Typing

**What goes wrong:** Results list blinks or jumps as user types.
**Why it happens:** Each keystroke triggers full results replacement.
**How to avoid:** Debounce input (50-100ms), only update results when stable. Show loading indicator during search.
**Warning signs:** Visual flashing, scroll position resets.

## Code Examples

### IPC Message Protocol

```rust
// Source: Custom design based on serde patterns
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SearchResponse {
    pub results: Vec<FileResult>,
    pub total_count: usize,
    pub search_time_ms: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileResult {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub modified: i64,
    pub is_dir: bool,
}
```

### Search Query Builder

```rust
// Build SQL from parsed query
pub fn build_query(parsed: &ParsedQuery) -> (String, Vec<SqlValue>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    // Name/pattern search
    if let Some(pattern) = &parsed.pattern {
        // Convert wildcards: * -> %, ? -> _
        let sql_pattern = pattern
            .replace('*', "%")
            .replace('?', "_");
        conditions.push("name LIKE ? ESCAPE '\\'");
        params.push(SqlValue::Text(sql_pattern));
    }

    // Extension filter
    if let Some(ext) = &parsed.extension {
        conditions.push("name LIKE ?");
        params.push(SqlValue::Text(format!("%.{}", ext)));
    }

    // Size filter
    if let Some((op, bytes)) = &parsed.size_filter {
        conditions.push(&format!("size {} ?", op.to_sql()));
        params.push(SqlValue::Integer(*bytes));
    }

    // Type filter
    if let Some(file_type) = &parsed.type_filter {
        let is_dir = match file_type {
            FileType::Folder => 1,
            FileType::File => 0,
        };
        conditions.push("is_dir = ?");
        params.push(SqlValue::Integer(is_dir));
    }

    // Date filter
    if let Some((op, timestamp)) = &parsed.date_filter {
        conditions.push(&format!("modified {} ?", op.to_sql()));
        params.push(SqlValue::Integer(*timestamp));
    }

    // Path scope
    if let Some(path_prefix) = &parsed.path_scope {
        // This requires path reconstruction - see note below
        conditions.push("/* path scope handled in post-filter */");
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, name, file_ref, volume_id, size, modified, is_dir
         FROM files {} LIMIT ? OFFSET ?",
        where_clause
    );

    (sql, params)
}
```

### File Actions

```rust
// Source: https://docs.rs/opener/latest/opener/
use std::path::Path;

pub fn open_file(path: &Path) -> Result<()> {
    opener::open(path)?;
    Ok(())
}

pub fn reveal_in_explorer(path: &Path) -> Result<()> {
    opener::reveal(path)?;
    Ok(())
}

// Source: https://docs.rs/arboard/latest/arboard/
pub fn copy_path_to_clipboard(path: &Path) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(path.to_string_lossy().to_string())?;
    Ok(())
}
```

### System Theme Detection

```rust
// Source: https://docs.rs/egui/latest/egui/
impl eframe::App for SearchApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Follow system theme (SRUI-04)
        ctx.set_theme(egui::Theme::System);

        // Or detect and use specific theme
        let theme = ctx.options(|opt| opt.theme_preference);
        match theme {
            egui::ThemePreference::Dark => { /* dark mode styling */ }
            egui::ThemePreference::Light => { /* light mode styling */ }
            egui::ThemePreference::System => { /* follow system */ }
        }
    }
}
```

### Efficient Virtual List for Results

```rust
// Source: https://docs.rs/egui/latest/egui/containers/scroll_area/
fn show_results(&mut self, ui: &mut egui::Ui) {
    let row_height = 24.0;
    let total_rows = self.results.len();

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show_rows(ui, row_height, total_rows, |ui, row_range| {
            for idx in row_range {
                let result = &self.results[idx];
                let is_selected = idx == self.selected_index;

                let response = ui.selectable_label(
                    is_selected,
                    format!("{} - {}", result.name, result.path)
                );

                if response.clicked() {
                    self.selected_index = idx;
                }
                if response.double_clicked() {
                    self.open_file(&result.path);
                }

                // Scroll selected item into view
                if is_selected {
                    response.scroll_to_me(Some(egui::Align::Center));
                }
            }
        });
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `RegisterHotKey` + message loop | win-hotkeys callback API | 2024 | Cleaner Rust integration, no raw WinAPI |
| Custom named pipe wrapper | tokio::net::windows::named_pipe | tokio 1.x | Built-in async support |
| egui 0.28 theming | egui 0.32+ ThemePreference | 2024 | Native system theme detection |
| Manual SQL string building | Query builder with prepared statements | Always | SQL injection prevention |

**Deprecated/outdated:**
- **winapi crate for hotkeys:** Use `windows` crate (Microsoft official) or `win-hotkeys`
- **egui 0.28 and earlier:** Theme detection improvements in 0.32+
- **SQLite LIKE without FTS5:** For substring search with 1M+ files, use FTS5 trigram

## Open Questions

1. **Path scope filtering performance**
   - What we know: Files table stores parent_ref, not full path. Path reconstruction is expensive.
   - What's unclear: Best strategy for path-based filtering at scale.
   - Recommendation: Either (a) store computed full path in a separate column with index, (b) use recursive CTE for path filtering, or (c) post-filter results in memory after initial search. Option (a) is fastest but requires schema change.

2. **Window show/hide lifecycle**
   - What we know: Windows visibility bug affects egui viewport commands.
   - What's unclear: Whether minimize-to-tray avoids the issue.
   - Recommendation: Test both approaches during implementation. If issues persist, consider spawning a new process for each search session.

3. **Search history storage**
   - What we know: SRUI-05 requires recent searches dropdown.
   - What's unclear: Where to persist (service DB, client-side file, registry).
   - Recommendation: Store in service DB (new table) so history persists across UI restarts.

## Sources

### Primary (HIGH confidence)
- [tokio named_pipe documentation](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/) - Named pipe server/client API
- [egui 0.33 documentation](https://docs.rs/egui/latest/egui/) - UI framework API, ViewportBuilder
- [pest.rs](https://pest.rs/) - Parser generator documentation
- [win-hotkeys GitHub](https://github.com/iholston/win-hotkeys) - Global hotkey registration
- [opener crate](https://docs.rs/opener/latest/opener/) - File opening and reveal
- [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html) - Trigram tokenizer for substring search

### Secondary (MEDIUM confidence)
- [egui GitHub issues](https://github.com/emilk/egui/issues/5229) - Windows visibility bug documentation
- [global-hotkey GitHub](https://github.com/tauri-apps/global-hotkey) - Alternative hotkey library
- [arboard crate](https://lib.rs/crates/arboard) - Clipboard operations
- [interprocess crate](https://docs.rs/interprocess/latest/interprocess/) - Alternative IPC library

### Tertiary (LOW confidence)
- Community discussions on egui keyboard navigation patterns
- Performance estimates for SQLite with 1M+ rows (based on reported benchmarks)

## Metadata

**Confidence breakdown:**
- Standard Stack: HIGH - All libraries verified via official documentation, versions confirmed
- Architecture Patterns: HIGH - Based on official API examples and project research
- Pitfalls: MEDIUM - Based on GitHub issues and community reports

**Research date:** 2026-01-24
**Valid until:** 2026-02-24 (30 days - stable ecosystem)

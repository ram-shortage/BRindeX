---
phase: "03"
plan: "03"
subsystem: search-parser
tags: [pest, grammar, parser, sql-generation, filters]

dependency_graph:
  requires: ["01-03"]
  provides: ["search-syntax-parser", "sql-query-builder", "filter-types"]
  affects: ["03-04", "03-05"]

tech_stack:
  added:
    - pest 2.8 (PEG grammar parser)
    - pest_derive 2.8 (grammar derive macros)
    - chrono 0.4 (date/time handling)
  patterns:
    - Grammar-first DSL parsing
    - Parameterized SQL generation
    - Visitor pattern for parse tree

key_files:
  created:
    - src/search/mod.rs
    - src/search/grammar.pest
    - src/search/parser.rs
    - src/search/filters.rs
    - src/search/query.rs
  modified:
    - Cargo.toml
    - src/lib.rs

decisions:
  - name: pest grammar for search syntax
    rationale: Grammar files are more maintainable than hand-rolled parsers
    scope: Phase 3

  - name: Windows path special handling
    rationale: Drive letters (C:) conflict with filter syntax (ext:); added path_value rule
    scope: Plan 03-03

  - name: Path scope deferred to post-filter
    rationale: Path reconstruction is expensive; SQL filter would require schema change or CTE
    scope: Plan 03-03

metrics:
  duration: 5 min
  completed: 2026-01-24
  tasks: 3/3
  test_count: 44
---

# Phase 03 Plan 03: Search Syntax Parser Summary

**Pest grammar parser for filters and wildcards with SQL query generation**

## What Was Built

Search module (`src/search/`) providing grammar-based query parsing and SQL generation:

1. **Filter Types** (`filters.rs`)
   - `Filter` enum: Extension, Size, Type, Modified, PathScope
   - `SizeOp` enum: GreaterThan, GreaterEqual, LessThan, LessEqual with `to_sql()`
   - `DateOp` enum: Same operators for date comparisons
   - `FileType` enum: File, Folder

2. **Pest Grammar** (`grammar.pest`)
   - Query structure: `term*` where term is filter or word
   - Filter syntax: `filter_type:filter_value`
   - Supported filters: ext, size, type, modified, path
   - Comparisons: >=, <=, >, < with size/date values
   - Size units: b, kb, mb, gb, tb (case-insensitive)
   - Date values: ISO (YYYY-MM-DD) or relative (today, yesterday, lastweek, lastmonth, lastyear)
   - Wildcards: * and ? in patterns
   - Quoted strings for values with spaces
   - Windows paths: Special rule to handle drive letters (C:\)

3. **Parser** (`parser.rs`)
   - `parse_query()` - Main entry point
   - `ParsedQuery` struct with pattern and filters Vec
   - Size value conversion (10mb -> 10485760 bytes)
   - Relative date conversion using chrono (today -> Unix timestamp)
   - ISO date parsing

4. **SQL Query Builder** (`query.rs`)
   - `build_sql_query()` - Generate parameterized SQL
   - `build_sql_query_with_limit()` - Custom result limits
   - `SqlParam` enum for Text/Integer parameters
   - Wildcard conversion: * -> %, ? -> _
   - Special character escaping: %, _, \
   - SQL injection prevention via parameterization

## Key Implementation Details

### Grammar Design

```pest
filter = { filter_type ~ ":" ~ filter_value }
filter_type = { "ext" | "size" | "type" | "modified" | "path" }
filter_value = { quoted_string | comparison | path_value | word }
```

The `path_value` rule was added to handle Windows paths like `C:\Projects` which would otherwise conflict with the `:` filter separator.

### SQL Generation

Pattern without wildcards becomes substring search:
- Input: `document` -> SQL: `name LIKE '%document%'`

Pattern with wildcards uses direct translation:
- Input: `*.pdf` -> SQL: `name LIKE '%.pdf'`
- Input: `doc?.txt` -> SQL: `name LIKE 'doc_.txt'`

All values are parameterized to prevent SQL injection:
```rust
conditions.push("name LIKE ? ESCAPE '\\'");
params.push(SqlParam::Text(sql_pattern));
```

### Path Scope Limitation

Path filtering (`path:C:\Projects`) is parsed but not added to SQL WHERE clause. The current schema stores `parent_ref` not full paths, so filtering by path would require:
1. Schema change to store computed `full_path` column
2. Recursive CTE for path reconstruction during query
3. Post-filter results in memory

For now, path scope is returned in `ParsedQuery.filters` for caller to handle via post-filtering.

## Commits

| Hash | Description |
|------|-------------|
| 4aa141c | feat(03-03): add pest dependencies and search module structure |
| b7c7c27 | feat(03-03): implement pest grammar and search query parser |
| d002718 | feat(03-03): implement SQL query builder with parameterized queries |

## Test Coverage

44 tests covering:
- Filter type parsing (ext, size, type, modified, path)
- Size unit conversion (b, kb, mb, gb, tb)
- Comparison operators (>, >=, <, <=)
- Wildcard patterns (* and ?)
- Relative dates (today, yesterday, lastweek, lastmonth, lastyear)
- ISO date parsing
- Quoted strings
- Combined queries
- SQL generation correctness
- SQL injection prevention
- Special character escaping

## Verification Results

- [x] `cargo check --lib` passes
- [x] `cargo test search --lib` passes (44 tests)
- [x] Grammar file exists at src/search/grammar.pest
- [x] parse_query("test ext:pdf size:>10mb") returns ParsedQuery with pattern and 2 filters
- [x] build_sql_query produces valid SQL WHERE clause
- [x] All filter types work: ext, size, type, modified, path

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Windows path parsing failed**
- **Found during:** Task 2 testing
- **Issue:** `path:C:\Projects` failed to parse because `:` after `C` was interpreted as filter separator
- **Fix:** Added `path_value` grammar rule to handle Windows drive letter paths
- **Files modified:** src/search/grammar.pest, src/search/parser.rs
- **Commit:** b7c7c27

## Next Phase Readiness

This plan delivers the search syntax parser. The search module is ready for integration with:
- IPC layer (03-04) to receive search requests
- Search execution to use generated SQL
- UI (future) to provide search input

Dependencies satisfied:
- Filter types defined and exported
- Parser converts text queries to structured data
- SQL builder generates parameterized queries

Note: Path scope filtering will need additional work when implementing search execution - either schema change for stored paths or post-filtering strategy.

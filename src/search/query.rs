//! SQL query builder for search queries.
//!
//! Converts ParsedQuery into parameterized SQL WHERE clauses.
//! Uses prepared statement parameters to prevent SQL injection.

use super::filters::*;
use super::parser::ParsedQuery;

/// SQL parameter value for prepared statements.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    /// Text parameter (strings, patterns)
    Text(String),
    /// Integer parameter (sizes, timestamps, booleans)
    Integer(i64),
}

/// Build SQL query from parsed search query.
///
/// Returns a tuple of (SQL SELECT statement, parameters).
/// The statement uses `?` placeholders for prepared statement binding.
///
/// # Examples
///
/// ```
/// use ffi::search::{parse_query, build_sql_query, SqlParam};
///
/// let parsed = parse_query("report ext:pdf").unwrap();
/// let (sql, params) = build_sql_query(&parsed);
/// assert!(sql.contains("WHERE"));
/// assert!(sql.contains("name LIKE ?"));
/// ```
pub fn build_sql_query(parsed: &ParsedQuery) -> (String, Vec<SqlParam>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<SqlParam> = Vec::new();

    // Handle pattern (name search with wildcards)
    if let Some(ref pattern) = parsed.pattern {
        let sql_pattern = convert_wildcards_to_sql(pattern);
        conditions.push("name LIKE ? ESCAPE '\\'".to_string());
        params.push(SqlParam::Text(sql_pattern));
    }

    // Handle filters
    for filter in &parsed.filters {
        match filter {
            Filter::Extension(ext) => {
                // Match files ending with .ext (case-insensitive via COLLATE NOCASE index)
                conditions.push("name LIKE ?".to_string());
                params.push(SqlParam::Text(format!("%.{}", ext)));
            }
            Filter::Size(op, bytes) => {
                conditions.push(format!("size {} ?", op.to_sql()));
                params.push(SqlParam::Integer(*bytes));
            }
            Filter::Type(file_type) => {
                let is_dir = match file_type {
                    FileType::Folder => 1,
                    FileType::File => 0,
                };
                conditions.push("is_dir = ?".to_string());
                params.push(SqlParam::Integer(is_dir));
            }
            Filter::Modified(op, timestamp) => {
                conditions.push(format!("modified {} ?", op.to_sql()));
                params.push(SqlParam::Integer(*timestamp));
            }
            Filter::PathScope(path) => {
                // NOTE: Path scope filtering requires path reconstruction which is expensive.
                // For now, we add a comment indicating this needs special handling.
                // Options for future implementation:
                // 1. Store computed full_path column with index
                // 2. Use recursive CTE to filter by path prefix
                // 3. Post-filter results in memory after initial search
                // For now, we skip this in SQL and let caller handle post-filtering.
                let _ = path; // Acknowledge unused for now
                // Don't add to conditions - will be handled by post-filter
            }
        }
    }

    // Build WHERE clause
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Build complete SQL
    let sql = format!(
        "SELECT id, volume_id, file_ref, parent_ref, name, size, modified, is_dir \
         FROM files {} \
         ORDER BY name COLLATE NOCASE \
         LIMIT ?",
        where_clause
    );

    // Add limit parameter
    params.push(SqlParam::Integer(100)); // Default limit

    (sql, params)
}

/// Convert wildcard pattern to SQL LIKE pattern.
///
/// - `*` becomes `%` (match any sequence)
/// - `?` becomes `_` (match single character)
/// - `%`, `_`, `\` in input are escaped with `\`
/// - If no wildcards, wraps in `%..%` for substring match
fn convert_wildcards_to_sql(pattern: &str) -> String {
    let has_wildcards = pattern.contains('*') || pattern.contains('?');

    let mut result = String::with_capacity(pattern.len() + 4);

    // If no wildcards, make it a substring search
    if !has_wildcards {
        result.push('%');
    }

    for c in pattern.chars() {
        match c {
            '*' => result.push('%'),
            '?' => result.push('_'),
            '%' => {
                result.push('\\');
                result.push('%');
            }
            '_' => {
                result.push('\\');
                result.push('_');
            }
            '\\' => {
                result.push('\\');
                result.push('\\');
            }
            _ => result.push(c),
        }
    }

    // If no wildcards, close substring search
    if !has_wildcards {
        result.push('%');
    }

    result
}

/// Build SQL query with custom limit.
pub fn build_sql_query_with_limit(parsed: &ParsedQuery, limit: i64) -> (String, Vec<SqlParam>) {
    let (sql, mut params) = build_sql_query(parsed);
    // Replace the default limit
    if let Some(last) = params.last_mut() {
        *last = SqlParam::Integer(limit);
    }
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::parse_query;

    #[test]
    fn test_simple_word() {
        let parsed = parse_query("document").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("WHERE"));
        assert!(sql.contains("name LIKE ? ESCAPE '\\'"));
        assert_eq!(params[0], SqlParam::Text("%document%".to_string()));
    }

    #[test]
    fn test_wildcard_asterisk() {
        let parsed = parse_query("*.pdf").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("name LIKE ? ESCAPE '\\'"));
        assert_eq!(params[0], SqlParam::Text("%.pdf".to_string()));
    }

    #[test]
    fn test_wildcard_question() {
        let parsed = parse_query("doc?.txt").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("name LIKE ? ESCAPE '\\'"));
        assert_eq!(params[0], SqlParam::Text("doc_.txt".to_string()));
    }

    #[test]
    fn test_extension_filter() {
        let parsed = parse_query("ext:pdf").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("name LIKE ?"));
        assert_eq!(params[0], SqlParam::Text("%.pdf".to_string()));
    }

    #[test]
    fn test_size_greater() {
        let parsed = parse_query("size:>10mb").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("size > ?"));
        assert_eq!(params[0], SqlParam::Integer(10 * 1024 * 1024));
    }

    #[test]
    fn test_size_less() {
        let parsed = parse_query("size:<1kb").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("size < ?"));
        assert_eq!(params[0], SqlParam::Integer(1024));
    }

    #[test]
    fn test_size_greater_equal() {
        let parsed = parse_query("size:>=5gb").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("size >= ?"));
        assert_eq!(params[0], SqlParam::Integer(5 * 1024 * 1024 * 1024));
    }

    #[test]
    fn test_size_less_equal() {
        let parsed = parse_query("size:<=100mb").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("size <= ?"));
        assert_eq!(params[0], SqlParam::Integer(100 * 1024 * 1024));
    }

    #[test]
    fn test_type_folder() {
        let parsed = parse_query("type:folder").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("is_dir = ?"));
        assert_eq!(params[0], SqlParam::Integer(1));
    }

    #[test]
    fn test_type_file() {
        let parsed = parse_query("type:file").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("is_dir = ?"));
        assert_eq!(params[0], SqlParam::Integer(0));
    }

    #[test]
    fn test_modified_filter() {
        let parsed = parse_query("modified:>yesterday").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("modified > ?"));
        // Timestamp should be a positive integer
        if let SqlParam::Integer(ts) = &params[0] {
            assert!(*ts > 0);
        } else {
            panic!("Expected Integer parameter");
        }
    }

    #[test]
    fn test_combined_filters() {
        let parsed = parse_query("report ext:pdf size:>1mb").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        assert!(sql.contains("name LIKE ? ESCAPE '\\'"));
        assert!(sql.contains("name LIKE ?"));
        assert!(sql.contains("size > ?"));
        assert!(sql.contains(" AND "));

        // Check params order: pattern, extension, size, limit
        assert_eq!(params[0], SqlParam::Text("%report%".to_string()));
        assert_eq!(params[1], SqlParam::Text("%.pdf".to_string()));
        assert_eq!(params[2], SqlParam::Integer(1024 * 1024));
        assert_eq!(params[3], SqlParam::Integer(100)); // default limit
    }

    #[test]
    fn test_empty_query() {
        let parsed = parse_query("").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        // Should not have WHERE clause but still have LIMIT
        assert!(!sql.contains("WHERE"));
        assert!(sql.contains("LIMIT ?"));
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], SqlParam::Integer(100));
    }

    #[test]
    fn test_sql_injection_prevention() {
        // These dangerous inputs should be safely parameterized
        let parsed = parse_query("'; DROP TABLE files; --").unwrap();
        let (sql, params) = build_sql_query(&parsed);

        // SQL should use placeholders, not inline the value
        assert!(sql.contains("name LIKE ? ESCAPE '\\'"));
        assert!(!sql.contains("DROP TABLE"));

        // The dangerous string should be in params, safely escaped
        if let SqlParam::Text(text) = &params[0] {
            assert!(text.contains("DROP TABLE"));
        }
    }

    #[test]
    fn test_escape_special_chars() {
        // Test that % and _ in input are escaped
        let parsed = parse_query("100%").unwrap();
        let (_sql, params) = build_sql_query(&parsed);

        if let SqlParam::Text(text) = &params[0] {
            assert!(text.contains("\\%"));
        }

        let parsed = parse_query("file_name").unwrap();
        let (_sql, params) = build_sql_query(&parsed);

        if let SqlParam::Text(text) = &params[0] {
            assert!(text.contains("\\_"));
        }
    }

    #[test]
    fn test_custom_limit() {
        let parsed = parse_query("test").unwrap();
        let (_sql, params) = build_sql_query_with_limit(&parsed, 50);

        // Last param should be the custom limit
        assert_eq!(params.last(), Some(&SqlParam::Integer(50)));
    }

    #[test]
    fn test_path_scope_not_in_sql() {
        // Path scope is noted but not added to SQL (requires post-filtering)
        let parsed = parse_query(r"path:C:\Projects").unwrap();
        let (sql, _params) = build_sql_query(&parsed);

        // Should not crash, and should not have path in WHERE
        // (path filtering handled separately)
        assert!(!sql.contains("C:\\"));
    }

    #[test]
    fn test_wildcard_conversion() {
        assert_eq!(convert_wildcards_to_sql("*.pdf"), "%.pdf");
        assert_eq!(convert_wildcards_to_sql("doc?.txt"), "doc_.txt");
        assert_eq!(convert_wildcards_to_sql("document"), "%document%");
        assert_eq!(convert_wildcards_to_sql("100%"), "%100\\%%");
        assert_eq!(convert_wildcards_to_sql("file_name"), "%file\\_name%");
    }

    #[test]
    fn test_multiple_wildcards() {
        let parsed = parse_query("*test*").unwrap();
        let (_sql, params) = build_sql_query(&parsed);

        assert_eq!(params[0], SqlParam::Text("%test%".to_string()));
    }

    #[test]
    fn test_quoted_literal() {
        let parsed = parse_query(r#"ext:"my file.txt""#).unwrap();
        let (_sql, params) = build_sql_query(&parsed);

        assert_eq!(params[0], SqlParam::Text("%.my file.txt".to_string()));
    }
}

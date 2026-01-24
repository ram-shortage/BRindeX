//! SQL query builder for search queries.
//!
//! Converts ParsedQuery into parameterized SQL WHERE clauses.

use super::parser::ParsedQuery;

/// SQL parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    /// Text parameter
    Text(String),
    /// Integer parameter
    Integer(i64),
}

/// Build SQL query from parsed search query.
///
/// Returns a tuple of (WHERE clause, parameters).
///
/// # Examples
///
/// ```ignore
/// let (sql, params) = build_sql_query(&parsed_query);
/// // sql: "WHERE name LIKE ? ESCAPE '\\' AND size > ?"
/// // params: [Text("%report%"), Integer(10485760)]
/// ```
pub fn build_sql_query(_parsed: &ParsedQuery) -> (String, Vec<SqlParam>) {
    // Placeholder - will be implemented in Task 3
    (String::new(), Vec::new())
}

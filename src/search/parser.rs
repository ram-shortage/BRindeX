//! Search query parser using pest grammar.
//!
//! Parses search queries like `report ext:pdf size:>10mb` into
//! structured ParsedQuery with pattern and filters.

use crate::{FFIError, Result};
use super::filters::*;

/// A parsed search query containing optional pattern and filters.
#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    /// Name pattern with wildcards (* and ?)
    pub pattern: Option<String>,
    /// Parsed filters (ext, size, type, modified, path)
    pub filters: Vec<Filter>,
}

/// Parse a search query string into structured query.
///
/// # Examples
///
/// ```ignore
/// let query = parse_query("report ext:pdf size:>10mb")?;
/// assert_eq!(query.pattern, Some("report".to_string()));
/// assert_eq!(query.filters.len(), 2);
/// ```
pub fn parse_query(_input: &str) -> Result<ParsedQuery> {
    // Placeholder - will be implemented in Task 2
    Ok(ParsedQuery::default())
}

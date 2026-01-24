//! Search module - query parsing, filter types, and SQL generation.
//!
//! This module provides search syntax parsing using a pest grammar,
//! enabling queries like `report ext:pdf size:>10mb modified:today`.

pub mod filters;
pub mod parser;
pub mod query;

pub use filters::*;
pub use parser::{parse_query, ParsedQuery};
pub use query::build_sql_query;

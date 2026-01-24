//! Filter types for search queries.
//!
//! Defines the structured filter types that result from parsing
//! search syntax like `ext:pdf`, `size:>10mb`, `type:folder`.

/// A parsed search filter.
#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    /// Extension filter: ext:pdf
    Extension(String),
    /// Size filter: size:>10mb (value in bytes)
    Size(SizeOp, i64),
    /// Type filter: type:folder
    Type(FileType),
    /// Modified date filter: modified:>2024-01-01 (value as Unix timestamp)
    Modified(DateOp, i64),
    /// Path scope filter: path:C:\Projects
    PathScope(String),
}

/// Comparison operators for size filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeOp {
    /// Greater than: size:>10mb
    GreaterThan,
    /// Greater than or equal: size:>=10mb
    GreaterEqual,
    /// Less than: size:<10mb
    LessThan,
    /// Less than or equal: size:<=10mb
    LessEqual,
}

impl SizeOp {
    /// Convert to SQL comparison operator.
    pub fn to_sql(&self) -> &'static str {
        match self {
            SizeOp::GreaterThan => ">",
            SizeOp::GreaterEqual => ">=",
            SizeOp::LessThan => "<",
            SizeOp::LessEqual => "<=",
        }
    }
}

/// Comparison operators for date filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOp {
    /// Greater than (after): modified:>2024-01-01
    GreaterThan,
    /// Greater than or equal: modified:>=2024-01-01
    GreaterEqual,
    /// Less than (before): modified:<2024-01-01
    LessThan,
    /// Less than or equal: modified:<=2024-01-01
    LessEqual,
}

impl DateOp {
    /// Convert to SQL comparison operator.
    pub fn to_sql(&self) -> &'static str {
        match self {
            DateOp::GreaterThan => ">",
            DateOp::GreaterEqual => ">=",
            DateOp::LessThan => "<",
            DateOp::LessEqual => "<=",
        }
    }
}

/// File type for type filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory/folder
    Folder,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_op_to_sql() {
        assert_eq!(SizeOp::GreaterThan.to_sql(), ">");
        assert_eq!(SizeOp::GreaterEqual.to_sql(), ">=");
        assert_eq!(SizeOp::LessThan.to_sql(), "<");
        assert_eq!(SizeOp::LessEqual.to_sql(), "<=");
    }

    #[test]
    fn test_date_op_to_sql() {
        assert_eq!(DateOp::GreaterThan.to_sql(), ">");
        assert_eq!(DateOp::GreaterEqual.to_sql(), ">=");
        assert_eq!(DateOp::LessThan.to_sql(), "<");
        assert_eq!(DateOp::LessEqual.to_sql(), "<=");
    }
}

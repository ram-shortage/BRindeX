//! Search query parser using pest grammar.
//!
//! Parses search queries like `report ext:pdf size:>10mb` into
//! structured ParsedQuery with pattern and filters.

use chrono::{Local, NaiveDate, Duration, TimeZone};
use pest::Parser;
use pest_derive::Parser;

use crate::{FFIError, Result};
use super::filters::*;

#[derive(Parser)]
#[grammar = "src/search/grammar.pest"]
struct SearchParser;

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
/// ```
/// use ffi::search::parse_query;
///
/// let query = parse_query("report ext:pdf").unwrap();
/// assert_eq!(query.pattern, Some("report".to_string()));
/// assert_eq!(query.filters.len(), 1);
/// ```
pub fn parse_query(input: &str) -> Result<ParsedQuery> {
    let pairs = SearchParser::parse(Rule::query, input)
        .map_err(|e| FFIError::Search(format!("Parse error: {}", e)))?;

    let mut pattern_parts: Vec<String> = Vec::new();
    let mut filters: Vec<Filter> = Vec::new();

    for pair in pairs {
        if pair.as_rule() == Rule::query {
            for inner in pair.into_inner() {
                if inner.as_rule() == Rule::term {
                    for term_inner in inner.into_inner() {
                        match term_inner.as_rule() {
                            Rule::word => {
                                pattern_parts.push(term_inner.as_str().to_string());
                            }
                            Rule::filter => {
                                if let Some(filter) = parse_filter(term_inner)? {
                                    filters.push(filter);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    let pattern = if pattern_parts.is_empty() {
        None
    } else {
        Some(pattern_parts.join(" "))
    };

    Ok(ParsedQuery { pattern, filters })
}

/// Parse a filter term into a Filter enum.
fn parse_filter(pair: pest::iterators::Pair<Rule>) -> Result<Option<Filter>> {
    let mut filter_type: Option<&str> = None;
    let mut filter_value: Option<pest::iterators::Pair<Rule>> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::filter_type => {
                filter_type = Some(inner.as_str());
            }
            Rule::filter_value => {
                filter_value = Some(inner);
            }
            _ => {}
        }
    }

    let filter_type = filter_type.ok_or_else(|| FFIError::Search("Missing filter type".to_string()))?;
    let filter_value = filter_value.ok_or_else(|| FFIError::Search("Missing filter value".to_string()))?;

    match filter_type {
        "ext" => {
            let ext = extract_value_string(&filter_value);
            Ok(Some(Filter::Extension(ext)))
        }
        "size" => {
            let (op, bytes) = parse_size_filter(&filter_value)?;
            Ok(Some(Filter::Size(op, bytes)))
        }
        "type" => {
            let type_str = extract_value_string(&filter_value).to_lowercase();
            let file_type = match type_str.as_str() {
                "folder" | "dir" | "directory" => FileType::Folder,
                "file" => FileType::File,
                _ => return Err(FFIError::Search(format!("Unknown type: {}", type_str))),
            };
            Ok(Some(Filter::Type(file_type)))
        }
        "modified" => {
            let (op, timestamp) = parse_date_filter(&filter_value)?;
            Ok(Some(Filter::Modified(op, timestamp)))
        }
        "path" => {
            let path = extract_value_string(&filter_value);
            Ok(Some(Filter::PathScope(path)))
        }
        _ => Ok(None),
    }
}

/// Extract string value from filter_value pair.
fn extract_value_string(pair: &pest::iterators::Pair<Rule>) -> String {
    for inner in pair.clone().into_inner() {
        match inner.as_rule() {
            Rule::quoted_string => {
                // Extract inner content without quotes
                for quoted_inner in inner.into_inner() {
                    if quoted_inner.as_rule() == Rule::inner {
                        return quoted_inner.as_str().to_string();
                    }
                }
            }
            Rule::word => {
                return inner.as_str().to_string();
            }
            Rule::path_value => {
                // Windows path like C:\Projects
                return inner.as_str().to_string();
            }
            Rule::comparison => {
                // For comparison, skip the operator and get the value
                for comp_inner in inner.into_inner() {
                    if comp_inner.as_rule() == Rule::word {
                        return comp_inner.as_str().to_string();
                    }
                }
            }
            _ => {}
        }
    }
    pair.as_str().to_string()
}

/// Parse size filter value into operator and bytes.
fn parse_size_filter(pair: &pest::iterators::Pair<Rule>) -> Result<(SizeOp, i64)> {
    for inner in pair.clone().into_inner() {
        if inner.as_rule() == Rule::comparison {
            return parse_size_comparison(inner);
        }
        if inner.as_rule() == Rule::word {
            // Simple value without operator, treat as >=
            let bytes = parse_size_value(inner.as_str())?;
            return Ok((SizeOp::GreaterEqual, bytes));
        }
    }
    Err(FFIError::Search("Invalid size filter".to_string()))
}

/// Parse size comparison (>10mb, >=1gb, etc).
fn parse_size_comparison(pair: pest::iterators::Pair<Rule>) -> Result<(SizeOp, i64)> {
    let mut op: Option<SizeOp> = None;
    let mut bytes: Option<i64> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::comparator => {
                op = Some(match inner.as_str() {
                    ">" => SizeOp::GreaterThan,
                    ">=" => SizeOp::GreaterEqual,
                    "<" => SizeOp::LessThan,
                    "<=" => SizeOp::LessEqual,
                    _ => return Err(FFIError::Search(format!("Unknown comparator: {}", inner.as_str()))),
                });
            }
            Rule::size_value => {
                bytes = Some(parse_size_value_pair(inner)?);
            }
            Rule::word => {
                // Fallback for simple values
                bytes = Some(parse_size_value(inner.as_str())?);
            }
            _ => {}
        }
    }

    let op = op.ok_or_else(|| FFIError::Search("Missing size operator".to_string()))?;
    let bytes = bytes.ok_or_else(|| FFIError::Search("Missing size value".to_string()))?;

    Ok((op, bytes))
}

/// Parse size value from pair (number + unit).
fn parse_size_value_pair(pair: pest::iterators::Pair<Rule>) -> Result<i64> {
    let mut number: Option<i64> = None;
    let mut unit: Option<&str> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::number => {
                number = Some(inner.as_str().parse()
                    .map_err(|_| FFIError::Search(format!("Invalid number: {}", inner.as_str())))?);
            }
            Rule::size_unit => {
                unit = Some(inner.as_str());
            }
            _ => {}
        }
    }

    let number = number.ok_or_else(|| FFIError::Search("Missing size number".to_string()))?;
    let unit = unit.unwrap_or("b");

    let multiplier = match unit.to_lowercase().as_str() {
        "b" => 1i64,
        "kb" => 1024i64,
        "mb" => 1024i64 * 1024,
        "gb" => 1024i64 * 1024 * 1024,
        "tb" => 1024i64 * 1024 * 1024 * 1024,
        _ => return Err(FFIError::Search(format!("Unknown size unit: {}", unit))),
    };

    Ok(number * multiplier)
}

/// Parse size value from string (e.g., "10mb").
fn parse_size_value(s: &str) -> Result<i64> {
    let s = s.to_lowercase();

    // Find where digits end
    let (num_str, unit_str) = s.chars()
        .enumerate()
        .find(|(_, c)| !c.is_ascii_digit())
        .map(|(i, _)| s.split_at(i))
        .unwrap_or((&s, "b"));

    let number: i64 = num_str.parse()
        .map_err(|_| FFIError::Search(format!("Invalid size number: {}", num_str)))?;

    let unit = if unit_str.is_empty() { "b" } else { unit_str };
    let multiplier = match unit {
        "b" => 1i64,
        "kb" => 1024i64,
        "mb" => 1024i64 * 1024,
        "gb" => 1024i64 * 1024 * 1024,
        "tb" => 1024i64 * 1024 * 1024 * 1024,
        _ => return Err(FFIError::Search(format!("Unknown size unit: {}", unit))),
    };

    Ok(number * multiplier)
}

/// Parse date filter value into operator and Unix timestamp.
fn parse_date_filter(pair: &pest::iterators::Pair<Rule>) -> Result<(DateOp, i64)> {
    for inner in pair.clone().into_inner() {
        if inner.as_rule() == Rule::comparison {
            return parse_date_comparison(inner);
        }
        if inner.as_rule() == Rule::word {
            // Simple value without operator (relative date), treat as >=
            let timestamp = parse_relative_date(inner.as_str())?;
            return Ok((DateOp::GreaterEqual, timestamp));
        }
    }
    Err(FFIError::Search("Invalid date filter".to_string()))
}

/// Parse date comparison (>2024-01-01, >=today, etc).
fn parse_date_comparison(pair: pest::iterators::Pair<Rule>) -> Result<(DateOp, i64)> {
    let mut op: Option<DateOp> = None;
    let mut timestamp: Option<i64> = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::comparator => {
                op = Some(match inner.as_str() {
                    ">" => DateOp::GreaterThan,
                    ">=" => DateOp::GreaterEqual,
                    "<" => DateOp::LessThan,
                    "<=" => DateOp::LessEqual,
                    _ => return Err(FFIError::Search(format!("Unknown comparator: {}", inner.as_str()))),
                });
            }
            Rule::date_value => {
                timestamp = Some(parse_date_value_pair(inner)?);
            }
            Rule::word => {
                // Fallback for relative dates
                timestamp = Some(parse_relative_date(inner.as_str())?);
            }
            _ => {}
        }
    }

    let op = op.ok_or_else(|| FFIError::Search("Missing date operator".to_string()))?;
    let timestamp = timestamp.ok_or_else(|| FFIError::Search("Missing date value".to_string()))?;

    Ok((op, timestamp))
}

/// Parse date value from pair (ISO date or relative date).
fn parse_date_value_pair(pair: pest::iterators::Pair<Rule>) -> Result<i64> {
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::iso_date => {
                return parse_iso_date(inner.as_str());
            }
            Rule::relative_date => {
                return parse_relative_date(inner.as_str());
            }
            _ => {}
        }
    }
    Err(FFIError::Search("Invalid date value".to_string()))
}

/// Parse ISO date (YYYY-MM-DD) to Unix timestamp.
fn parse_iso_date(s: &str) -> Result<i64> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| FFIError::Search(format!("Invalid ISO date: {}", s)))?;

    let datetime = date.and_hms_opt(0, 0, 0)
        .ok_or_else(|| FFIError::Search("Invalid date".to_string()))?;

    let local_datetime = Local.from_local_datetime(&datetime)
        .single()
        .ok_or_else(|| FFIError::Search("Ambiguous datetime".to_string()))?;

    Ok(local_datetime.timestamp())
}

/// Parse relative date (today, yesterday, lastweek, etc) to Unix timestamp.
fn parse_relative_date(s: &str) -> Result<i64> {
    let now = Local::now();
    let today_start = now.date_naive().and_hms_opt(0, 0, 0)
        .ok_or_else(|| FFIError::Search("Invalid date".to_string()))?;
    let today_start = Local.from_local_datetime(&today_start)
        .single()
        .ok_or_else(|| FFIError::Search("Ambiguous datetime".to_string()))?;

    let timestamp = match s.to_lowercase().as_str() {
        "today" => today_start.timestamp(),
        "yesterday" => (today_start - Duration::days(1)).timestamp(),
        "lastweek" => (today_start - Duration::days(7)).timestamp(),
        "lastmonth" => (today_start - Duration::days(30)).timestamp(),
        "lastyear" => (today_start - Duration::days(365)).timestamp(),
        _ => return Err(FFIError::Search(format!("Unknown relative date: {}", s))),
    };

    Ok(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_word() {
        let query = parse_query("document").unwrap();
        assert_eq!(query.pattern, Some("document".to_string()));
        assert!(query.filters.is_empty());
    }

    #[test]
    fn test_parse_multiple_words() {
        let query = parse_query("my document").unwrap();
        assert_eq!(query.pattern, Some("my document".to_string()));
    }

    #[test]
    fn test_parse_wildcard_asterisk() {
        let query = parse_query("*.pdf").unwrap();
        assert_eq!(query.pattern, Some("*.pdf".to_string()));
    }

    #[test]
    fn test_parse_wildcard_question() {
        let query = parse_query("doc?.txt").unwrap();
        assert_eq!(query.pattern, Some("doc?.txt".to_string()));
    }

    #[test]
    fn test_parse_extension_filter() {
        let query = parse_query("ext:pdf").unwrap();
        assert!(query.pattern.is_none());
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Extension("pdf".to_string()));
    }

    #[test]
    fn test_parse_size_greater() {
        let query = parse_query("size:>10mb").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 10 * 1024 * 1024));
    }

    #[test]
    fn test_parse_size_less() {
        let query = parse_query("size:<1kb").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Size(SizeOp::LessThan, 1024));
    }

    #[test]
    fn test_parse_size_greater_equal() {
        let query = parse_query("size:>=5gb").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterEqual, 5 * 1024 * 1024 * 1024));
    }

    #[test]
    fn test_parse_type_folder() {
        let query = parse_query("type:folder").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Type(FileType::Folder));
    }

    #[test]
    fn test_parse_type_file() {
        let query = parse_query("type:file").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Type(FileType::File));
    }

    #[test]
    fn test_parse_modified_today() {
        let query = parse_query("modified:today").unwrap();
        assert_eq!(query.filters.len(), 1);
        if let Filter::Modified(op, _ts) = &query.filters[0] {
            assert_eq!(*op, DateOp::GreaterEqual);
            // Timestamp should be start of today - exact value depends on current time
        } else {
            panic!("Expected Modified filter");
        }
    }

    #[test]
    fn test_parse_modified_comparison() {
        let query = parse_query("modified:>yesterday").unwrap();
        assert_eq!(query.filters.len(), 1);
        if let Filter::Modified(op, _ts) = &query.filters[0] {
            assert_eq!(*op, DateOp::GreaterThan);
        } else {
            panic!("Expected Modified filter");
        }
    }

    #[test]
    fn test_parse_path_scope() {
        let query = parse_query(r"path:C:\Projects").unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::PathScope(r"C:\Projects".to_string()));
    }

    #[test]
    fn test_parse_combined() {
        let query = parse_query("report ext:pdf size:>1mb").unwrap();
        assert_eq!(query.pattern, Some("report".to_string()));
        assert_eq!(query.filters.len(), 2);
        assert_eq!(query.filters[0], Filter::Extension("pdf".to_string()));
        assert_eq!(query.filters[1], Filter::Size(SizeOp::GreaterThan, 1024 * 1024));
    }

    #[test]
    fn test_parse_quoted_string() {
        let query = parse_query(r#"ext:"my extension""#).unwrap();
        assert_eq!(query.filters.len(), 1);
        assert_eq!(query.filters[0], Filter::Extension("my extension".to_string()));
    }

    #[test]
    fn test_parse_empty_query() {
        let query = parse_query("").unwrap();
        assert!(query.pattern.is_none());
        assert!(query.filters.is_empty());
    }

    #[test]
    fn test_size_units() {
        // Test all size units
        let query = parse_query("size:>1b").unwrap();
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 1));

        let query = parse_query("size:>1KB").unwrap();
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 1024));

        let query = parse_query("size:>1MB").unwrap();
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 1024 * 1024));

        let query = parse_query("size:>1GB").unwrap();
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 1024 * 1024 * 1024));

        let query = parse_query("size:>1TB").unwrap();
        assert_eq!(query.filters[0], Filter::Size(SizeOp::GreaterThan, 1024i64 * 1024 * 1024 * 1024));
    }

    #[test]
    fn test_relative_dates() {
        // These should not error - exact timestamps depend on current time
        parse_query("modified:yesterday").unwrap();
        parse_query("modified:lastweek").unwrap();
        parse_query("modified:lastmonth").unwrap();
        parse_query("modified:lastyear").unwrap();
    }

    #[test]
    fn test_iso_date() {
        let query = parse_query("modified:>2024-01-15").unwrap();
        assert_eq!(query.filters.len(), 1);
        if let Filter::Modified(op, ts) = &query.filters[0] {
            assert_eq!(*op, DateOp::GreaterThan);
            // Should be a valid Unix timestamp in the past
            assert!(*ts > 0);
            assert!(*ts < chrono::Local::now().timestamp());
        } else {
            panic!("Expected Modified filter");
        }
    }
}

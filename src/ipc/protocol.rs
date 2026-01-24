//! IPC protocol types for search requests and responses.
//!
//! Uses length-prefixed JSON messages for reliable framing over named pipes.
//! Format: 4-byte little-endian length prefix followed by JSON bytes.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{FFIError, Result};

/// Named pipe path for the FFI search service.
/// Uses Windows named pipe format: \\.\pipe\<name>
pub const PIPE_NAME: &str = r"\\.\pipe\FFI_Search";

/// Search request from UI to service.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchRequest {
    /// Search query string (supports wildcards)
    pub query: String,
    /// Maximum number of results to return
    pub limit: usize,
    /// Offset for pagination
    pub offset: usize,
}

/// Search response from service to UI.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchResponse {
    /// List of matching files
    pub results: Vec<FileResult>,
    /// Total count of matches (may be more than results.len() if paginated)
    pub total_count: usize,
    /// Time taken to execute search in milliseconds
    pub search_time_ms: u64,
}

/// A single file result returned from search.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileResult {
    /// Database ID of the file
    pub id: i64,
    /// Filename (not full path)
    pub name: String,
    /// Full reconstructed path
    pub path: String,
    /// File size in bytes
    pub size: i64,
    /// Last modified time as Unix timestamp
    pub modified: i64,
    /// Whether this is a directory
    pub is_dir: bool,
}

/// Read a length-prefixed JSON message from an async reader.
///
/// Message format:
/// - 4 bytes: little-endian u32 message length
/// - N bytes: JSON-encoded message
///
/// # Errors
/// Returns error if read fails, message is too large, or JSON parsing fails.
pub async fn read_message<T, R>(reader: &mut R) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
    R: AsyncReadExt + Unpin,
{
    // Read 4-byte length prefix
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await.map_err(|e| {
        FFIError::Ipc(format!("Failed to read message length: {}", e))
    })?;

    let len = u32::from_le_bytes(len_buf) as usize;

    // Sanity check: reject messages over 16MB
    const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
    if len > MAX_MESSAGE_SIZE {
        return Err(FFIError::Ipc(format!(
            "Message too large: {} bytes (max {})",
            len, MAX_MESSAGE_SIZE
        )));
    }

    // Read message body
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await.map_err(|e| {
        FFIError::Ipc(format!("Failed to read message body: {}", e))
    })?;

    // Deserialize JSON
    serde_json::from_slice(&buf).map_err(|e| {
        FFIError::Ipc(format!("Failed to parse message: {}", e))
    })
}

/// Write a length-prefixed JSON message to an async writer.
///
/// Message format:
/// - 4 bytes: little-endian u32 message length
/// - N bytes: JSON-encoded message
///
/// # Errors
/// Returns error if serialization or write fails.
pub async fn write_message<T, W>(writer: &mut W, message: &T) -> Result<()>
where
    T: Serialize,
    W: AsyncWriteExt + Unpin,
{
    // Serialize to JSON
    let json = serde_json::to_vec(message).map_err(|e| {
        FFIError::Ipc(format!("Failed to serialize message: {}", e))
    })?;

    // Write length prefix
    let len = json.len() as u32;
    writer.write_all(&len.to_le_bytes()).await.map_err(|e| {
        FFIError::Ipc(format!("Failed to write message length: {}", e))
    })?;

    // Write message body
    writer.write_all(&json).await.map_err(|e| {
        FFIError::Ipc(format!("Failed to write message body: {}", e))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_serialization() {
        let request = SearchRequest {
            query: "test*.txt".to_string(),
            limit: 100,
            offset: 0,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: SearchRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.query, "test*.txt");
        assert_eq!(parsed.limit, 100);
        assert_eq!(parsed.offset, 0);
    }

    #[test]
    fn test_search_response_serialization() {
        let response = SearchResponse {
            results: vec![
                FileResult {
                    id: 1,
                    name: "test.txt".to_string(),
                    path: "C:\\Users\\test.txt".to_string(),
                    size: 1024,
                    modified: 1700000000,
                    is_dir: false,
                },
            ],
            total_count: 1,
            search_time_ms: 5,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: SearchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].name, "test.txt");
        assert_eq!(parsed.total_count, 1);
        assert_eq!(parsed.search_time_ms, 5);
    }

    #[test]
    fn test_file_result_serialization() {
        let result = FileResult {
            id: 42,
            name: "document.pdf".to_string(),
            path: "C:\\Documents\\document.pdf".to_string(),
            size: 2048,
            modified: 1700000000,
            is_dir: false,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("document.pdf"));

        let parsed: FileResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 42);
        assert!(!parsed.is_dir);
    }
}

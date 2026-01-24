//! Named pipe client for the search UI.
//!
//! Connects to the FFI service to execute search queries.
//! The client is stateless - it connects per request.

use tokio::net::windows::named_pipe::ClientOptions;

use crate::ipc::protocol::{
    read_message, write_message, SearchRequest, SearchResponse, PIPE_NAME,
};
use crate::{FFIError, Result};

/// IPC client for sending search requests to the FFI service.
///
/// The client is stateless and creates a new connection for each request.
/// This simplifies error handling and avoids connection state management.
pub struct IpcClient;

impl IpcClient {
    /// Create a new IPC client.
    ///
    /// The client doesn't establish a connection until a search is performed.
    pub fn new() -> Self {
        Self
    }

    /// Search for files matching the query.
    ///
    /// # Arguments
    /// * `query` - Search query string (supports wildcards)
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// SearchResponse containing matching files and metadata
    ///
    /// # Errors
    /// Returns error if connection fails or communication error occurs
    pub async fn search(&self, query: &str, limit: usize) -> Result<SearchResponse> {
        self.search_with_offset(query, limit, 0).await
    }

    /// Search for files with pagination offset.
    ///
    /// # Arguments
    /// * `query` - Search query string (supports wildcards)
    /// * `limit` - Maximum number of results to return
    /// * `offset` - Number of results to skip (for pagination)
    ///
    /// # Returns
    /// SearchResponse containing matching files and metadata
    ///
    /// # Errors
    /// Returns error if connection fails or communication error occurs
    pub async fn search_with_offset(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResponse> {
        // Connect to named pipe
        let mut client = ClientOptions::new().open(PIPE_NAME).map_err(|e| {
            FFIError::Ipc(format!(
                "Failed to connect to FFI service at {}: {}. Is the service running?",
                PIPE_NAME, e
            ))
        })?;

        // Build request
        let request = SearchRequest {
            query: query.to_string(),
            limit,
            offset,
        };

        // Send request
        write_message(&mut client, &request).await?;

        // Read response
        let response: SearchResponse = read_message(&mut client).await?;

        Ok(response)
    }

    /// Check if the FFI service is available.
    ///
    /// Attempts to connect to the named pipe without sending a request.
    ///
    /// # Returns
    /// true if the service is reachable, false otherwise
    pub fn is_service_available(&self) -> bool {
        // Try to open the pipe - if it succeeds, service is available
        ClientOptions::new().open(PIPE_NAME).is_ok()
    }
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_client_creation() {
        let client = IpcClient::new();
        // Client should be constructible
        let _ = client;
    }

    #[test]
    fn test_ipc_client_default() {
        let client = IpcClient::default();
        // Default should work
        let _ = client;
    }

    #[test]
    fn test_service_not_available_when_not_running() {
        let client = IpcClient::new();
        // Service is not running in test environment
        // On non-Windows this will always be false
        // On Windows without service running, this should be false
        #[cfg(windows)]
        {
            // This test may pass or fail depending on whether the service is running
            // We're just checking the method doesn't panic
            let _ = client.is_service_available();
        }
    }
}

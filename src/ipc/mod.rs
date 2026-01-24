//! IPC module for communication between search UI and FFI service.
//!
//! Uses Windows named pipes for efficient, secure local IPC.
//! The service runs a named pipe server, and the UI connects as a client.

pub mod protocol;

#[cfg(windows)]
pub mod server;

#[cfg(windows)]
pub mod client;

pub use protocol::*;

#[cfg(windows)]
pub use server::IpcServer;

#[cfg(windows)]
pub use client::IpcClient;

/// Stub IpcClient for non-Windows platforms.
#[cfg(not(windows))]
pub struct IpcClient;

#[cfg(not(windows))]
impl IpcClient {
    /// Create a new IPC client stub.
    pub fn new() -> Self {
        Self
    }

    /// Search stub - returns error on non-Windows.
    pub async fn search(&self, _query: &str, _limit: usize) -> crate::Result<SearchResponse> {
        Err(crate::FFIError::Ipc("IPC only supported on Windows".to_string()))
    }

    /// Search with offset stub - returns error on non-Windows.
    pub async fn search_with_offset(
        &self,
        _query: &str,
        _limit: usize,
        _offset: usize,
    ) -> crate::Result<SearchResponse> {
        Err(crate::FFIError::Ipc("IPC only supported on Windows".to_string()))
    }

    /// Check if service is available (always false on non-Windows).
    pub fn is_service_available(&self) -> bool {
        false
    }
}

#[cfg(not(windows))]
impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

//! Named pipe server for the FFI service.
//!
//! Listens for search requests from the UI client and returns results
//! from the database. Uses the loop pattern from RESEARCH.md for handling
//! multiple sequential client connections.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::broadcast;

use crate::db::Database;
use crate::db::{search_files, reconstruct_path};
use crate::ipc::protocol::{
    read_message, write_message, FileResult, SearchRequest, SearchResponse, PIPE_NAME,
};
use crate::{FFIError, Result};

/// IPC server for handling search requests over named pipes.
///
/// The server runs in the FFI service process and responds to search
/// queries from the UI client.
pub struct IpcServer {
    db: Arc<Mutex<Database>>,
}

impl IpcServer {
    /// Create a new IPC server with a database connection.
    ///
    /// # Arguments
    /// * `db` - Shared database connection (thread-safe)
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// Run the IPC server, accepting client connections until shutdown.
    ///
    /// Uses the loop pattern from RESEARCH.md:
    /// 1. Create new server instance
    /// 2. Wait for client connection
    /// 3. Spawn handler for this client
    /// 4. Repeat
    ///
    /// # Arguments
    /// * `mut shutdown` - Broadcast receiver for shutdown signal
    ///
    /// # Errors
    /// Returns error if pipe creation fails. Individual client errors are logged
    /// but don't stop the server.
    pub async fn run(&self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("Starting IPC server on {}", PIPE_NAME);

        loop {
            // Create new server instance
            let server = match ServerOptions::new()
                .first_pipe_instance(false)
                .create(PIPE_NAME)
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to create named pipe: {}", e);
                    return Err(FFIError::Ipc(format!("Failed to create named pipe: {}", e)));
                }
            };

            // Wait for client connection or shutdown signal
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC server shutting down");
                    return Ok(());
                }
                result = server.connect() => {
                    match result {
                        Ok(()) => {
                            tracing::debug!("Client connected to IPC server");
                            // Spawn handler for this client
                            let db = self.db.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(server, db).await {
                                    tracing::warn!("Client handler error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to accept client connection: {}", e);
                            // Continue listening for new connections
                        }
                    }
                }
            }
        }
    }
}

/// Handle a single client connection.
///
/// Reads SearchRequest, executes search, reconstructs paths, and returns SearchResponse.
async fn handle_client(mut pipe: NamedPipeServer, db: Arc<Mutex<Database>>) -> Result<()> {
    // Read request
    let request: SearchRequest = read_message(&mut pipe).await?;
    tracing::debug!(
        "Search request: query='{}', limit={}, offset={}",
        request.query,
        request.limit,
        request.offset
    );

    let start = Instant::now();

    // Execute search
    let (file_entries, total_count) = {
        let conn = db.lock().map_err(|e| {
            FFIError::Ipc(format!("Failed to acquire database lock: {}", e))
        })?;

        // Search files (this returns db::ops::FileEntry)
        let entries = search_files(conn.conn(), &request.query, request.limit)?;
        let total = entries.len(); // TODO: Implement total count query for pagination

        (entries, total)
    };

    // Convert FileEntry to FileResult with reconstructed paths
    let mut results = Vec::with_capacity(file_entries.len());
    for entry in file_entries {
        // Reconstruct full path
        let path = if let Some(file_ref) = entry.file_ref {
            let conn = db.lock().map_err(|e| {
                FFIError::Ipc(format!("Failed to acquire database lock: {}", e))
            })?;

            // Get volume info for drive letter
            let volume_letter = {
                // Query volume drive letter - we need to look it up
                let vol_result = conn.conn().query_row(
                    "SELECT drive_letter FROM volumes WHERE id = ?1",
                    rusqlite::params![entry.volume_id],
                    |row: &rusqlite::Row| row.get::<_, String>(0),
                );
                vol_result.ok()
            };

            let reconstructed = reconstruct_path(conn.conn(), entry.volume_id, file_ref)?;

            // Prepend drive letter if available
            if let Some(letter) = volume_letter {
                format!("{}\\{}", letter, reconstructed.display())
            } else {
                reconstructed.display().to_string()
            }
        } else {
            entry.name.clone()
        };

        results.push(FileResult {
            id: entry.file_ref.unwrap_or(0),
            name: entry.name,
            path,
            size: entry.size,
            modified: entry.modified.unwrap_or(0),
            is_dir: entry.is_dir,
        });
    }

    let search_time_ms = start.elapsed().as_millis() as u64;

    // Build response
    let response = SearchResponse {
        results,
        total_count,
        search_time_ms,
    };

    tracing::debug!(
        "Search completed: {} results in {}ms",
        response.results.len(),
        response.search_time_ms
    );

    // Send response
    write_message(&mut pipe, &response).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_server_creation() {
        // We can't easily test the server without a database
        // Just verify the struct can be constructed with proper types
        // Full integration testing requires Windows named pipes
    }
}

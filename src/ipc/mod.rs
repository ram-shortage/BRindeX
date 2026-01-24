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

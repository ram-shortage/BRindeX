//! FFI Windows Service entry point.
//!
//! This binary runs as a Windows service, managing the file index database
//! and providing fast filename lookups.
//!
//! # Usage
//!
//! The service is registered and controlled via `sc.exe`:
//! ```cmd
//! sc create FFIService binPath= "C:\path\to\ffi-service.exe"
//! sc start FFIService
//! sc stop FFIService
//! sc delete FFIService
//! ```
//!
//! Logs are written to `C:\ProgramData\FFI\logs\ffi-service.log`.

#[cfg(windows)]
use std::ffi::OsString;

#[cfg(windows)]
use windows_service::{define_windows_service, service_dispatcher};

use ffi::service::{run_service, ServiceConfig};

#[cfg(windows)]
use ffi::service::SERVICE_NAME;

#[cfg(windows)]
define_windows_service!(ffi_service_main, service_main);

/// Service entry point called by Windows SCM.
#[cfg(windows)]
fn service_main(arguments: Vec<OsString>) {
    // Initialize logging before service starts
    init_logging();

    // Run the service
    if let Err(e) = run_service(arguments) {
        tracing::error!("Service failed: {}", e);
    }
}

/// Initialize tracing with file appender for service logging.
fn init_logging() {
    let config = ServiceConfig::load();
    let log_dir = config.data_dir.join("logs");

    // Create log directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        eprintln!("Failed to create log directory {:?}: {}", log_dir, e);
        // Fall back to stderr logging
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .init();
        return;
    }

    // Set up daily rotating file appender
    let file_appender = tracing_appender::rolling::daily(&log_dir, "ffi-service.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter("info")
        .with_ansi(false) // No ANSI colors in log files
        .init();

    tracing::info!("Logging initialized to {:?}", log_dir);

    // Log version info on startup
    tracing::info!(
        "FastFileIndex Service v{} starting",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start the service dispatcher
    // This blocks until the service is stopped
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

/// Non-Windows entry point for development/testing.
#[cfg(not(windows))]
fn main() {
    init_logging();
    tracing::warn!("FFI Service requires Windows to run as a service");
    tracing::info!("On non-Windows platforms, this binary can only be used for testing");

    // For testing, we can still call run_service (which returns immediately on non-Windows)
    if let Err(e) = run_service(vec![]) {
        tracing::error!("Service failed: {}", e);
    }
}

//! Windows service lifecycle management.
//!
//! This module handles the Windows service lifecycle including:
//! - Service registration and control
//! - State transitions (Starting -> Running -> Stopping -> Stopped)
//! - Configuration loading

pub mod config;
pub mod control;

pub use config::ServiceConfig;
pub use control::ServiceState;

#[cfg(windows)]
pub use control::create_event_handler;

use std::ffi::OsString;

#[cfg(windows)]
use std::sync::mpsc;
#[cfg(windows)]
use std::time::Duration;

#[cfg(windows)]
use windows_service::service::{
    ServiceControlAccept, ServiceExitCode, ServiceState as WinServiceState, ServiceStatus,
    ServiceType,
};
#[cfg(windows)]
use windows_service::service_control_handler;

use crate::Result;

/// Service name as registered with Windows SCM
pub const SERVICE_NAME: &str = "FFIService";

/// Service display name shown in Services console
pub const SERVICE_DISPLAY_NAME: &str = "FastFileIndex Service";

/// Run the FFI Windows service.
///
/// This function implements the full service lifecycle:
/// 1. Create shutdown channel
/// 2. Register control handler with SCM
/// 3. Report StartPending state
/// 4. Initialize service resources
/// 5. Report Running state
/// 6. Wait for shutdown signal
/// 7. Report StopPending state
/// 8. Clean up resources
/// 9. Report Stopped state
#[cfg(windows)]
pub fn run_service(_arguments: Vec<OsString>) -> Result<()> {
    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    // Create and register the control handler
    let event_handler = create_event_handler(shutdown_tx);
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .map_err(|e| crate::FFIError::Service(format!("Failed to register control handler: {}", e)))?;

    tracing::info!("Service control handler registered");

    // Report StartPending with 60 second wait_hint
    let mut status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: WinServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(60),
        process_id: None,
    };
    status_handle
        .set_service_status(status.clone())
        .map_err(|e| crate::FFIError::Service(format!("Failed to set StartPending status: {}", e)))?;
    tracing::info!("Reported StartPending to SCM");

    // Increment checkpoint to show progress during initialization
    status.checkpoint = 1;
    status_handle
        .set_service_status(status.clone())
        .map_err(|e| crate::FFIError::Service(format!("Failed to update checkpoint: {}", e)))?;
    tracing::debug!("Initialization checkpoint 1");

    // Load configuration
    let config = ServiceConfig::load();
    tracing::info!("Loaded configuration: data_dir={:?}", config.data_dir);

    status.checkpoint = 2;
    status_handle
        .set_service_status(status.clone())
        .map_err(|e| crate::FFIError::Service(format!("Failed to update checkpoint: {}", e)))?;
    tracing::debug!("Initialization checkpoint 2");

    // TODO: Initialize database (Plan 01-02)
    // TODO: Start indexer (Plan 01-03)

    // Report Running - accept STOP and SHUTDOWN controls
    status.current_state = WinServiceState::Running;
    status.controls_accepted = ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN;
    status.checkpoint = 0;
    status.wait_hint = Duration::default();
    status_handle
        .set_service_status(status.clone())
        .map_err(|e| crate::FFIError::Service(format!("Failed to set Running status: {}", e)))?;
    tracing::info!("Service is now Running");

    // Wait for shutdown signal from control handler
    tracing::info!("Waiting for shutdown signal...");
    shutdown_rx.recv().ok();
    tracing::info!("Shutdown signal received");

    // Report StopPending
    status.current_state = WinServiceState::StopPending;
    status.controls_accepted = ServiceControlAccept::empty();
    status.wait_hint = Duration::from_secs(30);
    status_handle
        .set_service_status(status.clone())
        .map_err(|e| crate::FFIError::Service(format!("Failed to set StopPending status: {}", e)))?;
    tracing::info!("Reported StopPending to SCM");

    // TODO: Graceful shutdown - stop indexer, close database (Plans 01-02, 01-03)

    // Report Stopped
    status.current_state = WinServiceState::Stopped;
    status.wait_hint = Duration::default();
    status_handle
        .set_service_status(status)
        .map_err(|e| crate::FFIError::Service(format!("Failed to set Stopped status: {}", e)))?;
    tracing::info!("Service stopped successfully");

    Ok(())
}

/// Stub for non-Windows platforms (for development/testing).
#[cfg(not(windows))]
pub fn run_service(_arguments: Vec<OsString>) -> Result<()> {
    tracing::warn!("run_service called on non-Windows platform - this is a no-op");
    Ok(())
}

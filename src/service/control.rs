//! Service control handler for Windows service events.
//!
//! Handles Stop, Shutdown, and Interrogate control events from the
//! Windows Service Control Manager (SCM).

use std::sync::mpsc::Sender;

#[cfg(windows)]
use windows_service::service::ServiceControl;
#[cfg(windows)]
use windows_service::service_control_handler::ServiceControlHandlerResult;

/// State shared with the service control event handler.
///
/// Contains the shutdown signal sender to notify the main service
/// loop when a stop/shutdown event is received.
pub struct ServiceState {
    /// Sender to signal service shutdown
    pub shutdown_tx: Sender<()>,
}

impl ServiceState {
    /// Create a new ServiceState with the given shutdown channel sender.
    pub fn new(shutdown_tx: Sender<()>) -> Self {
        Self { shutdown_tx }
    }
}

/// Create a service control event handler function.
///
/// Returns a closure that handles control events from the SCM:
/// - Stop: Sends shutdown signal, returns NoError
/// - Shutdown: Sends shutdown signal, returns NoError
/// - Interrogate: Returns NoError (no action needed)
/// - Other: Returns NotImplemented
#[cfg(windows)]
pub fn create_event_handler(
    shutdown_tx: Sender<()>,
) -> impl FnMut(ServiceControl) -> ServiceControlHandlerResult {
    move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                tracing::info!("Received Stop control event");
                shutdown_tx.send(()).ok();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Shutdown => {
                tracing::info!("Received Shutdown control event");
                shutdown_tx.send(()).ok();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => {
                tracing::debug!("Received Interrogate control event");
                ServiceControlHandlerResult::NoError
            }
            _ => {
                tracing::debug!("Received unhandled control event: {:?}", control_event);
                ServiceControlHandlerResult::NotImplemented
            }
        }
    }
}

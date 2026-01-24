//! FFI Search UI - Global hotkey search popup.
//!
//! This binary provides the user-facing search interface:
//! - Global hotkey (Ctrl+Space) to show/hide the popup
//! - Search-as-you-type with results from FFI service
//! - Keyboard navigation (Up/Down/Enter/Esc)
//! - File actions (open, reveal, copy path)

use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use eframe::egui;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use ffi::ui::SearchApp;
#[cfg(windows)]
use ffi::ui::HotkeyManager;

/// Main entry point for the FFI search UI.
fn main() -> eframe::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "ffi_search=info,ffi=info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("FFI Search UI starting");

    // Create channel for hotkey events
    let (hotkey_tx, hotkey_rx) = mpsc::channel();

    // Visibility state shared with hotkey manager
    let visible = Arc::new(AtomicBool::new(true));

    // Setup global hotkey (Ctrl+Space)
    #[cfg(windows)]
    {
        let tx = hotkey_tx;
        match HotkeyManager::new(move || {
            info!("Hotkey triggered");
            let _ = tx.send(());
        }) {
            Ok(mut hkm) => {
                if let Err(e) = hkm.start() {
                    warn!("Failed to start hotkey listener: {}. Use window focus instead.", e);
                }
                // Keep the hotkey manager alive by leaking it
                // It needs to stay alive for the hotkey to work
                std::mem::forget(hkm);
            }
            Err(e) => {
                warn!("Failed to register hotkey: {}. Use window focus instead.", e);
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = hotkey_tx; // Suppress unused warning on non-Windows
        warn!("Global hotkey not supported on this platform");
    }

    // Create tokio runtime for async IPC
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // Configure eframe window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 400.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_transparent(true),
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "FFI Search",
        options,
        Box::new(move |cc| {
            // Set up dark mode by default (follows system)
            egui_extras::install_image_loaders(&cc.egui_ctx);

            Ok(Box::new(SearchApp::new(
                cc,
                runtime.handle().clone(),
                hotkey_rx,
                visible,
            )))
        }),
    )
}

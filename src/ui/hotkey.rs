//! Global hotkey registration and handling.
//!
//! Registers Ctrl+Space as the global hotkey to show/hide the search popup.
//! Currently Windows-only using the global-hotkey crate.

use crate::{FFIError, Result};

#[cfg(windows)]
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, hotkey::{HotKey, Modifiers, Code}};

#[cfg(windows)]
use std::thread;

/// Manager for global hotkey registration.
#[cfg(windows)]
pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    hotkey_id: u32,
    callback: Box<dyn Fn() + Send + 'static>,
}

#[cfg(windows)]
impl HotkeyManager {
    /// Create a new hotkey manager with the given callback.
    ///
    /// Registers Ctrl+Space as the default hotkey.
    pub fn new<F: Fn() + Send + 'static>(on_hotkey: F) -> Result<Self> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| FFIError::Ipc(format!("Failed to create hotkey manager: {}", e)))?;

        // Register Ctrl+Space
        let hotkey = HotKey::new(Some(Modifiers::CONTROL), Code::Space);
        let hotkey_id = hotkey.id();

        manager.register(hotkey)
            .map_err(|e| FFIError::Ipc(format!("Failed to register hotkey Ctrl+Space: {}", e)))?;

        tracing::info!("Registered global hotkey: Ctrl+Space (id: {})", hotkey_id);

        Ok(Self {
            manager,
            hotkey_id,
            callback: Box::new(on_hotkey),
        })
    }

    /// Start listening for hotkey events in a background thread.
    pub fn start(&mut self) -> Result<()> {
        let hotkey_id = self.hotkey_id;

        // We need to use a channel to forward events since the callback can't be cloned
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn thread to receive hotkey events
        thread::spawn(move || {
            loop {
                if let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                    if event.id == hotkey_id {
                        let _ = tx.send(());
                    }
                }
            }
        });

        // Spawn another thread to invoke the callback
        // This is necessary because we can't move the callback into the first thread
        let callback = std::mem::replace(&mut self.callback, Box::new(|| {}));
        thread::spawn(move || {
            while rx.recv().is_ok() {
                callback();
            }
        });

        Ok(())
    }
}

#[cfg(windows)]
impl Drop for HotkeyManager {
    fn drop(&mut self) {
        // Hotkey is automatically unregistered when manager is dropped
        tracing::info!("Hotkey manager dropped, hotkey unregistered");
    }
}

/// Stub for non-Windows platforms.
#[cfg(not(windows))]
pub struct HotkeyManager;

#[cfg(not(windows))]
impl HotkeyManager {
    /// Create a new hotkey manager (stub for non-Windows).
    pub fn new<F: Fn() + Send + 'static>(_on_hotkey: F) -> Result<Self> {
        Err(FFIError::Ipc("Global hotkeys are only supported on Windows".to_string()))
    }

    /// Start listening (stub for non-Windows).
    pub fn start(&mut self) -> Result<()> {
        Err(FFIError::Ipc("Global hotkeys are only supported on Windows".to_string()))
    }
}

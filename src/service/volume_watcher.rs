//! Volume mount/unmount detection via WM_DEVICECHANGE.
//!
//! This module monitors for volume arrival and removal events using
//! Windows device change notifications. On non-Windows platforms,
//! provides no-op stubs.

use std::sync::mpsc::{self, Receiver, Sender};

/// Volume lifecycle events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeEvent {
    /// Volume was mounted at the given drive letter.
    Mounted(char),
    /// Volume was unmounted from the given drive letter.
    Unmounted(char),
}

/// Handle for a running volume watcher thread.
pub struct VolumeWatcherHandle {
    #[allow(dead_code)]
    handle: Option<std::thread::JoinHandle<()>>,
}

impl VolumeWatcherHandle {
    /// Stop the watcher and wait for thread to finish.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(()) => tracing::info!("Volume watcher thread stopped"),
                Err(_) => tracing::error!("Volume watcher thread panicked"),
            }
        }
    }
}

/// Start the volume watcher in a background thread.
///
/// Returns a handle for lifecycle management, shutdown sender, and event receiver.
///
/// On Windows, creates a hidden window to receive WM_DEVICECHANGE messages.
/// On non-Windows, returns immediately with a dummy receiver.
#[cfg(windows)]
pub fn start_volume_watcher() -> (VolumeWatcherHandle, Sender<()>, Receiver<VolumeEvent>) {
    let (event_tx, event_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        volume_watcher_loop(event_tx, shutdown_rx);
    });

    (
        VolumeWatcherHandle {
            handle: Some(handle),
        },
        shutdown_tx,
        event_rx,
    )
}

/// Windows implementation of volume watcher loop.
#[cfg(windows)]
fn volume_watcher_loop(event_tx: Sender<VolumeEvent>, shutdown_rx: Receiver<()>) {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PeekMessageW,
        PostQuitMessage, RegisterClassW, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
        MSG, PM_NOREMOVE, WINDOW_EX_STYLE, WM_DEVICECHANGE, WNDCLASSW, WS_OVERLAPPED,
    };
    use windows::core::PCWSTR;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::OnceLock;

    // Device change constants
    const DBT_DEVICEARRIVAL: u32 = 0x8000;
    const DBT_DEVICEREMOVECOMPLETE: u32 = 0x8004;
    const DBT_DEVTYP_VOLUME: u32 = 0x00000002;

    #[repr(C)]
    struct DevBroadcastVolume {
        dbcv_size: u32,
        dbcv_devicetype: u32,
        dbcv_reserved: u32,
        dbcv_unitmask: u32,
        dbcv_flags: u16,
    }

    fn get_drive_letters_from_mask(mask: u32) -> Vec<char> {
        let mut letters = Vec::new();
        for i in 0..26u8 {
            if mask & (1 << i) != 0 {
                letters.push((b'A' + i) as char);
            }
        }
        letters
    }

    // Store event_tx in a static so wnd_proc can access it
    static EVENT_TX: OnceLock<Sender<VolumeEvent>> = OnceLock::new();
    EVENT_TX.get_or_init(|| event_tx.clone());

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_DEVICECHANGE {
            let event_type = wparam.0 as u32;

            match event_type {
                DBT_DEVICEARRIVAL => {
                    if lparam.0 != 0 {
                        let header = lparam.0 as *const DevBroadcastVolume;
                        if (*header).dbcv_devicetype == DBT_DEVTYP_VOLUME {
                            let drives = get_drive_letters_from_mask((*header).dbcv_unitmask);
                            if let Some(tx) = EVENT_TX.get() {
                                for drive in drives {
                                    tracing::info!("Volume mounted: {}", drive);
                                    let _ = tx.send(VolumeEvent::Mounted(drive));
                                }
                            }
                        }
                    }
                }
                DBT_DEVICEREMOVECOMPLETE => {
                    if lparam.0 != 0 {
                        let header = lparam.0 as *const DevBroadcastVolume;
                        if (*header).dbcv_devicetype == DBT_DEVTYP_VOLUME {
                            let drives = get_drive_letters_from_mask((*header).dbcv_unitmask);
                            if let Some(tx) = EVENT_TX.get() {
                                for drive in drives {
                                    tracing::info!("Volume unmounted: {}", drive);
                                    let _ = tx.send(VolumeEvent::Unmounted(drive));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            return LRESULT(1); // TRUE - message processed
        }

        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    // Helper to encode string to wide
    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    tracing::info!("Volume watcher thread starting...");

    // Register window class
    let class_name = to_wide("FFIVolumeWatcher");
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: unsafe { GetModuleHandleW(None).unwrap_or_default().into() },
        hIcon: Default::default(),
        hCursor: Default::default(),
        hbrBackground: Default::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
    };

    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        tracing::error!("Failed to register volume watcher window class");
        return;
    }

    // Create hidden window
    let hwnd = unsafe {
        match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR::from_raw(class_name.as_ptr()),
            PCWSTR::from_raw(to_wide("FFI Volume Watcher").as_ptr()),
            WS_OVERLAPPED,
            0, 0, 0, 0, // x, y, width, height (doesn't matter for hidden window)
            None,
            None,
            Some(wc.hInstance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!("Failed to create volume watcher window: {}", e);
                return;
            }
        }
    };

    if hwnd.is_invalid() {
        tracing::error!("Volume watcher window handle is invalid");
        return;
    }

    tracing::info!("Volume watcher window created, entering message loop");

    // Message loop with shutdown check
    let mut msg = MSG::default();
    loop {
        // Check for shutdown signal
        match shutdown_rx.try_recv() {
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::info!("Volume watcher received shutdown signal");
                unsafe { PostQuitMessage(0) };
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        // Process Windows messages with timeout
        let has_message = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE).as_bool() };

        if has_message {
            let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if !result.as_bool() {
                break; // WM_QUIT
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            // No message, sleep briefly to avoid busy-waiting
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    tracing::info!("Volume watcher thread exiting");
}

/// Non-Windows stub.
#[cfg(not(windows))]
pub fn start_volume_watcher() -> (VolumeWatcherHandle, Sender<()>, Receiver<VolumeEvent>) {
    let (event_tx, event_rx) = mpsc::channel();
    let (shutdown_tx, _shutdown_rx) = mpsc::channel();

    // Drop the sender immediately - no events will ever be sent
    drop(event_tx);

    tracing::info!("Volume watcher not available on this platform");

    (
        VolumeWatcherHandle { handle: None },
        shutdown_tx,
        event_rx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_event() {
        let mounted = VolumeEvent::Mounted('C');
        let unmounted = VolumeEvent::Unmounted('D');

        assert_eq!(mounted, VolumeEvent::Mounted('C'));
        assert_ne!(mounted, unmounted);
    }
}

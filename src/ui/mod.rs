//! Search UI components for the FFI desktop application.
//!
//! Provides the egui-based search popup with global hotkey activation,
//! keyboard navigation, and file actions.

pub mod app;
pub mod hotkey;
pub mod results;
pub mod actions;

pub use app::SearchApp;
pub use hotkey::HotkeyManager;

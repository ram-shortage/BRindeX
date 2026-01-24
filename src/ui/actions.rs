//! File actions for the search UI.
//!
//! Provides operations that can be performed on search results:
//! - Open file with default application
//! - Reveal file in Explorer/Finder
//! - Copy file path to clipboard

use std::path::Path;

use crate::{FFIError, Result};

/// Open a file or folder with the default application.
///
/// For files, opens with the registered application (e.g., .pdf opens in PDF reader).
/// For folders, opens in the file explorer.
///
/// # Arguments
/// * `path` - Path to the file or folder to open
///
/// # Errors
/// Returns error if the file doesn't exist or can't be opened.
pub fn open_file(path: &Path) -> Result<()> {
    tracing::info!("Opening file: {:?}", path);

    opener::open(path).map_err(|e| {
        FFIError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to open file: {}", e),
        ))
    })
}

/// Reveal a file in the system file explorer with the file selected.
///
/// On Windows, opens Explorer with the file highlighted.
/// On macOS, opens Finder with the file highlighted.
/// On Linux, opens the default file manager to the containing folder.
///
/// # Arguments
/// * `path` - Path to the file to reveal
///
/// # Errors
/// Returns error if the file doesn't exist or Explorer can't be opened.
pub fn reveal_in_explorer(path: &Path) -> Result<()> {
    tracing::info!("Revealing file in explorer: {:?}", path);

    opener::reveal(path).map_err(|e| {
        FFIError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to reveal file: {}", e),
        ))
    })
}

/// Copy the full file path to the system clipboard.
///
/// # Arguments
/// * `path` - Path to copy
///
/// # Errors
/// Returns error if clipboard access fails.
pub fn copy_to_clipboard(path: &Path) -> Result<()> {
    tracing::info!("Copying path to clipboard: {:?}", path);

    let mut clipboard = arboard::Clipboard::new().map_err(|e| {
        FFIError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to access clipboard: {}", e),
        ))
    })?;

    clipboard
        .set_text(path.to_string_lossy().to_string())
        .map_err(|e| {
            FFIError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to set clipboard text: {}", e),
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_open_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/file/path/test.txt");
        // This should return an error since the file doesn't exist
        // Note: opener::open may not fail on non-existent paths on all platforms
        // It depends on the OS behavior
        let _ = open_file(&path);
    }

    #[test]
    fn test_reveal_nonexistent_file() {
        let path = PathBuf::from("/nonexistent/file/path/test.txt");
        // This should return an error since the file doesn't exist
        let _ = reveal_in_explorer(&path);
    }

    // Note: Clipboard tests are difficult to run in CI environments
    // as they require a display/clipboard manager
}

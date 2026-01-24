//! Service configuration loading.

use std::path::PathBuf;

/// Service configuration
pub struct ServiceConfig {
    /// Data directory for database and logs (defaults to C:\ProgramData\FFI)
    pub data_dir: PathBuf,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(r"C:\ProgramData\FFI"),
        }
    }
}

impl ServiceConfig {
    /// Load configuration from file or return defaults.
    pub fn load() -> Self {
        // TODO: Add config file loading in future
        Self::default()
    }
}

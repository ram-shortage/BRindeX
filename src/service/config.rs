//! Service configuration loading and TOML persistence.
//!
//! Provides TOML-based configuration for the FFI service including:
//! - General settings (data directory, poll intervals, retention)
//! - Per-volume configuration (enabled, reconciliation intervals)
//! - Exclude patterns (paths and extensions)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{FFIError, Result};

/// Default USN polling interval in seconds (30 seconds per CONTEXT.md).
fn default_poll_interval() -> u64 {
    30
}

/// Default offline retention period in days.
fn default_offline_retention() -> u32 {
    7
}

/// Default volume enabled state.
fn default_true() -> bool {
    true
}

/// Default FAT reconciliation interval in minutes.
fn default_reconcile_interval() -> u64 {
    30
}

/// Main configuration structure for the FFI service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// General service settings.
    #[serde(default)]
    pub general: GeneralConfig,

    /// Per-volume configuration, keyed by drive letter (e.g., "C", "D").
    #[serde(default)]
    pub volumes: HashMap<String, VolumeConfig>,

    /// Path and extension exclusion patterns.
    #[serde(default)]
    pub exclude: ExcludeConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            volumes: HashMap::new(),
            exclude: ExcludeConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from the standard config path.
    ///
    /// Returns default configuration if the file doesn't exist or can't be parsed.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        if !path.exists() {
            tracing::info!("Config file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&path)
            .map_err(|e| FFIError::Config(format!("Failed to read config file: {}", e)))?;

        toml::from_str(&contents)
            .map_err(|e| FFIError::Config(format!("Failed to parse config file: {}", e)))
    }

    /// Save configuration to the standard config path.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| FFIError::Config(format!("Failed to create config directory: {}", e)))?;
        }

        let contents = toml::to_string_pretty(self)
            .map_err(|e| FFIError::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(&path, contents)
            .map_err(|e| FFIError::Config(format!("Failed to write config file: {}", e)))?;

        tracing::info!("Configuration saved to {:?}", path);
        Ok(())
    }

    /// Get the standard configuration file path.
    ///
    /// Returns `%PROGRAMDATA%\FFI\config.toml` on Windows,
    /// or `/var/lib/ffi/config.toml` on Unix.
    pub fn config_path() -> PathBuf {
        #[cfg(windows)]
        {
            // Use directories crate to get proper Windows paths
            if let Some(data_dir) = directories::BaseDirs::new() {
                // data_local_dir gives us %LOCALAPPDATA%, but we want %PROGRAMDATA%
                // For system services, use a fixed path
                PathBuf::from(r"C:\ProgramData\FFI\config.toml")
            } else {
                PathBuf::from(r"C:\ProgramData\FFI\config.toml")
            }
        }

        #[cfg(not(windows))]
        {
            PathBuf::from("/var/lib/ffi/config.toml")
        }
    }

    /// Get the data directory from config, or default.
    pub fn data_dir(&self) -> PathBuf {
        self.general.data_dir.clone().unwrap_or_else(|| {
            #[cfg(windows)]
            {
                PathBuf::from(r"C:\ProgramData\FFI")
            }
            #[cfg(not(windows))]
            {
                PathBuf::from("/var/lib/ffi")
            }
        })
    }

    /// Check if a volume is enabled for indexing.
    pub fn is_volume_enabled(&self, drive_letter: char) -> bool {
        let key = drive_letter.to_string();
        self.volumes
            .get(&key)
            .map(|v| v.enabled)
            .unwrap_or(false) // Volumes must be explicitly enabled per CONTEXT.md
    }

    /// Get reconciliation interval for a volume (FAT volumes only).
    pub fn reconcile_interval_mins(&self, drive_letter: char) -> u64 {
        let key = drive_letter.to_string();
        self.volumes
            .get(&key)
            .map(|v| v.reconcile_interval_mins)
            .unwrap_or_else(default_reconcile_interval)
    }
}

/// General service configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Data directory for database and logs.
    /// Defaults to `%PROGRAMDATA%\FFI` on Windows.
    pub data_dir: Option<PathBuf>,

    /// USN Journal polling interval in seconds.
    /// Default: 30 seconds (per CONTEXT.md decision).
    #[serde(default = "default_poll_interval")]
    pub usn_poll_interval_secs: u64,

    /// Days to keep offline volume data before auto-deletion.
    /// Default: 7 days (per CONTEXT.md decision).
    #[serde(default = "default_offline_retention")]
    pub offline_retention_days: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            usn_poll_interval_secs: default_poll_interval(),
            offline_retention_days: default_offline_retention(),
        }
    }
}

/// Per-volume configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    /// Whether indexing is enabled for this volume.
    /// Volumes must be explicitly enabled (per CONTEXT.md).
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// FAT reconciliation interval in minutes.
    /// Default: 30 minutes (per CONTEXT.md decision).
    #[serde(default = "default_reconcile_interval")]
    pub reconcile_interval_mins: u64,
}

impl Default for VolumeConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            reconcile_interval_mins: default_reconcile_interval(),
        }
    }
}

/// Path and extension exclusion configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExcludeConfig {
    /// Path prefixes to exclude from indexing.
    /// Example: `["C:\\Windows\\Temp", "C:\\$Recycle.Bin"]`
    #[serde(default)]
    pub paths: Vec<String>,

    /// File extensions to exclude (without leading dot).
    /// Example: `["tmp", "log", "bak"]`
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl ExcludeConfig {
    /// Check if a path should be excluded based on prefix matching.
    pub fn should_exclude_path(&self, path: &str) -> bool {
        let path_lower = path.to_lowercase();
        self.paths.iter().any(|prefix| {
            let prefix_lower = prefix.to_lowercase();
            path_lower.starts_with(&prefix_lower)
        })
    }

    /// Check if a file extension should be excluded.
    pub fn should_exclude_extension(&self, ext: &str) -> bool {
        let ext_lower = ext.to_lowercase();
        self.extensions.iter().any(|e| e.to_lowercase() == ext_lower)
    }
}

// Legacy ServiceConfig for backward compatibility during transition
/// Legacy service configuration (deprecated, use Config instead).
#[deprecated(note = "Use Config::load() instead")]
pub struct ServiceConfig {
    /// Data directory for database and logs.
    pub data_dir: PathBuf,
}

#[allow(deprecated)]
impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(r"C:\ProgramData\FFI"),
        }
    }
}

#[allow(deprecated)]
impl ServiceConfig {
    /// Load configuration from file or return defaults.
    pub fn load() -> Self {
        // Bridge to new config system
        match Config::load() {
            Ok(config) => Self {
                data_dir: config.data_dir(),
            },
            Err(e) => {
                tracing::warn!("Failed to load config, using defaults: {}", e);
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.general.usn_poll_interval_secs, 30);
        assert_eq!(config.general.offline_retention_days, 7);
        assert!(config.volumes.is_empty());
        assert!(config.exclude.paths.is_empty());
    }

    #[test]
    fn test_config_roundtrip() {
        let mut config = Config::default();
        config.general.usn_poll_interval_secs = 60;
        config.volumes.insert(
            "C".to_string(),
            VolumeConfig {
                enabled: true,
                reconcile_interval_mins: 45,
            },
        );
        config.exclude.paths.push(r"C:\Windows\Temp".to_string());
        config.exclude.extensions.push("tmp".to_string());

        // Serialize and deserialize
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.general.usn_poll_interval_secs, 60);
        assert!(parsed.volumes.get("C").unwrap().enabled);
        assert_eq!(parsed.volumes.get("C").unwrap().reconcile_interval_mins, 45);
        assert!(parsed.exclude.paths.contains(&r"C:\Windows\Temp".to_string()));
        assert!(parsed.exclude.extensions.contains(&"tmp".to_string()));
    }

    #[test]
    fn test_volume_not_enabled_by_default() {
        let config = Config::default();
        // Volumes must be explicitly listed to be enabled
        assert!(!config.is_volume_enabled('C'));
        assert!(!config.is_volume_enabled('D'));
    }

    #[test]
    fn test_volume_enabled_when_configured() {
        let mut config = Config::default();
        config.volumes.insert(
            "C".to_string(),
            VolumeConfig {
                enabled: true,
                reconcile_interval_mins: 30,
            },
        );

        assert!(config.is_volume_enabled('C'));
        assert!(!config.is_volume_enabled('D'));
    }

    #[test]
    fn test_exclude_path_matching() {
        let exclude = ExcludeConfig {
            paths: vec![r"C:\Windows\Temp".to_string(), r"C:\$Recycle.Bin".to_string()],
            extensions: vec![],
        };

        assert!(exclude.should_exclude_path(r"C:\Windows\Temp\file.txt"));
        assert!(exclude.should_exclude_path(r"c:\windows\temp\subdir\file.txt")); // case-insensitive
        assert!(!exclude.should_exclude_path(r"C:\Users\file.txt"));
    }

    #[test]
    fn test_exclude_extension_matching() {
        let exclude = ExcludeConfig {
            paths: vec![],
            extensions: vec!["tmp".to_string(), "log".to_string()],
        };

        assert!(exclude.should_exclude_extension("tmp"));
        assert!(exclude.should_exclude_extension("TMP")); // case-insensitive
        assert!(exclude.should_exclude_extension("log"));
        assert!(!exclude.should_exclude_extension("txt"));
    }

    #[test]
    fn test_parse_sample_config() {
        let toml_str = r#"
[general]
usn_poll_interval_secs = 30
offline_retention_days = 7

[volumes.C]
enabled = true

[volumes.D]
enabled = true
reconcile_interval_mins = 60

[volumes.E]
enabled = false

[exclude]
paths = [
    "C:\\Windows\\Temp",
    "C:\\$Recycle.Bin",
]
extensions = ["tmp", "log", "bak"]
"#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.usn_poll_interval_secs, 30);
        assert!(config.is_volume_enabled('C'));
        assert!(config.is_volume_enabled('D'));
        assert!(!config.is_volume_enabled('E'));
        assert_eq!(config.reconcile_interval_mins('D'), 60);
        assert_eq!(config.exclude.paths.len(), 2);
        assert_eq!(config.exclude.extensions.len(), 3);
    }
}

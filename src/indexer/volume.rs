//! Volume detection and filesystem type classification.
//!
//! This module detects available volumes and determines their filesystem type
//! to choose the appropriate indexing strategy (MFT for NTFS, walkdir for FAT).

/// Filesystem type of a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeType {
    /// NTFS - supports MFT enumeration
    NTFS,
    /// FAT32 - requires directory walking
    FAT32,
    /// exFAT - requires directory walking
    ExFAT,
    /// Unknown filesystem type
    Unknown,
}

/// Information about a detected volume.
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    /// Drive letter (e.g., 'C')
    pub drive_letter: char,
    /// Volume serial number as hex string
    pub volume_serial: String,
    /// Filesystem type
    pub fs_type: VolumeType,
    /// Total size in bytes
    pub total_size: u64,
    /// Free space in bytes
    pub free_space: u64,
}

/// Check if a volume is NTFS (supports MFT enumeration).
pub fn is_ntfs(info: &VolumeInfo) -> bool {
    info.fs_type == VolumeType::NTFS
}

/// Detect all available volumes on the system.
///
/// On Windows, iterates through drive letters A-Z and queries each for
/// filesystem information using Windows API calls.
///
/// On non-Windows platforms, returns an empty vector.
#[cfg(windows)]
pub fn detect_volumes() -> Vec<VolumeInfo> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetVolumeInformationW,
    };
    use windows::core::PCWSTR;

    let mut volumes = Vec::new();

    for letter in 'A'..='Z' {
        let root_path = format!("{}:\\", letter);
        let root_wide: Vec<u16> = OsStr::new(&root_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Buffer for filesystem name
        let mut fs_name_buf: [u16; 256] = [0; 256];
        let mut volume_serial: u32 = 0;

        // Get volume information
        let result = unsafe {
            GetVolumeInformationW(
                PCWSTR::from_raw(root_wide.as_ptr()),
                None,                          // Volume name buffer (we don't need it)
                Some(&mut volume_serial),      // Serial number
                None,                          // Max component length (we don't need it)
                None,                          // Filesystem flags (we don't need it)
                Some(&mut fs_name_buf),        // Filesystem name buffer
            )
        };

        if result.is_err() {
            // Drive doesn't exist or isn't accessible, skip it
            continue;
        }

        // Parse filesystem name
        let fs_name_len = fs_name_buf.iter().position(|&c| c == 0).unwrap_or(fs_name_buf.len());
        let fs_name = String::from_utf16_lossy(&fs_name_buf[..fs_name_len]);

        let fs_type = match fs_name.to_uppercase().as_str() {
            "NTFS" => VolumeType::NTFS,
            "FAT32" => VolumeType::FAT32,
            "EXFAT" => VolumeType::ExFAT,
            _ => VolumeType::Unknown,
        };

        // Get disk space information
        let mut total_bytes: u64 = 0;
        let mut free_bytes: u64 = 0;

        let space_result = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR::from_raw(root_wide.as_ptr()),
                None,                        // Free bytes available to caller
                Some(&mut total_bytes),      // Total bytes
                Some(&mut free_bytes),       // Total free bytes
            )
        };

        if space_result.is_err() {
            // Could get volume info but not space info, use zeros
            total_bytes = 0;
            free_bytes = 0;
        }

        volumes.push(VolumeInfo {
            drive_letter: letter,
            volume_serial: format!("{:08X}", volume_serial),
            fs_type,
            total_size: total_bytes,
            free_space: free_bytes,
        });

        tracing::debug!(
            "Detected volume {}: {} (serial: {:08X}, total: {} GB, free: {} GB)",
            letter,
            fs_name,
            volume_serial,
            total_bytes / 1_073_741_824,
            free_bytes / 1_073_741_824
        );
    }

    volumes
}

/// Stub for non-Windows platforms - returns empty list.
#[cfg(not(windows))]
pub fn detect_volumes() -> Vec<VolumeInfo> {
    tracing::warn!("Volume detection is only available on Windows");
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ntfs() {
        let ntfs_volume = VolumeInfo {
            drive_letter: 'C',
            volume_serial: "12345678".to_string(),
            fs_type: VolumeType::NTFS,
            total_size: 1_000_000_000,
            free_space: 500_000_000,
        };
        assert!(is_ntfs(&ntfs_volume));

        let fat32_volume = VolumeInfo {
            drive_letter: 'D',
            volume_serial: "ABCDEF01".to_string(),
            fs_type: VolumeType::FAT32,
            total_size: 100_000_000,
            free_space: 50_000_000,
        };
        assert!(!is_ntfs(&fat32_volume));
    }

    #[test]
    fn test_volume_type_equality() {
        assert_eq!(VolumeType::NTFS, VolumeType::NTFS);
        assert_ne!(VolumeType::NTFS, VolumeType::FAT32);
        assert_ne!(VolumeType::FAT32, VolumeType::ExFAT);
    }
}

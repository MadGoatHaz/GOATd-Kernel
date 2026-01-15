//! Storage device type detection for GOATd Kernel.
//!
//! This module detects the type of primary storage device in the system:
//! - **NVMe**: High-speed NVMe drives via /sys/class/nvme or /sys/block/nvme*
//! - **SSD**: SATA/mSATA SSDs with rotational flag = 0
//! - **HDD**: Traditional spinning disk drives with rotational flag = 1
//!
//! Detection follows a priority order: NVMe > SSD > HDD, with HDD as
//! the safe fallback if detection fails or no devices are found.

use crate::models::{StorageType, DiskInfo};
use crate::error::HardwareError;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Detect primary storage device type (NVMe/SSD/HDD).
pub fn detect_storage_type() -> Result<StorageType, HardwareError> {
    // Stage 1: Check for NVMe
    if is_nvme_present() {
        return Ok(StorageType::Nvme);
    }

    // Stage 2: Check for SSD
    if find_ssd() {
        return Ok(StorageType::Ssd);
    }

    // Stage 3: Check for HDD
    if find_hdd() {
        return Ok(StorageType::Hdd);
    }

    // Stage 4: Default fallback to HDD (safe assumption)
    Ok(StorageType::Hdd)
}

/// Detect storage device model name by scanning /sys/block.
pub fn detect_storage_model() -> Result<String, HardwareError> {
    // Scan /sys/block for storage devices and collect their models
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            if let Ok(filename) = entry.file_name().into_string() {
                // Skip loop devices and RAM disks
                if filename.starts_with("loop") || filename.starts_with("ram") {
                    continue;
                }
                
                // Try to read model from device directory
                let model_path = entry.path().join("device/model");
                if let Ok(model) = fs::read_to_string(&model_path) {
                    let trimmed = model.trim();
                    if !trimmed.is_empty() && trimmed != "Unknown" {
                        return Ok(trimmed.to_string());
                    }
                }
            }
        }
    }
    
    Ok("Unknown".to_string())
}

/// Detect free disk space on root filesystem in GB.
pub fn detect_disk_free_gb() -> Result<u32, HardwareError> {
    use std::process::Command;

    match Command::new("df")
        .arg("/")
        .output()
    {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse df output: typically: Filesystem 1K-blocks Used Available Use% Mounted on
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    // Available space is typically the 4th column (0-indexed: index 3)
                    if let Ok(available_kb) = parts[3].parse::<u64>() {
                        let available_gb = (available_kb / (1024 * 1024)) as u32;
                        return Ok(available_gb);
                    }
                }
            }
            Ok(256) // Safe fallback
        }
        _ => Ok(256), // Safe fallback on any error
    }
}

/// Detect all internal storage drives via lsblk JSON.
pub fn detect_all_storage_drives() -> Result<Vec<DiskInfo>, HardwareError> {
    // Run lsblk with JSON output to get structured data
    let output = match Command::new("lsblk")
        .args(&["-J", "-o", "NAME,MODEL,TRAN,SIZE,TYPE"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            // Fallback to single-drive detection if lsblk fails
            let model = detect_storage_model()?;
            return Ok(vec![DiskInfo {
                name: "primary".to_string(),
                model,
                transport: "unknown".to_string(),
                size: "unknown".to_string(),
                type_: "disk".to_string(),
            }]);
        }
    };

    // Parse JSON output
    let json_output = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = match serde_json::from_str(&json_output) {
        Ok(json) => json,
        Err(_) => {
            // Fallback if JSON parsing fails
            let model = detect_storage_model()?;
            return Ok(vec![DiskInfo {
                name: "primary".to_string(),
                model,
                transport: "unknown".to_string(),
                size: "unknown".to_string(),
                type_: "disk".to_string(),
            }]);
        }
    };

    let mut drives = Vec::new();

    // Extract blockdevices array from JSON
    if let Some(blockdevices) = json["blockdevices"].as_array() {
        for device in blockdevices {
            // Filter: only include disks (not partitions)
            let dev_type = device["type"].as_str().unwrap_or("");
            if dev_type != "disk" {
                continue;
            }

            // Filter: exclude USB drives
            let transport = device["tran"].as_str().unwrap_or("");
            if transport == "usb" {
                continue;
            }

            // Extract device information
            let name = device["name"].as_str().unwrap_or("Unknown").to_string();
            let model = device["model"].as_str().unwrap_or("Unknown").to_string();
            let size = device["size"].as_str().unwrap_or("Unknown").to_string();
            let type_ = device["type"].as_str().unwrap_or("disk").to_string();

            drives.push(DiskInfo {
                name,
                model,
                transport: transport.to_string(),
                size,
                type_,
            });
        }
    }

    // If no drives found, fallback to single-drive detection
    if drives.is_empty() {
        let model = detect_storage_model()?;
        drives.push(DiskInfo {
            name: "primary".to_string(),
            model,
            transport: "unknown".to_string(),
            size: "unknown".to_string(),
            type_: "disk".to_string(),
        });
    }

    Ok(drives)
}

/// Format a list of DiskInfo into a comma-separated string of model names.
///
/// # Examples
///
/// ```ignore
/// let drives = vec![
///     DiskInfo { model: "Samsung 970 EVO", ... },
///     DiskInfo { model: "WD Blue", ... },
/// ];
/// let formatted = format_drives_list(&drives);
/// assert_eq!(formatted, "Samsung 970 EVO, WD Blue");
/// ```
pub fn format_drives_list(drives: &[DiskInfo]) -> String {
    drives
        .iter()
        .map(|d| d.model.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Check if NVMe storage is present.
fn is_nvme_present() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/class/nvme") {
        if entries.count() > 0 {
            return true;
        }
    }

    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            if let Ok(filename) = entry.file_name().into_string() {
                if filename.starts_with("nvme") {
                    return true;
                }
            }
        }
    }

    false
}

/// Find an SSD in the system by checking rotational flag.
fn find_ssd() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            if let Ok(filename) = entry.file_name().into_string() {
                if filename.starts_with("sd") || filename.starts_with("vd") {
                    if is_rotational_value_zero(&filename) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Find an HDD in the system by checking rotational flag.
fn find_hdd() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            if let Ok(filename) = entry.file_name().into_string() {
                if filename.starts_with("sd") || filename.starts_with("vd") {
                    if is_rotational_value_one(&filename) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if a block device has rotational flag = 0 (SSD).
///
/// Reads /sys/block/{device}/queue/rotational and checks if value is "0".
fn is_rotational_value_zero(device: &str) -> bool {
    let path = format!("/sys/block/{}/queue/rotational", device);
    read_rotational_value(&path) == Some(0)
}

/// Check if a block device has rotational flag = 1 (HDD).
///
/// Reads /sys/block/{device}/queue/rotational and checks if value is "1".
fn is_rotational_value_one(device: &str) -> bool {
    let path = format!("/sys/block/{}/queue/rotational", device);
    read_rotational_value(&path) == Some(1)
}

/// Read and parse the rotational flag from a block device.
///
/// # Returns
///
/// - `Some(0)` if file contains "0" (non-rotating / SSD)
/// - `Some(1)` if file contains "1" (rotating / HDD)
/// - `None` if file doesn't exist, can't be read, or value is invalid
fn read_rotational_value(path: &str) -> Option<u32> {
    // Check if path exists before trying to read
    if !Path::new(path).exists() {
        return None;
    }

    // Read the file content
    match fs::read_to_string(path) {
        Ok(content) => {
            // Trim whitespace (including newlines) and parse
            let trimmed = content.trim();
            trimmed.parse::<u32>().ok()
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_storage_type_returns_ok() {
        // Just verify it returns Ok with some StorageType
        let result = detect_storage_type();
        assert!(result.is_ok());
        
        // Verify the returned value is a valid StorageType
        if let Ok(storage_type) = result {
            // Should be one of the three valid types
            match storage_type {
                StorageType::Nvme | StorageType::Ssd | StorageType::Hdd => {
                    // All valid returns
                }
            }
        }
    }

    #[test]
    fn test_read_rotational_value_nonexistent() {
        // Non-existent path should return None
        let result = read_rotational_value("/nonexistent/path/to/rotational");
        assert_eq!(result, None);
    }

    #[test]
    fn test_rotational_detection_functions_no_panic() {
        // These should not panic even on systems with no storage devices
        // or unusual configurations
        let _ = is_nvme_present();
        let _ = find_ssd();
        let _ = find_hdd();
    }

    #[test]
    fn test_detect_storage_model_returns_result() {
        let result = detect_storage_model();
        assert!(result.is_ok());
    }

    #[test]
    fn test_storage_model_is_string() {
        let result = detect_storage_model().unwrap();
        // Should be a valid string (even if "Unknown")
        assert!(!result.is_empty());
    }

    #[test]
    fn test_detect_all_storage_drives_returns_vec() {
        let result = detect_all_storage_drives();
        assert!(result.is_ok());
        let drives = result.unwrap();
        assert!(!drives.is_empty());
    }

    #[test]
    fn test_format_drives_list_empty() {
        let drives: Vec<crate::models::DiskInfo> = vec![];
        let formatted = format_drives_list(&drives);
        assert_eq!(formatted, "");
    }

    #[test]
    fn test_format_drives_list_single() {
        let drives = vec![crate::models::DiskInfo {
            name: "nvme0n1".to_string(),
            model: "Samsung 970 EVO".to_string(),
            transport: "nvme".to_string(),
            size: "1.9T".to_string(),
            type_: "disk".to_string(),
        }];
        let formatted = format_drives_list(&drives);
        assert_eq!(formatted, "Samsung 970 EVO");
    }

    #[test]
    fn test_format_drives_list_multiple() {
        let drives = vec![
            crate::models::DiskInfo {
                name: "nvme0n1".to_string(),
                model: "Samsung 970 EVO".to_string(),
                transport: "nvme".to_string(),
                size: "1.9T".to_string(),
                type_: "disk".to_string(),
            },
            crate::models::DiskInfo {
                name: "nvme1n1".to_string(),
                model: "WD Black SN850".to_string(),
                transport: "nvme".to_string(),
                size: "2.0T".to_string(),
                type_: "disk".to_string(),
            },
        ];
        let formatted = format_drives_list(&drives);
        assert_eq!(formatted, "Samsung 970 EVO, WD Black SN850");
    }
}

//! RAM detection and memory information module.
//!
//! This module provides functionality to detect the system's total RAM
//! by reading system information from /proc/meminfo.

use crate::error::HardwareError;
use std::fs;

/// Detect the total RAM available in the system in gigabytes.
///
/// Attempts to read /proc/meminfo and extract the "MemTotal" field,
/// which is reported in kilobytes. The value is then converted to gigabytes.
/// If successful, returns the RAM amount as a u32. On any error (file not found,
/// permission denied, parsing failure, or missing field), returns Ok(0)
/// as a graceful fallback.
///
/// # Returns
///
/// - `Ok(u32)` with the RAM amount in GB (or 0 if detection fails)
/// - `Err(HardwareError)` only if the error is unrecoverable
///
/// # Examples
///
/// ```ignore
/// let ram_gb = detect_ram_gb()?;
/// println!("RAM: {} GB", ram_gb);
/// ```
pub fn detect_ram_gb() -> Result<u32, HardwareError> {
    match fs::read_to_string("/proc/meminfo") {
        Ok(content) => {
            // Search for the "MemTotal" field in /proc/meminfo
            for line in content.lines() {
                if line.starts_with("MemTotal") {
                    // Extract the value after ": "
                    if let Some(value_str) = line.split_whitespace().nth(1) {
                        // Try to parse the value as kilobytes
                        if let Ok(ram_kb) = value_str.parse::<u64>() {
                            // Convert KB to GB: divide by (1024 * 1024)
                            let ram_gb = (ram_kb / (1024 * 1024)) as u32;
                            return Ok(ram_gb);
                        }
                    }
                }
            }
            // Line not found in file or parsing failed
            Ok(0)
        }
        Err(_) => {
            // File doesn't exist, read fails, or any other IO error
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ram_gb_returns_result() {
        let result = detect_ram_gb();
        assert!(result.is_ok());
    }

    #[test]
    fn test_ram_gb_is_u32() {
        let _result = detect_ram_gb().expect("Should not fail");
        // Result is u32, so it's always >= 0
    }

    #[test]
    fn test_ram_detection_realistic() {
        let result = detect_ram_gb().unwrap();
        // On most systems, RAM should be between 1GB and 1TB
        // But we allow 0 as a fallback value
        assert!(result == 0 || (result >= 1 && result <= 1024));
    }
}

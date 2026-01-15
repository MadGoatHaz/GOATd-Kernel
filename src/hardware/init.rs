//! Init system detection module.
//!
//! This module provides functionality to detect which init system is installed
//! and active on the system (systemd, runit, openrc, dinit, or unknown).

use crate::error::HardwareError;
use std::fs;
use std::path::Path;

/// Detect init system (systemd/runit/openrc/dinit).
pub fn detect_init_system() -> Result<String, HardwareError> {
    if let Some(init) = detect_via_proc_1_comm() {
        return Ok(init);
    }

    if let Some(init) = detect_via_sbin_init() {
        return Ok(init);
    }

    if let Some(init) = detect_via_systemd_run() {
        return Ok(init);
    }

    let init = detect_via_etc_directories();
    Ok(init)
}

/// Try to detect init system via /proc/1/comm file.
///
/// This is the most reliable method: reads the init process name from
/// /proc/1/comm which contains the command name of process ID 1.
///
/// # Returns
///
/// - `Some(String)` with init system name if successfully detected
/// - `None` if file cannot be read or name doesn't match known systems
fn detect_via_proc_1_comm() -> Option<String> {
    match fs::read_to_string("/proc/1/comm") {
        Ok(content) => {
            let comm = content.trim();
            match comm {
                "systemd" => Some("systemd".to_string()),
                "openrc" => Some("openrc".to_string()),
                "runit" => Some("runit".to_string()),
                "dinit" => Some("dinit".to_string()),
                _ => None,
            }
        }
        Err(_) => None,
    }
}

/// Try to detect init system via /sbin/init symlink or file.
///
/// Reads the /sbin/init symlink target (or file path) and checks if the
/// path contains references to known init systems.
///
/// # Returns
///
/// - `Some(String)` with init system name if symlink target matches a known init
/// - `None` if /sbin/init cannot be read or target doesn't match known systems
fn detect_via_sbin_init() -> Option<String> {
    // Try to read symlink target
    match fs::read_link("/sbin/init") {
        Ok(path_buf) => {
            let path_str = path_buf.to_string_lossy();
            if path_str.contains("systemd") {
                return Some("systemd".to_string());
            } else if path_str.contains("openrc") {
                return Some("openrc".to_string());
            } else if path_str.contains("runit") {
                return Some("runit".to_string());
            } else if path_str.contains("dinit") {
                return Some("dinit".to_string());
            }
        }
        Err(_) => {}
    }

    None
}

/// Try to detect init system via /run/systemd/system/ directory.
///
/// Checks if the systemd runtime directory exists, which indicates
/// systemd is the init system.
///
/// # Returns
///
/// - `Some("systemd")` if /run/systemd/system/ exists
/// - `None` otherwise
fn detect_via_systemd_run() -> Option<String> {
    if Path::new("/run/systemd/system").exists() {
        Some("systemd".to_string())
    } else {
        None
    }
}

/// Try to detect init system via init system config directories.
///
/// Checks for the presence of init system-specific configuration directories:
/// - /etc/runit/ for runit
/// - /etc/openrc/ for OpenRC
/// - /etc/dinit.d/ for dinit
///
/// # Returns
///
/// - `"runit"` if /etc/runit/ exists
/// - `"openrc"` if /etc/openrc/ exists
/// - `"dinit"` if /etc/dinit.d/ exists
/// - `"unknown"` if none exist
fn detect_via_etc_directories() -> String {
    // Check for runit config directory
    if Path::new("/etc/runit").exists() {
        return "runit".to_string();
    }

    // Check for OpenRC config directory
    if Path::new("/etc/openrc").exists() {
        return "openrc".to_string();
    }

    // Check for dinit config directory
    if Path::new("/etc/dinit.d").exists() {
        return "dinit".to_string();
    }

    // Default fallback
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_init_system_returns_result() {
        let result = detect_init_system();
        assert!(result.is_ok());
    }

    #[test]
    fn test_init_system_is_valid_string() {
        let init_sys = detect_init_system().unwrap();
        assert!(
            init_sys == "systemd"
                || init_sys == "runit"
                || init_sys == "openrc"
                || init_sys == "dinit"
                || init_sys == "unknown"
        );
    }

    #[test]
    fn test_init_system_not_empty() {
        let init_sys = detect_init_system().unwrap();
        assert!(!init_sys.is_empty());
    }

    #[test]
    fn test_proc_1_comm_detection() {
        // This test will only work on Linux systems where /proc/1/comm exists
        if Path::new("/proc/1/comm").exists() {
            let result = detect_via_proc_1_comm();
            // Result can be Some or None depending on the system
            // Just verify the function doesn't panic
            let _ = result;
        }
    }

    #[test]
    fn test_sbin_init_detection() {
        // This test verifies the function doesn't panic
        let result = detect_via_sbin_init();
        // Result can be Some or None depending on the system
        let _ = result;
    }

    #[test]
    fn test_systemd_run_detection() {
        // This test verifies the function works
        let result = detect_via_systemd_run();
        // Result can be Some or None depending on the system
        let _ = result;
    }

    #[test]
    fn test_etc_directories_detection() {
        let result = detect_via_etc_directories();
        assert!(
            result == "runit"
                || result == "openrc"
                || result == "dinit"
                || result == "unknown"
        );
    }
}

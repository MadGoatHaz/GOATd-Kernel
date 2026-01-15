//! Driver exclusion management module.
//!
//! Manages driver exclusion lists for filtering out specific drivers.
//! Provides functionality to add, remove, and validate driver exclusions
//! while protecting essential drivers from being excluded.
//!
//! This module handles:
//! - Adding drivers to the exclusion list
//! - Removing drivers from the exclusion list
//! - Batch application of multiple exclusions
//! - Validation of exclusion lists
//! - Integration with the whitelist to prevent excluding essential drivers
//!
//! # Examples
//!
//! ```no_run
//! use goatd_kernel::config::exclusions::*;
//! use goatd_kernel::models::KernelConfig;
//! use std::collections::HashMap;
//!
//! let mut config = KernelConfig::default();
//! config.kernel_version = "6.6.0".to_string();
//! config.use_whitelist = true;
//!
//! // Add a non-essential driver to exclusions
//! add_exclusion(&mut config, "nouveau_fake")?;
//!
//! // Cannot add an essential driver
//! let result = add_exclusion(&mut config, "i915");  // Error: essential driver
//! assert!(result.is_err());
//!
//! // Get list of current exclusions
//! let exclusions = get_exclusions(&config);
//! assert!(exclusions.contains(&"nouveau_fake".to_string()));
//!
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::ConfigError;
use crate::models::{KernelConfig, GpuVendor, HardwareInfo};
use super::whitelist;

/// Add a driver exclusion to the configuration.
///
/// Excludes a specific driver from the kernel build. The driver name is
/// normalized to lowercase and checked against the essential drivers whitelist.
/// Prevents duplicate exclusions.
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
/// * `driver` - Name of the driver to exclude (case-insensitive)
///
/// # Returns
///
/// `Ok(())` if the driver was successfully added to exclusions.
/// `Err(ConfigError::ValidationFailed)` if the driver is essential and cannot be excluded.
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::add_exclusion;
/// # use goatd_kernel::models::KernelConfig;
/// # use std::collections::HashMap;
/// #
/// # let mut config = KernelConfig::default();
/// # config.use_whitelist = true;
/// #
/// // Add a non-essential driver
/// add_exclusion(&mut config, "custom_driver")?;
///
/// // Try to add an essential driver (fails with ValidationFailed)
/// assert!(add_exclusion(&mut config, "ext4").is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn add_exclusion(config: &mut KernelConfig, driver: &str) -> Result<(), ConfigError> {
    // Check if driver is essential
    if whitelist::is_essential_driver(driver) {
        return Err(ConfigError::ValidationFailed(
            format!(
                "Cannot exclude essential driver '{}': it is required for basic system functionality",
                driver
            )
        ));
    }

    // Normalize driver name to lowercase
    let normalized_driver = driver.to_lowercase();

    // Check for duplicate (case-insensitive)
    let driver_lower = normalized_driver.to_lowercase();
    if config.driver_exclusions
        .iter()
        .any(|d| d.to_lowercase() == driver_lower)
    {
        // Driver already in exclusions, not an error - just skip
        return Ok(());
    }

    // Add to exclusions
    config.driver_exclusions.push(normalized_driver);
    Ok(())
}

/// Remove a driver exclusion from the configuration.
///
/// Removes a driver from the exclusion list if present. Performs case-insensitive
/// matching to handle driver name variations.
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
/// * `driver` - Name of the driver to remove from exclusions (case-insensitive)
///
/// # Returns
///
/// `true` if the driver was found and removed, `false` if it was not in the exclusion list.
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::*;
/// # use goatd_kernel::models::KernelConfig;
/// # use std::collections::HashMap;
/// #
/// # let mut config = KernelConfig::default();
/// # config.driver_exclusions = vec!["custom_driver".to_string()];
/// #
/// // Remove an excluded driver
/// assert!(remove_exclusion(&mut config, "custom_driver"));
/// assert!(config.driver_exclusions.is_empty());
///
/// // Try to remove a non-existent driver
/// assert!(!remove_exclusion(&mut config, "nonexistent"));
/// ```
pub fn remove_exclusion(config: &mut KernelConfig, driver: &str) -> bool {
    let driver_lower = driver.to_lowercase();
    
    if let Some(index) = config.driver_exclusions
        .iter()
        .position(|d| d.to_lowercase() == driver_lower)
    {
        config.driver_exclusions.remove(index);
        true
    } else {
        false
    }
}

/// Clear all driver exclusions from the configuration.
///
/// Removes all drivers from the exclusion list, resetting the configuration
/// to have no excluded drivers.
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::clear_exclusions;
/// # use goatd_kernel::models::KernelConfig;
/// #
/// # let mut config = KernelConfig::default();
/// # config.driver_exclusions = vec!["custom_driver".to_string()];
/// #
/// clear_exclusions(&mut config);
/// assert!(config.driver_exclusions.is_empty());
/// ```
pub fn clear_exclusions(config: &mut KernelConfig) {
    config.driver_exclusions.clear();
}

/// Get the list of currently excluded drivers.
///
/// Returns a sorted vector of all drivers currently in the exclusion list.
/// The returned list is sorted for consistent ordering and easier comparison.
///
/// # Arguments
///
/// * `config` - Reference to KernelConfig to read from
///
/// # Returns
///
/// A sorted vector of driver names currently in the exclusion list.
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::*;
/// # use goatd_kernel::models::KernelConfig;
/// #
/// # let mut config = KernelConfig::default();
/// # config.driver_exclusions = vec!["zebra".to_string(), "alpha".to_string()];
/// #
/// let exclusions = get_exclusions(&config);
/// assert_eq!(exclusions.len(), 2);
/// // Note: returned list is sorted
/// assert_eq!(exclusions[0], "alpha");
/// assert_eq!(exclusions[1], "zebra");
/// ```
pub fn get_exclusions(config: &KernelConfig) -> Vec<String> {
    let mut exclusions = config.driver_exclusions.clone();
    exclusions.sort();
    exclusions
}

/// Validate the exclusion list in the configuration.
///
/// Checks that no essential drivers are in the exclusion list by delegating
/// to the whitelist validation. This ensures the exclusion list complies
/// with whitelist protection rules.
///
/// # Arguments
///
/// * `config` - Reference to KernelConfig to validate
///
/// # Returns
///
/// `Ok(())` if the exclusion list is valid (no essential drivers are excluded).
/// `Err(ConfigError::ValidationFailed)` if essential drivers are in the exclusion list.
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::validate_exclusions;
/// # use goatd_kernel::models::KernelConfig;
/// #
/// # let mut config = KernelConfig::default();
/// # config.driver_exclusions = vec!["custom_driver".to_string()];
/// #
/// // Valid configuration: no essential drivers excluded
/// assert!(validate_exclusions(&config).is_ok());
/// ```
pub fn validate_exclusions(config: &KernelConfig) -> Result<(), ConfigError> {
    // Use whitelist validation to check no essential drivers are excluded
    whitelist::validate_whitelist(config)
}

/// Apply multiple driver exclusions to the configuration in batch.
///
/// Attempts to add multiple drivers to the exclusion list at once. If any
/// driver fails validation (e.g., is essential), the operation stops at
/// the first error. Earlier additions are kept (idempotent behavior).
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
/// * `exclusions` - Slice of driver names to exclude
///
/// # Returns
///
/// `Ok(())` if all drivers were successfully added.
/// `Err(ConfigError::ValidationFailed)` if any driver is essential or invalid.
/// On error, some drivers may have been added before the error occurred.
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::apply_exclusions;
/// # use goatd_kernel::models::KernelConfig;
/// #
/// # let mut config = KernelConfig::default();
/// #
/// // Apply multiple non-essential drivers
/// apply_exclusions(&mut config, &["driver1", "driver2"])?;
/// assert_eq!(config.driver_exclusions.len(), 2);
///
/// // If any driver is essential, the whole operation fails at that driver
/// let result = apply_exclusions(&mut config, &["driver3", "i915"]);
/// assert!(result.is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn apply_exclusions(config: &mut KernelConfig, exclusions: &[&str]) -> Result<(), ConfigError> {
    for driver in exclusions {
        add_exclusion(config, driver)?;
    }
    Ok(())
}

/// Apply GPU-aware driver exclusions based on detected hardware.
///
/// Automatically excludes GPU drivers that don't match the detected GPU vendor.
/// This ensures aggressive module filtering by removing unused GPU driver subsystems.
///
/// **GPU Driver Exclusion Policy**:
/// - **NVIDIA GPU detected**: Excludes AMD (amdgpu, radeon) and Intel (i915, xe) drivers
/// - **AMD GPU detected**: Excludes NVIDIA (nouveau, nvidia) and Intel (i915, xe) drivers
/// - **Intel GPU detected**: Excludes NVIDIA (nouveau, nvidia) and AMD (amdgpu, radeon) drivers
/// - **No GPU detected**: Excludes all dedicated GPU drivers (nouveau, nvidia, amdgpu, radeon, i915, xe)
///
/// This is designed to work seamlessly with modprobed-db filtering to achieve
/// maximum kernel size reduction when building on systems with specific GPUs.
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
/// * `hardware_info` - Detected hardware information including GPU vendor
///
/// # Returns
///
/// `Ok(())` if GPU exclusions were successfully applied.
/// `Err(ConfigError)` if an essential driver cannot be excluded (should not happen).
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::exclusions::apply_gpu_exclusions;
/// # use goatd_kernel::models::KernelConfig;
/// # use goatd_kernel::hardware::{HardwareInfo, GpuVendor};
/// #
/// # let mut config = KernelConfig::default();
/// # let hardware = HardwareInfo {
/// #     gpu_vendor: GpuVendor::NVIDIA,
/// #     ..HardwareInfo::default()
/// # };
/// #
/// // Automatically exclude AMD and Intel GPU drivers when NVIDIA is detected
/// apply_gpu_exclusions(&mut config, &hardware)?;
/// assert!(config.driver_exclusions.iter().any(|d| d.contains("amdgpu")));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn apply_gpu_exclusions(config: &mut KernelConfig, hardware_info: &HardwareInfo) -> Result<(), ConfigError> {
    match hardware_info.gpu_vendor {
        GpuVendor::Nvidia => {
            // NVIDIA GPU: exclude AMD and Intel drivers
            let gpu_drivers = vec!["amdgpu", "radeon", "i915", "xe"];
            apply_exclusions(config, &gpu_drivers)?;
            eprintln!("[GPU-EXCLUSION] NVIDIA GPU detected: excluding AMD (amdgpu, radeon) and Intel (i915, xe) drivers");
        },
        GpuVendor::Amd => {
            // AMD GPU: exclude NVIDIA and Intel drivers
            let gpu_drivers = vec!["nouveau", "nvidia", "i915", "xe"];
            apply_exclusions(config, &gpu_drivers)?;
            eprintln!("[GPU-EXCLUSION] AMD GPU detected: excluding NVIDIA (nouveau, nvidia) and Intel (i915, xe) drivers");
        },
        GpuVendor::Intel => {
            // Intel GPU: exclude NVIDIA and AMD drivers
            let gpu_drivers = vec!["nouveau", "nvidia", "amdgpu", "radeon"];
            apply_exclusions(config, &gpu_drivers)?;
            eprintln!("[GPU-EXCLUSION] Intel GPU detected: excluding NVIDIA (nouveau, nvidia) and AMD (amdgpu, radeon) drivers");
        },
        GpuVendor::Unknown => {
            // No dedicated GPU: exclude all dedicated GPU drivers
            let gpu_drivers = vec!["nouveau", "nvidia", "amdgpu", "radeon", "i915", "xe"];
            apply_exclusions(config, &gpu_drivers)?;
            eprintln!("[GPU-EXCLUSION] No dedicated GPU detected: excluding all GPU drivers (nouveau, nvidia, amdgpu, radeon, i915, xe)");
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_exclusion_non_essential() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            use_whitelist: true,
            ..KernelConfig::default()
        };

        let result = add_exclusion(&mut config, "custom_driver");
        assert!(result.is_ok());
        assert_eq!(config.driver_exclusions.len(), 1);
        assert_eq!(config.driver_exclusions[0], "custom_driver");
    }

    #[test]
    fn test_add_exclusion_essential_driver() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            use_whitelist: true,
            ..KernelConfig::default()
        };

        let result = add_exclusion(&mut config, "ext4");
        assert!(result.is_err());
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_add_exclusion_case_insensitive() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            use_whitelist: true,
            ..KernelConfig::default()
        };

        let result = add_exclusion(&mut config, "Custom_Driver");
        assert!(result.is_ok());
        assert_eq!(config.driver_exclusions[0], "custom_driver");
    }

    #[test]
    fn test_add_exclusion_prevents_duplicates() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = add_exclusion(&mut config, "custom_driver");
        assert!(result.is_ok());
        assert_eq!(config.driver_exclusions.len(), 1);
    }

    #[test]
    fn test_add_exclusion_prevents_duplicates_case_insensitive() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = add_exclusion(&mut config, "CUSTOM_DRIVER");
        assert!(result.is_ok());
        assert_eq!(config.driver_exclusions.len(), 1);
    }

    #[test]
    fn test_remove_exclusion_found() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = remove_exclusion(&mut config, "custom_driver");
        assert!(result);
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_remove_exclusion_not_found() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = remove_exclusion(&mut config, "nonexistent");
        assert!(!result);
        assert_eq!(config.driver_exclusions.len(), 1);
    }

    #[test]
    fn test_remove_exclusion_case_insensitive() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = remove_exclusion(&mut config, "CUSTOM_DRIVER");
        assert!(result);
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_clear_exclusions() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec![
                "driver1".to_string(),
                "driver2".to_string(),
                "driver3".to_string(),
            ],
            ..KernelConfig::default()
        };

        clear_exclusions(&mut config);
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_get_exclusions_empty() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec![],
            ..KernelConfig::default()
        };

        let exclusions = get_exclusions(&config);
        assert!(exclusions.is_empty());
    }

    #[test]
    fn test_get_exclusions_sorted() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["zebra".to_string(), "alpha".to_string(), "beta".to_string()],
            ..KernelConfig::default()
        };

        let exclusions = get_exclusions(&config);
        assert_eq!(exclusions.len(), 3);
        assert_eq!(exclusions[0], "alpha");
        assert_eq!(exclusions[1], "beta");
        assert_eq!(exclusions[2], "zebra");
    }

    #[test]
    fn test_validate_exclusions_clean() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["custom_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = validate_exclusions(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_exclusions_violation() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["ext4".to_string()],
            ..KernelConfig::default()
        };

        let result = validate_exclusions(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_exclusions_all_valid() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let drivers = vec!["driver1", "driver2", "driver3"];
        let result = apply_exclusions(&mut config, &drivers);
        assert!(result.is_ok());
        assert_eq!(config.driver_exclusions.len(), 3);
    }

    #[test]
    fn test_apply_exclusions_stops_on_error() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let drivers = vec!["driver1", "ext4", "driver3"];
        let result = apply_exclusions(&mut config, &drivers);
        assert!(result.is_err());
        // driver1 should have been added before error
        assert_eq!(config.driver_exclusions.len(), 1);
        assert_eq!(config.driver_exclusions[0], "driver1");
    }

    #[test]
    fn test_apply_exclusions_empty() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let drivers: Vec<&str> = vec![];
        let result = apply_exclusions(&mut config, &drivers);
        assert!(result.is_ok());
    }

    #[test]
    fn test_essential_driver_protection() {
        let essential_drivers = vec!["ext4", "nvme", "evdev", "hid", "usbhid"];
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        for driver in essential_drivers {
            let result = add_exclusion(&mut config, driver);
            assert!(
                result.is_err(),
                "Essential driver {} should not be excludable",
                driver
            );
            assert!(config.driver_exclusions.is_empty());
        }
    }

    #[test]
    fn test_apply_gpu_exclusions_nvidia() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        let result = apply_gpu_exclusions(&mut config, &hardware);
        assert!(result.is_ok());
        
        // AMD and Intel drivers should be excluded
        let exclusions = get_exclusions(&config);
        assert!(exclusions.contains(&"amdgpu".to_string()));
        assert!(exclusions.contains(&"radeon".to_string()));
        assert!(exclusions.contains(&"i915".to_string()));
        assert!(exclusions.contains(&"xe".to_string()));
        
        // NVIDIA drivers should NOT be excluded
        assert!(!exclusions.contains(&"nouveau".to_string()));
        assert!(!exclusions.contains(&"nvidia".to_string()));
    }

    #[test]
    fn test_apply_gpu_exclusions_amd() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Amd,
            ..HardwareInfo::default()
        };

        let result = apply_gpu_exclusions(&mut config, &hardware);
        assert!(result.is_ok());
        
        // NVIDIA and Intel drivers should be excluded
        let exclusions = get_exclusions(&config);
        assert!(exclusions.contains(&"nouveau".to_string()));
        assert!(exclusions.contains(&"nvidia".to_string()));
        assert!(exclusions.contains(&"i915".to_string()));
        assert!(exclusions.contains(&"xe".to_string()));
        
        // AMD drivers should NOT be excluded
        assert!(!exclusions.contains(&"amdgpu".to_string()));
        assert!(!exclusions.contains(&"radeon".to_string()));
    }

    #[test]
    fn test_apply_gpu_exclusions_intel() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Intel,
            ..HardwareInfo::default()
        };

        let result = apply_gpu_exclusions(&mut config, &hardware);
        assert!(result.is_ok());
        
        // NVIDIA and AMD drivers should be excluded
        let exclusions = get_exclusions(&config);
        assert!(exclusions.contains(&"nouveau".to_string()));
        assert!(exclusions.contains(&"nvidia".to_string()));
        assert!(exclusions.contains(&"amdgpu".to_string()));
        assert!(exclusions.contains(&"radeon".to_string()));
        
        // Intel drivers should NOT be excluded
        assert!(!exclusions.contains(&"i915".to_string()));
        assert!(!exclusions.contains(&"xe".to_string()));
    }

    #[test]
    fn test_apply_gpu_exclusions_unknown() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Unknown,
            ..HardwareInfo::default()
        };

        let result = apply_gpu_exclusions(&mut config, &hardware);
        assert!(result.is_ok());
        
        // All GPU drivers should be excluded
        let exclusions = get_exclusions(&config);
        assert!(exclusions.contains(&"nouveau".to_string()));
        assert!(exclusions.contains(&"nvidia".to_string()));
        assert!(exclusions.contains(&"amdgpu".to_string()));
        assert!(exclusions.contains(&"radeon".to_string()));
        assert!(exclusions.contains(&"i915".to_string()));
        assert!(exclusions.contains(&"xe".to_string()));
    }
}

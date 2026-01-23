//! Config validation.

use crate::error::ConfigError;
use crate::models::{KernelConfig, LtoType};
use std::collections::HashMap;

/// Validate kernel version (X.Y.Z format or "latest" sentinel).
///
/// Accepts either:
/// - "latest" - sentinel value for dynamic versioning (will be resolved during build preparation)
/// - X.Y.Z format - concrete semantic version (e.g., "6.6.0", "6.6.0-generic")
pub fn validate_kernel_version(version: &str) -> Result<(), ConfigError> {
    // Check if version is empty
    if version.is_empty() {
        return Err(ConfigError::ValidationFailed(
            "Kernel version cannot be empty".to_string(),
        ));
    }

    // STEP 1: Accept "latest" as special sentinel value for dynamic versioning
    // This value is resolved to a concrete version during build preparation phase
    if version == "latest" {
        return Ok(());
    }

    // STEP 2: Validate concrete versions in X.Y.Z format
    // Split on first '-' to separate version from suffix (e.g., "6.6.0-generic")
    let version_part = version.split('-').next().unwrap_or(version);

    // Split into parts by '.'
    let parts: Vec<&str> = version_part.split('.').collect();

    // Require at least 3 parts (X.Y.Z)
    if parts.len() < 3 {
        return Err(ConfigError::ValidationFailed(format!(
            "Kernel version must follow semantic versioning (X.Y.Z) or be 'latest', got: {}",
            version
        )));
    }

    // Validate each part is a non-negative number
    for (i, part) in parts.iter().enumerate().take(3) {
        if part.is_empty() {
            return Err(ConfigError::ValidationFailed(format!(
                "Kernel version part {} is empty: {}",
                i, version
            )));
        }

        // Check if it's a valid positive number
        if part.parse::<u32>().is_err() {
            return Err(ConfigError::ValidationFailed(format!(
                "Kernel version part {} must be a non-negative number, got: {}",
                i, part
            )));
        }
    }

    Ok(())
}

/// Validate config options (keys/values non-empty).
pub fn validate_config_options(options: &HashMap<String, String>) -> Result<(), ConfigError> {
    for (key, value) in options.iter() {
        // Check if key is empty
        if key.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "Configuration option key cannot be empty".to_string(),
            ));
        }

        // Check if value is empty
        if value.is_empty() {
            return Err(ConfigError::ValidationFailed(format!(
                "Configuration option '{}' has an empty value",
                key
            )));
        }

        // Validate key format: should be uppercase alphanumeric with underscores
        // Allow CONFIG_ prefix and alphanumeric + underscore
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(ConfigError::ValidationFailed(format!(
                "Configuration option key '{}' contains invalid characters. \
                 Keys must be alphanumeric with underscores only",
                key
            )));
        }

        // Validate value: allow alphanumeric, underscores, hyphens, and dots
        if !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(ConfigError::ValidationFailed(format!(
                "Configuration option '{}' has invalid value '{}'. \
                 Values must be alphanumeric with underscores, hyphens, or dots only",
                key, value
            )));
        }
    }

    Ok(())
}

/// Detect config conflicts.
pub fn detect_conflicts(config: &KernelConfig) -> Result<(), ConfigError> {
    // Conflict 1: Full LTO + many exclusions
    if config.lto_type == LtoType::Full && config.driver_exclusions.len() > 10 {
        return Err(ConfigError::ConflictDetected(
            format!(
                "Potential conflict detected: Full LTO with {} driver exclusions may cause compilation issues. \
                 Consider using Thin LTO instead, or reduce the number of excluded drivers",
                config.driver_exclusions.len()
            )
        ));
    }

    // Conflict 2: Full LTO + many options
    if config.lto_type == LtoType::Full && config.config_options.len() > 15 {
        return Err(ConfigError::ConflictDetected(
            format!(
                "Potential conflict detected: Full LTO with {} custom config options may cause issues. \
                 Consider using Thin LTO, or reduce the number of custom options",
                config.config_options.len()
            )
        ));
    }

    // Conflict 3: Whitelist requires modprobed-db (CRITICAL SAFETY CONSTRAINT)
    if config.use_whitelist && !config.use_modprobed {
        return Err(ConfigError::ConflictDetected(
            "Safety constraint violated: 'Use whitelist safety net' requires 'Use modprobed-db (Driver Auto-Discovery)' to be enabled. \
             The whitelist is a protection layer for modprobed-db auto-discovery and cannot function independently."
                .to_string(),
        ));
    }

    // Conflict 4: Whitelist + exclusions conflict
    if config.use_whitelist && !config.driver_exclusions.is_empty() {
        return Err(ConfigError::ConflictDetected(
            "Conflicting driver settings: cannot use both whitelist (approved drivers) and exclusions (blocked drivers) simultaneously. \
             Choose one filtering strategy"
                .to_string(),
        ));
    }

    // Conflict 5: Full LTO + old kernel version
    let version_parts: Vec<&str> = config.version.split('.').collect();
    if !version_parts.is_empty() {
        if let Ok(major_version) = version_parts[0].parse::<u32>() {
            // Full LTO requires kernel >= 5.15 (reasonable assumption)
            if config.lto_type == LtoType::Full && major_version < 5 {
                return Err(ConfigError::ConflictDetected(
                    format!(
                        "Conflict detected: Full LTO requires kernel version 5.15 or later, but kernel {} is too old",
                        config.version
                    )
                ));
            }
        }
    }

    Ok(())
}

/// Comprehensive validation of all config params.
pub fn validate_all(config: &KernelConfig) -> Result<(), ConfigError> {
    validate_kernel_version(&config.version)?;
    validate_config_options(&config.config_options)?;
    detect_conflicts(config)?;
    Ok(())
}

/// Validate config (alias for validate_all).
pub fn validate_config(config: &KernelConfig) -> Result<(), ConfigError> {
    validate_all(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== validate_kernel_version tests ==========

    #[test]
    fn test_validate_kernel_version_valid() {
        assert!(validate_kernel_version("6.6.0").is_ok());
        assert!(validate_kernel_version("6.7.1").is_ok());
        assert!(validate_kernel_version("5.15.0").is_ok());
        assert!(validate_kernel_version("6.6.0-generic").is_ok());
        assert!(validate_kernel_version("5.15.0-arch1-1").is_ok());
    }

    #[test]
    fn test_validate_kernel_version_latest_sentinel() {
        // "latest" is a valid sentinel value for dynamic versioning
        assert!(validate_kernel_version("latest").is_ok());
    }

    #[test]
    fn test_validate_kernel_version_invalid_empty() {
        assert!(validate_kernel_version("").is_err());
    }

    #[test]
    fn test_validate_kernel_version_invalid_format() {
        assert!(validate_kernel_version("6").is_err());
        assert!(validate_kernel_version("6.6").is_err());
        assert!(validate_kernel_version("invalid").is_err());
        assert!(validate_kernel_version("6.6.x").is_err());
    }

    #[test]
    fn test_validate_kernel_version_invalid_missing_parts() {
        let result = validate_kernel_version("6..0");
        assert!(result.is_err());
    }

    // ========== validate_config_options tests ==========

    #[test]
    fn test_validate_config_options_empty() {
        let options = HashMap::new();
        assert!(validate_config_options(&options).is_ok());
    }

    #[test]
    fn test_validate_config_options_valid() {
        let mut options = HashMap::new();
        options.insert("CONFIG_FOO".to_string(), "y".to_string());
        options.insert("CONFIG_BAR".to_string(), "n".to_string());
        options.insert("CONFIG_VALUE".to_string(), "1024".to_string());
        options.insert("CONFIG_PATH".to_string(), "some-value".to_string());
        assert!(validate_config_options(&options).is_ok());
    }

    #[test]
    fn test_validate_config_options_empty_key() {
        let mut options = HashMap::new();
        options.insert("".to_string(), "value".to_string());
        assert!(validate_config_options(&options).is_err());
    }

    #[test]
    fn test_validate_config_options_empty_value() {
        let mut options = HashMap::new();
        options.insert("CONFIG_FOO".to_string(), "".to_string());
        assert!(validate_config_options(&options).is_err());
    }

    #[test]
    fn test_validate_config_options_invalid_key_chars() {
        let mut options = HashMap::new();
        options.insert("CONFIG-FOO".to_string(), "value".to_string());
        assert!(validate_config_options(&options).is_err());
    }

    #[test]
    fn test_validate_config_options_invalid_value_chars() {
        let mut options = HashMap::new();
        options.insert("CONFIG_FOO".to_string(), "value@invalid".to_string());
        assert!(validate_config_options(&options).is_err());
    }

    #[test]
    fn test_validate_config_options_valid_value_with_dots() {
        let mut options = HashMap::new();
        options.insert("CONFIG_VERSION".to_string(), "1.2.3".to_string());
        assert!(validate_config_options(&options).is_ok());
    }

    // ========== detect_conflicts tests ==========

    #[test]
    fn test_detect_conflicts_none() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_ok());
    }

    #[test]
    fn test_detect_conflicts_full_lto_many_exclusions() {
        let mut exclusions = vec![];
        for i in 0..15 {
            exclusions.push(format!("driver_{}", i));
        }
        let config = KernelConfig {
            lto_type: LtoType::Full,
            driver_exclusions: exclusions,
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_err());
    }

    #[test]
    fn test_detect_conflicts_full_lto_many_options() {
        let mut options = HashMap::new();
        for i in 0..20 {
            options.insert(format!("CONFIG_OPTION_{}", i), "y".to_string());
        }
        let config = KernelConfig {
            lto_type: LtoType::Full,
            config_options: options,
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_err());
    }

    #[test]
    fn test_detect_conflicts_whitelist_requires_modprobed() {
        // CRITICAL SAFETY CONSTRAINT: whitelist without modprobed-db is invalid
        let config = KernelConfig {
            use_whitelist: true,
            use_modprobed: false,  // Modprobed is disabled
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_err());
    }

    #[test]
    fn test_detect_conflicts_whitelist_with_modprobed_ok() {
        // Whitelist WITH modprobed-db is allowed
        let config = KernelConfig {
            use_whitelist: true,
            use_modprobed: true,  // Modprobed is enabled
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_ok());
    }

    #[test]
    fn test_detect_conflicts_whitelist_and_exclusions() {
        let config = KernelConfig {
            use_whitelist: true,
            use_modprobed: true,  // Modprobed needed for whitelist
            driver_exclusions: vec!["nouveau".to_string()],
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_err());
    }

    #[test]
    fn test_detect_conflicts_full_lto_old_kernel() {
        let mut config = KernelConfig::default();
        config.version = "4.19.0".to_string();
        config.lto_type = LtoType::Full;
        assert!(detect_conflicts(&config).is_err());
    }

    #[test]
    fn test_detect_conflicts_none_lto_old_kernel() {
        // None LTO (no LTO) should work with older kernels
        let mut config = KernelConfig::default();
        config.version = "4.19.0".to_string();
        config.lto_type = LtoType::None;
        assert!(detect_conflicts(&config).is_ok());
    }

    #[test]
    fn test_detect_conflicts_full_lto_moderate_exclusions() {
        // Full LTO with few exclusions should be OK
        let config = KernelConfig {
            lto_type: LtoType::Full,
            driver_exclusions: vec!["nouveau".to_string(), "amdgpu".to_string()],
            ..Default::default()
        };
        assert!(detect_conflicts(&config).is_ok());
    }

    // ========== validate_all tests ==========

    #[test]
    fn test_validate_all_valid_config() {
        let mut options = HashMap::new();
        options.insert("CONFIG_DEBUG".to_string(), "y".to_string());

        let config = KernelConfig {
            driver_exclusions: vec!["nouveau".to_string()],
            config_options: options,
            ..Default::default()
        };
        assert!(validate_all(&config).is_ok());
    }

    #[test]
    fn test_validate_all_invalid_version() {
        let mut config = KernelConfig::default();
        config.version = "invalid".to_string();
        assert!(validate_all(&config).is_err());
    }

    #[test]
    fn test_validate_all_invalid_options() {
        let mut options = HashMap::new();
        options.insert("CONFIG_FOO".to_string(), "".to_string());

        let config = KernelConfig {
            config_options: options,
            ..Default::default()
        };
        assert!(validate_all(&config).is_err());
    }

    #[test]
    fn test_validate_all_conflict() {
        let config = KernelConfig {
            use_whitelist: true,
            driver_exclusions: vec!["nouveau".to_string()],
            ..Default::default()
        };
        assert!(validate_all(&config).is_err());
    }
}

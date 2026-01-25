//! modprobed-db integration module.
//!
//! Manages modprobed-db integration for kernel module filtering.
//! Parses modprobed-db.json to detect used kernel modules and applies
//! filtering to the kernel configuration.
//!
//! This module handles:
//! - Loading modprobed-db.json files
//! - Parsing module lists from JSON
//! - Integrating module information into KernelConfig
//! - Utility functions for module lookups
//!
//! # Examples
//!
//! ```no_run
//! use std::path::Path;
//! use std::collections::HashSet;
//! # use goatd_kernel::config::modprobed::*;
//!
//! // Load modprobed database
//! let modules = load_modprobed_db(Path::new("modprobed-db.json"))?;
//! println!("Found {} used modules", modules.len());
//!
//! // Check if a specific module is used
//! if is_module_used("nouveau", &modules) {
//!     println!("nvidia driver is used");
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::ConfigError;
use crate::models::KernelConfig;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Load modprobed-db.json and extract the set of used kernel modules.
///
/// Reads a modprobed-db.json file and parses the module list. Handles
/// missing files and invalid JSON gracefully by returning an empty set.
///
/// # Arguments
///
/// * `path` - Path to the modprobed-db.json file
///
/// # Returns
///
/// Returns a HashSet of module names (lowercase for case-insensitive matching).
/// On error (missing file or invalid JSON), returns empty set with appropriate logging.
///
/// # Errors
///
/// This function does NOT error on missing/invalid files. Instead:
/// - Missing file: logs warning and returns empty set
/// - Invalid JSON: logs warning and returns empty set
///
/// Returns ConfigError only for unexpected I/O failures.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// # use goatd_kernel::config::modprobed::load_modprobed_db;
///
/// match load_modprobed_db(Path::new("/etc/modprobed-db.json")) {
///     Ok(modules) => println!("Loaded {} modules", modules.len()),
///     Err(e) => eprintln!("Unexpected I/O error: {}", e),
/// }
/// ```
pub fn load_modprobed_db(path: &Path) -> Result<HashSet<String>, ConfigError> {
    // Check if file exists
    if !path.exists() {
        eprintln!(
            "Warning: modprobed-db.json not found at {}. Using empty module set.",
            path.display()
        );
        return Ok(HashSet::new());
    }

    // Read file content
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Warning: Failed to read modprobed-db.json at {}: {}. Using empty module set.",
                path.display(),
                e
            );
            return Ok(HashSet::new());
        }
    };

    // Parse JSON content
    match parse_modprobed_json(&content) {
        Ok(modules) => Ok(modules),
        Err(_) => {
            eprintln!(
                "Warning: Failed to parse modprobed-db.json at {}. Using empty module set.",
                path.display()
            );
            Ok(HashSet::new())
        }
    }
}

/// Parse a modprobed-db.json string and extract the module list.
///
/// Expects JSON in the typical modprobed-db format:
/// ```json
/// {
///   "modules": ["module1", "module2", ...]
/// }
/// ```
///
/// Deduplicates and lowercases module names for consistent handling.
///
/// # Arguments
///
/// * `json_str` - JSON string containing module list
///
/// # Returns
///
/// Returns a HashSet of lowercase module names on success.
///
/// # Errors
///
/// Returns ConfigError::InvalidJson if:
/// - JSON is malformed
/// - "modules" field is missing
/// - "modules" field is not an array
/// - Array elements are not strings
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::modprobed::parse_modprobed_json;
/// # use std::collections::HashSet;
///
/// let json = r#"{"modules": ["nouveau", "NVIDIA", "i915"]}"#;
/// let modules = parse_modprobed_json(json).unwrap();
/// assert_eq!(modules.len(), 3);
/// assert!(modules.contains("nouveau"));
/// assert!(modules.contains("nvidia")); // lowercase
/// ```
pub fn parse_modprobed_json(json_str: &str) -> Result<HashSet<String>, ConfigError> {
    // Parse JSON string
    let value: Value = serde_json::from_str(json_str).map_err(ConfigError::InvalidJson)?;

    // Extract modules array
    let modules_array = value
        .get("modules")
        .ok_or_else(|| {
            ConfigError::ValidationFailed(
                "Missing 'modules' field in modprobed-db.json".to_string(),
            )
        })?
        .as_array()
        .ok_or_else(|| {
            ConfigError::ValidationFailed("'modules' field must be an array".to_string())
        })?;

    // Extract module names and deduplicate
    let mut modules = HashSet::new();
    for item in modules_array {
        if let Some(module_name) = item.as_str() {
            // Store lowercase for case-insensitive matching
            modules.insert(module_name.to_lowercase());
        } else {
            return Err(ConfigError::ValidationFailed(
                "Array elements must be strings (module names)".to_string(),
            ));
        }
    }

    Ok(modules)
}

/// Check if a module is used according to the modprobed-db set.
///
/// Performs case-insensitive matching: converts the input module name to
/// lowercase and checks if it exists in the set.
///
/// # Arguments
///
/// * `module_name` - Name of the module to check
/// * `modprobed_modules` - HashSet of known used modules
///
/// # Returns
///
/// Returns true if the module is in the set, false otherwise.
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::modprobed::is_module_used;
/// # use std::collections::HashSet;
///
/// let modules: HashSet<_> = vec!["nouveau", "i915"]
///     .iter()
///     .map(|s| s.to_string())
///     .collect();
///
/// assert!(is_module_used("nouveau", &modules));
/// assert!(is_module_used("NOUVEAU", &modules)); // case-insensitive
/// assert!(!is_module_used("nvidia", &modules));
/// ```
pub fn is_module_used(module_name: &str, modprobed_modules: &HashSet<String>) -> bool {
    modprobed_modules.contains(&module_name.to_lowercase())
}

/// Apply modprobed-db module filtering to kernel configuration.
///
/// Integrates the set of used modules from modprobed-db into the kernel
/// configuration. This updates the configuration to avoid excluding modules
/// that are actively used on the system.
///
/// Currently tracks which modules are in use; this can be extended to
/// automatically filter driver_exclusions based on usage.
///
/// # Arguments
///
/// * `config` - KernelConfig to update
/// * `modprobed_modules` - HashSet of modules detected as used
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::modprobed::*;
/// # use goatd_kernel::models::{KernelConfig, LtoType};
/// # use std::collections::HashSet;
/// # use std::collections::HashMap;
///
/// let mut config = KernelConfig::default();
/// config.use_modprobed = true;
/// config.driver_exclusions = vec!["nouveau".to_string()];
///
/// let modules: HashSet<_> = vec!["nouveau"]
///     .iter()
///     .map(|s| s.to_string())
///     .collect();
///
/// add_missing_modules(&mut config, &modules);
/// ```
pub fn add_missing_modules(config: &mut KernelConfig, modprobed_modules: &HashSet<String>) {
    // Filter driver exclusions to remove any modules that are actively used
    if !modprobed_modules.is_empty() {
        config
            .driver_exclusions
            .retain(|excluded| !is_module_used(excluded, modprobed_modules));
    }
}

/// Prepare and apply modprobed-db module filtering to the configuration.
///
/// This is the main public API for modprobed-db integration. It:
/// 1. Loads the modprobed-db.json file (typically at standard locations)
/// 2. Parses the module list
/// 3. Applies filtering to the configuration
///
/// The function attempts to load the database from a standard location.
/// If the file doesn't exist or is invalid, it gracefully continues with
/// an empty module set.
///
/// # Arguments
///
/// * `config` - The KernelConfig to update with modprobed filtering
///
/// # Returns
///
/// Result indicating success or ConfigError
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::modprobed::prepare_modprobed_db;
/// # use goatd_kernel::models::KernelConfig;
///
/// let mut config = KernelConfig::default();
/// prepare_modprobed_db(&mut config)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn prepare_modprobed_db(config: &mut KernelConfig) -> Result<(), ConfigError> {
    // Attempt to load modprobed-db from common locations
    let db_paths = vec![
        Path::new("/etc/modprobed-db.json"),
        Path::new("/usr/local/etc/modprobed-db.json"),
        Path::new("./modprobed-db.json"),
    ];

    let mut modules = HashSet::new();

    // Try to load from each path until one succeeds
    for path in db_paths {
        if path.exists() {
            modules = load_modprobed_db(path)?;
            if !modules.is_empty() {
                eprintln!("Loaded modprobed-db from {}", path.display());
                break;
            }
        }
    }

    // Apply the loaded modules to the configuration
    add_missing_modules(config, &modules);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_modprobed_json_valid() {
        let json = r#"{"modules": ["nouveau", "i915", "amdgpu"]}"#;
        let modules = parse_modprobed_json(json).unwrap();

        assert_eq!(modules.len(), 3);
        assert!(modules.contains("nouveau"));
        assert!(modules.contains("i915"));
        assert!(modules.contains("amdgpu"));
    }

    #[test]
    fn test_parse_modprobed_json_case_conversion() {
        let json = r#"{"modules": ["NOUVEAU", "I915", "AmdGPU"]}"#;
        let modules = parse_modprobed_json(json).unwrap();

        // All should be lowercase
        assert!(modules.contains("nouveau"));
        assert!(modules.contains("i915"));
        assert!(modules.contains("amdgpu"));
    }

    #[test]
    fn test_parse_modprobed_json_deduplication() {
        let json = r#"{"modules": ["nouveau", "NOUVEAU", "i915", "i915"]}"#;
        let modules = parse_modprobed_json(json).unwrap();

        // Duplicates should be removed
        assert_eq!(modules.len(), 2);
    }

    #[test]
    fn test_parse_modprobed_json_missing_modules_field() {
        let json = r#"{"other_field": []}"#;
        let result = parse_modprobed_json(json);

        assert!(matches!(result, Err(ConfigError::ValidationFailed(_))));
    }

    #[test]
    fn test_parse_modprobed_json_non_array_modules() {
        let json = r#"{"modules": "not_an_array"}"#;
        let result = parse_modprobed_json(json);

        assert!(matches!(result, Err(ConfigError::ValidationFailed(_))));
    }

    #[test]
    fn test_parse_modprobed_json_non_string_elements() {
        let json = r#"{"modules": ["nouveau", 123, "i915"]}"#;
        let result = parse_modprobed_json(json);

        assert!(matches!(result, Err(ConfigError::ValidationFailed(_))));
    }

    #[test]
    fn test_parse_modprobed_json_empty_array() {
        let json = r#"{"modules": []}"#;
        let modules = parse_modprobed_json(json).unwrap();

        assert_eq!(modules.len(), 0);
    }

    #[test]
    fn test_is_module_used_exact_match() {
        let modules: HashSet<_> = vec!["nouveau", "i915"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert!(is_module_used("nouveau", &modules));
        assert!(is_module_used("i915", &modules));
        assert!(!is_module_used("amdgpu", &modules));
    }

    #[test]
    fn test_is_module_used_case_insensitive() {
        let modules: HashSet<_> = vec!["nouveau", "i915"]
            .iter()
            .map(|s| s.to_string())
            .collect();

        assert!(is_module_used("NOUVEAU", &modules));
        assert!(is_module_used("Nouveau", &modules));
        assert!(is_module_used("I915", &modules));
        assert!(is_module_used("i915", &modules));
    }

    #[test]
    fn test_is_module_used_empty_set() {
        let modules = HashSet::new();

        assert!(!is_module_used("nouveau", &modules));
    }

    #[test]
    fn test_load_modprobed_db_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("modprobed-db.json");

        let json = r#"{"modules": ["nouveau", "i915"]}"#;
        let mut file = fs::File::create(&db_path).unwrap();
        file.write_all(json.as_bytes()).unwrap();

        let modules = load_modprobed_db(&db_path).unwrap();
        assert_eq!(modules.len(), 2);
        assert!(modules.contains("nouveau"));
        assert!(modules.contains("i915"));
    }

    #[test]
    fn test_load_modprobed_db_missing_file() {
        let result = load_modprobed_db(Path::new("/nonexistent/modprobed-db.json"));

        // Should return Ok with empty set, not error
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_load_modprobed_db_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("modprobed-db.json");

        let mut file = fs::File::create(&db_path).unwrap();
        file.write_all(b"{ invalid json }").unwrap();

        let result = load_modprobed_db(&db_path);

        // Should return Ok with empty set, not error
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_add_missing_modules_removes_used() {
        let mut config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["nouveau".to_string(), "amdgpu".to_string()],
            ..crate::models::KernelConfig::default()
        };

        let modules: HashSet<_> = vec!["nouveau"].iter().map(|s| s.to_string()).collect();

        add_missing_modules(&mut config, &modules);

        // nouveau should be removed, amdgpu should remain
        assert_eq!(config.driver_exclusions.len(), 1);
        assert_eq!(config.driver_exclusions[0], "amdgpu");
    }

    #[test]
    fn test_add_missing_modules_case_insensitive() {
        let mut config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["NOUVEAU".to_string()],
            ..crate::models::KernelConfig::default()
        };

        let modules: HashSet<_> = vec!["nouveau"].iter().map(|s| s.to_string()).collect();

        add_missing_modules(&mut config, &modules);

        // NOUVEAU should be removed (case-insensitive match)
        assert_eq!(config.driver_exclusions.len(), 0);
    }

    #[test]
    fn test_add_missing_modules_empty_set() {
        let mut config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["nouveau".to_string(), "amdgpu".to_string()],
            ..crate::models::KernelConfig::default()
        };

        let modules = HashSet::new();

        add_missing_modules(&mut config, &modules);

        // Nothing should be removed (empty set)
        assert_eq!(config.driver_exclusions.len(), 2);
    }
}

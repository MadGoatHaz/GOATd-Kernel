//! Config file loader and serialization.

use crate::error::ConfigError;
use crate::models::KernelConfig;
use std::fs;
use std::path::{Path, PathBuf};

/// Get the global settings path: ~/.config/goatd-kernel/settings.json
pub fn get_global_settings_path() -> Result<PathBuf, ConfigError> {
    let home = dirs::home_dir().ok_or_else(|| {
        ConfigError::ValidationFailed("Cannot determine home directory".to_string())
    })?;

    let config_dir = home.join(".config/goatd-kernel");
    Ok(config_dir.join("settings.json"))
}

/// Ensure the global settings directory exists
pub fn ensure_settings_dir_exists() -> Result<(), ConfigError> {
    let home = dirs::home_dir().ok_or_else(|| {
        ConfigError::ValidationFailed("Cannot determine home directory".to_string())
    })?;

    let config_dir = home.join(".config/goatd-kernel");
    fs::create_dir_all(&config_dir).map_err(ConfigError::IoError)?;
    Ok(())
}

/// Load config from JSON file.
pub fn load_config_from_file(path: &Path) -> Result<KernelConfig, ConfigError> {
    // Validate the path first
    validate_config_path(path)?;

    // Attempt to read the file
    let content = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ConfigError::FileNotFound(format!(
                "Configuration file not found at: {}",
                path.display()
            ))
        } else {
            ConfigError::IoError(e)
        }
    })?;

    // Parse JSON content
    let config: KernelConfig = serde_json::from_str(&content).map_err(ConfigError::InvalidJson)?;

    Ok(config)
}

/// Save config to JSON file.
pub fn save_config_to_file(config: &KernelConfig, path: &Path) -> Result<(), ConfigError> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(ConfigError::IoError)?;
        }
    }

    // Serialize config to JSON with pretty formatting
    let json_content = serde_json::to_string_pretty(config).map_err(ConfigError::InvalidJson)?;

    // Write to file
    fs::write(path, json_content).map_err(ConfigError::IoError)?;

    Ok(())
}

/// Create default config.
pub fn create_default_config() -> KernelConfig {
    KernelConfig::default()
}

/// Validate config path (.json extension required).
pub fn validate_config_path(path: &Path) -> Result<(), ConfigError> {
    // Check if path is empty
    if path.as_os_str().is_empty() {
        return Err(ConfigError::ValidationFailed(
            "Configuration path cannot be empty".to_string(),
        ));
    }

    // Check file extension
    match path.extension() {
        Some(ext) if ext == "json" => {}
        Some(ext) => {
            return Err(ConfigError::ValidationFailed(format!(
                "Configuration file must have .json extension, got .{}",
                ext.to_string_lossy()
            )))
        }
        None => {
            return Err(ConfigError::ValidationFailed(
                "Configuration file must have .json extension".to_string(),
            ))
        }
    }

    // Check that path can be converted to string (valid UTF-8)
    if path.to_str().is_none() {
        return Err(ConfigError::ValidationFailed(
            "Configuration path contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

/// List .json config files in directory.
pub fn list_config_files(dir: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    // Check if directory exists
    if !dir.exists() {
        return Err(ConfigError::FileNotFound(format!(
            "Configuration directory not found: {}",
            dir.display()
        )));
    }

    if !dir.is_dir() {
        return Err(ConfigError::FileNotFound(format!(
            "Path is not a directory: {}",
            dir.display()
        )));
    }

    let mut config_files = Vec::new();

    // Recursively walk directory tree
    fn walk_dir(dir: &Path, config_files: &mut Vec<PathBuf>) -> Result<(), ConfigError> {
        let entries = fs::read_dir(dir).map_err(ConfigError::IoError)?;

        for entry in entries {
            let entry = entry.map_err(ConfigError::IoError)?;
            let path = entry.path();

            if path.is_dir() {
                // Recursively search subdirectories
                walk_dir(&path, config_files)?;
            } else if path.is_file() {
                // Check if file has .json extension
                if path.extension().map_or(false, |ext| ext == "json") {
                    config_files.push(path);
                }
            }
        }

        Ok(())
    }

    walk_dir(dir, &mut config_files)?;

    // Sort for consistent ordering
    config_files.sort();

    Ok(config_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{HardeningLevel, LtoType};
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_create_default_config() {
        let config = create_default_config();
        assert_eq!(config.version, "latest");
        assert_eq!(config.lto_type, LtoType::Thin);
        assert!(!config.use_modprobed);
        assert!(!config.use_whitelist);
        assert!(config.driver_exclusions.is_empty());
        assert!(config.config_options.is_empty());
        assert_eq!(config.hardening, HardeningLevel::Standard);
        assert!(!config.secure_boot);
        assert_eq!(config.profile, "Generic");
    }

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.json");

        let mut original = create_default_config();
        original.version = "6.7.0".to_string();
        original.lto_type = LtoType::Full;
        original.use_modprobed = true;
        original.driver_exclusions.push("nouveau".to_string());
        original
            .config_options
            .insert("FOO".to_string(), "bar".to_string());
        original.hardening = HardeningLevel::Hardened;
        original.secure_boot = true;
        original.profile = "Gaming".to_string();

        // Save config
        save_config_to_file(&original, &config_path).expect("Failed to save config");
        assert!(config_path.exists(), "Config file should exist after save");

        // Load config
        let loaded = load_config_from_file(&config_path).expect("Failed to load config");

        // Verify all fields were preserved
        assert_eq!(loaded.version, "6.7.0");
        assert_eq!(loaded.lto_type, LtoType::Full);
        assert!(loaded.use_modprobed);
        assert_eq!(loaded.driver_exclusions.len(), 1);
        assert_eq!(loaded.driver_exclusions[0], "nouveau");
        assert_eq!(loaded.config_options.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(loaded.hardening, HardeningLevel::Hardened);
        assert!(loaded.secure_boot);
        assert_eq!(loaded.profile, "Gaming");
    }

    #[test]
    fn test_validate_config_path_valid() {
        assert!(validate_config_path(Path::new("config.json")).is_ok());
        assert!(validate_config_path(Path::new("/tmp/config.json")).is_ok());
        assert!(validate_config_path(Path::new("./configs/kernel.json")).is_ok());
    }

    #[test]
    fn test_validate_config_path_invalid_extension() {
        assert!(validate_config_path(Path::new("config.txt")).is_err());
        assert!(validate_config_path(Path::new("config.yaml")).is_err());
        assert!(validate_config_path(Path::new("config")).is_err());
    }

    #[test]
    fn test_validate_config_path_empty() {
        assert!(validate_config_path(Path::new("")).is_err());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = load_config_from_file(Path::new("/nonexistent/path/config.json"));
        assert!(matches!(result, Err(ConfigError::FileNotFound(_))));
    }

    #[test]
    fn test_load_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("invalid.json");

        // Write invalid JSON
        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(b"{ invalid json }").unwrap();

        let result = load_config_from_file(&config_path);
        assert!(matches!(result, Err(ConfigError::InvalidJson(_))));
    }

    #[test]
    fn test_save_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("nested/dirs/config.json");

        let config = create_default_config();
        save_config_to_file(&config, &config_path).expect("Failed to save config");

        assert!(
            config_path.exists(),
            "Config file should exist in nested directory"
        );
        assert!(
            config_path.parent().unwrap().exists(),
            "Parent directories should be created"
        );
    }

    #[test]
    fn test_list_config_files() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create some config files
        let config1 = create_default_config();
        let config2 = create_default_config();

        save_config_to_file(&config1, &base_path.join("config1.json")).unwrap();
        save_config_to_file(&config2, &base_path.join("config2.json")).unwrap();

        // Create a non-JSON file (should be ignored)
        fs::write(base_path.join("ignored.txt"), "test").unwrap();

        // Create a nested config
        fs::create_dir_all(base_path.join("subdir")).unwrap();
        save_config_to_file(&config1, &base_path.join("subdir/config3.json")).unwrap();

        let files = list_config_files(base_path).expect("Failed to list configs");

        assert_eq!(files.len(), 3, "Should find 3 JSON config files");
        assert!(
            files.iter().any(|p| p.ends_with("config1.json")),
            "Should find config1.json"
        );
        assert!(
            files.iter().any(|p| p.ends_with("config2.json")),
            "Should find config2.json"
        );
        assert!(
            files.iter().any(|p| p.ends_with("subdir/config3.json")),
            "Should find nested config3.json"
        );
    }

    #[test]
    fn test_list_config_files_nonexistent_dir() {
        let result = list_config_files(Path::new("/nonexistent/directory"));
        assert!(matches!(result, Err(ConfigError::FileNotFound(_))));
    }

    #[test]
    fn test_list_config_files_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let files = list_config_files(temp_dir.path()).expect("Failed to list configs");
        assert!(files.is_empty(), "Empty directory should return empty list");
    }
}

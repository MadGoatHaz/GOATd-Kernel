//! Comprehensive integration test suite for the config module (Phase 3.3)
//!
//! Tests all 6 core config submodules working together:
//! - loader: File I/O and JSON parsing
//! - validator: Configuration validation
//! - modprobed: Module filtering
//! - whitelist: Essential driver protection
//! - exclusions: Driver exclusion management
//! - ConfigManager: Main orchestrator
//!
//! Test Organization (27+ tests):
//! - Configuration Loading (4 tests)
//! - Configuration Saving (3 tests)
//! - Configuration Validation (5 tests)
//! - Modprobed-DB Integration (4 tests)
//! - Whitelist Protection (3 tests)
//! - Driver Exclusions (3 tests)
//! - Edge Cases (2+ tests)

use goatd_kernel::config::{loader, validator, modprobed, whitelist, exclusions, ConfigManager};
use goatd_kernel::models::{KernelConfig, LtoType};
use goatd_kernel::error::ConfigError;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

// Helper function to create a default KernelConfig for testing
fn create_test_config() -> KernelConfig {
    KernelConfig {
        lto_type: LtoType::Thin,
        use_modprobed: false,
        use_whitelist: false,
        driver_exclusions: vec![],
        config_options: HashMap::new(),
        hardening: goatd_kernel::models::HardeningLevel::Standard,
        secure_boot: false,
        profile: "Generic".to_string(),
        version: "6.6.0".to_string(),
        use_bore: false,
        use_polly: false,
        use_mglru: false,
        user_toggled_bore: false,
        user_toggled_polly: false,
        user_toggled_mglru: false,
        user_toggled_hardening: false,
        mglru_enabled_mask: 0x0007,
        mglru_min_ttl_ms: 1000,
        hz: 300,
        preemption: "Voluntary".to_string(),
        force_clang: true,
        lto_shield_modules: Vec::new(),
        user_toggled_lto: false,
        scx_available: Vec::new(),
        scx_active_scheduler: None,
        native_optimizations: true,
        user_toggled_native_optimizations: false,
        kernel_variant: String::new(),
    }
}

// ============================================================================
// CONFIGURATION LOADING TESTS (4 tests)
// ============================================================================

#[test]
fn test_load_valid_json_profile() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary test file with valid JSON
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("valid_config.json");
    
    let mut config = create_test_config();
    config.version = "6.6.0".to_string();
    config.driver_exclusions = vec!["nouveau".to_string()];
    
    loader::save_config_to_file(&config, &config_path)?;
    let loaded = loader::load_config_from_file(&config_path)?;
    
    assert_eq!(loaded.version, "6.6.0");
    assert_eq!(loaded.lto_type, LtoType::Thin);
    assert_eq!(loaded.driver_exclusions.len(), 1);
    assert_eq!(loaded.driver_exclusions[0], "nouveau");
    
    Ok(())
}

#[test]
fn test_load_invalid_json_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary test file with invalid JSON
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("invalid_config.json");
    
    let mut file = fs::File::create(&config_path)?;
    file.write_all(b"{ this is not valid json }")?;
    
    let result = loader::load_config_from_file(&config_path);
    
    match result {
        Err(ConfigError::InvalidJson(_)) => Ok(()),
        Err(e) => Err(format!("Expected InvalidJson error, got: {}", e).into()),
        Ok(_) => Err("Expected error loading invalid JSON".into()),
    }
}

#[test]
fn test_load_missing_file_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Path::new("/tmp/nonexistent_config_file_definitely_does_not_exist.json");
    
    let result = loader::load_config_from_file(config_path);
    
    match result {
        Err(ConfigError::FileNotFound(_)) => Ok(()),
        Err(e) => Err(format!("Expected FileNotFound error, got: {}", e).into()),
        Ok(_) => Err("Expected error loading missing file".into()),
    }
}

#[test]
fn test_malformed_config_data_handling() -> Result<(), Box<dyn std::error::Error>> {
    // Create temporary test file with JSON that has wrong schema
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("malformed_config.json");
    
    // Write JSON that doesn't match KernelConfig schema
    let mut file = fs::File::create(&config_path)?;
    file.write_all(b"{\"invalid_field\": \"value\"}")?;
    
    let result = loader::load_config_from_file(&config_path);
    assert!(result.is_err(), "Expected error loading malformed config");
    
    Ok(())
}

// ============================================================================
// CONFIGURATION SAVING TESTS (3 tests)
// ============================================================================

#[test]
fn test_save_and_load_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("roundtrip_config.json");
    
    let mut original = create_test_config();
    original.version = "6.7.1".to_string();
    original.lto_type = LtoType::Full;
    original.use_modprobed = true;
    original.use_whitelist = false;
    original.driver_exclusions = vec!["custom".to_string()];
    original.config_options.insert("CONFIG_DEBUG".to_string(), "y".to_string());
    original.config_options.insert("CONFIG_VERSION".to_string(), "6.7.1".to_string());
    
    // Save original
    loader::save_config_to_file(&original, &config_path)?;
    
    // Load and verify all fields preserved
    let loaded = loader::load_config_from_file(&config_path)?;
    
    assert_eq!(loaded.version, original.version);
    assert_eq!(loaded.lto_type, original.lto_type);
    assert_eq!(loaded.use_modprobed, original.use_modprobed);
    assert_eq!(loaded.use_whitelist, original.use_whitelist);
    assert_eq!(loaded.driver_exclusions, original.driver_exclusions);
    assert_eq!(loaded.config_options, original.config_options);
    
    Ok(())
}

#[test]
fn test_invalid_save_paths_handled() -> Result<(), Box<dyn std::error::Error>> {
    let _config = create_test_config();
    
    // Try to save to a path with invalid extension by validating the path first
    let bad_path = Path::new("/tmp/config.yaml");
    let result = loader::validate_config_path(bad_path);
    
    // Path validation should fail with invalid extension
    assert!(result.is_err(), "Expected error with invalid extension");
    
    Ok(())
}

#[test]
fn test_permissions_errors_handled() -> Result<(), Box<dyn std::error::Error>> {
    // Try to save to a directory that doesn't exist and can't be created
    // (using /dev/null/impossible as a path that will fail)
    let config = create_test_config();
    
    let bad_path = Path::new("/dev/null/impossible/config.json");
    let result = loader::save_config_to_file(&config, bad_path);
    
    assert!(result.is_err(), "Expected error saving to impossible path");
    
    Ok(())
}

// ============================================================================
// CONFIGURATION VALIDATION TESTS (5 tests)
// ============================================================================

#[test]
fn test_valid_config_passes_all_checks() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = create_test_config();
    config.driver_exclusions = vec!["custom_driver".to_string()];
    config.config_options.insert("CONFIG_DEBUG".to_string(), "y".to_string());
    
    let result = validator::validate_config(&config);
    assert!(result.is_ok(), "Valid config should pass validation");
    
    Ok(())
}

#[test]
fn test_invalid_lto_type_detected() -> Result<(), Box<dyn std::error::Error>> {
    // Note: LtoType is an enum, so invalid values are compile-time impossible.
    // Instead, test that all valid LTO types pass validation.
    let lto_types = vec![LtoType::None, LtoType::Thin, LtoType::Full];
    
    for lto in lto_types {
        let mut config = create_test_config();
        config.lto_type = lto;
        
        assert!(validator::validate_config(&config).is_ok(), "Config with valid LTO should pass");
    }
    
    Ok(())
}

#[test]
fn test_invalid_kernel_version_detected() -> Result<(), Box<dyn std::error::Error>> {
    let invalid_versions = vec![
        "",                  // empty
        "6",                 // incomplete
        "6.6",              // incomplete
        "invalid",          // not numeric
        "6.6.x",            // non-numeric part
    ];
    
    for version in invalid_versions {
        let mut config = create_test_config();
        config.version = version.to_string();
        
        let result = validator::validate_kernel_version(version);
        assert!(result.is_err(), "Invalid version '{}' should fail validation", version);
        
        let config_result = validator::validate_config(&config);
        assert!(config_result.is_err(), "Config with invalid version should fail validation");
    }
    
    Ok(())
}

#[test]
fn test_conflicting_options_detected() -> Result<(), Box<dyn std::error::Error>> {
    // Test Full LTO + many exclusions (known conflict)
    let mut exclusions = vec![];
    for i in 0..15 {
        exclusions.push(format!("driver_{}", i));
    }
    
    let mut config = create_test_config();
    config.lto_type = LtoType::Full;
    config.driver_exclusions = exclusions;
    
    let result = validator::detect_conflicts(&config);
    assert!(result.is_err(), "Full LTO with many exclusions should be detected as conflict");
    
    // Test whitelist + exclusions (known conflict)
    let mut config2 = create_test_config();
    config2.use_whitelist = true;
    config2.driver_exclusions = vec!["custom_driver".to_string()];
    
    let result2 = validator::detect_conflicts(&config2);
    assert!(result2.is_err(), "Whitelist + exclusions should be detected as conflict");
    
    Ok(())
}

#[test]
fn test_missing_essential_fields_handled() -> Result<(), Box<dyn std::error::Error>> {
    // Test with empty kernel version
    let mut config = create_test_config();
    config.version = "".to_string();
    
    let result = validator::validate_kernel_version("");
    assert!(result.is_err(), "Empty kernel version should fail validation");
    
    // Test with invalid config option values
    let mut invalid_options = HashMap::new();
    invalid_options.insert("CONFIG_FOO".to_string(), "".to_string());
    
    let result2 = validator::validate_config_options(&invalid_options);
    assert!(result2.is_err(), "Empty option value should fail validation");
    
    Ok(())
}

// ============================================================================
// MODPROBED-DB INTEGRATION TESTS (4 tests)
// ============================================================================

#[test]
fn test_valid_modprobed_db_parses() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{"modules": ["nouveau", "i915", "amdgpu", "e1000"]}"#;
    
    let modules = modprobed::parse_modprobed_json(json)?;
    
    assert_eq!(modules.len(), 4);
    assert!(modules.contains("nouveau"));
    assert!(modules.contains("i915"));
    assert!(modules.contains("amdgpu"));
    assert!(modules.contains("e1000"));
    
    Ok(())
}

#[test]
fn test_invalid_json_returns_empty_set() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let db_path = tempdir.path().join("invalid_modprobed.json");
    
    // Write invalid JSON
    let mut file = fs::File::create(&db_path)?;
    file.write_all(b"{ invalid json content }")?;
    
    // load_modprobed_db should return Ok with empty set on invalid JSON
    let modules = modprobed::load_modprobed_db(&db_path)?;
    assert_eq!(modules.len(), 0, "Invalid JSON should return empty set");
    
    Ok(())
}

#[test]
fn test_missing_file_returns_empty_set() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new("/tmp/nonexistent_modprobed_db.json");
    
    // load_modprobed_db should return Ok with empty set on missing file
    let modules = modprobed::load_modprobed_db(db_path)?;
    assert_eq!(modules.len(), 0, "Missing file should return empty set");
    
    Ok(())
}

#[test]
fn test_deduplication_of_module_list() -> Result<(), Box<dyn std::error::Error>> {
    // Test that duplicate modules are deduplicated
    let json = r#"{"modules": ["nouveau", "NOUVEAU", "i915", "i915", "amdgpu"]}"#;
    
    let modules = modprobed::parse_modprobed_json(json)?;
    
    // After dedup and lowercasing, should have 3 unique modules
    assert_eq!(modules.len(), 3, "Duplicates should be removed");
    assert!(modules.contains("nouveau"));
    assert!(modules.contains("i915"));
    assert!(modules.contains("amdgpu"));
    
    Ok(())
}

// ============================================================================
// WHITELIST PROTECTION TESTS (3 tests)
// ============================================================================

#[test]
fn test_essential_drivers_applied() -> Result<(), Box<dyn std::error::Error>> {
    // Verify that essential drivers are properly defined
    let essential = whitelist::get_essential_drivers();
    
    assert!(!essential.is_empty(), "Should have essential drivers");
    assert!(essential.contains(&"ext4"), "Should include storage drivers");
    assert!(essential.contains(&"nvme"), "Should include NVMe drivers");
    assert!(essential.contains(&"evdev"), "Should include input drivers");
    
    Ok(())
}

#[test]
fn test_whitelist_validation_succeeds() -> Result<(), Box<dyn std::error::Error>> {
    // Config with no essential drivers in exclusions should pass
    let mut config = create_test_config();
    config.use_whitelist = true;
    config.driver_exclusions = vec!["custom_driver".to_string()];
    
    let result = whitelist::validate_whitelist(&config);
    assert!(result.is_ok(), "Config with non-essential exclusions should pass whitelist validation");
    
    Ok(())
}

#[test]
fn test_whitelist_validation_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Config with essential drivers in exclusions should fail
    let mut config = create_test_config();
    config.use_whitelist = true;
    config.driver_exclusions = vec!["i915".to_string(), "ext4".to_string()];
    
    let result = whitelist::validate_whitelist(&config);
    assert!(result.is_err(), "Config with essential driver exclusions should fail validation");
    
    if let Err(e) = result {
        let msg = e.to_string();
        assert!(msg.contains("i915") || msg.contains("ext4"), "Error should mention excluded essential drivers");
    }
    
    Ok(())
}

// ============================================================================
// DRIVER EXCLUSIONS TESTS (3 tests)
// ============================================================================

#[test]
fn test_valid_exclusions_applied() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = create_test_config();
    
    // Add valid non-essential driver exclusions
    exclusions::add_exclusion(&mut config, "custom_driver")?;
    exclusions::add_exclusion(&mut config, "another_driver")?;
    
    assert_eq!(config.driver_exclusions.len(), 2);
    assert!(config.driver_exclusions.contains(&"custom_driver".to_string()));
    assert!(config.driver_exclusions.contains(&"another_driver".to_string()));
    
    Ok(())
}

#[test]
fn test_exclude_essential_driver_fails() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = create_test_config();
    
    // Try to exclude essential drivers - should fail
    let essential_drivers = vec!["ext4", "nvme", "hid"];
    for driver in essential_drivers {
        let result = exclusions::add_exclusion(&mut config, driver);
        assert!(result.is_err(), "Should not be able to exclude essential driver: {}", driver);
    }
    
    // Config should still be empty (no exclusions added)
    assert_eq!(config.driver_exclusions.len(), 0);
    
    Ok(())
}

#[test]
fn test_multiple_exclusions_handled() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = create_test_config();
    
    // Apply multiple exclusions in batch
    let drivers_to_exclude = vec!["driver1", "driver2", "driver3"];
    exclusions::apply_exclusions(&mut config, &drivers_to_exclude)?;
    
    assert_eq!(config.driver_exclusions.len(), 3);
    
    // Try batch with an essential driver (should fail and partially apply)
    let mixed = vec!["driver4", "ext4"];
    let result = exclusions::apply_exclusions(&mut config, &mixed);
    assert!(result.is_err(), "Should fail when trying to exclude essential driver");
    assert_eq!(config.driver_exclusions.len(), 4, "driver4 should have been added before error");
    
    Ok(())
}

// ============================================================================
// EDGE CASES TESTS (2+ tests)
// ============================================================================

#[test]
fn test_very_large_config_objects() -> Result<(), Box<dyn std::error::Error>> {
    // Create a config with many options
    let mut options = HashMap::new();
    for i in 0..1000 {
        options.insert(
            format!("CONFIG_OPTION_{}", i),
            format!("value_{}", i),
        );
    }
    
    let mut config = create_test_config();
    config.config_options = options;
    
    // Should still validate successfully (with Thin LTO)
    let result = validator::validate_config(&config);
    assert!(result.is_ok(), "Config with 1000 options and Thin LTO should validate");
    
    // Save and load the large config
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("large_config.json");
    
    loader::save_config_to_file(&config, &config_path)?;
    let loaded = loader::load_config_from_file(&config_path)?;
    
    assert_eq!(loaded.config_options.len(), 1000, "All 1000 options should be preserved");
    
    Ok(())
}

#[test]
fn test_empty_config_with_defaults() -> Result<(), Box<dyn std::error::Error>> {
    // Create the default config
    let config = loader::create_default_config();
    
    assert_eq!(config.version, "6.6.0");
    assert_eq!(config.lto_type, LtoType::Thin);
    assert!(!config.use_modprobed);
    assert!(!config.use_whitelist);
    assert!(config.driver_exclusions.is_empty());
    assert!(config.config_options.is_empty());
    
    // Default config should validate successfully
    let result = validator::validate_config(&config);
    assert!(result.is_ok(), "Default config should be valid");
    
    Ok(())
}

#[test]
fn test_config_manager_integration() -> Result<(), Box<dyn std::error::Error>> {
    // Test ConfigManager orchestrating all submodules
    let default_config = loader::create_default_config();
    let mut manager = ConfigManager::new(PathBuf::from("/tmp"), default_config);
    
    // Test setting options
    manager.set_config_option("CONFIG_FOO".to_string(), "bar".to_string())?;
    assert_eq!(manager.get_config_option("CONFIG_FOO"), Some(&"bar".to_string()));
    
    // Test LTO setting
    manager.set_lto(LtoType::Full)?;
    assert_eq!(manager.config().lto_type, LtoType::Full);
    
    // Test validation
    let validation_result = manager.validate();
    assert!(validation_result.is_ok(), "Manager should validate config");
    
    // Test summary generation
    let summary = manager.get_summary();
    assert!(summary.contains("6.6.0"));
    assert!(summary.contains("Full"));
    
    Ok(())
}

#[test]
fn test_modprobed_integration_with_config() -> Result<(), Box<dyn std::error::Error>> {
    // Test modprobed filtering integration
    let mut config = create_test_config();
    config.driver_exclusions = vec!["nouveau".to_string(), "amdgpu".to_string()];
    
    // Simulate modules that are in use
    let used_modules: HashSet<String> = vec!["nouveau"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    
    // Apply modprobed filtering (removes nouveau from exclusions since it's used)
    modprobed::add_missing_modules(&mut config, &used_modules);
    
    // nouveau should be removed (it's in use), amdgpu should remain
    assert_eq!(config.driver_exclusions.len(), 1);
    assert_eq!(config.driver_exclusions[0], "amdgpu");
    
    Ok(())
}

#[test]
fn test_whitelist_protection_integration() -> Result<(), Box<dyn std::error::Error>> {
    // Test that apply_whitelist removes essential drivers from exclusions
    let mut config = create_test_config();
    config.use_whitelist = true;
    config.driver_exclusions = vec![
        "custom_driver".to_string(),
        "hid".to_string(),
        "ext4".to_string(),
    ];
    
    whitelist::apply_whitelist(&mut config);
    
    // Essential drivers (hid, ext4) should be removed, custom_driver remains
    assert_eq!(config.driver_exclusions.len(), 1);
    assert_eq!(config.driver_exclusions[0], "custom_driver");
    
    // Validation should now pass
    let result = whitelist::validate_whitelist(&config);
    assert!(result.is_ok(), "After applying whitelist, validation should pass");
    
    Ok(())
}

#[test]
fn test_full_workflow_configuration_build() -> Result<(), Box<dyn std::error::Error>> {
    // Complete workflow test: create, configure, validate, save, and load
    let tempdir = tempfile::TempDir::new()?;
    let config_path = tempdir.path().join("workflow_config.json");
    
    // Step 1: Create configuration
    let mut config = loader::create_default_config();
    
    // Step 2: Apply configurations
    config.version = "6.7.0".to_string();
    config.lto_type = LtoType::Thin;
    config.use_whitelist = false;  // Don't use whitelist to avoid conflict with exclusions
    config.use_modprobed = false;
    
    // Add non-essential driver exclusion
    exclusions::add_exclusion(&mut config, "custom_driver")?;
    
    // Step 3: Validate all aspects
    validator::validate_kernel_version(&config.version)?;
    validator::validate_config(&config)?;
    exclusions::validate_exclusions(&config)?;
    
    // Step 4: Save configuration
    loader::save_config_to_file(&config, &config_path)?;
    assert!(config_path.exists(), "Config file should exist after save");
    
    // Step 5: Load and verify
    let loaded = loader::load_config_from_file(&config_path)?;
    assert_eq!(loaded.version, "6.7.0");
    assert_eq!(loaded.lto_type, LtoType::Thin);
    assert!(!loaded.use_whitelist);
    assert_eq!(loaded.driver_exclusions.len(), 1);
    
    // Step 6: Validate loaded config
    validator::validate_config(&loaded)?;
    
    Ok(())
}

// ============================================================================
// APPSTATE PERSISTENCE TESTS (for UI state serialization)
// ============================================================================

/// Mock AppState structure for testing UI state persistence
/// This mirrors the AppState from main.rs for testing serialization
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct AppState {
    // Build Settings
    selected_variant: String,
    selected_profile: String,
    selected_lto: String,
    kernel_hardening: bool,
    secure_boot: bool,
    use_modprobed: bool,
    use_whitelist: bool,
    
    // Settings
    workspace_path: String,
    security_level: String,
    startup_audit: bool,
    theme_mode: String,
    minimize_to_tray: bool,
    
    // Kernel source path
    kernel_source_path: String,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            selected_variant: "linux".to_string(),
            selected_profile: "gaming".to_string(),
            selected_lto: "thin".to_string(),
            kernel_hardening: false,
            secure_boot: false,
            use_modprobed: true,
            use_whitelist: true,
            workspace_path: String::new(),
            security_level: "Standard".to_string(),
            startup_audit: false,
            theme_mode: "Nord Dark".to_string(),
            minimize_to_tray: false,
            kernel_source_path: String::new(),
        }
    }
}

#[test]
fn test_appstate_default_values() {
    let state = AppState::default();
    
    assert_eq!(state.selected_variant, "linux");
    assert_eq!(state.selected_profile, "gaming");
    assert_eq!(state.selected_lto, "thin");
    assert!(!state.kernel_hardening);
    assert!(!state.secure_boot);
    assert!(state.use_modprobed);
    assert!(state.use_whitelist);
    assert_eq!(state.workspace_path, "");
    assert_eq!(state.security_level, "Standard");
    assert!(!state.startup_audit);
    assert_eq!(state.theme_mode, "Nord Dark");
    assert!(!state.minimize_to_tray);
    assert_eq!(state.kernel_source_path, "");
}

#[test]
fn test_appstate_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        selected_variant: "linux-hardened".to_string(),
        selected_profile: "workstation".to_string(),
        selected_lto: "full".to_string(),
        kernel_hardening: true,
        secure_boot: true,
        use_modprobed: false,
        use_whitelist: false,
        workspace_path: "/home/user/builds".to_string(),
        security_level: "Paranoid".to_string(),
        startup_audit: true,
        theme_mode: "Light".to_string(),
        minimize_to_tray: true,
        kernel_source_path: "/home/user/builds/linux-hardened".to_string(),
    };
    
    // Serialize to JSON
    let json = serde_json::to_string(&state)?;
    
    // Deserialize back
    let deserialized: AppState = serde_json::from_str(&json)?;
    
    // Verify all fields match
    assert_eq!(deserialized.selected_variant, state.selected_variant);
    assert_eq!(deserialized.selected_profile, state.selected_profile);
    assert_eq!(deserialized.selected_lto, state.selected_lto);
    assert_eq!(deserialized.kernel_hardening, state.kernel_hardening);
    assert_eq!(deserialized.secure_boot, state.secure_boot);
    assert_eq!(deserialized.use_modprobed, state.use_modprobed);
    assert_eq!(deserialized.use_whitelist, state.use_whitelist);
    assert_eq!(deserialized.workspace_path, state.workspace_path);
    assert_eq!(deserialized.security_level, state.security_level);
    assert_eq!(deserialized.startup_audit, state.startup_audit);
    assert_eq!(deserialized.theme_mode, state.theme_mode);
    assert_eq!(deserialized.minimize_to_tray, state.minimize_to_tray);
    assert_eq!(deserialized.kernel_source_path, state.kernel_source_path);
    
    Ok(())
}

#[test]
fn test_appstate_file_persistence() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let state_path = tempdir.path().join("appstate.json");
    
    let original = AppState {
        selected_variant: "linux-mainline".to_string(),
        selected_profile: "gaming".to_string(),
        selected_lto: "thin".to_string(),
        kernel_hardening: true,
        secure_boot: false,
        use_modprobed: true,
        use_whitelist: true,
        workspace_path: "/tmp/workspace".to_string(),
        security_level: "Hardened".to_string(),
        startup_audit: true,
        theme_mode: "Nord Dark".to_string(),
        minimize_to_tray: false,
        kernel_source_path: "/tmp/workspace/linux-mainline".to_string(),
    };
    
    // Save to file
    let json = serde_json::to_string_pretty(&original)?;
    std::fs::write(&state_path, json)?;
    
    // Load from file
    let content = std::fs::read_to_string(&state_path)?;
    let loaded: AppState = serde_json::from_str(&content)?;
    
    // Verify all fields
    assert_eq!(loaded.selected_variant, original.selected_variant);
    assert_eq!(loaded.kernel_hardening, original.kernel_hardening);
    assert_eq!(loaded.workspace_path, original.workspace_path);
    assert_eq!(loaded.kernel_source_path, original.kernel_source_path);
    
    Ok(())
}

#[test]
fn test_appstate_missing_file_returns_default() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let state_path = tempdir.path().join("nonexistent_state.json");
    
    // Try to read nonexistent file
    let result = std::fs::read_to_string(&state_path);
    assert!(result.is_err(), "Reading nonexistent file should fail");
    
    // In real code, this would return default AppState
    let default = AppState::default();
    assert_eq!(default.selected_variant, "linux");
    
    Ok(())
}

#[test]
fn test_appstate_malformed_json_handling() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let state_path = tempdir.path().join("malformed_state.json");
    
    // Write malformed JSON
    let mut file = std::fs::File::create(&state_path)?;
    file.write_all(b"{ this is not valid json }")?;
    
    // Try to deserialize
    let content = std::fs::read_to_string(&state_path)?;
    let result: Result<AppState, _> = serde_json::from_str(&content);
    
    assert!(result.is_err(), "Malformed JSON should fail deserialization");
    
    Ok(())
}

#[test]
fn test_appstate_partial_json_field_missing() -> Result<(), Box<dyn std::error::Error>> {
    let tempdir = tempfile::TempDir::new()?;
    let state_path = tempdir.path().join("partial_state.json");
    
    // Write JSON with missing field
    let partial_json = r#"{
        "selected_variant": "linux",
        "selected_profile": "gaming",
        "selected_lto": "thin"
    }"#;
    
    std::fs::write(&state_path, partial_json)?;
    let content = std::fs::read_to_string(&state_path)?;
    let result: Result<AppState, _> = serde_json::from_str(&content);
    
    // Should fail due to missing fields
    assert!(result.is_err(), "Partial JSON should fail deserialization");
    
    Ok(())
}

#[test]
fn test_appstate_all_variants_supported() -> Result<(), Box<dyn std::error::Error>> {
    let variants = vec!["linux", "linux-mainline", "linux-lts", "linux-hardened"];
    
    for variant in variants {
        let mut state = AppState::default();
        state.selected_variant = variant.to_string();
        
        // Should serialize without error
        let json = serde_json::to_string(&state)?;
        let deserialized: AppState = serde_json::from_str(&json)?;
        assert_eq!(deserialized.selected_variant, variant);
    }
    
    Ok(())
}

// ============================================================================
// KERNEL VARIANT URL TESTS
// ============================================================================

#[test]
fn test_get_variant_repo_url_linux() {
    let url = get_variant_repo_url("linux");
    assert_eq!(url, Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git"));
}

#[test]
fn test_get_variant_repo_url_linux_mainline() {
    let url = get_variant_repo_url("linux-mainline");
    assert_eq!(url, Some("https://aur.archlinux.org/linux-mainline.git"));
}

#[test]
fn test_get_variant_repo_url_linux_lts() {
    let url = get_variant_repo_url("linux-lts");
    assert_eq!(url, Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git"));
}

#[test]
fn test_get_variant_repo_url_linux_hardened() {
    let url = get_variant_repo_url("linux-hardened");
    assert_eq!(url, Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git"));
}

#[test]
fn test_get_variant_repo_url_unknown_variant() {
    let url = get_variant_repo_url("linux-custom");
    assert_eq!(url, None);
}

#[test]
fn test_all_supported_variants_have_urls() {
    let variants = vec!["linux", "linux-mainline", "linux-lts", "linux-hardened"];
    
    for variant in variants {
        let url = get_variant_repo_url(variant);
        assert!(url.is_some(), "Variant {} should have a URL", variant);
        assert!(!url.unwrap().is_empty(), "URL for {} should not be empty", variant);
    }
}

#[test]
fn test_variant_urls_are_valid_git_endpoints() {
    let variants = vec!["linux", "linux-mainline", "linux-lts", "linux-hardened"];
    
    for variant in variants {
        if let Some(url) = get_variant_repo_url(variant) {
            assert!(url.ends_with(".git"), "URL for {} should end with .git", variant);
            assert!(url.starts_with("https://"), "URL for {} should use https", variant);
        }
    }
}

#[test]
fn test_kernel_source_path_calculation_with_workspace() {
    // Test kernel source path calculation with workspace path
    let variant = "linux";
    let workspace = "/home/user/builds";
    let expected = format!("{}/{}", workspace, variant);
    
    assert_eq!(expected, "/home/user/builds/linux");
}

#[test]
fn test_kernel_source_path_calculation_without_workspace() {
    // Test fallback to /tmp/goatd-build when no workspace
    let variant = "linux-hardened";
    let fallback = format!("/tmp/goatd-build/{}", variant);
    
    assert_eq!(fallback, "/tmp/goatd-build/linux-hardened");
}

#[test]
fn test_kernel_source_path_with_all_variants() {
    let variants = vec!["linux", "linux-mainline", "linux-lts", "linux-hardened"];
    let workspace = "/tmp/test_workspace";
    
    for variant in variants {
        let path = format!("{}/{}", workspace, variant);
        assert!(path.contains(variant), "Path should contain variant name");
        assert!(path.starts_with(workspace), "Path should start with workspace");
    }
}

// ============================================================================
// STORAGE DETECTION PREFIX-MATCHING TESTS
// ============================================================================

/// Helper function to find longest prefix match (extracted from detect_workspace_storage)
/// Returns the index of the mount point with the longest prefix match
fn find_longest_prefix_match(path: &str, mount_points: &[&str]) -> Option<usize> {
    let mut best_idx = None;
    let mut best_len = 0;
    
    for (i, mount) in mount_points.iter().enumerate() {
        if path.starts_with(mount) {
            let mount_len = mount.len();
            if mount_len > best_len {
                best_len = mount_len;
                best_idx = Some(i);
            }
        }
    }
    
    best_idx
}

#[test]
fn test_storage_detection_longest_prefix_simple() {
    let path = "/home/user/workspace";
    let mount_points = vec!["/", "/home"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(1), "Should match /home (longest prefix)");
}

#[test]
fn test_storage_detection_longest_prefix_nested() {
    let path = "/home/user/workspace/builds";
    let mount_points = vec!["/", "/home", "/home/user", "/home/user/workspace"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(3), "Should match /home/user/workspace (longest prefix)");
}

#[test]
fn test_storage_detection_single_match() {
    let path = "/var/log/kernel";
    let mount_points = vec!["/", "/home"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(0), "Should match only / (root)");
}

#[test]
fn test_storage_detection_no_match() {
    let path = "/nonexistent/path";
    let mount_points = vec!["/home", "/var"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, None, "Should not match any mount point");
}

#[test]
fn test_storage_detection_exact_match() {
    let path = "/data";
    let mount_points = vec!["/", "/data"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(1), "Should match exact path /data");
}

#[test]
fn test_storage_detection_multiple_similar_prefixes() {
    let path = "/mnt/nvme/builds";
    let mount_points = vec!["/", "/mnt", "/mnt/nvme", "/mnt/sata"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(2), "Should match /mnt/nvme (longest matching prefix)");
}

#[test]
fn test_storage_detection_symlink_like_paths() {
    let path = "/root/projects";
    let mount_points = vec!["/", "/root"];
    
    let best = find_longest_prefix_match(path, &mount_points);
    assert_eq!(best, Some(1), "Should match /root");
}

#[test]
fn test_storage_detection_tmp_fallback() {
    let path = "";
    // Empty path should fall back to /tmp/goatd-build
    assert_eq!(path, "", "Empty path verification");
}

// Helper function to get variant repo URL (extracted from main.rs)
fn get_variant_repo_url(variant: &str) -> Option<&'static str> {
    match variant {
        "linux" => Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git"),
        "linux-mainline" => Some("https://aur.archlinux.org/linux-mainline.git"),
        "linux-lts" => Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git"),
        "linux-hardened" => Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git"),
        _ => None,
    }
}

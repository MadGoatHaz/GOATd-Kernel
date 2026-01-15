//! Configuration module for kernel build management.
//!
//! This module provides a comprehensive configuration system for managing kernel builds,
//! including loading, validating, and managing configuration profiles. It coordinates
//! with hardware detection to create optimal build configurations.
//!
//! # Module Structure
//!
//! - `loader`: Handles loading configurations from files and serialization formats
//! - `validator`: Validates configuration parameters and detects conflicts
//! - `modprobed`: Manages modprobed-db integration for module filtering
//! - `whitelist`: Manages driver whitelist functionality
//! - `exclusions`: Manages driver exclusion lists
//!
//! # Configuration Flow
//!
//! 1. Hardware detection provides system capabilities
//! 2. ConfigManager loads or creates KernelConfig
//! 3. Validator ensures all settings are compatible
//! 4. Submodules apply specialized configuration logic
//! 5. Final config is ready for build orchestration
//!
//! # Settings Management
//!
//! The `SettingsManager` provides thread-safe access to `AppState`:
//! - Uses `Arc<RwLock<AppState>>` for parallel reads
//! - Persists state to `config/settings.json`
//! - Handles serialization/deserialization

pub mod loader;
pub mod validator;
pub mod modprobed;
pub mod whitelist;
pub mod exclusions;
pub mod profiles;
pub mod finalizer;

use crate::error::ConfigError;
use crate::models::{KernelConfig, LtoType, HardeningLevel};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Application state for managing build configuration and settings
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AppState {
    // Build Settings
    pub selected_variant: String,
    pub selected_profile: String,
    pub selected_lto: String,
    pub selected_scx_profile: String,
    pub selected_scx_mode: String,
    pub kernel_hardening: HardeningLevel,
    pub secure_boot: bool,
    pub use_modprobed: bool,
    pub use_whitelist: bool,
    pub use_polly: bool,
    pub use_mglru: bool,
    pub native_optimizations: bool,
    
    // Override flags: Track if user manually toggled a feature
    // These prevent profile changes from wiping out user customizations
    pub user_toggled_polly: bool,
    pub user_toggled_mglru: bool,
    pub user_toggled_hardening: bool,
    pub user_toggled_lto: bool,
    pub user_toggled_bore: bool,
    
    // Settings
    pub workspace_path: String,
    pub security_level: String,
    pub startup_audit: bool,
    pub theme_mode: String,
    pub minimize_to_tray: bool,
    
    // Kernel source path (where the repository was cloned)
    pub kernel_source_path: String,
    
    // Security & Verification Settings
    pub verify_signatures: bool,
    
    // UI Customization Settings
    pub theme_idx: usize,
    pub ui_font_size: f32,
    pub show_fps: bool,
    pub auto_scroll_logs: bool,
    pub check_for_updates: bool,
    pub save_window_state: bool,
    
    // Debug Settings
    pub debug_logging: bool,
    pub tokio_tracing: bool,
    pub audit_on_startup: bool,
    
    // Performance monitoring settings
    /// Whether to continue monitoring when window is minimized
    pub perf_background_enabled: bool,
    /// Alert threshold for background spikes (microseconds)
    pub perf_alert_threshold_us: f32,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            selected_variant: "linux".to_string(),
            selected_profile: "gaming".to_string(),  // LOWERCASE - matches profiles.rs HashMap keys
            selected_lto: "thin".to_string(),
            selected_scx_profile: "Default (Safe)".to_string(),
            selected_scx_mode: "Auto".to_string(),
            kernel_hardening: HardeningLevel::Standard,
            secure_boot: false,
            use_modprobed: true,
            use_whitelist: true,
            use_polly: false,
            use_mglru: false,
            native_optimizations: true,
            user_toggled_polly: false,
            user_toggled_mglru: false,
            user_toggled_hardening: false,
            user_toggled_lto: false,
            user_toggled_bore: false,
            workspace_path: String::new(),
            security_level: "Standard".to_string(),
            startup_audit: false,
            theme_mode: "4rchCybrPnk".to_string(),
            minimize_to_tray: false,
            kernel_source_path: String::new(),
            verify_signatures: true,
            theme_idx: 0,
            ui_font_size: 12.0,
            show_fps: true,
            auto_scroll_logs: true,
            check_for_updates: true,
            save_window_state: true,
            debug_logging: false,
            tokio_tracing: false,
            audit_on_startup: false,
            perf_background_enabled: true,
            perf_alert_threshold_us: 500.0,
        }
    }
}

/// Thread-safe settings manager for AppState persistence
///
/// Uses `Arc<RwLock<AppState>>` for parallel read access while ensuring
/// exclusive write access. State is persisted to `config/settings.json`.
pub struct SettingsManager;

impl SettingsManager {
    /// Load AppState from config/settings.json, or return defaults if file doesn't exist
    ///
    /// STRICT SANITIZATION: Validates paths to ensure they're not stale absolute paths.
    /// If a path is absolute and doesn't exist, it's ACTIVELY RESET to `.` (relative CWD).
    /// This prevents outdated mount points or removed paths from breaking builds.
    ///
    /// ERROR HANDLING: If deserialization fails, logs a warning and returns defaults
    /// instead of panicking. This provides graceful fallback when config format changes.
    pub fn load() -> Result<AppState, ConfigError> {
        let config_path = "config/settings.json";
        
        match std::fs::read_to_string(config_path) {
            Ok(content) => {
                match serde_json::from_str::<AppState>(&content) {
                    Ok(mut state) => {
                        // STRICT SANITIZATION: Validate and reset workspace_path if absolute and non-existent
                        if !state.workspace_path.is_empty() {
                            let path = std::path::PathBuf::from(&state.workspace_path);
                            if path.is_absolute() && !path.exists() {
                                eprintln!("[Config] [CRITICAL] STALE PATH DETECTED - workspace_path: {}", state.workspace_path);
                                eprintln!("[Config] [CRITICAL] Path is absolute but DOES NOT EXIST");
                                eprintln!("[Config] [CRITICAL] ACTIVELY RESETTING to relative CWD: '.'");
                                state.workspace_path = ".".to_string();
                            }
                        }
                        
                        // STRICT SANITIZATION: Validate and reset kernel_source_path if absolute and non-existent
                        if !state.kernel_source_path.is_empty() {
                            let path = std::path::PathBuf::from(&state.kernel_source_path);
                            if path.is_absolute() && !path.exists() {
                                eprintln!("[Config] [CRITICAL] STALE PATH DETECTED - kernel_source_path: {}", state.kernel_source_path);
                                eprintln!("[Config] [CRITICAL] Path is absolute but DOES NOT EXIST");
                                eprintln!("[Config] [CRITICAL] ACTIVELY RESETTING to relative CWD: '.'");
                                state.kernel_source_path = ".".to_string();
                            }
                        }
                        
                        Ok(state)
                    }
                    Err(e) => {
                        // Graceful fallback: Log warning and return defaults instead of panicking
                        eprintln!("[Config] [WARNING] Failed to parse settings.json, falling back to defaults: {}", e);
                        Ok(AppState::default())
                    }
                }
            }
            Err(_) => Ok(AppState::default()),
        }
    }

    /// Save AppState to config/settings.json
    pub fn save(state: &AppState) -> Result<(), ConfigError> {
        let config_path = "config/settings.json";
        
        // Ensure config directory exists
        let config_dir = std::path::Path::new("config");
        if !config_dir.exists() {
            std::fs::create_dir_all(config_dir)
                .map_err(ConfigError::IoError)?;
        }
        
        // Serialize and write to file
        let content = serde_json::to_string_pretty(state)
            .map_err(ConfigError::InvalidJson)?;
        
        std::fs::write(config_path, content)
            .map_err(ConfigError::IoError)?;
        
        Ok(())
    }

    /// Create a thread-safe shared instance of AppState
    pub fn new_shared() -> Result<Arc<RwLock<AppState>>, ConfigError> {
        let state = Self::load()?;
        Ok(Arc::new(RwLock::new(state)))
    }
}

/// Primary configuration manager for kernel builds.
///
/// ConfigManager handles the loading, validation, and management of kernel
/// build configurations. It coordinates multiple submodules to provide a
/// complete configuration system.
///
/// # Fields
///
/// * `config_dir` - Directory for storing configuration files
/// * `config` - Current active kernel configuration
/// * `default_config` - Fallback configuration for defaults
#[derive(Debug, Clone)]
pub struct ConfigManager {
    config_dir: PathBuf,
    config: KernelConfig,
}

impl ConfigManager {
    /// Create a new ConfigManager with a configuration directory and initial config.
    ///
    /// # Arguments
    ///
    /// * `config_dir` - Path to the configuration directory
    /// * `config` - Initial KernelConfig
    ///
    /// # Returns
    ///
    /// A new ConfigManager instance
    pub fn new(config_dir: PathBuf, config: KernelConfig) -> Self {
        ConfigManager { config_dir, config }
    }

    /// Get the configuration directory path.
    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }

    /// Get a reference to the current configuration.
    pub fn config(&self) -> &KernelConfig {
        &self.config
    }

    /// Get a mutable reference to the current configuration.
    pub fn config_mut(&mut self) -> &mut KernelConfig {
        &mut self.config
    }

    /// Update the entire configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - New KernelConfig to use
    pub fn set_config(&mut self, config: KernelConfig) {
        self.config = config;
    }

    /// Load a configuration from a file.
    ///
    /// Reads a JSON configuration file and deserializes it into a KernelConfig.
    /// Updates the manager's current configuration if successful.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Returns
    ///
    /// Result containing the loaded KernelConfig or ConfigError
    pub fn load_from_file(&mut self, path: &PathBuf) -> Result<(), ConfigError> {
        self.config = loader::load_config_from_file(path)?;
        Ok(())
    }

    /// Save the current configuration to a file.
    ///
    /// Serializes the current KernelConfig as JSON and writes to the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the configuration will be saved
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), ConfigError> {
        loader::save_config_to_file(&self.config, path)?;
        Ok(())
    }

    /// Validate the current configuration.
    ///
    /// Runs all validation checks on the current configuration, including:
    /// - Parameter range checks
    /// - Conflicting setting detection
    /// - Module compatibility verification
    ///
    /// # Returns
    ///
    /// Result indicating validity or ConfigError with details
    pub fn validate(&self) -> Result<(), ConfigError> {
        validator::validate_config(&self.config)
    }

    /// Apply modprobed-db filtering settings to the configuration.
    ///
    /// Enables modprobed-db module filtering and applies detected module list
    /// to the configuration.
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn apply_modprobed(&mut self) -> Result<(), ConfigError> {
        self.config.use_modprobed = true;
        modprobed::prepare_modprobed_db(&mut self.config)?;
        Ok(())
    }

    /// Apply driver whitelist to the configuration.
    ///
    /// Enables driver whitelist filtering and applies approved driver list
    /// to configuration options.
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn apply_whitelist(&mut self) -> Result<(), ConfigError> {
        self.config.use_whitelist = true;
        whitelist::apply_whitelist(&mut self.config);
        Ok(())
    }

    /// Add a driver exclusion to the configuration.
    ///
    /// Excludes a specific driver from the build by adding it to the
    /// driver_exclusions list.
    ///
    /// # Arguments
    ///
    /// * `driver` - Name of the driver to exclude
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn exclude_driver(&mut self, driver: String) -> Result<(), ConfigError> {
        exclusions::add_exclusion(&mut self.config, &driver)?;
        Ok(())
    }

    /// Clear all driver exclusions from the configuration.
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn clear_exclusions(&mut self) -> Result<(), ConfigError> {
        self.config.driver_exclusions.clear();
        Ok(())
    }

    /// Switch to a build profile by name.
    ///
    /// Sets the profile name in the configuration. The actual profile application
    /// (loading defaults, MGLRU tuning, etc.) happens in the Finalizer, which is
    /// called during the orchestration phase.
    ///
    /// # Arguments
    ///
    /// * `profile_name` - Name of the profile to use (e.g., "gaming", "workstation")
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError if profile not found
    pub fn apply_profile(&mut self, profile_name: &str) -> Result<(), ConfigError> {
        // Verify the profile exists
        profiles::get_profile(profile_name)
            .ok_or_else(|| ConfigError::ValidationFailed(
                format!("Unknown profile: {}", profile_name)
            ))?;
        
        // Set the profile name (actual application happens in Finalizer)
        self.config.profile = profile_name.to_string();
        Ok(())
    }

    /// Set the LTO optimization level.
    ///
    /// # Arguments
    ///
    /// * `lto_type` - The LTO level to use (Base, Thin, or Full)
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn set_lto(&mut self, lto_type: LtoType) -> Result<(), ConfigError> {
        self.config.lto_type = lto_type;
        Ok(())
    }

    /// Set a kernel configuration option.
    ///
    /// Adds or updates a kernel-level configuration option. These are applied
    /// during the configuration phase of the build.
    ///
    /// # Arguments
    ///
    /// * `key` - Configuration option key
    /// * `value` - Configuration option value
    ///
    /// # Returns
    ///
    /// Result indicating success or ConfigError
    pub fn set_config_option(&mut self, key: String, value: String) -> Result<(), ConfigError> {
        self.config.config_options.insert(key, value);
        Ok(())
    }

    /// Get a kernel configuration option by key.
    ///
    /// # Arguments
    ///
    /// * `key` - Configuration option key
    ///
    /// # Returns
    ///
    /// Option containing the value if found
    pub fn get_config_option(&self, key: &str) -> Option<&String> {
        self.config.config_options.get(key)
    }

    /// Create a default configuration for a given kernel version.
    ///
    /// Constructs a new KernelConfig with sensible defaults for the specified
    /// kernel version, based on standard optimization practices.
    ///
    /// # Arguments
    ///
    /// * `kernel_version` - The kernel version string (e.g., "6.6.0")
    ///
    /// # Returns
    ///
    /// A new default KernelConfig
    pub fn create_default_config(kernel_version: String) -> KernelConfig {
        let mut config = KernelConfig::default();
        config.version = kernel_version;
        config
    }

    /// Get configuration summary as a formatted string.
    ///
    /// Returns a human-readable summary of the current configuration state,
    /// useful for logging and debugging.
    ///
    /// # Returns
    ///
    /// A formatted configuration summary string
    pub fn get_summary(&self) -> String {
        format!(
            "Configuration Summary:\n  Kernel: {}\n  LTO: {:?}\n  Modprobed: {}\n  Whitelist: {}\n  Exclusions: {}",
            self.config.version,
            self.config.lto_type,
            self.config.use_modprobed,
            self.config.use_whitelist,
            self.config.driver_exclusions.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_manager_creation() {
        let config = ConfigManager::create_default_config("6.6.0".to_string());
        let manager = ConfigManager::new(PathBuf::from("/tmp"), config);
        
        assert_eq!(manager.config_dir(), &PathBuf::from("/tmp"));
        assert_eq!(manager.config().version, "6.6.0");
        assert_eq!(manager.config().lto_type, LtoType::Thin);
    }

    #[test]
    fn test_set_lto() {
        let config = ConfigManager::create_default_config("6.6.0".to_string());
        let mut manager = ConfigManager::new(PathBuf::from("/tmp"), config);
        
        manager.set_lto(LtoType::Full).unwrap();
        assert_eq!(manager.config().lto_type, LtoType::Full);
    }

    #[test]
    fn test_set_config_option() {
        let config = ConfigManager::create_default_config("6.6.0".to_string());
        let mut manager = ConfigManager::new(PathBuf::from("/tmp"), config);
        
        manager.set_config_option("FOO".to_string(), "bar".to_string()).ok();
        assert_eq!(manager.get_config_option("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_get_summary() {
        let config = ConfigManager::create_default_config("6.6.0".to_string());
        let manager = ConfigManager::new(PathBuf::from("/tmp"), config);
        
        let summary = manager.get_summary();
        assert!(summary.contains("6.6.0"));
        assert!(summary.contains("Thin"));
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert_eq!(state.selected_variant, "linux");
        assert_eq!(state.selected_profile, "gaming");
        assert_eq!(state.selected_lto, "thin");
    }
}

//! Unified error type hierarchy for GOATd Kernel
//!
//! Provides structured error handling with HardwareError, ConfigError, BuildError,
//! PatchError, ValidationError, and AppError.

use std::io;
use thiserror::Error;

/// Hardware detection and system information errors.
#[derive(Error, Debug)]
pub enum HardwareError {
    #[error("GPU detection failed: {0}")]
    GpuDetectionFailed(String),

    #[error("Boot manager detection failed: {0}")]
    BootDetectionFailed(String),

    #[error("Init system detection failed: {0}")]
    InitDetectionFailed(String),

    #[error("System info unavailable: {0}")]
    SystemInfoUnavailable(String),

    #[error("IO error during hardware detection: {0}")]
    IoError(#[from] io::Error),
}

/// Configuration file parsing and validation errors.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found: {0}")]
    FileNotFound(String),

    #[error("Invalid JSON in config: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Configuration validation failed: {0}")]
    ValidationFailed(String),

    #[error("Conflicting settings detected: {0}")]
    ConflictDetected(String),

    #[error("IO error during config operations: {0}")]
    IoError(#[from] io::Error),
}

/// Build process execution errors.
#[derive(Error, Debug)]
pub enum BuildError {
    #[error("Preparation phase failed: {0}")]
    PreparationFailed(String),

    #[error("Configuration phase failed: {0}")]
    ConfigurationFailed(String),

    #[error("Patching phase failed: {0}")]
    PatchingFailed(String),

    #[error("Build phase failed: {0}")]
    BuildFailed(String),

    #[error("Build cancelled by user")]
    BuildCancelled,

    #[error("Validation phase failed: {0}")]
    ValidationFailed(String),
}

/// Kernel patching operation errors.
#[derive(Error, Debug)]
pub enum PatchError {
    #[error("Invalid regex pattern: {0}")]
    RegexInvalid(String),

    #[error("Patch target file not found: {0}")]
    FileNotFound(String),

    #[error("Patch application failed: {0}")]
    PatchFailed(String),

    #[error("Patch validation failed: {0}")]
    ValidationFailed(String),
}

/// Build result validation errors.
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Required artifact missing: {0}")]
    ArtifactMissing(String),

    #[error("Configuration invalid for validation: {0}")]
    ConfigInvalid(String),

    #[error("Boot not ready: {0}")]
    BootNotReady(String),
}

/// Global error type for all GOATd Kernel modules (Phase 4)
///
/// Provides unified error categorization and user-facing messages.
/// All new module methods should return `Result<T, AppError>` instead of `Box<dyn Error>`.
///
/// # Phase 0 Requirement (Finding 3.2)
/// Unified error type ensures consistent error handling across all modules.
#[derive(Error, Debug, Clone)]
pub enum AppError {
    /// OS command failed (e.g., pacman, uname, pkexec)
    #[error("Command '{cmd}' failed: {reason}")]
    OsCommand { cmd: String, reason: String },

    /// Hardware detection failed
    #[error("Hardware detection failed: {0}")]
    HardwareDetection(String),

    /// Kernel config parse or read error
    #[error("Kernel config error: {0}")]
    KernelConfig(String),

    /// File I/O error (read/write/delete)
    #[error("I/O error: {0}")]
    Io(String),

    /// Settings persist or deserialize error
    #[error("Settings error: {0}")]
    Settings(String),

    /// Module initialization failed
    #[error("Module initialization failed: {0}")]
    ModuleInit(String),

    /// Invalid input (e.g., package name with shell chars)
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Invalid path for Kbuild operations (contains spaces or colons)
    #[error("Invalid path for Kbuild: {0}")]
    InvalidPath(String),

    /// Audit operation cancelled or timed out
    #[error("Audit error: {0}")]
    Audit(String),
}

impl AppError {
    /// Get a user-facing error message suitable for UI display
    pub fn user_message(&self) -> String {
        match self {
            AppError::OsCommand { cmd, reason } => {
                format!("Failed to execute '{}': {}", cmd, reason)
            }
            AppError::HardwareDetection(msg) => format!("Could not detect hardware: {}", msg),
            AppError::KernelConfig(msg) => format!("Kernel configuration error: {}", msg),
            AppError::Io(msg) => format!("File operation failed: {}", msg),
            AppError::Settings(msg) => format!("Settings error: {}", msg),
            AppError::ModuleInit(msg) => format!("Failed to initialize application: {}", msg),
            AppError::InvalidInput(msg) => format!("Invalid input: {}", msg),
            AppError::InvalidPath(msg) => format!("Invalid workspace path for Kbuild: {}", msg),
            AppError::Audit(msg) => format!("System audit failed: {}", msg),
        }
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Io(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Io(s.to_string())
    }
}

/// Top-level result type for operations that may fail.
/// Use this as the return type for all fallible functions.
/// Example: `fn risky_operation() -> Result<String>`
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_error_display() {
        let err = HardwareError::GpuDetectionFailed("NVIDIA not found".to_string());
        assert_eq!(err.to_string(), "GPU detection failed: NVIDIA not found");
    }

    #[test]
    fn test_config_error_display() {
        let err = ConfigError::FileNotFound("/etc/config.json".to_string());
        assert_eq!(
            err.to_string(),
            "Configuration file not found: /etc/config.json"
        );
    }

    #[test]
    fn test_result_type_ok() {
        let result: Result<i32> = Ok(42);
        assert!(result.is_ok());
    }

    #[test]
    fn test_result_type_err() {
        let result: Result<i32> = Err("test error".into());
        assert!(result.is_err());
    }
}

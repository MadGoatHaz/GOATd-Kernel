//! GOATd Kernel Rust Backend
//!
//! This crate provides the Rust core infrastructure for the GOATd Kernel Builder,
//! offering compiled performance, type safety, and async state management for all
//! kernel build operations. It exposes a Rust API via Egui UI frontend.
//!
//! **Architecture**: The Egui UI frontend communicates with this Rust backend
//! for all build orchestration.
//!
//! The system is organized into functional modules:
//! - **error**: Unified error type hierarchy
//! - **models**: Core data structures and types
//! - **policy**: Hardware policy engine for configuration decisions
//! - **hardware**: Hardware detection utilities (Phase 1)
//! - **system**: OS abstraction, logging, and security wrappers (Phase 1)
//! - **config**: Configuration management utilities (Phase 2)
//! - **ui**: UI controller and Egui integration (Phase 2-3)
//! - **orchestrator**: Async build coordination and state management (Phases 1-5)
//! - **kernel**: Kernel management (package management, audit) (Phases 1-3)
//! - **validator**: Build validation utilities (Phase 5)

#![allow(dead_code)]

// Core foundational modules
pub mod error;
pub mod models;

// Hardware policy engine for GPU, LTO, and driver decisions
pub mod policy;

// Phase 1: Hardware detection module
pub mod hardware;

// Phase 1: System abstraction module (OS wrappers, logging)
pub mod system;

// Phase 2: Configuration management module
pub mod config;

// Phase 2-3: UI controller and Egui integration
pub mod ui;

// Robust, decoupled logging system
pub mod log_collector;

// Phases 1-5: Build orchestration utilities and async state management
pub mod orchestrator;

// Re-export the log crate for macro usage
pub use log;

// Re-export logging initialization functions from system module
pub use system::{flush_all_logs, initialize_logging};

// Re-export log collector for use throughout the system
pub use log_collector::{LogCollector, LogLine};

// Phase 1-3: Kernel management module (package management, audit)
pub mod kernel;

// ============================================================================
// PUBLIC RE-EXPORTS FOR CONVENIENCE
// ============================================================================

// Re-export error types for easy access
pub use error::{BuildError, ConfigError, HardwareError, PatchError, Result, ValidationError};

// Re-export model types for easy access
pub use models::{
    BootManager,
    BootType,
    BuildPhase,
    BuildResult,
    BuildState,
    // Enums
    GpuVendor,
    // Hardware structs
    HardwareInfo,
    InitSystem,

    // Build structs
    KernelConfig,
    LtoType,
    Patch,
    PatchResult,
    PatchType,
    StorageType,
    ValidationCheck,
};

// Re-export policy engine types for easy access
pub use policy::{
    DriverPolicy, GpuDecision, GpuDetectionInfo, HardwarePolicy, LtoDecision,
    PolicyApplicationResult,
};

// Re-export hardware detector
pub use hardware::HardwareDetector;

// Re-export config types and SettingsManager
pub use config::{AppState, ConfigManager, SettingsManager};

// Re-export UI controller and traits (Phase 2)
pub use ui::AppController;

// Re-export orchestrator utilities and state management
pub use orchestrator::{
    configure_build,
    prepare_build_environment,
    prepare_kernel_build,
    // Stateless utility functions
    validate_hardware,
    validate_kernel_build,
    validate_kernel_config,
    // Async orchestrator and state management
    AsyncOrchestrator,
    BuildPhaseState,
    OrchestrationState,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant() {
        assert_eq!(VERSION, "0.1.0");
    }

    #[test]
    fn test_error_reexport() {
        // Verify error types are accessible via crate root
        let _: Result<i32> = Ok(42);
    }

    #[test]
    fn test_models_reexport() {
        // Verify model types are accessible via crate root
        let _gpu = GpuVendor::Nvidia;
        let _lto = LtoType::Full;
    }

    #[test]
    fn test_enum_variants_accessible() {
        assert_eq!(BuildPhase::Preparation, BuildPhase::Preparation);
        assert_eq!(BootType::Efi, BootType::Efi);
    }
}

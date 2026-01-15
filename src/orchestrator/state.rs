//! Build State Management and Phase Tracking
//!
//! This module provides the state tracking structures used by the async orchestrator
//! to manage kernel build execution across multiple phases.
//!
//! **Architecture**:
//! - `BuildPhase`: Enum representing discrete build phases
//! - `BuildState`: Struct tracking current phase, progress, and hardware context
//! - State transitions are managed by the async orchestrator
//!
//! The Python orchestrator provides phase orchestration logic, while this module
//! provides the data structures for async state management within Rust.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::models::{HardwareInfo, KernelConfig, BuildState};

/// Build phase enumeration - discrete states in the build lifecycle.
///
/// Represents the current phase of kernel build execution.
/// The orchestrator transitions between these phases sequentially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildPhaseState {
    /// Phase 1: Hardware detection and environment validation
    Preparation,
    
    /// Phase 2: Kernel config loading and policy application
    Configuration,
    
    /// Phase 3: LTO shielding, ICF removal, patch application
    Patching,
    
    /// Phase 4: makepkg execution and build monitoring
    Building,
    
    /// Phase 5: Artifact verification and LTO confirmation
    Validation,

    /// Phase 6: Automated installation of built packages
    Installation,
    
    /// Build completed successfully
    Completed,
    
    /// Build failed, recovery/rollback available
    Failed,
}

impl BuildPhaseState {
    /// Get the human-readable name for this phase.
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildPhaseState::Preparation => "preparation",
            BuildPhaseState::Configuration => "configuration",
            BuildPhaseState::Patching => "patching",
            BuildPhaseState::Building => "building",
            BuildPhaseState::Validation => "validation",
            BuildPhaseState::Installation => "installation",
            BuildPhaseState::Completed => "completed",
            BuildPhaseState::Failed => "failed",
        }
    }

    /// Get all valid phase transitions FROM this phase.
    pub fn valid_next_phases(&self) -> Vec<BuildPhaseState> {
        match self {
            BuildPhaseState::Preparation => vec![BuildPhaseState::Configuration, BuildPhaseState::Failed],
            BuildPhaseState::Configuration => vec![BuildPhaseState::Patching, BuildPhaseState::Failed],
            BuildPhaseState::Patching => vec![BuildPhaseState::Building, BuildPhaseState::Failed],
            BuildPhaseState::Building => vec![BuildPhaseState::Validation, BuildPhaseState::Failed],
            BuildPhaseState::Validation => vec![BuildPhaseState::Installation, BuildPhaseState::Completed, BuildPhaseState::Failed],
            BuildPhaseState::Installation => vec![BuildPhaseState::Completed, BuildPhaseState::Failed],
            BuildPhaseState::Completed => vec![],
            BuildPhaseState::Failed => vec![BuildPhaseState::Preparation], // Allow recovery restart
        }
    }

    /// Check if a transition to the given phase is valid.
    pub fn can_transition_to(&self, next: BuildPhaseState) -> bool {
        self.valid_next_phases().contains(&next)
    }
}

/// Build execution state snapshot for tracking progress and recovery.
///
/// Maintained by the async orchestrator and persisted to checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationState {
    /// Current build phase
    pub phase: BuildPhaseState,

    /// Overall progress percentage (0-100)
    pub progress: u32,

    /// Hardware snapshot at build start
    pub hardware: HardwareInfo,

    /// Active kernel configuration
    pub config: KernelConfig,

    /// Number of patches applied
    pub patches_applied: u32,

    /// Number of patches failed
    pub patches_failed: u32,

    /// Build start timestamp
    pub start_time: SystemTime,

    /// Last phase update timestamp
    pub last_update_time: SystemTime,

    /// Error message if phase failed
    pub error: Option<String>,

    /// Path to checkpoint file (for recovery)
    pub checkpoint_path: Option<PathBuf>,
}

impl OrchestrationState {
    /// Create a new orchestration state for a build execution.
    pub fn new(
        hardware: HardwareInfo,
        config: KernelConfig,
    ) -> Self {
        let now = SystemTime::now();
        OrchestrationState {
            phase: BuildPhaseState::Preparation,
            progress: 0,
            hardware,
            config,
            patches_applied: 0,
            patches_failed: 0,
            start_time: now,
            last_update_time: now,
            error: None,
            checkpoint_path: None,
        }
    }

    /// Attempt to transition to the next phase.
    pub fn transition_to(&mut self, next_phase: BuildPhaseState) -> Result<(), String> {
        if !self.phase.can_transition_to(next_phase) {
            return Err(format!(
                "Invalid phase transition: {} -> {}",
                self.phase.as_str(),
                next_phase.as_str()
            ));
        }
        self.phase = next_phase;
        self.last_update_time = SystemTime::now();
        Ok(())
    }

    /// Update progress percentage (0-100).
    pub fn set_progress(&mut self, percent: u32) {
        self.progress = percent.min(100);
        self.last_update_time = SystemTime::now();
    }

    /// Record a patch application attempt.
    pub fn record_patch_applied(&mut self, success: bool) {
        if success {
            self.patches_applied += 1;
        } else {
            self.patches_failed += 1;
        }
    }

    /// Record an error and mark phase as failed.
    pub fn record_error(&mut self, error: String) {
        self.error = Some(error);
        self.phase = BuildPhaseState::Failed;
        self.last_update_time = SystemTime::now();
    }

    /// Get time elapsed since build start.
    pub fn elapsed_since_start(&self) -> Result<std::time::Duration, std::time::SystemTimeError> {
        self.start_time.elapsed()
    }

    /// Convert to a legacy BuildState for serialization compatibility.
    pub fn to_build_state(&self) -> BuildState {
        let phase = match self.phase {
            BuildPhaseState::Preparation => crate::models::BuildPhase::Preparation,
            BuildPhaseState::Configuration => crate::models::BuildPhase::Configuration,
            BuildPhaseState::Patching => crate::models::BuildPhase::Patching,
            BuildPhaseState::Building => crate::models::BuildPhase::Building,
            BuildPhaseState::Validation => crate::models::BuildPhase::Validation,
            BuildPhaseState::Installation => crate::models::BuildPhase::Validation,
            // Completed and Failed don't have direct equivalents in BuildPhase,
            // so we map them to the last valid phase (Validation)
            BuildPhaseState::Completed => crate::models::BuildPhase::Validation,
            BuildPhaseState::Failed => crate::models::BuildPhase::Validation,
        };
        BuildState {
            phase,
            progress_percent: self.progress,
            hardware: self.hardware.clone(),
            config: self.config.clone(),
            patches_applied: vec![],
            checkpoint_timestamp: SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_transitions() {
        assert!(BuildPhaseState::Preparation.can_transition_to(BuildPhaseState::Configuration));
        assert!(!BuildPhaseState::Preparation.can_transition_to(BuildPhaseState::Validation));
    }

    #[test]
    fn test_orchestration_state_creation() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Intel i7".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 32,
            disk_free_gb: 100,
            gpu_vendor: crate::models::GpuVendor::Nvidia,
            gpu_model: "NVIDIA RTX 3080".to_string(),
            storage_type: crate::models::StorageType::Nvme,
            storage_model: "Samsung 970 EVO".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
             version: "6.6.0".to_string(),
             lto_type: crate::models::LtoType::Thin,
             use_modprobed: true,
             use_whitelist: false,
             driver_exclusions: vec![],
             config_options: std::collections::HashMap::new(),
             hardening: crate::models::HardeningLevel::Standard,
             secure_boot: false,
             profile: "Generic".to_string(),
             use_bore: false,
             use_polly: false,
             use_mglru: false,
             user_toggled_bore: false,
             user_toggled_polly: false,
             user_toggled_mglru: false,
             user_toggled_hardening: false,
             user_toggled_lto: false,
             mglru_enabled_mask: 0x0007,
             mglru_min_ttl_ms: 1000,
             hz: 300,
             preemption: "Voluntary".to_string(),
             force_clang: true,
             lto_shield_modules: vec![],
             scx_available: vec![],
             scx_active_scheduler: None,
          };

          let state = OrchestrationState::new(hw.clone(), config.clone());
        assert_eq!(state.phase, BuildPhaseState::Preparation);
        assert_eq!(state.progress, 0);
    }

    #[test]
    fn test_transition_to_next_phase() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test".to_string(),
            cpu_cores: 4,
            cpu_threads: 8,
            ram_gb: 16,
            disk_free_gb: 50,
            gpu_vendor: crate::models::GpuVendor::Unknown,
            gpu_model: "Unknown".to_string(),
            storage_type: crate::models::StorageType::Ssd,
            storage_model: "Unknown".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "unknown".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.0.0".to_string(),
            lto_type: crate::models::LtoType::Full,
            use_modprobed: false,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
            hardening: crate::models::HardeningLevel::Standard,
            secure_boot: false,
            profile: "Generic".to_string(),
            use_bore: false,
            use_polly: false,
            use_mglru: false,
            user_toggled_bore: false,
            user_toggled_polly: false,
            user_toggled_mglru: false,
             user_toggled_hardening: false,
             user_toggled_lto: false,
             mglru_enabled_mask: 0x0007,
             mglru_min_ttl_ms: 1000,
             hz: 300,
             preemption: "Voluntary".to_string(),
             force_clang: true,
             lto_shield_modules: vec![],
             scx_available: vec![],
             scx_active_scheduler: None,
         };

         let mut state = OrchestrationState::new(hw, config);
        assert!(state.transition_to(BuildPhaseState::Configuration).is_ok());
        assert_eq!(state.phase, BuildPhaseState::Configuration);
    }

    #[test]
    fn test_invalid_phase_transition() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test".to_string(),
            cpu_cores: 4,
            cpu_threads: 8,
            ram_gb: 16,
            disk_free_gb: 50,
            gpu_vendor: crate::models::GpuVendor::Unknown,
            gpu_model: "Unknown".to_string(),
            storage_type: crate::models::StorageType::Ssd,
            storage_model: "Unknown".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "unknown".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.0.0".to_string(),
            lto_type: crate::models::LtoType::Full,
            use_modprobed: false,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
            hardening: crate::models::HardeningLevel::Standard,
            secure_boot: false,
            profile: "Generic".to_string(),
            use_bore: false,
            use_polly: false,
            use_mglru: false,
            user_toggled_bore: false,
            user_toggled_polly: false,
            user_toggled_mglru: false,
             user_toggled_hardening: false,
             user_toggled_lto: false,
             mglru_enabled_mask: 0x0007,
             mglru_min_ttl_ms: 1000,
             hz: 300,
             preemption: "Voluntary".to_string(),
             force_clang: true,
             lto_shield_modules: vec![],
             scx_available: vec![],
             scx_active_scheduler: None,
         };

        let mut state = OrchestrationState::new(hw, config);
        assert!(state.transition_to(BuildPhaseState::Validation).is_err());
    }

    #[test]
    fn test_record_patch_applied() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test".to_string(),
            cpu_cores: 4,
            cpu_threads: 8,
            ram_gb: 16,
            disk_free_gb: 50,
            gpu_vendor: crate::models::GpuVendor::Unknown,
            gpu_model: "Unknown".to_string(),
            storage_type: crate::models::StorageType::Ssd,
            storage_model: "Unknown".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "unknown".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.0.0".to_string(),
            lto_type: crate::models::LtoType::Full,
            use_modprobed: false,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
            hardening: crate::models::HardeningLevel::Standard,
            secure_boot: false,
            profile: "Generic".to_string(),
            use_bore: false,
            use_polly: false,
            use_mglru: false,
            user_toggled_bore: false,
            user_toggled_polly: false,
            user_toggled_mglru: false,
            user_toggled_hardening: false,
            user_toggled_lto: false,
            mglru_enabled_mask: 0x0007,
            mglru_min_ttl_ms: 1000,
            hz: 300,
            preemption: "Voluntary".to_string(),
            force_clang: true,
            lto_shield_modules: vec![],
            scx_available: vec![],
            scx_active_scheduler: None,
        };

        let mut state = OrchestrationState::new(hw, config);
        state.record_patch_applied(true);
        state.record_patch_applied(true);
        state.record_patch_applied(false);
        
        assert_eq!(state.patches_applied, 2);
        assert_eq!(state.patches_failed, 1);
    }
}

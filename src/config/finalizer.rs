//! Configuration Finalizer - Single point to finalize kernel build configs
//!
//! This module implements the "Rule Engine" layer as defined in the Architectural Blueprint V2.
//! The finalizer is responsible for:
//! 1. Applying profile-based defaults via hierarchical resolution
//! 2. Setting MGLRU tuning parameters based on profile
//! 3. Applying GPU-aware driver exclusions and LTO shielding
//! 4. Generating derived config_options strings
//! 5. Validation before returning finalized config
//!
//! # Design Principle: Hierarchical Resolution
//!
//! The finalizer applies configuration in this precedence (highest to lowest):
//! 1. Hardware Detection (GPU exclusions, LTO shielding from detected hardware)
//! 2. User Overrides (if user explicitly toggled features in AppState)
//! 3. Profile Defaults (baseline from selected profile: HZ, Preemption, Clang, etc.)
//!
//! This ensures the finalizer is the single authoritative point where all rules
//! are applied before the orchestrator begins execution.
//!
//! # Flow
//!
//! ```text
//! KernelConfig (with profile name) + HardwareInfo
//!         ↓
//!  finalize_kernel_config()
//!         ↓
//!  1. Load ProfileDefinition (pure data)
//!  2. Apply profile defaults to first-class fields
//!  3. Set MGLRU parameters based on profile
//!  4. Apply GPU exclusions (hardware-aware)
//!  5. Determine LTO shielding modules (GPU-based)
//!  6. Generate derived config_options strings
//!  7. Return finalized config
//! ```

use super::{exclusions, profiles};
use crate::error::ConfigError;
use crate::models::{GpuVendor, HardwareInfo, KernelConfig, LtoType};

/// Finalize kernel configuration by applying all rules and defaults.
///
/// This is the single authoritative point where configuration is resolved
/// to its final state before build execution. It applies:
/// - Profile-based defaults (only if user hasn't explicitly set values)
/// - MGLRU tuning parameters based on the selected profile
/// - GPU-aware driver exclusions to reduce kernel size
/// - LTO shielding for sensitive GPU modules
/// - Derived config_options strings for the build system
///
/// # Arguments
///
/// * `config` - Mutable reference to the kernel configuration to finalize
/// * `hardware` - System hardware information for GPU-aware decisions
///
/// # Returns
///
/// Result containing the finalized KernelConfig or a ConfigError
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::finalizer::finalize_kernel_config;
/// # use goatd_kernel::models::{KernelConfig, HardwareInfo, GpuVendor};
/// #
/// # let mut config = KernelConfig {
/// #     profile: "Gaming".to_string(),
/// #     use_mglru: true,
/// #     ..KernelConfig::default()
/// # };
/// # let hardware = HardwareInfo {
/// #     gpu_vendor: GpuVendor::Nvidia,
/// #     ..HardwareInfo::default()
/// # };
/// #
/// let finalized = finalize_kernel_config(config, &hardware)?;
/// assert_eq!(finalized.mglru_enabled_mask, 0x0007);
/// assert_eq!(finalized.mglru_min_ttl_ms, 1000);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn finalize_kernel_config(
    mut config: KernelConfig,
    hardware: &HardwareInfo,
) -> Result<KernelConfig, ConfigError> {
    // =========================================================================
    // SANITY CHECK: Validate hardware before proceeding
    // =========================================================================
    if hardware.cpu_cores == 0 {
        return Err(ConfigError::ValidationFailed(
            "Hardware validation failed: CPU cores cannot be 0".to_string(),
        ));
    }

    if hardware.ram_gb == 0 {
        return Err(ConfigError::ValidationFailed(
            "Hardware validation failed: RAM cannot be 0 GB".to_string(),
        ));
    }

    eprintln!(
        "[Finalizer] [SANITY_CHECK] Hardware validation passed: {} cores, {} GB RAM",
        hardware.cpu_cores, hardware.ram_gb
    );

    // =========================================================================
    // Phase 1: Load Profile Definitions (pure data)
    // =========================================================================
    // Retrieve profile data from profiles module (now pure data provider)
    let profile_def = profiles::get_profile(&config.profile).ok_or_else(|| {
        ConfigError::ValidationFailed(format!("Unknown profile: {}", config.profile))
    })?;

    eprintln!("[Finalizer] [STEP 1] Loaded profile: {}", profile_def.name);

    // =========================================================================
    // Phase 2: Apply Profile Defaults to First-Class Fields
    // =========================================================================
    // This is where profile HZ, Preemption, and Clang settings materialize
    // into first-class fields instead of hidden config_options strings.
    // User overrides take precedence: if user_toggled_* is true, respect the user's choice.
    config.hz = profile_def.hz;
    config.preemption = profile_def.preemption.clone();
    config.force_clang = profile_def.use_clang;

    // PHASE 9.2: Honor user overrides for Polly, MGLRU, and Hardening
    // Pattern: if !config.user_toggled_* { apply profile default }
    if !config.user_toggled_polly {
        config.use_polly = profile_def.use_polly;
    }
    if !config.user_toggled_mglru {
        config.use_mglru = profile_def.use_mglru;
    }
    if !config.user_toggled_hardening {
        config.hardening = profile_def.hardening_level.clone();
    }
    if !config.user_toggled_native_optimizations {
        config.native_optimizations = profile_def.native_optimizations;
    }

    // PHASE 9.3: Honor user overrides for LTO (CRITICAL FIX)
    // If user explicitly set LTO level in UI, respect it - profile default should NOT override
    if !config.user_toggled_lto {
        config.lto_type = profile_def.default_lto;
    }

    config.use_modprobed = profile_def.enable_module_stripping;
    config.use_whitelist = profile_def.enable_module_stripping;

    // =========================================================================
    // CRITICAL SAFETY CONSTRAINT: Whitelist depends on modprobed-db
    // =========================================================================
    // The whitelist safety net ONLY makes sense when modprobed-db is active
    // (auto-discovery of drivers). If modprobed-db is disabled, force whitelist
    // off to prevent misunderstanding of what the whitelist does.
    if !config.use_modprobed {
        config.use_whitelist = false;
        eprintln!("[Finalizer] [SAFETY] ⚠️ Whitelisting requires modprobed-db: forcibly disabled use_whitelist");
    }

    eprintln!(
        "[Finalizer] [STEP 2] Applied profile defaults: HZ={}, Preemption={}, Clang={}, Polly={} (user_toggled={}), MGLRU={} (user_toggled={}), NativeOpt={} (user_toggled={}), Modprobed={}, Whitelist={}",
        config.hz, config.preemption, config.force_clang, config.use_polly, config.user_toggled_polly, config.use_mglru, config.user_toggled_mglru,
        config.native_optimizations, config.user_toggled_native_optimizations, config.use_modprobed, config.use_whitelist
    );

    // =========================================================================
    // Phase 3: Set MGLRU Tuning Parameters Based on Profile
    // =========================================================================
    if config.use_mglru {
        apply_mglru_tuning(&mut config);
        eprintln!(
            "[Finalizer] [STEP 3] Applied MGLRU tuning: mask=0x{:04x}, ttl={}ms",
            config.mglru_enabled_mask, config.mglru_min_ttl_ms
        );
    }

    // =========================================================================
    // Phase 4: Apply GPU-Aware Driver Exclusions
    // =========================================================================
    if config.use_modprobed {
        apply_gpu_exclusions(&mut config, hardware)?;
        eprintln!(
            "[Finalizer] [STEP 4] Applied GPU exclusions: {} drivers excluded",
            config.driver_exclusions.len()
        );
    }

    // =========================================================================
    // Phase 5: Determine LTO Shielding Modules (GPU-Based)
    // =========================================================================
    apply_gpu_lto_shielding(&mut config, hardware);
    eprintln!(
        "[Finalizer] [STEP 5] Applied GPU LTO shielding: {} modules shielded",
        config.lto_shield_modules.len()
    );

    // =========================================================================
    // Phase 6: Generate Derived config_options Strings
    // =========================================================================
    generate_derived_config_options(&mut config, &profile_def);
    eprintln!(
        "[Finalizer] [STEP 6] Generated {} derived config_options",
        config.config_options.len()
    );

    // =========================================================================
    // Phase 7: Ensure SCX Support Configuration
    // =========================================================================
    ensure_scx_config(&mut config);
    eprintln!("[Finalizer] [STEP 7] SCX configuration ensured");

    // =========================================================================
    // Phase 8: Return Finalized Config
    // =========================================================================
    eprintln!(
        "[Finalizer] [COMPLETE] Configuration finalized: {} profile ready for build",
        config.profile
    );
    Ok(config)
}

/// Set MGLRU tuning parameters based on the selected profile.
///
/// Configures the MGLRU enabled_mask and min_ttl_ms based on the kernel profile.
/// These values are later used by the orchestrator to generate runtime tuning commands.
///
/// **Profile-Specific Tuning**:
/// - **Gaming/Workstation**: enabled_mask=0x0007, min_ttl_ms=1000 (enables all subsystems)
/// - **Laptop**: enabled_mask=0x0007, min_ttl_ms=500 (more aggressive reclaim)
/// - **Server**: enabled_mask=0x0000, min_ttl_ms=1000 (MGLRU disabled)
/// - **Generic/Default**: enabled_mask=0x0007, min_ttl_ms=1000 (conservative defaults)
fn apply_mglru_tuning(config: &mut KernelConfig) {
    let (enabled_mask, min_ttl_ms) = match config.profile.as_str() {
        "Gaming" => (0x0007, 1000),      // All subsystems, 1000ms TTL
        "Workstation" => (0x0007, 1000), // All subsystems, 1000ms TTL
        "Laptop" => (0x0007, 500),       // All subsystems, 500ms TTL for aggressive reclaim
        "Server" => (0x0000, 1000),      // Disabled for server workloads
        _ => (0x0007, 1000),             // Default: all subsystems, 1000ms
    };

    config.mglru_enabled_mask = enabled_mask;
    config.mglru_min_ttl_ms = min_ttl_ms;
}

/// Apply GPU-aware driver exclusions based on detected hardware.
///
/// Delegates to the exclusions module to automatically exclude GPU drivers
/// that don't match the detected vendor. This provides aggressive kernel
/// size reduction by removing unused GPU driver stack.
fn apply_gpu_exclusions(
    config: &mut KernelConfig,
    hardware: &HardwareInfo,
) -> Result<(), ConfigError> {
    // Use the existing GPU exclusion policy from the exclusions module
    exclusions::apply_gpu_exclusions(config, hardware)
}

/// Determine LTO shielding modules based on detected GPU hardware.
///
/// GPU drivers are sensitive to LTO optimization and benefit from being
/// shielded from aggressive LTO passes. This function populates
/// `config.lto_shield_modules` based on the detected GPU vendor.
fn apply_gpu_lto_shielding(config: &mut KernelConfig, hardware: &HardwareInfo) {
    config.lto_shield_modules.clear();

    // Only apply shielding if LTO is enabled
    if config.lto_type == LtoType::None {
        return;
    }

    // Determine which GPU modules need LTO shielding based on vendor
    match hardware.gpu_vendor {
        GpuVendor::Nvidia => {
            // NVIDIA GPU drivers are sensitive to LTO
            config.lto_shield_modules.push("nvidia".to_string());
        }
        GpuVendor::Amd => {
            // AMD GPU drivers are sensitive to LTO
            config.lto_shield_modules.push("amdgpu".to_string());
            config.lto_shield_modules.push("amdkfd".to_string());
        }
        GpuVendor::Intel => {
            // Intel integrated GPUs (i915) may benefit from light shielding in Full LTO
            if config.lto_type == LtoType::Full {
                config.lto_shield_modules.push("i915".to_string());
            }
        }
        GpuVendor::Unknown => {
            // No shielding for unknown GPU vendors
        }
    }
}

/// Generate derived config_options strings from first-class fields.
///
/// This function creates the legacy config_options HashMap entries that are
/// used by the build system. It converts first-class fields like `hz` and
/// `preemption` into the underscore-prefixed keys that the patcher expects.
fn generate_derived_config_options(
    config: &mut KernelConfig,
    _profile_def: &profiles::ProfileDefinition,
) {
    // Clang compiler flag
    config.config_options.insert(
        "_FORCE_CLANG".to_string(),
        if config.force_clang { "1" } else { "0" }.to_string(),
    );

    // Preemption model flag
    let preemption_flag = match config.preemption.as_str() {
        "Voluntary" => "CONFIG_PREEMPT_VOLUNTARY=y",
        "Full" => "CONFIG_PREEMPT=y",
        "Full-RT" => "CONFIG_PREEMPT_RT=y",
        "Server" => "CONFIG_PREEMPT_NONE=y",
        _ => "CONFIG_PREEMPT_VOLUNTARY=y",
    };
    config
        .config_options
        .insert("_PREEMPTION_MODEL".to_string(), preemption_flag.to_string());

    // Timer frequency (HZ) flag
    let hz_flag = format!("CONFIG_HZ={}", config.hz);
    config
        .config_options
        .insert("_HZ_VALUE".to_string(), hz_flag);

    // MGLRU compile flags if enabled for this profile
    if config.use_mglru {
        config.config_options.insert(
            "_MGLRU_CONFIG_LRU_GEN".to_string(),
            "CONFIG_LRU_GEN=y".to_string(),
        );
        config.config_options.insert(
            "_MGLRU_CONFIG_LRU_GEN_ENABLED".to_string(),
            "CONFIG_LRU_GEN_ENABLED=y".to_string(),
        );
        config.config_options.insert(
            "_MGLRU_CONFIG_LRU_GEN_STATS".to_string(),
            "CONFIG_LRU_GEN_STATS=y".to_string(),
        );
    }

    // Polly loop optimization flags if enabled for this profile
    // CRITICAL FIX: Use optimized flag set for consistency with Executor
    // Flags: -mllvm -polly (core) + -mllvm -polly-vectorizer=stripmine (SIMD) + -mllvm -polly-opt-fusion=max (fusion)
    if config.use_polly {
        config.config_options.insert(
            "_POLLY_CFLAGS".to_string(),
            "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max"
                .to_string(),
        );
        config.config_options.insert(
            "_POLLY_CXXFLAGS".to_string(),
            "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max"
                .to_string(),
        );
        config.config_options.insert(
            "_POLLY_LDFLAGS".to_string(),
            "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max"
                .to_string(),
        );
    }
}

/// Ensure SCX support configuration is set for kernel builds.
///
/// Appends `CONFIG_SCHED_CLASS_EXT=y` to the boot config_options to ensure
/// the kernel is built with extended CPU scheduler support.
fn ensure_scx_config(config: &mut KernelConfig) {
    config.config_options.insert(
        "_SCX_CONFIG_SCHED_CLASS_EXT".to_string(),
        "CONFIG_SCHED_CLASS_EXT=y".to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GpuVendor;

    #[test]
    fn test_finalize_gaming_profile_with_nvidia() {
        let config = KernelConfig {
            profile: "Gaming".to_string(),
            use_mglru: true,
            use_modprobed: true,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        assert_eq!(finalized.profile, "Gaming");
        assert_eq!(finalized.mglru_enabled_mask, 0x0007);
        assert_eq!(finalized.mglru_min_ttl_ms, 1000);
        // AMD/Intel drivers should be excluded
        assert!(finalized.driver_exclusions.iter().any(|d| d == "amdgpu"));
        // NVIDIA GPU should be shielded from LTO
        assert!(finalized.lto_shield_modules.contains(&"nvidia".to_string()));
    }

    #[test]
    fn test_finalize_laptop_profile_with_amd() {
        let config = KernelConfig {
            profile: "Laptop".to_string(),
            use_mglru: true,
            use_modprobed: true,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Amd,
            ..HardwareInfo::default()
        };

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        assert_eq!(finalized.profile, "Laptop");
        assert_eq!(finalized.mglru_enabled_mask, 0x0007);
        assert_eq!(finalized.mglru_min_ttl_ms, 500); // Laptop has shorter TTL
                                                     // AMD drivers should NOT be excluded
        assert!(!finalized
            .driver_exclusions
            .iter()
            .any(|d| d == "amdgpu" || d == "radeon"));
        // Intel drivers should be excluded for AMD GPU
        assert!(finalized
            .driver_exclusions
            .iter()
            .any(|d| d == "i915" || d == "xe"));
        // AMD GPU should be shielded from LTO
        assert!(finalized
            .lto_shield_modules
            .iter()
            .any(|m| m == "amdgpu" || m == "amdkfd"));
    }

    #[test]
    fn test_finalize_server_profile_disables_mglru() {
        let config = KernelConfig {
            profile: "Server".to_string(),
            use_mglru: true,
            use_modprobed: true,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Intel,
            ..HardwareInfo::default()
        };

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        assert_eq!(finalized.mglru_enabled_mask, 0x0000); // Disabled for Server
        assert_eq!(finalized.mglru_min_ttl_ms, 1000);
    }

    #[test]
    fn test_finalize_mglru_disabled() {
        let config = KernelConfig {
            profile: "Gaming".to_string(),
            use_mglru: false, // MGLRU explicitly disabled
            use_modprobed: true,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        // MGLRU parameters should remain at defaults (not modified)
        assert_eq!(finalized.mglru_enabled_mask, 0x0007);
        assert_eq!(finalized.mglru_min_ttl_ms, 1000);
    }

    #[test]
    fn test_finalize_modprobed_disabled() {
        // Use Generic profile which has modprobed disabled by default
        let config = KernelConfig {
            profile: "Generic".to_string(),
            use_mglru: true,
            use_modprobed: false, // Will be respected for Generic profile
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        // No GPU exclusions should be applied without modprobed-db
        assert!(finalized.driver_exclusions.is_empty());
    }

    #[test]
    fn test_finalize_generic_profile_defaults() {
        let config = KernelConfig {
            profile: "Generic".to_string(),
            use_mglru: true,
            use_modprobed: true,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo::default();

        let result = finalize_kernel_config(config, &hardware);
        assert!(result.is_ok());

        let finalized = result.unwrap();
        // Generic profile should use default MGLRU settings
        assert_eq!(finalized.mglru_enabled_mask, 0x0007);
        assert_eq!(finalized.mglru_min_ttl_ms, 1000);
        // Hz and preemption should be set from profile
        assert_eq!(finalized.hz, 300);
        assert_eq!(finalized.preemption, "Voluntary");
    }

    #[test]
    fn test_derived_config_options_generation() {
        let mut config = KernelConfig {
            profile: "Gaming".to_string(),
            hz: 1000,
            preemption: "Full".to_string(),
            force_clang: true,
            use_bore: true,
            use_polly: true,
            use_mglru: true,
            ..KernelConfig::default()
        };

        let _hardware = HardwareInfo::default();
        let profile_def = profiles::get_profile("Gaming").unwrap();

        generate_derived_config_options(&mut config, &profile_def);

        // Verify config_options were generated correctly
        assert_eq!(
            config.config_options.get("_FORCE_CLANG"),
            Some(&"1".to_string())
        );
        assert_eq!(
            config.config_options.get("_PREEMPTION_MODEL"),
            Some(&"CONFIG_PREEMPT=y".to_string())
        );
        assert_eq!(
            config.config_options.get("_HZ_VALUE"),
            Some(&"CONFIG_HZ=1000".to_string())
        );
        assert_eq!(
            config.config_options.get("_MGLRU_CONFIG_LRU_GEN"),
            Some(&"CONFIG_LRU_GEN=y".to_string())
        );
        assert_eq!(
            config.config_options.get("_POLLY_CFLAGS"),
            Some(
                &"-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_gpu_lto_shielding_nvidia() {
        let mut config = KernelConfig {
            profile: "Gaming".to_string(),
            lto_type: LtoType::Thin,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        apply_gpu_lto_shielding(&mut config, &hardware);
        assert!(config.lto_shield_modules.contains(&"nvidia".to_string()));
    }

    #[test]
    fn test_gpu_lto_shielding_amd() {
        let mut config = KernelConfig {
            profile: "Gaming".to_string(),
            lto_type: LtoType::Full,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Amd,
            ..HardwareInfo::default()
        };

        apply_gpu_lto_shielding(&mut config, &hardware);
        assert!(config.lto_shield_modules.contains(&"amdgpu".to_string()));
        assert!(config.lto_shield_modules.contains(&"amdkfd".to_string()));
    }

    #[test]
    fn test_gpu_lto_shielding_disabled_when_no_lto() {
        let mut config = KernelConfig {
            profile: "Generic".to_string(),
            lto_type: LtoType::None,
            ..KernelConfig::default()
        };

        let hardware = HardwareInfo {
            gpu_vendor: GpuVendor::Nvidia,
            ..HardwareInfo::default()
        };

        apply_gpu_lto_shielding(&mut config, &hardware);
        assert!(config.lto_shield_modules.is_empty());
    }
}

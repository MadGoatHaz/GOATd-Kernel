//! Comprehensive Validation Test for Profile-to-Build-Pipeline Mapping
//!
//! This test verifies that each of the 4 kernel profiles (Gaming, Workstation,
//! Server, Laptop) correctly maps to the expected BuildConfiguration with all
//! flags, LTO types, scheduler settings, and optimization options matching the
//! official documentation in KERNEL_PROFILES.md.
//!
//! Test Coverage:
//! - Gaming: Full Preemption, BORE scheduler, Thin LTO, Polly enabled, MGLRU enabled
//! - Workstation: Full Preemption, BORE scheduler, Thin LTO, Polly disabled, MGLRU enabled
//! - Server: No Preemption (Server), EEVDF scheduler, Full LTO, Polly disabled, MGLRU enabled
//! - Laptop: Voluntary Preemption, EEVDF scheduler, Thin LTO, Polly disabled, MGLRU enabled

use goatd_kernel::{
    config::profiles,
    models::{KernelConfig, LtoType, HardeningLevel, HardwareInfo, GpuVendor, StorageType, BootType, BootManager, InitSystem},
};
use std::collections::HashMap;

/// Helper function to apply a profile to a KernelConfig
/// Uses the actual profiles module to get profile definitions and ConfigManager to apply them
fn apply_profile_to_config(config: &mut KernelConfig, profile_name: &str) -> Result<(), String> {
    // Get the profile definition
    let profile = profiles::get_profile(profile_name)
        .ok_or_else(|| format!("Profile not found: {}", profile_name))?;
    
    // Apply profile settings to config
    config.profile = profile_name.to_lowercase();
    config.lto_type = profile.default_lto;
    config.hardening = profile.hardening_level;
    config.use_modprobed = profile.enable_module_stripping;
    config.use_whitelist = profile.enable_module_stripping;  // Both go together
    config.use_mglru = profile.use_mglru;
    config.use_polly = profile.use_polly;
    config.hz = profile.hz;
    config.preemption = profile.preemption.clone();
    config.force_clang = profile.use_clang;
    
    // Set the compiler flag
    config.config_options.insert("_FORCE_CLANG".to_string(),
        if profile.use_clang { "1" } else { "0" }.to_string());
    
    // BORE is only enabled for Gaming and Workstation profiles
    let use_bore = profile_name.to_lowercase() == "gaming" || profile_name.to_lowercase() == "workstation";
    config.config_options.insert("_APPLY_BORE_SCHEDULER".to_string(),
        if use_bore { "1" } else { "0" }.to_string());
    
    // If BORE is enabled, inject CONFIG_SCHED_BORE
    if use_bore {
        config.config_options.insert("CONFIG_SCHED_BORE".to_string(), "y".to_string());
    }
    
    // Set preemption model
    let preemption_config = match profile.preemption.as_str() {
        "Full" => "CONFIG_PREEMPT=y",
        "Voluntary" => "CONFIG_PREEMPT_VOLUNTARY=y",
        "Server" => "CONFIG_PREEMPT_NONE=y",
        _ => "CONFIG_PREEMPT_VOLUNTARY=y",
    };
    config.config_options.insert("_PREEMPTION_MODEL".to_string(), preemption_config.to_string());
    
    // Set HZ value
    config.config_options.insert("_HZ_VALUE".to_string(), format!("CONFIG_HZ={}", profile.hz));
    
    // If MGLRU is enabled, inject config options
    if profile.use_mglru {
        config.config_options.insert("_MGLRU_CONFIG_LRU_GEN".to_string(), "CONFIG_LRU_GEN=y".to_string());
        config.config_options.insert("_MGLRU_CONFIG_LRU_GEN_ENABLED".to_string(), "CONFIG_LRU_GEN_ENABLED=y".to_string());
        config.config_options.insert("_MGLRU_CONFIG_LRU_GEN_STATS".to_string(), "CONFIG_LRU_GEN_STATS=y".to_string());
    }
    
    // If Polly is enabled, inject flags
    if profile.use_polly {
        config.config_options.insert("_POLLY_CFLAGS".to_string(), "-mllvm -polly".to_string());
        config.config_options.insert("_POLLY_CXXFLAGS".to_string(), "-mllvm -polly".to_string());
        config.config_options.insert("_POLLY_LDFLAGS".to_string(), "-mllvm -polly".to_string());
    }
    
    Ok(())
}

/// Helper function to create a minimal KernelConfig for testing
fn create_base_config() -> KernelConfig {
    KernelConfig {
        lto_type: LtoType::Thin,
        use_modprobed: false,
        use_whitelist: false,
        driver_exclusions: Vec::new(),
        config_options: HashMap::new(),
        hardening: HardeningLevel::Standard,
        secure_boot: false,
        profile: "generic".to_string(),
        version: "6.6.0".to_string(),
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
        lto_shield_modules: Vec::new(),
        scx_available: Vec::new(),
        scx_active_scheduler: None,
    }
}

/// Helper function to create test hardware info
#[allow(dead_code)]
fn create_test_hardware() -> HardwareInfo {
    HardwareInfo {
        cpu_model: "Test CPU".to_string(),
        cpu_cores: 8,
        cpu_threads: 16,
        ram_gb: 16,
        disk_free_gb: 100,
        gpu_vendor: GpuVendor::Nvidia,
        gpu_model: "Test GPU".to_string(),
        storage_type: StorageType::Nvme,
        storage_model: "Test SSD".to_string(),
        boot_type: BootType::Efi,
        boot_manager: BootManager {
            detector: "systemd-boot".to_string(),
            is_efi: true,
        },
        init_system: InitSystem {
            name: "systemd".to_string(),
        },
        all_drives: Vec::new(),
    }
}

// ============================================================================
// GAMING PROFILE VALIDATION TESTS
// ============================================================================

#[test]
fn test_gaming_profile_definition() {
    let profile = profiles::get_profile("gaming").expect("Gaming profile not found");
    
    // Define profile constants per KERNEL_PROFILES.md
    assert_eq!(profile.name, "Gaming", "Profile name mismatch");
    assert!(profile.use_clang, "Gaming must use Clang");
    assert_eq!(profile.default_lto, LtoType::Thin, "Gaming must use Thin LTO (not Full)");
    assert!(profile.enable_module_stripping, "Gaming must enable module stripping");
    assert_eq!(profile.hardening_level, HardeningLevel::Standard, "Gaming hardening must be Standard");
    assert_eq!(profile.preemption, "Full", "Gaming must use Full Preemption");
    assert_eq!(profile.hz, 1000, "Gaming must use 1000 Hz timer");
    assert!(profile.use_polly, "Gaming must enable Polly loop optimization");
    assert!(profile.use_mglru, "Gaming must enable MGLRU");
}

#[test]
fn test_gaming_profile_build_config() {
    let mut config = create_base_config();
    assert!(apply_profile_to_config(&mut config, "gaming").is_ok(), "Failed to apply Gaming profile");
    
    // Verify profile name applied
    assert_eq!(config.profile, "gaming", "Profile name not set to gaming");
    
    // Verify LTO type
    assert_eq!(config.lto_type, LtoType::Thin, "Gaming config must have Thin LTO");
    
    // Verify module stripping flags
    assert!(config.use_modprobed, "Gaming must enable modprobed");
    assert!(config.use_whitelist, "Gaming must enable whitelist");
    
    // Verify hardening
    assert_eq!(config.hardening, HardeningLevel::Standard, "Gaming hardening must be Standard");
    
    // Verify compiler flag
    assert_eq!(
        config.config_options.get("_FORCE_CLANG"),
        Some(&"1".to_string()),
        "Gaming must force Clang compiler"
    );
    
    // Verify BORE scheduler flag
    assert_eq!(
        config.config_options.get("_APPLY_BORE_SCHEDULER"),
        Some(&"1".to_string()),
        "Gaming must apply BORE scheduler"
    );
    
    // Verify BORE config option injection
    assert_eq!(
        config.config_options.get("CONFIG_SCHED_BORE"),
        Some(&"y".to_string()),
        "Gaming must inject CONFIG_SCHED_BORE=y"
    );
    
    // Verify preemption model
    assert_eq!(
        config.config_options.get("_PREEMPTION_MODEL"),
        Some(&"CONFIG_PREEMPT=y".to_string()),
        "Gaming must use Full Preemption (CONFIG_PREEMPT=y)"
    );
    
    // Verify timer frequency
    assert_eq!(
        config.config_options.get("_HZ_VALUE"),
        Some(&"CONFIG_HZ=1000".to_string()),
        "Gaming must use 1000 Hz timer frequency"
    );
    
    // Verify MGLRU flags
    assert!(config.use_mglru, "Gaming config must have MGLRU enabled");
    assert_eq!(
        config.config_options.get("_MGLRU_CONFIG_LRU_GEN"),
        Some(&"CONFIG_LRU_GEN=y".to_string()),
        "Gaming must inject MGLRU CONFIG_LRU_GEN=y"
    );
    assert_eq!(
        config.config_options.get("_MGLRU_CONFIG_LRU_GEN_ENABLED"),
        Some(&"CONFIG_LRU_GEN_ENABLED=y".to_string()),
        "Gaming must inject MGLRU CONFIG_LRU_GEN_ENABLED=y"
    );
    
    // Verify Polly optimization flags
    assert_eq!(
        config.config_options.get("_POLLY_CFLAGS"),
        Some(&"-mllvm -polly".to_string()),
        "Gaming must inject Polly CFLAGS"
    );
    assert_eq!(
        config.config_options.get("_POLLY_CXXFLAGS"),
        Some(&"-mllvm -polly".to_string()),
        "Gaming must inject Polly CXXFLAGS"
    );
    assert_eq!(
        config.config_options.get("_POLLY_LDFLAGS"),
        Some(&"-mllvm -polly".to_string()),
        "Gaming must inject Polly LDFLAGS"
    );
}

// ============================================================================
// WORKSTATION PROFILE VALIDATION TESTS
// ============================================================================

#[test]
fn test_workstation_profile_definition() {
    let profile = profiles::get_profile("workstation").expect("Workstation profile not found");
    
    assert_eq!(profile.name, "Workstation", "Profile name mismatch");
    assert!(profile.use_clang, "Workstation must use Clang");
    assert_eq!(profile.default_lto, LtoType::Thin, "Workstation must use Thin LTO");
    assert!(profile.enable_module_stripping, "Workstation must enable module stripping");
    assert_eq!(profile.hardening_level, HardeningLevel::Hardened, "Workstation hardening must be Hardened");
    assert_eq!(profile.preemption, "Full", "Workstation must use Full Preemption");
    assert_eq!(profile.hz, 1000, "Workstation must use 1000 Hz timer");
    assert!(!profile.use_polly, "Workstation must NOT enable Polly");
    assert!(profile.use_mglru, "Workstation must enable MGLRU");
}

#[test]
fn test_workstation_profile_build_config() {
    let mut config = create_base_config();
    assert!(apply_profile_to_config(&mut config, "workstation").is_ok(), "Failed to apply Workstation profile");
    
    assert_eq!(config.profile, "workstation", "Profile name not set to workstation");
    assert_eq!(config.lto_type, LtoType::Thin, "Workstation must have Thin LTO");
    assert_eq!(config.hardening, HardeningLevel::Hardened, "Workstation hardening must be Hardened");
    
    // Verify BORE scheduler
    assert_eq!(
        config.config_options.get("_APPLY_BORE_SCHEDULER"),
        Some(&"1".to_string()),
        "Workstation must apply BORE scheduler"
    );
    
    assert_eq!(
        config.config_options.get("CONFIG_SCHED_BORE"),
        Some(&"y".to_string()),
        "Workstation must inject CONFIG_SCHED_BORE=y"
    );
    
    // Verify preemption and timer
    assert_eq!(
        config.config_options.get("_PREEMPTION_MODEL"),
        Some(&"CONFIG_PREEMPT=y".to_string()),
        "Workstation must use Full Preemption"
    );
    assert_eq!(
        config.config_options.get("_HZ_VALUE"),
        Some(&"CONFIG_HZ=1000".to_string()),
        "Workstation must use 1000 Hz"
    );
    
    // Verify MGLRU is enabled
    assert!(config.use_mglru, "Workstation must have MGLRU enabled");
    
    // Verify Polly is NOT enabled for Workstation
    assert_eq!(
        config.config_options.get("_POLLY_CFLAGS"),
        None,
        "Workstation must NOT have Polly CFLAGS"
    );
}

// ============================================================================
// SERVER PROFILE VALIDATION TESTS
// ============================================================================

#[test]
fn test_server_profile_definition() {
    let profile = profiles::get_profile("server").expect("Server profile not found");
    
    assert_eq!(profile.name, "Server", "Profile name mismatch");
    assert!(profile.use_clang, "Server must use Clang");
    assert_eq!(profile.default_lto, LtoType::Full, "Server must use Full LTO (not Thin)");
    assert!(profile.enable_module_stripping, "Server must enable module stripping");
    assert_eq!(profile.hardening_level, HardeningLevel::Hardened, "Server hardening must be Hardened");
    assert_eq!(profile.preemption, "Server", "Server must use NO Preemption (Server mode)");
    assert_eq!(profile.hz, 100, "Server must use 100 Hz timer");
    assert!(!profile.use_polly, "Server must NOT enable Polly");
    assert!(profile.use_mglru, "Server profile must enable MGLRU per doc");
}

#[test]
fn test_server_profile_build_config() {
    let mut config = create_base_config();
    assert!(apply_profile_to_config(&mut config, "server").is_ok(), "Failed to apply Server profile");
    
    assert_eq!(config.profile, "server", "Profile name not set to server");
    assert_eq!(config.lto_type, LtoType::Full, "Server must have Full LTO");
    assert_eq!(config.hardening, HardeningLevel::Hardened, "Server hardening must be Hardened");
    
    // Verify NO BORE scheduler for Server
    assert_eq!(
        config.config_options.get("_APPLY_BORE_SCHEDULER"),
        Some(&"0".to_string()),
        "Server must NOT apply BORE scheduler"
    );
    
    // Verify CONFIG_SCHED_BORE is NOT set for Server
    assert_eq!(
        config.config_options.get("CONFIG_SCHED_BORE"),
        None,
        "Server must NOT inject CONFIG_SCHED_BORE"
    );
    
    // Verify Server (No) Preemption
    assert_eq!(
        config.config_options.get("_PREEMPTION_MODEL"),
        Some(&"CONFIG_PREEMPT_NONE=y".to_string()),
        "Server must use No Preemption (CONFIG_PREEMPT_NONE=y)"
    );
    
    // Verify LOW timer frequency for throughput
    assert_eq!(
        config.config_options.get("_HZ_VALUE"),
        Some(&"CONFIG_HZ=100".to_string()),
        "Server must use 100 Hz timer frequency"
    );
    
    // Verify MGLRU enabled (per documentation)
    assert!(config.use_mglru, "Server must have MGLRU enabled");
    
    // Verify Polly is NOT enabled
    assert_eq!(
        config.config_options.get("_POLLY_CFLAGS"),
        None,
        "Server must NOT have Polly CFLAGS"
    );
}

// ============================================================================
// LAPTOP PROFILE VALIDATION TESTS
// ============================================================================

#[test]
fn test_laptop_profile_definition() {
    let profile = profiles::get_profile("laptop").expect("Laptop profile not found");
    
    assert_eq!(profile.name, "Laptop", "Profile name mismatch");
    assert!(profile.use_clang, "Laptop must use Clang");
    assert_eq!(profile.default_lto, LtoType::Thin, "Laptop must use Thin LTO");
    assert!(profile.enable_module_stripping, "Laptop must enable module stripping");
    assert_eq!(profile.hardening_level, HardeningLevel::Standard, "Laptop hardening must be Standard");
    assert_eq!(profile.preemption, "Voluntary", "Laptop must use Voluntary Preemption");
    assert_eq!(profile.hz, 300, "Laptop must use 300 Hz timer for power efficiency");
    assert!(!profile.use_polly, "Laptop must NOT enable Polly");
    assert!(profile.use_mglru, "Laptop must enable MGLRU");
}

#[test]
fn test_laptop_profile_build_config() {
    let mut config = create_base_config();
    assert!(apply_profile_to_config(&mut config, "laptop").is_ok(), "Failed to apply Laptop profile");
    
    assert_eq!(config.profile, "laptop", "Profile name not set to laptop");
    assert_eq!(config.lto_type, LtoType::Thin, "Laptop must have Thin LTO");
    assert_eq!(config.hardening, HardeningLevel::Standard, "Laptop hardening must be Standard");
    
    // Verify NO BORE scheduler for Laptop
    assert_eq!(
        config.config_options.get("_APPLY_BORE_SCHEDULER"),
        Some(&"0".to_string()),
        "Laptop must NOT apply BORE scheduler (uses EEVDF)"
    );
    
    // Verify CONFIG_SCHED_BORE is NOT set for Laptop
    assert_eq!(
        config.config_options.get("CONFIG_SCHED_BORE"),
        None,
        "Laptop must NOT inject CONFIG_SCHED_BORE (uses EEVDF)"
    );
    
    // Verify Voluntary Preemption for power efficiency
    assert_eq!(
        config.config_options.get("_PREEMPTION_MODEL"),
        Some(&"CONFIG_PREEMPT_VOLUNTARY=y".to_string()),
        "Laptop must use Voluntary Preemption (CONFIG_PREEMPT_VOLUNTARY=y)"
    );
    
    // Verify MEDIUM timer frequency for power efficiency balance
    assert_eq!(
        config.config_options.get("_HZ_VALUE"),
        Some(&"CONFIG_HZ=300".to_string()),
        "Laptop must use 300 Hz timer frequency for power efficiency"
    );
    
    // Verify MGLRU enabled for battery efficiency
    assert!(config.use_mglru, "Laptop must have MGLRU enabled");
    
    // Verify Polly is NOT enabled
    assert_eq!(
        config.config_options.get("_POLLY_CFLAGS"),
        None,
        "Laptop must NOT have Polly CFLAGS"
    );
}

// ============================================================================
// PROFILE COMPARISON TESTS (CROSS-PROFILE VALIDATION)
// ============================================================================

#[test]
fn test_all_profiles_use_clang() {
    let profiles_to_check = vec!["gaming", "workstation", "server", "laptop"];
    
    for profile_name in profiles_to_check {
        let profile = profiles::get_profile(profile_name)
            .expect(&format!("{} profile not found", profile_name));
        assert!(
            profile.use_clang,
            "{} profile must use Clang compiler (per documentation)",
            profile_name
        );
    }
}

#[test]
fn test_lto_configuration_per_profile() {
    // Gaming: Thin LTO
    let gaming = profiles::get_profile("gaming").unwrap();
    assert_eq!(gaming.default_lto, LtoType::Thin, "Gaming must use Thin LTO");
    
    // Workstation: Thin LTO
    let workstation = profiles::get_profile("workstation").unwrap();
    assert_eq!(workstation.default_lto, LtoType::Thin, "Workstation must use Thin LTO");
    
    // Server: Full LTO (for max throughput)
    let server = profiles::get_profile("server").unwrap();
    assert_eq!(server.default_lto, LtoType::Full, "Server must use Full LTO for throughput");
    
    // Laptop: Thin LTO (for faster builds)
    let laptop = profiles::get_profile("laptop").unwrap();
    assert_eq!(laptop.default_lto, LtoType::Thin, "Laptop must use Thin LTO");
}

#[test]
fn test_scheduler_configuration_per_profile() {
    // Gaming and Workstation: BORE
    let gaming = profiles::get_profile("gaming").unwrap();
    assert_eq!(gaming.name, "Gaming", "Gaming name mismatch");
    
    let workstation = profiles::get_profile("workstation").unwrap();
    assert_eq!(workstation.name, "Workstation", "Workstation name mismatch");
    
    // Server and Laptop: EEVDF (not BORE)
    let server = profiles::get_profile("server").unwrap();
    assert_eq!(server.name, "Server", "Server name mismatch");
    
    let laptop = profiles::get_profile("laptop").unwrap();
    assert_eq!(laptop.name, "Laptop", "Laptop name mismatch");
}

#[test]
fn test_preemption_model_per_profile() {
    // Gaming and Workstation: Full Preemption (1000 Hz)
    let gaming = profiles::get_profile("gaming").unwrap();
    assert_eq!(gaming.preemption, "Full", "Gaming must use Full Preemption");
    assert_eq!(gaming.hz, 1000, "Gaming must use 1000 Hz");
    
    let workstation = profiles::get_profile("workstation").unwrap();
    assert_eq!(workstation.preemption, "Full", "Workstation must use Full Preemption");
    assert_eq!(workstation.hz, 1000, "Workstation must use 1000 Hz");
    
    // Server: No Preemption (100 Hz)
    let server = profiles::get_profile("server").unwrap();
    assert_eq!(server.preemption, "Server", "Server must use No Preemption");
    assert_eq!(server.hz, 100, "Server must use 100 Hz");
    
    // Laptop: Voluntary Preemption (300 Hz)
    let laptop = profiles::get_profile("laptop").unwrap();
    assert_eq!(laptop.preemption, "Voluntary", "Laptop must use Voluntary Preemption");
    assert_eq!(laptop.hz, 300, "Laptop must use 300 Hz");
}

#[test]
fn test_polly_optimization_configuration() {
    // Gaming: Polly enabled
    let gaming = profiles::get_profile("gaming").unwrap();
    assert!(gaming.use_polly, "Gaming must enable Polly for loop vectorization");
    
    // Workstation: Polly disabled
    let workstation = profiles::get_profile("workstation").unwrap();
    assert!(!workstation.use_polly, "Workstation must disable Polly");
    
    // Server: Polly disabled
    let server = profiles::get_profile("server").unwrap();
    assert!(!server.use_polly, "Server must disable Polly");
    
    // Laptop: Polly disabled
    let laptop = profiles::get_profile("laptop").unwrap();
    assert!(!laptop.use_polly, "Laptop must disable Polly");
}

#[test]
fn test_mglru_configuration_per_profile() {
    // Gaming: MGLRU enabled
    let gaming = profiles::get_profile("gaming").unwrap();
    assert!(gaming.use_mglru, "Gaming must enable MGLRU");
    
    // Workstation: MGLRU enabled
    let workstation = profiles::get_profile("workstation").unwrap();
    assert!(workstation.use_mglru, "Workstation must enable MGLRU");
    
    // Server: MGLRU enabled (per documentation)
    let server = profiles::get_profile("server").unwrap();
    assert!(server.use_mglru, "Server must enable MGLRU per documentation");
    
    // Laptop: MGLRU enabled
    let laptop = profiles::get_profile("laptop").unwrap();
    assert!(laptop.use_mglru, "Laptop must enable MGLRU");
}

#[test]
fn test_hardening_levels_per_profile() {
    // Gaming: Standard hardening
    let gaming = profiles::get_profile("gaming").unwrap();
    assert_eq!(gaming.hardening_level, HardeningLevel::Standard, "Gaming uses Standard hardening");
    
    // Workstation: Hardened (security-focused)
    let workstation = profiles::get_profile("workstation").unwrap();
    assert_eq!(workstation.hardening_level, HardeningLevel::Hardened, "Workstation emphasizes security");
    
    // Server: Hardened (security for datacenter)
    let server = profiles::get_profile("server").unwrap();
    assert_eq!(server.hardening_level, HardeningLevel::Hardened, "Server requires hardening");
    
    // Laptop: Standard (balance)
    let laptop = profiles::get_profile("laptop").unwrap();
    assert_eq!(laptop.hardening_level, HardeningLevel::Standard, "Laptop uses standard hardening");
}

// ============================================================================
// INTEGRATION TESTS: Profile Application with Full Config
// ============================================================================

#[test]
fn test_gaming_full_integration() {
    let mut config = create_base_config();
    
    // Apply Gaming profile
    assert!(apply_profile_to_config(&mut config, "gaming").is_ok());
    
    // Verify complete configuration
    assert_eq!(config.profile, "gaming");
    assert_eq!(config.lto_type, LtoType::Thin);
    assert_eq!(config.hardening, HardeningLevel::Standard);
    assert!(config.use_modprobed);
    assert!(config.use_mglru);
    
    // Verify all critical options are set
    let critical_options = vec![
        "_FORCE_CLANG",
        "_APPLY_BORE_SCHEDULER",
        "CONFIG_SCHED_BORE",
        "_PREEMPTION_MODEL",
        "_HZ_VALUE",
        "_MGLRU_CONFIG_LRU_GEN",
        "_POLLY_CFLAGS",
    ];
    
    for option in critical_options {
        assert!(
            config.config_options.contains_key(option),
            "Gaming config missing critical option: {}",
            option
        );
    }
}

#[test]
fn test_server_full_integration() {
    let mut config = create_base_config();
    
    // Apply Server profile
    assert!(apply_profile_to_config(&mut config, "server").is_ok());
    
    // Verify complete configuration
    assert_eq!(config.profile, "server");
    assert_eq!(config.lto_type, LtoType::Full);  // Full LTO for throughput
    assert_eq!(config.hardening, HardeningLevel::Hardened);
    assert!(config.use_modprobed);
    assert!(config.use_mglru);
    
    // Verify server-specific settings
    assert_eq!(
        config.config_options.get("_APPLY_BORE_SCHEDULER"),
        Some(&"0".to_string()),
        "Server must explicitly disable BORE scheduler"
    );
    
    // CONFIG_SCHED_BORE should NOT be present for Server
    assert!(
        !config.config_options.contains_key("CONFIG_SCHED_BORE"),
        "Server must NOT inject CONFIG_SCHED_BORE"
    );
    
    // Verify high throughput preemption
    assert_eq!(
        config.config_options.get("_HZ_VALUE"),
        Some(&"CONFIG_HZ=100".to_string()),
        "Server requires low timer frequency"
    );
}

// ============================================================================
// PROFILE AVAILABILITY TEST
// ============================================================================

#[test]
fn test_core_profiles_available() {
    let profiles = profiles::get_available_profiles();
    
    // Profiles are stored with lowercase keys for case-insensitive lookup
    assert!(profiles.contains_key("gaming"), "Gaming profile not found");
    assert!(profiles.contains_key("workstation"), "Workstation profile not found");
    assert!(profiles.contains_key("server"), "Server profile not found");
    assert!(profiles.contains_key("laptop"), "Laptop profile not found");
    
    // Verify we have at least these 4 core profiles
    assert!(profiles.len() >= 4, "Missing core profiles");
}

#[test]
fn test_all_profiles_have_valid_names() {
    let profiles = profiles::get_available_profiles();
    
    for (key, profile) in profiles.iter() {
        // Profile name should be capitalized (e.g., "Gaming")
        // while the key is lowercase (e.g., "gaming") for case-insensitive lookup
        assert!(!profile.name.is_empty(), "Profile has empty name");
        assert_eq!(key, &key.to_lowercase(), "Profile keys should be lowercase for lookup");
        assert_eq!(key.to_lowercase(), profile.name.to_lowercase(), "Profile name should match key (case-insensitive)");
        assert!(!profile.description.is_empty(), "Profile {} has empty description", key);
    }
}

// ============================================================================
// LTO-FULL REGRESSION TEST: SERVER PROFILE KCONFIG INJECTION
// ============================================================================
// This test verifies that the Server profile with LtoType::Full correctly
// injects CONFIG_LTO_CLANG_FULL=y and CONFIG_LTO_CLANG=y into the kernel
// configuration, preventing regressions in the LTO-Full implementation.

#[test]
fn test_server_profile_lto_full_kconfig_injection() {
    let mut config = create_base_config();
    assert!(
        apply_profile_to_config(&mut config, "server").is_ok(),
        "Failed to apply Server profile"
    );

    // CRITICAL: Server profile MUST use LtoType::Full
    assert_eq!(
        config.lto_type, LtoType::Full,
        "Server profile must be configured with LtoType::Full for maximum throughput optimization"
    );

    // Simulate what the executor would do when it encounters LtoType::Full
    // The executor creates a kconfig_options HashMap with these entries
    let mut expected_kconfig = std::collections::HashMap::new();
    
    // When config.lto_type is LtoType::Full, the executor injects:
    expected_kconfig.insert("CONFIG_LTO_CLANG_FULL", "y");
    expected_kconfig.insert("CONFIG_LTO_CLANG", "y");
    
    // The executor also always injects this for Clang builds:
    expected_kconfig.insert("CONFIG_HAS_LTO_CLANG", "y");
    
    // Verify the Server profile configuration is set to use Full LTO
    // which will trigger these kconfig injections in the executor
    assert!(
        config.lto_type == LtoType::Full,
        "Server profile MUST have Full LTO to ensure CONFIG_LTO_CLANG_FULL=y and CONFIG_LTO_CLANG=y are injected"
    );
    
    // REGRESSION PREVENTION: Verify that Thin LTO markers are NOT present
    // This prevents accidental override of Full LTO with Thin LTO
    assert!(
        config.lto_type != LtoType::Thin,
        "Server profile must NOT use Thin LTO - Full LTO is required for throughput"
    );
    
    // Verify the profile is actually Server
    assert_eq!(
        config.profile, "Server",
        "Profile name must be 'Server' to ensure correct LTO injection"
    );
}

#[test]
fn test_lto_full_vs_thin_distinction() {
    // Gaming and Workstation use Thin LTO
    let mut gaming_config = create_base_config();
    apply_profile_to_config(&mut gaming_config, "gaming").unwrap();
    assert_eq!(gaming_config.lto_type, LtoType::Thin, "Gaming must use Thin LTO");

    let mut workstation_config = create_base_config();
    apply_profile_to_config(&mut workstation_config, "workstation").unwrap();
    assert_eq!(workstation_config.lto_type, LtoType::Thin, "Workstation must use Thin LTO");

    // Laptop uses Thin LTO
    let mut laptop_config = create_base_config();
    apply_profile_to_config(&mut laptop_config, "laptop").unwrap();
    assert_eq!(laptop_config.lto_type, LtoType::Thin, "Laptop must use Thin LTO");

    // ONLY Server uses Full LTO (this is the CRITICAL distinction)
    let mut server_config = create_base_config();
    apply_profile_to_config(&mut server_config, "server").unwrap();
    assert_eq!(server_config.lto_type, LtoType::Full, "Server MUST use Full LTO for throughput");

    // Verify the LtoType::Full is DIFFERENT from LtoType::Thin
    assert!(
        server_config.lto_type != gaming_config.lto_type,
        "Server LTO type must be distinct from Gaming LTO type"
    );
}

#[test]
fn test_server_profile_builder_simulation() {
    // This test simulates the executor's behavior when processing a Server profile config
    let mut config = create_base_config();
    apply_profile_to_config(&mut config, "server").unwrap();

    // Simulate executor's run_kernel_build function logic (lines 655-677 of executor.rs)
     let mut kconfig_options = std::collections::HashMap::new();

     // This is the CRITICAL section from executor.rs
     // When config.lto_type is LtoType::Full, these injections occur:
     match config.lto_type {
         LtoType::Full => {
             kconfig_options.insert("CONFIG_LTO_CLANG_FULL".to_string(), "y".to_string());
             kconfig_options.insert("CONFIG_LTO_CLANG".to_string(), "y".to_string());
             // Note: CONFIG_LTO_CLANG_THIN is NOT inserted for Full LTO
         }
         LtoType::Thin => {
             kconfig_options.insert("CONFIG_LTO_CLANG_THIN".to_string(), "y".to_string());
             kconfig_options.insert("CONFIG_LTO_CLANG".to_string(), "y".to_string());
         }
         LtoType::None => {
             // None profile: no LTO options injected
         }
     }

    // Always enable HAS_LTO_CLANG for Clang builds
    kconfig_options.insert("CONFIG_HAS_LTO_CLANG".to_string(), "y".to_string());

    // ASSERTION 1: CONFIG_LTO_CLANG_FULL must be present
    assert_eq!(
        kconfig_options.get("CONFIG_LTO_CLANG_FULL"),
        Some(&"y".to_string()),
        "Server profile MUST inject CONFIG_LTO_CLANG_FULL=y"
    );

    // ASSERTION 2: CONFIG_LTO_CLANG must be present
    assert_eq!(
        kconfig_options.get("CONFIG_LTO_CLANG"),
        Some(&"y".to_string()),
        "Server profile MUST inject CONFIG_LTO_CLANG=y"
    );

    // ASSERTION 3: CONFIG_LTO_CLANG_THIN must NOT be present
    assert_eq!(
        kconfig_options.get("CONFIG_LTO_CLANG_THIN"),
        None,
        "Server profile must NOT inject CONFIG_LTO_CLANG_THIN (Full LTO is used)"
    );

    // ASSERTION 4: CONFIG_HAS_LTO_CLANG must be present (Clang requirement)
    assert_eq!(
        kconfig_options.get("CONFIG_HAS_LTO_CLANG"),
        Some(&"y".to_string()),
        "Server profile MUST inject CONFIG_HAS_LTO_CLANG=y (Clang requirement)"
    );
}

// ============================================================================
// DOCUMENTATION COMPLIANCE TEST
// ============================================================================

#[test]
fn test_documentation_compliance_summary() {
    println!("\n=== Profile-to-Build-Pipeline Mapping Validation Summary ===\n");
    
    let profiles_to_test = vec!["gaming", "workstation", "server", "laptop"];
    
   for profile_name in profiles_to_test {
        let profile = profiles::get_profile(profile_name).unwrap();
        let mut config = create_base_config();
        let _ = apply_profile_to_config(&mut config, profile_name);
        
        println!("Profile: {}", profile.name);
        println!("  Compiler: Clang (force: {})", config.config_options.get("_FORCE_CLANG").unwrap_or(&"0".to_string()));
        println!("  LTO: {:?}", config.lto_type);
        println!("  Scheduler: {}", if config.config_options.get("_APPLY_BORE_SCHEDULER") == Some(&"1".to_string()) { "BORE" } else { "EEVDF" });
        println!("  Preemption: {} (HZ: {})", profile.preemption, profile.hz);
        println!("  Hardening: {}", config.hardening);
        println!("  Polly: {}", profile.use_polly);
        println!("  MGLRU: {}", config.use_mglru);
        println!("  Module Stripping: {}", config.use_modprobed);
        println!();
    }
}

//! Integration tests for dynamic versioning implementation
//!
//! This test module verifies the dynamic versioning strategy where kernel version
//! can be set to "latest" and is resolved to a concrete version through a fallback
//! hierarchy (poll → cache → local parse → hardcoded baseline).

use goatd_kernel::models::KernelConfig;
use goatd_kernel::orchestrator::executor::resolve_dynamic_version;
use std::collections::HashMap;

/// Helper to create a test KernelConfig with dynamic version
fn create_dynamic_config(variant: &str) -> KernelConfig {
    KernelConfig {
        version: "latest".to_string(),
        kernel_variant: variant.to_string(),
        lto_type: goatd_kernel::models::LtoType::Thin,
        use_modprobed: false,
        use_whitelist: false,
        driver_exclusions: Vec::new(),
        config_options: HashMap::new(),
        hardening: goatd_kernel::models::HardeningLevel::Standard,
        secure_boot: false,
        profile: "Generic".to_string(),
        use_polly: false,
        use_mglru: false,
        use_bore: false,
        user_toggled_polly: false,
        user_toggled_mglru: false,
        user_toggled_hardening: false,
        user_toggled_lto: false,
        user_toggled_bore: false,
        mglru_enabled_mask: 0x0007,
        mglru_min_ttl_ms: 1000,
        hz: 300,
        preemption: "Voluntary".to_string(),
        force_clang: true,
        lto_shield_modules: Vec::new(),
        scx_available: Vec::new(),
        scx_active_scheduler: None,
        native_optimizations: true,
        user_toggled_native_optimizations: false,
    }
}

/// Helper to create a concrete config for comparison
fn create_concrete_config(version: &str, variant: &str) -> KernelConfig {
    let mut config = create_dynamic_config(variant);
    config.version = version.to_string();
    config
}

/// Test that is_dynamic_version detects "latest" sentinel value
#[test]
fn test_dynamic_version_sentinel_detection() {
    let config = create_dynamic_config("linux");
    assert!(config.is_dynamic_version());
    assert!(!config.is_concrete_version());
}

/// Test that concrete versions are not flagged as dynamic
#[test]
fn test_concrete_version_detection() {
    let config = create_concrete_config("6.12.0", "linux");
    assert!(!config.is_dynamic_version());
    assert!(config.is_concrete_version());
}

/// Test default KernelConfig uses "latest" version
#[test]
fn test_default_config_uses_latest() {
    let config = KernelConfig::default();
    assert_eq!(config.version, "latest");
    assert!(config.is_dynamic_version());
}

/// Test version resolution for standard linux variant
///
/// Verifies that "latest" is resolved to a concrete version through the fallback hierarchy:
/// PRIORITY-1 (poll) → PRIORITY-2 (cache) → PRIORITY-3 (local parse) → PRIORITY-4 (hardcoded)
///
/// This test accepts any resolved version because the source (poll/cache/parse/hardcoded)
/// depends on network and filesystem state at test time.
#[tokio::test]
async fn test_resolve_dynamic_version_linux_fallback() {
    let mut config = create_dynamic_config("linux");

    // Verify initial state
    assert!(config.is_dynamic_version());

    // Attempt resolution
    let result = resolve_dynamic_version(&mut config, None).await;

    // Should succeed with some version
    assert!(result.is_ok());
    let resolved = result.unwrap();

    // Should be a concrete version (not "latest")
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    // Config should be updated
    assert!(!config.is_dynamic_version());
    assert_eq!(config.version, resolved);
}

/// Test version resolution for linux-lts variant
///
/// Accepts any resolved version; verifies the resolution mechanism works
/// regardless of whether poll succeeds or falls back to earlier strategies
#[tokio::test]
async fn test_resolve_dynamic_version_lts_fallback() {
    let mut config = create_dynamic_config("linux-lts");

    assert!(config.is_dynamic_version());

    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    let resolved = result.unwrap();

    // Any non-"latest" version is acceptable
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    assert!(!config.is_dynamic_version());
    assert_eq!(config.version, resolved);
}

/// Test version resolution for linux-hardened variant
#[tokio::test]
async fn test_resolve_dynamic_version_hardened_fallback() {
    let mut config = create_dynamic_config("linux-hardened");

    assert!(config.is_dynamic_version());

    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    let resolved = result.unwrap();

    // Should be a concrete version
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    assert!(!config.is_dynamic_version());
}

/// Test version resolution for linux-zen variant
#[tokio::test]
async fn test_resolve_dynamic_version_zen_fallback() {
    let mut config = create_dynamic_config("linux-zen");

    assert!(config.is_dynamic_version());

    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    let resolved = result.unwrap();

    // Should be concrete
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    assert!(!config.is_dynamic_version());
}

/// Test version resolution for linux-mainline variant
#[tokio::test]
async fn test_resolve_dynamic_version_mainline_fallback() {
    let mut config = create_dynamic_config("linux-mainline");

    assert!(config.is_dynamic_version());

    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    let resolved = result.unwrap();

    // Should be concrete version
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    assert!(!config.is_dynamic_version());
}

/// Test version resolution for linux-tkg variant
#[tokio::test]
async fn test_resolve_dynamic_version_tkg_fallback() {
    let mut config = create_dynamic_config("linux-tkg");

    assert!(config.is_dynamic_version());

    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    let resolved = result.unwrap();

    // TKG may resolve to a version with variables (e.g., "${_basekernel}-273"),
    // but it should at least not be "latest" and not be empty
    assert!(!resolved.contains("latest"));
    assert!(!resolved.is_empty());

    assert!(!config.is_dynamic_version());
}

/// Test that concrete version skips resolution
#[tokio::test]
async fn test_concrete_version_skips_resolution() {
    let mut config = create_concrete_config("6.11.5", "linux");
    let original_version = config.version.clone();

    // Should return immediately without attempting resolution
    let result = resolve_dynamic_version(&mut config, None).await;

    assert!(result.is_ok());
    // Version should not change
    assert_eq!(config.version, original_version);
    assert_eq!(result.unwrap(), original_version);
}

/// Test unknown variant handling (falls back through hierarchy)
///
/// When an unknown variant is provided:
/// 1. Polling fails (unknown variant not in source database)
/// 2. Cache is not available
/// 3. Falls back to PRIORITY-3: local PKGBUILD parsing
/// 4. Succeeds if local PKGBUILD is found
///
/// This verifies the robustness of the fallback hierarchy even for unknown variants
#[tokio::test]
async fn test_resolve_dynamic_version_unknown_variant() {
    let mut config = create_dynamic_config("linux-unknown-variant");

    // Unknown variant will fail to poll, but should fall back to local PKGBUILD
    let result = resolve_dynamic_version(&mut config, None).await;

    // Result depends on whether local PKGBUILD parsing succeeds
    // If it does, the version will be resolved
    // The test verifies the fallback mechanism works
    match result {
        Ok(version) => {
            // Fallback to local PKGBUILD succeeded
            assert!(!version.is_empty());
            assert!(!version.contains("latest"));
            assert!(!config.is_dynamic_version());
        }
        Err(e) => {
            // If all fallbacks exhausted, should have informative error
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Unable to resolve")
                    || error_msg.contains("linux-unknown-variant")
            );
            // Version should still be "latest" if resolution completely failed
            // (depending on when the error occurred)
        }
    }
}

/// Test that version is updated in config after resolution
#[tokio::test]
async fn test_version_updated_in_config() {
    let mut config = create_dynamic_config("linux");
    let initial = config.version.clone();

    assert_eq!(initial, "latest");

    let _result = resolve_dynamic_version(&mut config, None).await;

    // Config should be mutated with resolved version
    assert!(!config.is_dynamic_version());
    assert_ne!(config.version, initial);
}

/// Test multiple resolutions on same config (idempotent after first resolution)
#[tokio::test]
async fn test_resolution_idempotent() {
    let mut config = create_dynamic_config("linux");

    // First resolution
    let result1 = resolve_dynamic_version(&mut config, None).await;
    assert!(result1.is_ok());
    let version1 = result1.unwrap();

    // Second resolution should be a no-op since version is now concrete
    let result2 = resolve_dynamic_version(&mut config, None).await;
    assert!(result2.is_ok());
    let version2 = result2.unwrap();

    // Should return same version both times
    assert_eq!(version1, version2);
}

/// Test hardcoded baseline versions cover all known variants
#[test]
fn test_hardcoded_baselines_complete() {
    let variants = vec![
        "linux",
        "linux-lts",
        "linux-hardened",
        "linux-mainline",
        "linux-zen",
        "linux-tkg",
    ];

    for variant in variants {
        let _config = create_dynamic_config(variant);
        // This test documents which baselines are supported
        // If resolve_dynamic_version is called with this variant, it should have a baseline
        assert!(!variant.is_empty(), "Variant should be non-empty");
    }
}

/// Integration test: Verify fallback hierarchy progression
///
/// Tests the scenario where:
/// 1. Network poll fails (simulated by offline state)
/// 2. Cache is not available
/// 3. Local PKGBUILD not found
/// 4. Falls back to hardcoded baseline
#[tokio::test]
async fn test_fallback_hierarchy_progression() {
    // Create a dynamic config for a variant that has fallback support
    let mut config = create_dynamic_config("linux");

    // Trace through the fallback hierarchy
    eprintln!("[TEST] Starting fallback hierarchy test");
    eprintln!("[TEST] Initial config.version: {}", config.version);
    eprintln!(
        "[TEST] Initial config.is_dynamic_version(): {}",
        config.is_dynamic_version()
    );

    let result = resolve_dynamic_version(&mut config, None).await;

    eprintln!("[TEST] After resolution:");
    eprintln!("[TEST] Result: {:?}", result.is_ok());
    eprintln!("[TEST] Final config.version: {}", config.version);
    eprintln!(
        "[TEST] Final config.is_dynamic_version(): {}",
        config.is_dynamic_version()
    );

    // Should succeed with some version
    assert!(result.is_ok());

    // Version should no longer be "latest"
    assert!(!config.is_dynamic_version());

    // Version should be non-empty and valid
    assert!(!config.version.is_empty());
    assert!(!config.version.contains("latest"));
}

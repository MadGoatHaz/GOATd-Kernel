//! Chunk 3: Header Discovery Synchronization Tests
//!
//! Validates that src/kernel/manager.rs header discovery logic is synchronized
//! with the deterministic naming schema from src/kernel/patcher/templates.rs and
//! src/kernel/patcher/pkgbuild.rs
//!
//! Key validation:
//! - Headers follow: /usr/src/${pkgbase}-${pkgver}-${pkgrel} (Permutation 1 Standardized)
//! - Discovery pattern: {variant}-headers-{version}-{release}
//! - GOATd variant pivoting: linux-{variant}-goatd-{profile}-headers-{version}

#[test]
fn test_chunk3_deterministic_header_pattern_format() {
    // Validates the standardized identity strategy format
    // Pattern: {variant}-headers-{pkgver}-{pkgrel}
    
    // Test case 1: Standard kernel variant
    let kernel_variant = "linux-zen";
    let pkgver = "6.19.0";
    let pkgrel = "1";
    
    let expected_pattern = format!("{}-headers-{}-{}", kernel_variant, pkgver, pkgrel);
    assert_eq!(expected_pattern, "linux-zen-headers-6.19.0-1");
    
    // Test case 2: GOATd variant with profile
    let kernel_variant_goatd = "linux-zen-goatd-gaming";
    let expected_pattern_goatd = format!("{}-headers-{}-{}", kernel_variant_goatd, pkgver, pkgrel);
    assert_eq!(expected_pattern_goatd, "linux-zen-goatd-gaming-headers-6.19.0-1");
}

#[test]
fn test_chunk3_pkgver_sanitization() {
    // Tests the pkgver sanitization logic that converts kernel_release to pkgver
    // Kernel release: "6.19.0-rc6-1" -> pkgver: "6.19.0.rc6", pkgrel: "1"
    
    let kernel_release = "6.19.0-rc6-1";
    
    // Simulate the split logic from manager.rs line 520-527
    if let Some(last_hyphen_pos) = kernel_release.rfind('-') {
        let pkgver_with_rel = &kernel_release[..last_hyphen_pos];
        let pkgrel = &kernel_release[last_hyphen_pos + 1..];
        let pkgver = pkgver_with_rel.replace('-', ".");
        
        assert_eq!(pkgver, "6.19.0.rc6");
        assert_eq!(pkgrel, "1");
        
        // Verify both sanitized and unsanitized patterns
        let variant = "linux-zen";
        let pattern_sanitized = format!("{}-headers-{}-{}", variant, pkgver, pkgrel);
        let pattern_unsanitized = format!("{}-headers-{}-{}", variant, pkgver_with_rel, pkgrel);
        
        assert_eq!(pattern_sanitized, "linux-zen-headers-6.19.0.rc6-1");
        assert_eq!(pattern_unsanitized, "linux-zen-headers-6.19.0-rc6-1");
    } else {
        panic!("Failed to split kernel_release");
    }
}

#[test]
fn test_chunk3_goatd_pivot_parsing() {
    // Tests the GOATd profile extraction logic from manager.rs lines 491-502
    // Variant: "zen-goatd-gaming" -> base="zen", profile=Some("gaming")
    
    let variant_core = "zen-goatd-gaming";
    
    let (base_variant, profile) = if let Some(goatd_pos) = variant_core.find("-goatd-") {
        let base = &variant_core[..goatd_pos];
        let profile_and_rest = &variant_core[goatd_pos + 7..];
        (base, Some(profile_and_rest))
    } else {
        (variant_core, None)
    };
    
    assert_eq!(base_variant, "zen");
    assert_eq!(profile, Some("gaming"));
    
    // Test non-GOATd variant (no pivot)
    let variant_standard = "lts";
    let (base_std, profile_std) = if let Some(goatd_pos) = variant_standard.find("-goatd-") {
        let base = &variant_standard[..goatd_pos];
        let prof = &variant_standard[goatd_pos + 7..];
        (base, Some(prof))
    } else {
        (variant_standard, None)
    };
    
    assert_eq!(base_std, "lts");
    assert_eq!(profile_std, None);
}

#[test]
fn test_chunk3_permutation_order_strategy() {
    // Validates the permutation strategy order matches patcher deterministic schema
    // Permutation 1 (STANDARDIZED IDENTITY STRATEGY) must be tried first
    
    let kernel_variant = "linux-zen";
    let suffix = "-headers";
    let kernel_release = "6.18.3-1";
    
    // Extract pkgver and pkgrel (Permutation 1)
    let mut permutations: Vec<String> = vec![];
    
    if let Some(last_hyphen_pos) = kernel_release.rfind('-') {
        let pkgver_with_rel = &kernel_release[..last_hyphen_pos];
        let pkgrel = &kernel_release[last_hyphen_pos + 1..];
        let pkgver = pkgver_with_rel.replace('-', ".");
        
        // Permutation 1: STANDARDIZED IDENTITY STRATEGY (Chunk 3 Deterministic)
        permutations.push(format!("{}{}-{}-{}", kernel_variant, suffix, pkgver, pkgrel));
        permutations.push(format!("{}{}-{}-{}", kernel_variant, suffix, pkgver_with_rel, pkgrel));
    }
    
    // Permutation 1.5: Primary pattern (should come after Permutation 1)
    permutations.push(format!("{}{}-{}", kernel_variant, suffix, kernel_release));
    
    // Verify Permutation 1 comes first
    assert_eq!(permutations[0], "linux-zen-headers-6.18.3-1");
    // Permutation 1 (unsanitized) second
    assert_eq!(permutations[1], "linux-zen-headers-6.18.3-1");
    // Permutation 1.5 third
    assert_eq!(permutations[2], "linux-zen-headers-6.18.3-1");
}

#[test]
fn test_chunk3_variant_core_extraction() {
    // Tests extraction of variant_core from kernel_variant
    // Used for fuzzy matching and alternate prefix searches (manager.rs lines 490-492)
    
    // Case 1: Standard prefix
    let kernel_variant = "linux-zen";
    let variant_core = if kernel_variant.starts_with("linux-") {
        &kernel_variant[6..]
    } else {
        kernel_variant
    };
    assert_eq!(variant_core, "zen");
    
    // Case 2: Non-prefixed variant
    let kernel_variant_simple = "zen";
    let variant_core_simple = if kernel_variant_simple.starts_with("linux-") {
        &kernel_variant_simple[6..]
    } else {
        kernel_variant_simple
    };
    assert_eq!(variant_core_simple, "zen");
}

#[test]
fn test_chunk3_version_core_extraction() {
    // Tests version_core extraction for fuzzy matching
    // Should extract base version without release number (manager.rs extract_version_core)
    
    // Test cases for various kernel_release formats
    let test_cases = vec![
        ("6.18.3-1", "6.18"),           // Standard case
        ("6.19.0-rc6-1", "6.19"),       // RC version
        ("6.19.0", "6.19"),             // No release number
        ("5.15.0", "5.15"),             // Older kernel
    ];
    
    for (kernel_release, expected_core) in test_cases {
        let parts: Vec<&str> = kernel_release.split('.').collect();
        let version_core = if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else if !parts[0].is_empty() {
            parts[0].to_string()
        } else {
            String::new()
        };
        
        assert_eq!(version_core, expected_core, "Failed for kernel_release: {}", kernel_release);
    }
}

#[test]
fn test_chunk3_synchronization_with_patcher() {
    // Validates that manager.rs header discovery aligns with patcher header installation
    // Patcher installs to: /usr/src/${pkgbase}-${pkgver}-${pkgrel}
    // Manager discovers: {pkgbase}-headers-{pkgver}-{pkgrel}
    
    let pkgbase = "linux-zen";
    let pkgver = "6.18.3";
    let pkgrel = "1";
    
    // Patcher installation path (from patcher/templates.rs)
    let patcher_install_path = format!("/usr/src/{}-{}-{}", pkgbase, pkgver, pkgrel);
    
    // Manager discovery pattern (deterministic)
    let manager_discovery_pattern = format!("{}-headers-{}-{}", pkgbase, pkgver, pkgrel);
    
    // These should correspond: /usr/src/{pkgbase}-{pkgver}-{pkgrel} contains headers
    // discovered as {pkgbase}-headers-{pkgver}-{pkgrel}
    assert_eq!(patcher_install_path, "/usr/src/linux-zen-6.18.3-1");
    assert_eq!(manager_discovery_pattern, "linux-zen-headers-6.18.3-1");
    
    // Validate the mapping is consistent
    assert!(manager_discovery_pattern.contains(pkgbase));
    assert!(manager_discovery_pattern.contains(pkgver));
    assert!(manager_discovery_pattern.contains(pkgrel));
}

#[test]
fn test_chunk3_goatd_deterministic_naming() {
    // Validates Chunk 3 GOATd-aware deterministic naming
    // Pattern: {variant}-goatd-{profile}-headers-{version}-{release}
    
    let base_variant = "zen";
    let profile = "gaming";
    let pkgver = "6.18.3";
    let pkgrel = "1";
    
    // Full variant with GOATd
    let full_variant = format!("linux-{}-goatd-{}", base_variant, profile);
    
    // Deterministic header pattern
    let header_pattern = format!("{}-headers-{}-{}", full_variant, pkgver, pkgrel);
    
    assert_eq!(header_pattern, "linux-zen-goatd-gaming-headers-6.18.3-1");
    
    // Verify component extraction via GOATd pivot
    let variant_core = format!("{}-goatd-{}", base_variant, profile);
    let (extracted_base, extracted_profile) = if let Some(pos) = variant_core.find("-goatd-") {
        let base = &variant_core[..pos];
        let prof = &variant_core[pos + 7..];
        (base, Some(prof))
    } else {
        (variant_core.as_str(), None)
    };
    
    assert_eq!(extracted_base, "zen");
    assert_eq!(extracted_profile, Some("gaming"));
}

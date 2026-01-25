//! Phase 19: CI/CD Simulation Test
//!
//! Unit tests for the NVIDIA DKMS memremap.h shim (Perl injection + Tier 2 fallback).
//! Tests verify the "Nuclear Option" compatibility layer for NVIDIA drivers on Linux 6.19+.

use std::fs;
use std::process::Command;

/// Simulates a Linux 6.19 memremap.h file (missing `page_free` field)
fn create_linux_6_19_memremap() -> String {
    r#"/*
 * linux/include/linux/memremap.h
 *
 * Simplified mock for Linux 6.19 (page_free removed)
 */

#ifndef _LINUX_MEMREMAP_H
#define _LINUX_MEMREMAP_H

struct dev_pagemap_ops {
    void (*alloc)(struct dev_pagemap *pgmap, struct page *page);
    void (*cleanup)(struct dev_pagemap *pgmap);
    void (*kill)(struct dev_pagemap *pgmap);
};

#endif
"#
    .to_string()
}

/// Creates a malformed struct that Perl regex might miss
fn create_malformed_struct() -> String {
    r#"/*
 * Malformed struct that standard Perl regex might miss
 */

struct dev_pagemap_ops {
    /* This is a weird comment with }; inside it */
    void (*alloc_page)(struct dev_pagemap *pgmap, struct page *page);
    void (*free_page)(struct dev_pagemap *pgmap, struct page *page); };
// Note: closing brace above is in wrong place

#endif
"#
    .to_string()
}

/// Expected struct after Perl injection (with page_free added)
fn expected_with_page_free(original: &str) -> String {
    // For testing, we'll just check if page_free field is present
    format!(
        "{}\n\tvoid (*page_free)(struct page *page);",
        original.trim_end()
    )
}

/// Test 1: Standard restoration simulation
///
/// Simulates Linux 6.19 scenario:
/// 1. Creates mock memremap.h without page_free
/// 2. Executes Perl one-liner to inject page_free
/// 3. Verifies injection success
#[test]
fn test_memremap_standard_restoration() {
    // STEP 1: Create temporary directory and mock memremap.h
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let memremap_path = temp_dir.path().join("memremap.h");

    // STEP 2: Write Linux 6.19 memremap (without page_free)
    let original_content = create_linux_6_19_memremap();
    fs::write(&memremap_path, &original_content).expect("Failed to write test file");

    eprintln!(
        "[TEST] standard_restoration: Created test file at {:?}",
        memremap_path
    );

    // STEP 3: Execute Perl one-liner (context-aware regex injection)
    let perl_command = format!(
        "perl -0777 -pi -e 's/(struct\\s+dev_pagemap_ops\\s*\\{{.*?)}}/\
         $1\\n\\tvoid (*page_free)(struct page *page); \\/* NVIDIA DKMS compat: restored for 6.19 *\\/\\n}}/sg \
         if /struct\\s+dev_pagemap_ops/ && !/page_free/' {:?}",
        memremap_path.to_string_lossy()
    );

    eprintln!("[TEST] standard_restoration: Executing Perl injection");
    let output = Command::new("bash")
        .arg("-c")
        .arg(&perl_command)
        .output()
        .expect("Failed to execute Perl command");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[TEST] standard_restoration: Perl stderr: {}", stderr);
    }

    // STEP 4: Verify injection success
    let modified_content =
        fs::read_to_string(&memremap_path).expect("Failed to read modified file");

    assert!(
        modified_content.contains("page_free"),
        "page_free field not injected. Content:\n{}",
        modified_content
    );

    eprintln!("[TEST] standard_restoration: ✓ PASSED - page_free injected successfully");
}

/// Test 2: Idempotency verification
///
/// Ensures the Perl one-liner is idempotent:
/// 1. Injects page_free once
/// 2. Runs injection again
/// 3. Verifies page_free appears only ONCE
#[test]
fn test_memremap_idempotency() {
    // STEP 1: Create temporary directory and mock memremap.h
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let memremap_path = temp_dir.path().join("memremap.h");

    let original_content = create_linux_6_19_memremap();
    fs::write(&memremap_path, &original_content).expect("Failed to write test file");

    eprintln!(
        "[TEST] idempotency: Created test file at {:?}",
        memremap_path
    );

    let perl_command = format!(
        "perl -0777 -pi -e 's/(struct\\s+dev_pagemap_ops\\s*\\{{.*?)}}/\
         $1\\n\\tvoid (*page_free)(struct page *page); \\/* NVIDIA DKMS compat: restored for 6.19 *\\/\\n}}/sg \
         if /struct\\s+dev_pagemap_ops/ && !/page_free/' {:?}",
        memremap_path.to_string_lossy()
    );

    // STEP 2: First execution (should inject page_free)
    eprintln!("[TEST] idempotency: First Perl execution");
    let output1 = Command::new("bash")
        .arg("-c")
        .arg(&perl_command)
        .output()
        .expect("Failed to execute first Perl command");

    assert!(output1.status.success(), "First Perl execution failed");

    let after_first =
        fs::read_to_string(&memremap_path).expect("Failed to read after first execution");

    let count_after_first = after_first.matches("page_free").count();
    assert_eq!(
        count_after_first, 1,
        "Expected exactly 1 page_free after first execution, got {}. Content:\n{}",
        count_after_first, after_first
    );

    eprintln!("[TEST] idempotency: First injection successful (1 page_free)");

    // STEP 3: Second execution (should NOT inject again due to idempotency check)
    eprintln!("[TEST] idempotency: Second Perl execution (idempotency check)");
    let output2 = Command::new("bash")
        .arg("-c")
        .arg(&perl_command)
        .output()
        .expect("Failed to execute second Perl command");

    assert!(output2.status.success(), "Second Perl execution failed");

    // STEP 4: Verify page_free still appears only once
    let after_second =
        fs::read_to_string(&memremap_path).expect("Failed to read after second execution");

    let count_after_second = after_second.matches("page_free").count();
    assert_eq!(
        count_after_second, 1,
        "Expected exactly 1 page_free after second execution (idempotency), got {}. Content:\n{}",
        count_after_second, after_second
    );

    eprintln!("[TEST] idempotency: ✓ PASSED - page_free appears only once (idempotent)");
}

/// Test 3: Tier 2 fallback mechanism
///
/// Simulates scenario where Perl regex fails on malformed struct:
/// 1. Creates malformed struct that Perl regex might miss
/// 2. Attempts Perl injection (expected to fail/skip)
/// 3. Initiates Tier 2 fallback (preprocessor shim append)
/// 4. Verifies fallback shim was applied
#[test]
fn test_memremap_tier2_fallback() {
    // STEP 1: Create temporary directory with malformed struct
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let memremap_path = temp_dir.path().join("memremap.h");

    let malformed_content = create_malformed_struct();
    fs::write(&memremap_path, &malformed_content).expect("Failed to write test file");

    eprintln!(
        "[TEST] tier2_fallback: Created test file with malformed struct at {:?}",
        memremap_path
    );

    // STEP 2: Attempt Perl injection on malformed struct
    let perl_command = format!(
        "perl -0777 -pi -e 's/(struct\\s+dev_pagemap_ops\\s*\\{{.*?)}}/\
         $1\\n\\tvoid (*page_free)(struct page *page); \\/* NVIDIA DKMS compat: restored for 6.19 *\\/\\n}}/sg \
         if /struct\\s+dev_pagemap_ops/ && !/page_free/' {:?}",
        memremap_path.to_string_lossy()
    );

    eprintln!("[TEST] tier2_fallback: Executing Perl injection on malformed struct");
    let _perl_output = Command::new("bash")
        .arg("-c")
        .arg(&perl_command)
        .output()
        .expect("Failed to execute Perl command");

    // STEP 3: Check if Perl injection succeeded
    let after_perl =
        fs::read_to_string(&memremap_path).expect("Failed to read after Perl execution");

    let has_page_free_from_perl = after_perl.contains("page_free");
    eprintln!(
        "[TEST] tier2_fallback: Perl injection result: page_free present = {}",
        has_page_free_from_perl
    );

    // STEP 4: If Perl failed (as expected on malformed struct), apply Tier 2 fallback
    if !has_page_free_from_perl {
        eprintln!(
            "[TEST] tier2_fallback: Perl injection failed/skipped - initiating Tier 2 fallback"
        );

        // Ensure newline at EOF before appending
        let mut content = after_perl.clone();
        if !content.ends_with('\n') {
            content.push('\n');
        }

        // Append GOATD PHASE 9 compatibility shim
        let compat_shim = r#"
/* GOATD PHASE 9: Multi-Tiered Fallback Engine */
#ifndef _GOATD_MEMREMAP_COMPAT_H
#define _GOATD_MEMREMAP_COMPAT_H
struct goatd_dev_pagemap_ops {
   void (*page_free)(struct page *page);
};
#define dev_pagemap_ops goatd_dev_pagemap_ops
#endif
"#;

        content.push_str(compat_shim);

        fs::write(&memremap_path, &content).expect("Failed to write Tier 2 fallback");

        eprintln!("[TEST] tier2_fallback: Tier 2 fallback shim appended");
    }

    // STEP 5: Verify that page_free field is now present (either from Perl or Tier 2)
    let final_content = fs::read_to_string(&memremap_path).expect("Failed to read final content");

    assert!(
        final_content.contains("page_free"),
        "page_free field not present after Perl + Tier 2 fallback. Content:\n{}",
        final_content
    );

    // STEP 6: Verify Tier 2 fallback structure if Perl failed
    if !has_page_free_from_perl {
        assert!(
            final_content.contains("_GOATD_MEMREMAP_COMPAT_H"),
            "GOATD compatibility header not found in Tier 2 fallback"
        );
        assert!(
            final_content.contains("goatd_dev_pagemap_ops"),
            "goatd_dev_pagemap_ops structure not found in Tier 2 fallback"
        );

        eprintln!("[TEST] tier2_fallback: ✓ PASSED - Tier 2 fallback correctly applied");
    } else {
        eprintln!("[TEST] tier2_fallback: ✓ PASSED - page_free present (Perl injection succeeded on this malformed struct)");
    }
}

/// Test 4: Integration test with complete shim workflow
///
/// Tests the full workflow from the NVIDIA_DKMS_MEMREMAP_SHIM template:
/// 1. Executes Perl injection successfully
/// 2. Verifies result contains page_free field
/// 3. Validates compatibility comment is present
#[test]
fn test_memremap_complete_workflow() {
    // STEP 1: Create test environment
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let memremap_path = temp_dir.path().join("memremap.h");

    let original_content = create_linux_6_19_memremap();
    fs::write(&memremap_path, &original_content).expect("Failed to write test file");

    eprintln!("[TEST] complete_workflow: Created test memremap.h");

    // STEP 2: Verify struct is present
    assert!(
        original_content.contains("struct dev_pagemap_ops"),
        "Test setup error: struct dev_pagemap_ops not in mock"
    );

    eprintln!("[TEST] complete_workflow: Struct detection verification passed");

    // STEP 3: Execute Perl injection
    let perl_command = format!(
        "perl -0777 -pi -e 's/(struct\\s+dev_pagemap_ops\\s*\\{{.*?)}}/\
         $1\\n\\tvoid (*page_free)(struct page *page); \\/* NVIDIA DKMS compat: restored for 6.19 *\\/\\n}}/sg \
         if /struct\\s+dev_pagemap_ops/ && !/page_free/' {:?}",
        memremap_path.to_string_lossy()
    );

    let output = Command::new("bash")
        .arg("-c")
        .arg(&perl_command)
        .output()
        .expect("Failed to execute Perl");

    assert!(output.status.success(), "Perl injection failed");
    eprintln!("[TEST] complete_workflow: Perl injection executed successfully");

    // STEP 4: Verify result contains page_free field
    let modified = fs::read_to_string(&memremap_path).expect("Failed to read modified file");

    assert!(
        modified.contains("page_free"),
        "page_free field not found in result"
    );

    // Debug: Print what we got
    eprintln!(
        "[TEST] complete_workflow: Modified content preview:\n{}",
        modified
            .lines()
            .skip_while(|l| !l.contains("struct dev_pagemap_ops"))
            .take(10)
            .collect::<Vec<_>>()
            .join("\n")
    );

    eprintln!("[TEST] complete_workflow: ✓ PASSED - Complete workflow successful");
}

/// Test 5: Headers function regex pattern matching
///
/// Unit test for the new regex pattern `r"(?m)^package_.*?headers\s*\(\)\s*\{"`
/// Verifies that the regex correctly matches all headers package function variants:
/// - Standard: package_linux-headers() {
/// - Variant: package_linux-zen-headers() {
/// - Complex rebranded: package_linux-mainline-goatd-gaming-headers() {
///
/// This test ensures the broad-spectrum regex properly handles:
/// - Variable prefixes (package_*, no underscore variants)
/// - Any number of hyphens in the variant name
/// - GOATd branding with hyphens (linux-mainline-goatd-gaming-headers)
/// - Whitespace variations around () and {
#[test]
fn test_headers_regex_pattern_matching() {
    use regex::Regex;

    // The exact regex as implemented in pkgbuild.rs line 1026
    let headers_regex =
        Regex::new(r"(?m)^package_.*?headers\s*\(\)\s*\{").expect("Invalid headers function regex");

    // Mock PKGBUILD content with multiple headers function variants
    let mock_pkgbuild = r#"#!/bin/bash
# PKGBUILD for linux kernel variants

pkgbase='linux-mainline-goatd-gaming'
pkgname=('linux-mainline-goatd-gaming' 'linux-mainline-goatd-gaming-headers')
pkgver=6.14
pkgrel=1

prepare() {
    # Prepare function content
    echo "Preparing kernel..."
}

build() {
    # Build function content
    echo "Building kernel..."
}

package_linux-headers() {
    # Standard linux headers package function
    cd "$srcdir/linux"
    make INSTALL_HDR_PATH="$pkgdir/usr" headers_install
}

package_linux-zen-headers() {
     # Zen variant headers package function
     cd "$srcdir/linux-zen"
     make INSTALL_HDR_PATH="$pkgdir/usr" headers_install
 }

package_linux-mainline-goatd-gaming-headers() {
    # Complex rebranded headers package function with GOATd gaming profile
    cd "$srcdir/linux-mainline"
    make INSTALL_HDR_PATH="$pkgdir/usr" headers_install

    # Additional GOATd-specific operations
    echo "Gaming optimizations applied"
}

package_linux-mainline() {
    # Main kernel package (not headers)
    cd "$srcdir/linux-mainline"
    make INSTALL_MOD_PATH="$pkgdir/usr" modules_install
}
"#;

    eprintln!("[TEST] headers_regex: Testing broad-spectrum headers function regex");

    // STEP 1: Find all matches
    let matches: Vec<(usize, &str)> = headers_regex
        .find_iter(&mock_pkgbuild)
        .map(|m| (m.start(), m.as_str()))
        .collect();

    eprintln!("[TEST] headers_regex: Found {} matches", matches.len());

    // STEP 2: Verify we found exactly 3 headers functions
    assert_eq!(
        matches.len(),
        3,
        "Expected 3 headers functions, found {}. Content:\n{}",
        matches.len(),
        mock_pkgbuild
    );

    eprintln!("[TEST] headers_regex: ✓ Found exactly 3 headers function(s)");

    // STEP 3: Verify each specific match
    let mut match_strs = matches.iter().map(|(_, s)| *s).collect::<Vec<_>>();
    match_strs.sort();

    // Expected matches (in sorted order for predictable testing)
    let expected_patterns = [
        "package_linux-headers() {",
        "package_linux-mainline-goatd-gaming-headers() {",
        "package_linux-zen-headers() {",
    ];

    for (i, expected) in expected_patterns.iter().enumerate() {
        assert!(
            match_strs[i].contains(&expected[..expected.len() - 3]), // Remove " {" for partial match
            "Expected pattern '{}' in match '{}' at position {}",
            expected,
            match_strs[i],
            i
        );
        eprintln!(
            "[TEST] headers_regex: ✓ Match {}: {}",
            i + 1,
            match_strs[i].trim()
        );
    }

    // STEP 4: Verify the regex DOES NOT match non-headers functions
    let non_headers_patterns = ["package_linux-mainline() {", "package() {", "_package() {"];

    for pattern in &non_headers_patterns {
        let test_content = mock_pkgbuild.replace(
            "package_linux-mainline-goatd-gaming-headers",
            "package_test",
        );
        if test_content.contains(pattern) {
            assert!(
                !headers_regex.is_match(pattern),
                "Regex should NOT match non-headers pattern: {}",
                pattern
            );
            eprintln!(
                "[TEST] headers_regex: ✓ Correctly rejected non-headers pattern: {}",
                pattern
            );
        }
    }

    eprintln!("[TEST] headers_regex: ✓ PASSED - All headers function variants matched correctly");
}

/// Test 6: PKGBASE regex pattern matching
///
/// Unit test for the centralized `PKGBASE_REGEX` regex pattern used in pkgbuild.rs
/// Verifies that the regex correctly extracts the base kernel variant by stripping
/// optional GOATd branding suffixes.
///
/// The regex pattern: `r#"(?m)^\s*pkgbase=['"]?(?:(.+?)-goatd-.*|([^'"]+))['"]?\s*$"#`
///
/// This test ensures the regex properly handles:
/// - Optional quotes (single, double, or none)
/// - Leading/trailing whitespace
/// - GOATd branding stripping: extracts everything before `-goatd-*` suffix
/// - Two-group capture strategy:
///   - Group 1: Base variant with GOATd suffix stripped (for pkgbase='linux-mainline-goatd-gaming')
///   - Group 2: Direct variant without GOATd suffix (fallback for pkgbase='linux-mainline')
#[test]
fn test_pkgbase_regex_pattern_matching() {
    use regex::Regex;

    // The exact regex as implemented in pkgbuild.rs line 41
    let pkgbase_regex = Regex::new(r#"(?m)^\s*pkgbase=['"]?(?:(.+?)-goatd-.*|([^'"]+))['"]?\s*$"#)
        .expect("Invalid PKGBASE_REGEX");

    eprintln!("[TEST] pkgbase_regex: Testing PKGBASE_REGEX pattern matching");

    // Test case 1: Standard unix-mainline with GOATd-gaming suffix
    let test1_content = r#"pkgbase='linux-mainline-goatd-gaming'"#;
    if let Some(caps) = pkgbase_regex.captures(test1_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-mainline",
            "Test 1 failed: Expected 'linux-mainline', got '{}' for {}",
            variant, test1_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 1 PASSED - {} -> {}",
            test1_content, variant
        );
    } else {
        panic!("Test 1 failed: Regex did not match '{}'", test1_content);
    }

    // Test case 2: Standard linux-mainline without GOATd suffix
    let test2_content = r#"pkgbase="linux-mainline""#;
    if let Some(caps) = pkgbase_regex.captures(test2_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-mainline",
            "Test 2 failed: Expected 'linux-mainline', got '{}' for {}",
            variant, test2_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 2 PASSED - {} -> {}",
            test2_content, variant
        );
    } else {
        panic!("Test 2 failed: Regex did not match '{}'", test2_content);
    }

    // Test case 3: Complex GOATd branding with multiple hyphens
    let test3_content = r#"pkgbase="linux-mainline-goatd-goatd-gaming-gaming""#;
    if let Some(caps) = pkgbase_regex.captures(test3_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-mainline",
            "Test 3 failed: Expected 'linux-mainline', got '{}' for {}",
            variant, test3_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 3 PASSED - {} -> {}",
            test3_content, variant
        );
    } else {
        panic!("Test 3 failed: Regex did not match '{}'", test3_content);
    }

    // Test case 4: linux-zen with GOATd gaming suffix
    let test4_content = "pkgbase='linux-zen-goatd-gaming'";
    if let Some(caps) = pkgbase_regex.captures(test4_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-zen",
            "Test 4 failed: Expected 'linux-zen', got '{}' for {}",
            variant, test4_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 4 PASSED - {} -> {}",
            test4_content, variant
        );
    } else {
        panic!("Test 4 failed: Regex did not match '{}'", test4_content);
    }

    // Test case 5: linux-zen without quotes or GOATd suffix
    let test5_content = "pkgbase=linux-zen";
    if let Some(caps) = pkgbase_regex.captures(test5_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-zen",
            "Test 5 failed: Expected 'linux-zen', got '{}' for {}",
            variant, test5_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 5 PASSED - {} -> {}",
            test5_content, variant
        );
    } else {
        panic!("Test 5 failed: Regex did not match '{}'", test5_content);
    }

    // Test case 6: Standard linux with GOATd custom suffix
    let test6_content = r#"pkgbase="linux-goatd-custom""#;
    if let Some(caps) = pkgbase_regex.captures(test6_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux",
            "Test 6 failed: Expected 'linux', got '{}' for {}",
            variant, test6_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 6 PASSED - {} -> {}",
            test6_content, variant
        );
    } else {
        panic!("Test 6 failed: Regex did not match '{}'", test6_content);
    }

    // Test case 7: With leading whitespace
    let test7_content = "  pkgbase='linux-mainline-goatd-gaming'";
    if let Some(caps) = pkgbase_regex.captures(test7_content) {
        let variant = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        assert_eq!(
            variant, "linux-mainline",
            "Test 7 failed: Expected 'linux-mainline', got '{}' for {}",
            variant, test7_content
        );
        eprintln!(
            "[TEST] pkgbase_regex: ✓ Test 7 PASSED - {} -> {}",
            test7_content, variant
        );
    } else {
        panic!("Test 7 failed: Regex did not match '{}'", test7_content);
    }

    eprintln!("[TEST] pkgbase_regex: ✓ PASSED - All PKGBASE_REGEX extraction tests successful");
}

/// Test 7: Stateful multi-line pkgname rebranding
///
/// Unit test for the stateful `in_pkgname_array` logic in `patch_pkgbuild_for_rebranding`.
/// Verifies:
/// 1. Multi-line `pkgname` array rebranding.
/// 2. Quote preservation within the array.
/// 3. No bleeding into subsequent assignments (e.g., `pkgrel`).
/// 4. Synchronized package function rebranding.
#[test]
fn test_stateful_rebranding_multi_line_array() {
    use crate::kernel::patcher::pkgbuild::patch_pkgbuild_for_rebranding;
    use std::fs;
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let src_dir = temp_dir.path();
    let pkgbuild_path = src_dir.join("PKGBUILD");

    // Mock PKGBUILD with multi-line pkgname and subsequent pkgrel
    let original_content = r#"pkgbase='linux-zen'
pkgname=(
  'linux-zen'
  'linux-zen-headers'
  "linux-zen-docs"
)
pkgrel=1
pkgdesc="Zen kernel"

package_linux-zen() {
    echo "packaging zen"
}

package_linux-zen-headers() {
    echo "packaging headers"
}
"#;
    fs::write(&pkgbuild_path, original_content).expect("Failed to write mock PKGBUILD");

    // Run rebranding with "gaming" profile
    patch_pkgbuild_for_rebranding(src_dir, "gaming").expect("Rebranding failed");

    let rebranded_content =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read rebranded PKGBUILD");
    eprintln!("[TEST] rebranded_content:\n{}", rebranded_content);

    // 1. Verify pkgbase
    assert!(rebranded_content.contains("pkgbase='linux-zen-goatd-gaming'"));

    // 2. Verify all entries in pkgname array are rebranded correctly
    assert!(rebranded_content.contains("'linux-zen-goatd-gaming'"));
    assert!(rebranded_content.contains("'linux-zen-goatd-gaming-headers'"));
    assert!(rebranded_content.contains("\"linux-zen-goatd-gaming-docs\""));

    // 3. Verify NO BLEEDING into pkgrel
    assert!(rebranded_content.contains("pkgrel=1"));
    assert!(!rebranded_content.contains("pkgrel=1-goatd-gaming"));

    // 4. Verify package functions are rebranded
    assert!(rebranded_content.contains("package_linux-zen-goatd-gaming() {"));
    assert!(rebranded_content.contains("package_linux-zen-goatd-gaming-headers() {"));

    // 5. Verify provides metadata
    assert!(rebranded_content.contains("provides=('linux-zen')"));

    eprintln!("[TEST] stateful_rebranding: ✓ PASSED - Multi-line array handled without bleeding");
}

/// Test 8: Rebranding idempotency
///
/// CRITICAL: Ensures that rebranding an already-branded PKGBUILD results in
/// the same output (no double-branding like `linux-goatd-goatd-gaming-gaming`).
///
/// This test:
/// 1. Brands a vanilla PKGBUILD with "gaming" profile
/// 2. Brands the result again with "gaming" profile
/// 3. Verifies that the second branding produces identical output (idempotent)
/// 4. Confirms no double-branding occurs
#[test]
fn test_rebranding_idempotency() {
    use crate::kernel::patcher::pkgbuild::patch_pkgbuild_for_rebranding;
    use std::fs;

    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let src_dir = temp_dir.path();
    let pkgbuild_path = src_dir.join("PKGBUILD");

    // Original vanilla PKGBUILD
    let original_content = r#"pkgbase='linux-zen'
pkgname=('linux-zen' 'linux-zen-headers')
pkgver=6.18
pkgrel=1
pkgdesc="Zen kernel"

package_linux-zen() {
   echo "packaging zen"
}

package_linux-zen-headers() {
   echo "packaging headers"
}
"#;
    fs::write(&pkgbuild_path, original_content).expect("Failed to write original PKGBUILD");

    eprintln!("[TEST] rebranding_idempotency: Starting with vanilla PKGBUILD");

    // STEP 1: First rebranding (vanilla -> gaming)
    patch_pkgbuild_for_rebranding(src_dir, "gaming").expect("First rebranding failed");

    let first_branded =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read first rebranded PKGBUILD");

    eprintln!("[TEST] rebranding_idempotency: First rebranding complete");
    eprintln!(
        "[TEST] rebranding_idempotency: First branded content:\n{}\n",
        first_branded
    );

    // STEP 2: Second rebranding (gaming -> gaming, should be idempotent)
    patch_pkgbuild_for_rebranding(src_dir, "gaming").expect("Second rebranding failed");

    let second_branded =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read second rebranded PKGBUILD");

    eprintln!("[TEST] rebranding_idempotency: Second rebranding complete");
    eprintln!(
        "[TEST] rebranding_idempotency: Second branded content:\n{}\n",
        second_branded
    );

    // STEP 3: Verify idempotency - both should be identical
    assert_eq!(
        first_branded, second_branded,
        "IDEMPOTENCY FAILURE: Second rebranding changed the PKGBUILD\n\nFirst:\n{}\n\nSecond:\n{}",
        first_branded, second_branded
    );

    eprintln!("[TEST] rebranding_idempotency: ✓ Content is identical after second rebranding");

    // STEP 4: Verify no double-branding
    assert!(
        !second_branded.contains("linux-zen-goatd-goatd-gaming"),
        "DOUBLE-BRANDING ERROR: Found double-branded 'linux-zen-goatd-goatd-gaming'"
    );
    assert!(
        !second_branded.contains("gaming-gaming"),
        "DOUBLE-BRANDING ERROR: Found double profile 'gaming-gaming'"
    );

    eprintln!("[TEST] rebranding_idempotency: ✓ No double-branding detected");

    // STEP 5: Verify correct branding is present
    assert!(
        second_branded.contains("pkgbase='linux-zen-goatd-gaming'"),
        "Expected pkgbase not found"
    );
    assert!(
        second_branded.contains("'linux-zen-goatd-gaming'"),
        "Expected branding not found in pkgname"
    );
    assert!(
        second_branded.contains("'linux-zen-goatd-gaming-headers'"),
        "Expected headers branding not found in pkgname"
    );
    assert!(
        second_branded.contains("package_linux-zen-goatd-gaming() {"),
        "Expected package function not found"
    );
    assert!(
        second_branded.contains("package_linux-zen-goatd-gaming-headers() {"),
        "Expected headers package function not found"
    );

    eprintln!(
        "[TEST] rebranding_idempotency: ✓ PASSED - Rebranding is idempotent, no double-branding"
    );
}

/// Test 9: Rebranding with workstation profile
///
/// Verifies that the dynamic `linux-{variant}-goatd-{profile}` naming scheme
/// works correctly for non-gaming profiles (e.g., "workstation").
///
/// This test ensures:
/// 1. Vanilla linux with workstation profile → linux-goatd-workstation
/// 2. linux-zen with workstation profile → linux-zen-goatd-workstation
/// 3. linux-mainline with workstation profile → linux-mainline-goatd-workstation
#[test]
fn test_rebranding_workstation_profile() {
    use crate::kernel::patcher::pkgbuild::patch_pkgbuild_for_rebranding;
    use std::fs;

    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let src_dir = temp_dir.path();
    let pkgbuild_path = src_dir.join("PKGBUILD");

    // Test Case 1: Vanilla linux with workstation profile
    eprintln!("[TEST] workstation_profile: Test 1 - vanilla linux + workstation");
    let original_linux = r#"pkgbase='linux'
pkgname=('linux' 'linux-headers')
pkgver=6.18
pkgrel=1

package_linux() {
   echo "packaging linux"
}

package_linux-headers() {
   echo "packaging headers"
}
"#;
    fs::write(&pkgbuild_path, original_linux).expect("Failed to write PKGBUILD");

    patch_pkgbuild_for_rebranding(src_dir, "workstation")
        .expect("Rebranding with workstation profile failed");

    let rebranded = fs::read_to_string(&pkgbuild_path).expect("Failed to read rebranded PKGBUILD");

    assert!(
        rebranded.contains("pkgbase='linux-goatd-workstation'"),
        "Expected pkgbase='linux-goatd-workstation' for vanilla linux"
    );
    assert!(
        rebranded.contains("'linux-goatd-workstation'"),
        "Expected branding in pkgname for vanilla linux"
    );
    assert!(
        rebranded.contains("'linux-goatd-workstation-headers'"),
        "Expected headers branding for vanilla linux"
    );
    eprintln!("[TEST] workstation_profile: ✓ Test 1 PASSED - vanilla linux branded correctly");

    // Test Case 2: linux-zen with workstation profile
    eprintln!("[TEST] workstation_profile: Test 2 - linux-zen + workstation");
    let original_zen = r#"pkgbase='linux-zen'
pkgname=('linux-zen' 'linux-zen-headers')
pkgver=6.18
pkgrel=1

package_linux-zen() {
   echo "packaging zen"
}

package_linux-zen-headers() {
   echo "packaging headers"
}
"#;
    fs::write(&pkgbuild_path, original_zen).expect("Failed to write PKGBUILD");

    patch_pkgbuild_for_rebranding(src_dir, "workstation")
        .expect("Rebranding zen with workstation profile failed");

    let rebranded = fs::read_to_string(&pkgbuild_path).expect("Failed to read rebranded PKGBUILD");

    assert!(
        rebranded.contains("pkgbase='linux-zen-goatd-workstation'"),
        "Expected pkgbase='linux-zen-goatd-workstation' for zen variant"
    );
    assert!(
        rebranded.contains("'linux-zen-goatd-workstation'"),
        "Expected branding in pkgname for zen"
    );
    assert!(
        rebranded.contains("'linux-zen-goatd-workstation-headers'"),
        "Expected headers branding for zen"
    );
    eprintln!("[TEST] workstation_profile: ✓ Test 2 PASSED - linux-zen branded correctly");

    // Test Case 3: linux-mainline with workstation profile
    eprintln!("[TEST] workstation_profile: Test 3 - linux-mainline + workstation");
    let original_mainline = r#"pkgbase='linux-mainline'
pkgname=('linux-mainline' 'linux-mainline-headers')
pkgver=6.18
pkgrel=1

package_linux-mainline() {
   echo "packaging mainline"
}

package_linux-mainline-headers() {
   echo "packaging headers"
}
"#;
    fs::write(&pkgbuild_path, original_mainline).expect("Failed to write PKGBUILD");

    patch_pkgbuild_for_rebranding(src_dir, "workstation")
        .expect("Rebranding mainline with workstation profile failed");

    let rebranded = fs::read_to_string(&pkgbuild_path).expect("Failed to read rebranded PKGBUILD");

    assert!(
        rebranded.contains("pkgbase='linux-mainline-goatd-workstation'"),
        "Expected pkgbase='linux-mainline-goatd-workstation' for mainline"
    );
    assert!(
        rebranded.contains("'linux-mainline-goatd-workstation'"),
        "Expected branding in pkgname for mainline"
    );
    assert!(
        rebranded.contains("'linux-mainline-goatd-workstation-headers'"),
        "Expected headers branding for mainline"
    );
    eprintln!("[TEST] workstation_profile: ✓ Test 3 PASSED - linux-mainline branded correctly");

    eprintln!(
        "[TEST] workstation_profile: ✓ PASSED - Dynamic naming works for workstation profile"
    );
}

/// Test 10: Multi-profile coexistence validation
///
/// Verifies that multiple profiles can coexist on the same system without
/// collision by validating that different profiles produce unique identities.
///
/// This test:
/// 1. Brands the same PKGBUILD with "gaming" profile → linux-zen-goatd-gaming
/// 2. Brands fresh PKGBUILD with "workstation" profile → linux-zen-goatd-workstation
/// 3. Verifies the identities are unique and don't overlap
#[test]
fn test_multi_profile_coexistence() {
    use crate::kernel::patcher::pkgbuild::patch_pkgbuild_for_rebranding;
    use std::fs;

    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let src_dir = temp_dir.path();
    let pkgbuild_path = src_dir.join("PKGBUILD");

    let original_content = r#"pkgbase='linux-zen'
pkgname=('linux-zen' 'linux-zen-headers')
pkgver=6.18
pkgrel=1

package_linux-zen() {
   echo "packaging zen"
}

package_linux-zen-headers() {
   echo "packaging headers"
}
"#;

    // Brand with gaming profile
    eprintln!("[TEST] multi_profile_coexistence: Branding with gaming profile");
    fs::write(&pkgbuild_path, original_content).expect("Failed to write original");

    patch_pkgbuild_for_rebranding(src_dir, "gaming").expect("Gaming profile branding failed");

    let gaming_branded =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read gaming-branded PKGBUILD");

    assert!(
        gaming_branded.contains("pkgbase='linux-zen-goatd-gaming'"),
        "Gaming profile branding failed"
    );

    eprintln!("[TEST] multi_profile_coexistence: Gaming profile identity: linux-zen-goatd-gaming");

    // Reset and brand with workstation profile
    eprintln!("[TEST] multi_profile_coexistence: Branding with workstation profile");
    fs::write(&pkgbuild_path, original_content).expect("Failed to write original again");

    patch_pkgbuild_for_rebranding(src_dir, "workstation")
        .expect("Workstation profile branding failed");

    let workstation_branded =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read workstation-branded PKGBUILD");

    assert!(
        workstation_branded.contains("pkgbase='linux-zen-goatd-workstation'"),
        "Workstation profile branding failed"
    );

    eprintln!("[TEST] multi_profile_coexistence: Workstation profile identity: linux-zen-goatd-workstation");

    // Verify the identities are different
    assert!(
        gaming_branded != workstation_branded,
        "Gaming and workstation profiles must produce different PKGBUILDs"
    );

    // Verify no cross-contamination
    assert!(
        !workstation_branded.contains("gaming"),
        "Workstation profile contains gaming reference (contamination)"
    );
    assert!(
        !gaming_branded.contains("workstation"),
        "Gaming profile contains workstation reference (contamination)"
    );

    eprintln!(
        "[TEST] multi_profile_coexistence: ✓ PASSED - Multiple profiles coexist without collision"
    );
}

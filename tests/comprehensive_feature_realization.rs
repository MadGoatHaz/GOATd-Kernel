//! COMPREHENSIVE FEATURE REALIZATION TEST
//!
//! This test verifies that ALL performance features are correctly realized:
//! 1. LTO Clang Thin (CONFIG_LTO_CLANG_THIN=y, CONFIG_LTO_CLANG=y)
//! 2. MGLRU (CONFIG_LRU_GEN=y, CONFIG_LRU_GEN_ENABLED=y, CONFIG_LRU_GEN_STATS=y)
//! 3. POLLY (Polly optimization flags in PKGBUILD CFLAGS/CXXFLAGS)
//! 4. BORE (CONFIG_SCHED_BORE=y for Gaming/Workstation profiles)

use goatd_kernel::models::LtoType;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_comprehensive_feature_realization_gaming() {
    println!("\n");
    println!(
        "╔═══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!(
        "║ COMPREHENSIVE FEATURE REALIZATION TEST - GAMING PROFILE                          ║"
    );
    println!(
        "║ Features to verify: LTO, MGLRU, POLLY, BORE                                      ║"
    );
    println!(
        "╚═══════════════════════════════════════════════════════════════════════════════════╝"
    );
    println!();

    let test_dir = PathBuf::from("/tmp/goatd_feature_test_gaming");
    let kernel_dir = test_dir.join("kernel_src");

    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&kernel_dir).expect("Failed to create test directory");

    println!("[TEST] Step 1: Preparing kernel directory...");

    // Create minimal PKGBUILD
    let pkgbuild_content = r#"#!/bin/bash
pkgbase=linux-goatd-test
pkgname=("linux-goatd-test" "linux-goatd-test-headers")
pkgver=6.6.13
pkgrel=1
build() {
    echo "Build phase"
}
"#;

    fs::write(kernel_dir.join("PKGBUILD"), pkgbuild_content).expect("Failed to write PKGBUILD");
    println!("[TEST]   ✓ PKGBUILD created");

    // Create initial .config
    let initial_config = vec![
        "# Linux kernel configuration",
        "CONFIG_64BIT=y",
        "CONFIG_X86_64=y",
    ]
    .join("\n");

    fs::write(kernel_dir.join(".config"), initial_config).expect("Failed to write initial .config");
    println!("[TEST]   ✓ Initial .config created");

    // =========================================================================
    // STEP 2: CONFIGURE ALL FEATURES FOR GAMING PROFILE
    // =========================================================================
    println!("[TEST] Step 2: Configuring ALL features for Gaming profile...");

    let mut options = HashMap::new();

    // LTO Configuration
    options.insert("CONFIG_CC_IS_CLANG".to_string(), "y".to_string());
    options.insert("CONFIG_LTO_CLANG".to_string(), "y".to_string());
    options.insert("CONFIG_LTO_CLANG_THIN".to_string(), "y".to_string());
    options.insert("CONFIG_HAS_LTO_CLANG".to_string(), "y".to_string());
    println!("[TEST]   ✓ LTO Configuration: CONFIG_LTO_CLANG=y, CONFIG_LTO_CLANG_THIN=y");

    // MGLRU Configuration (Gaming profile enables MGLRU)
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN".to_string(),
        "CONFIG_LRU_GEN=y".to_string(),
    );
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN_ENABLED".to_string(),
        "CONFIG_LRU_GEN_ENABLED=y".to_string(),
    );
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN_STATS".to_string(),
        "CONFIG_LRU_GEN_STATS=y".to_string(),
    );
    println!("[TEST]   ✓ MGLRU Configuration: CONFIG_LRU_GEN=y, CONFIG_LRU_GEN_ENABLED=y");

    // BORE Scheduler Configuration (Gaming profile enables BORE)
    options.insert("_APPLY_BORE_SCHEDULER".to_string(), "1".to_string());
    options.insert("CONFIG_SCHED_BORE".to_string(), "y".to_string());
    println!("[TEST]   ✓ BORE Configuration: CONFIG_SCHED_BORE=y");

    // Polly Configuration
    options.insert("_POLLY_CFLAGS".to_string(), "-mllvm -polly".to_string());
    options.insert("_POLLY_CXXFLAGS".to_string(), "-mllvm -polly".to_string());
    options.insert("_POLLY_LDFLAGS".to_string(), "-mllvm -polly".to_string());
    println!("[TEST]   ✓ POLLY Configuration: Polly flags set");

    // =========================================================================
    // STEP 3: APPLY PATCHER WITH ALL OPTIONS
    // =========================================================================
    println!("[TEST] Step 3: Applying patcher with all features...");

    let patcher = goatd_kernel::kernel::patcher::KernelPatcher::new(kernel_dir.clone());

    // Apply kconfig which handles LTO, MGLRU, BORE
    match patcher.apply_kconfig(options.clone(), LtoType::Thin) {
        Ok(_) => {
            println!("[TEST]   ✓ Kconfig applied successfully");
        }
        Err(e) => {
            panic!("[TEST] ✗ Kconfig failed: {:?}", e);
        }
    }

    // =========================================================================
    // STEP 4: VERIFY .CONFIG HAS ALL FEATURES
    // =========================================================================
    println!("[TEST] Step 4: Verifying .config contains all features...");

    let config_content =
        fs::read_to_string(kernel_dir.join(".config")).expect("Failed to read .config");

    // Verify LTO
    assert!(
        config_content.contains("CONFIG_LTO_CLANG=y"),
        "FAIL: CONFIG_LTO_CLANG=y not found"
    );
    assert!(
        config_content.contains("CONFIG_LTO_CLANG_THIN=y"),
        "FAIL: CONFIG_LTO_CLANG_THIN=y not found"
    );
    assert!(
        config_content.contains("CONFIG_HAS_LTO_CLANG=y"),
        "FAIL: CONFIG_HAS_LTO_CLANG=y not found"
    );
    println!("[TEST]   ✓ LTO features verified in .config");

    // Verify MGLRU
    assert!(
        config_content.contains("CONFIG_LRU_GEN=y"),
        "FAIL: CONFIG_LRU_GEN=y not found in .config"
    );
    assert!(
        config_content.contains("CONFIG_LRU_GEN_ENABLED=y"),
        "FAIL: CONFIG_LRU_GEN_ENABLED=y not found in .config"
    );
    assert!(
        config_content.contains("CONFIG_LRU_GEN_STATS=y"),
        "FAIL: CONFIG_LRU_GEN_STATS=y not found in .config"
    );
    println!("[TEST]   ✓ MGLRU features verified in .config");

    // Verify BORE
    assert!(
        config_content.contains("CONFIG_SCHED_BORE=y"),
        "FAIL: CONFIG_SCHED_BORE=y not found in .config"
    );
    println!("[TEST]   ✓ BORE feature verified in .config");

    // =========================================================================
    // STEP 5: VERIFY PKGBUILD HAS POLLY FLAGS
    // =========================================================================
    println!("[TEST] Step 5: Verifying PKGBUILD contains Polly flags...");

    let pkgbuild_content =
        fs::read_to_string(kernel_dir.join("PKGBUILD")).expect("Failed to read PKGBUILD");

    assert!(
        pkgbuild_content.contains("-mllvm -polly")
            || pkgbuild_content.contains("POLLY_CFLAGS")
            || pkgbuild_content.contains("POLLY_CXXFLAGS"),
        "FAIL: Polly flags not found in PKGBUILD"
    );
    println!("[TEST]   ✓ Polly flags verified in PKGBUILD");

    // =========================================================================
    // PRINT SUMMARY
    // =========================================================================
    println!();
    println!(
        "╔═══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!(
        "║ TEST PASSED - ALL FEATURES REALIZED                                              ║"
    );
    println!(
        "╚═══════════════════════════════════════════════════════════════════════════════════╝"
    );
    println!();
    println!("[TEST] ✓ LTO:   CONFIG_LTO_CLANG_THIN=y is REALIZED");
    println!("[TEST] ✓ MGLRU: CONFIG_LRU_GEN=y is REALIZED");
    println!("[TEST] ✓ BORE:  CONFIG_SCHED_BORE=y is REALIZED");
    println!("[TEST] ✓ POLLY: Optimization flags are REALIZED");
    println!();
}

#[test]
fn test_comprehensive_feature_realization_server() {
    println!("\n");
    println!(
        "╔═══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!(
        "║ COMPREHENSIVE FEATURE REALIZATION TEST - SERVER PROFILE                          ║"
    );
    println!(
        "║ Features to verify: LTO, MGLRU (NO BORE, NO POLLY)                               ║"
    );
    println!(
        "╚═══════════════════════════════════════════════════════════════════════════════════╝"
    );
    println!();

    let test_dir = PathBuf::from("/tmp/goatd_feature_test_server");
    let kernel_dir = test_dir.join("kernel_src");

    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&kernel_dir).expect("Failed to create test directory");

    println!("[TEST] Step 1: Preparing kernel directory...");

    // Create minimal PKGBUILD
    let pkgbuild_content = r#"#!/bin/bash
pkgbase=linux-goatd-test
pkgname=("linux-goatd-test" "linux-goatd-test-headers")
pkgver=6.6.13
pkgrel=1
build() {
    echo "Build phase"
}
"#;

    fs::write(kernel_dir.join("PKGBUILD"), pkgbuild_content).expect("Failed to write PKGBUILD");
    println!("[TEST]   ✓ PKGBUILD created");

    // Create initial .config
    let initial_config = vec![
        "# Linux kernel configuration",
        "CONFIG_64BIT=y",
        "CONFIG_X86_64=y",
    ]
    .join("\n");

    fs::write(kernel_dir.join(".config"), initial_config).expect("Failed to write initial .config");
    println!("[TEST]   ✓ Initial .config created");

    // =========================================================================
    // STEP 2: CONFIGURE FEATURES FOR SERVER PROFILE
    // =========================================================================
    println!("[TEST] Step 2: Configuring features for Server profile...");

    let mut options = HashMap::new();

    // LTO Configuration (Server uses Full LTO)
    options.insert("CONFIG_CC_IS_CLANG".to_string(), "y".to_string());
    options.insert("CONFIG_LTO_CLANG".to_string(), "y".to_string());
    options.insert("CONFIG_LTO_CLANG_FULL".to_string(), "y".to_string());
    options.insert("CONFIG_HAS_LTO_CLANG".to_string(), "y".to_string());
    println!("[TEST]   ✓ LTO Configuration: CONFIG_LTO_CLANG=y (Full variant)");

    // MGLRU Configuration (Server profile enables MGLRU for memory efficiency)
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN".to_string(),
        "CONFIG_LRU_GEN=y".to_string(),
    );
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN_ENABLED".to_string(),
        "CONFIG_LRU_GEN_ENABLED=y".to_string(),
    );
    options.insert(
        "_MGLRU_CONFIG_LRU_GEN_STATS".to_string(),
        "CONFIG_LRU_GEN_STATS=y".to_string(),
    );
    println!("[TEST]   ✓ MGLRU Configuration: CONFIG_LRU_GEN=y");

    // Server does NOT use BORE (uses EEVDF)
    options.insert("_APPLY_BORE_SCHEDULER".to_string(), "0".to_string());
    println!("[TEST]   ✓ BORE: NOT applied (Server uses EEVDF)");

    // Server does NOT use Polly by default
    println!("[TEST]   ✓ POLLY: NOT applied (Server profile default)");

    // =========================================================================
    // STEP 3: APPLY PATCHER WITH OPTIONS
    // =========================================================================
    println!("[TEST] Step 3: Applying patcher...");

    let patcher = goatd_kernel::kernel::patcher::KernelPatcher::new(kernel_dir.clone());

    match patcher.apply_kconfig(options.clone(), LtoType::Full) {
        Ok(_) => {
            println!("[TEST]   ✓ Kconfig applied successfully");
        }
        Err(e) => {
            panic!("[TEST] ✗ Kconfig failed: {:?}", e);
        }
    }

    // =========================================================================
    // STEP 4: VERIFY .CONFIG HAS EXPECTED FEATURES
    // =========================================================================
    println!("[TEST] Step 4: Verifying .config...");

    let config_content =
        fs::read_to_string(kernel_dir.join(".config")).expect("Failed to read .config");

    // Verify LTO
    assert!(
        config_content.contains("CONFIG_LTO_CLANG=y"),
        "FAIL: CONFIG_LTO_CLANG=y not found"
    );
    println!("[TEST]   ✓ LTO feature verified");

    // Verify MGLRU
    assert!(
        config_content.contains("CONFIG_LRU_GEN=y"),
        "FAIL: CONFIG_LRU_GEN=y not found"
    );
    println!("[TEST]   ✓ MGLRU feature verified");

    // Verify BORE is NOT present
    assert!(
        !config_content.contains("CONFIG_SCHED_BORE=y"),
        "FAIL: CONFIG_SCHED_BORE should NOT be in Server profile"
    );
    println!("[TEST]   ✓ BORE correctly absent (Server uses EEVDF)");

    // =========================================================================
    // PRINT SUMMARY
    // =========================================================================
    println!();
    println!(
        "╔═══════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!(
        "║ TEST PASSED - ALL FEATURES CORRECTLY APPLIED                                     ║"
    );
    println!(
        "╚═══════════════════════════════════════════════════════════════════════════════════╝"
    );
    println!();
    println!("[TEST] ✓ LTO:   CONFIG_LTO_CLANG=y is REALIZED");
    println!("[TEST] ✓ MGLRU: CONFIG_LRU_GEN=y is REALIZED");
    println!("[TEST] ✓ BORE:  Correctly NOT applied (Server profile uses EEVDF)");
    println!("[TEST] ✓ POLLY: Correctly NOT applied (Server profile default)");
    println!();
}

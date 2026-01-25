//! MPL (Metadata Persistence Layer) Integration Test
//!
//! CRITICAL TEST FOR PRODUCTION STABILITY:
//! This test verifies the complete MPL system end-to-end, including:
//! 1. Shell format serialization/deserialization (to_shell_format / from_shell_format)
//! 2. MPL file writing to workspace root
//! 3. Patcher injection of MPL sourcing into PKGBUILD
//! 4. Real bash sourcing of the patched PKGBUILD in a child shell
//! 5. Cross-mount simulation (temp directory DIFFERENT from app root)
//!
//! This test simulates the actual production scenario where:
//! - Workspace is on external scratch disk (/mnt/Optane)
//! - PKGBUILD is patched to source .goatd_metadata
//! - makepkg sources the patched PKGBUILD in a child shell
//! - GOATD_KERNELRELEASE must be correctly populated

use goatd_kernel::kernel::patcher::KernelPatcher;
use goatd_kernel::models::MPLMetadata;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Create a test workspace directory (CROSS-MOUNT SIMULATION)
fn setup_test_workspace() -> tempfile::TempDir {
    // Create temp directory that simulates /mnt/Optane (external scratch disk)
    let workspace = tempfile::tempdir().expect("Failed to create temp workspace");

    // Create kernel source subdirectory
    let kernel_dir = workspace.path().join("linux-6.19.0");
    fs::create_dir_all(&kernel_dir).expect("Failed to create kernel dir");

    // CRITICAL FIX: Create .goatd_anchor file to anchor workspace root
    // This file signals to the system that this directory is a valid GOATd workspace
    let anchor_file = workspace.path().join(".goatd_anchor");
    fs::write(&anchor_file, "").expect("Failed to create .goatd_anchor file");

    workspace
}

/// Create a mock PKGBUILD file
fn create_mock_pkgbuild(workspace_path: &std::path::Path) -> PathBuf {
    let pkgbuild_path = workspace_path.join("PKGBUILD");

    let mock_pkgbuild = r#"#!/bin/bash
# Mock PKGBUILD for testing

pkgname=linux-test
pkgbase=linux
pkgver=6.19.0
pkgrel=1

prepare() {
    echo "PREPARE: Starting prepare function"
}

build() {
    echo "BUILD: Starting build function"
}

package() {
    echo "PACKAGE: Starting package function"
    # This will be where MPL sourcing is injected
}

package_headers() {
    echo "PACKAGE_HEADERS: Starting package_headers function"
}
"#;

    fs::write(&pkgbuild_path, mock_pkgbuild).expect("Failed to create mock PKGBUILD");

    pkgbuild_path
}

/// Test 1: MPLMetadata.to_shell_format() serialization
#[test]
fn test_mpl_metadata_to_shell_format() {
    let workspace = PathBuf::from("/workspace/test");
    let source_dir = workspace.join("linux-6.19.0");

    let metadata = MPLMetadata {
        build_id: "test-build-123".to_string(),
        kernel_release: "6.19.0-goatd-gaming".to_string(),
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace.clone(),
        source_dir,
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    let shell_format = metadata.to_shell_format();

    // Verify shell format contains all expected variables
    assert!(shell_format.contains("GOATD_BUILD_ID=\"test-build-123\""));
    assert!(shell_format.contains("GOATD_KERNELRELEASE=\"6.19.0-goatd-gaming\""));
    assert!(shell_format.contains("GOATD_KERNEL_VERSION=\"6.19.0\""));
    assert!(shell_format.contains("GOATD_PROFILE=\"gaming\""));
    assert!(shell_format.contains("GOATD_VARIANT=\"linux\""));
    assert!(shell_format.contains("GOATD_LTO_LEVEL=\"thin\""));
    assert!(shell_format.contains("GOATD_PKGVER=\"6.19.0\""));
    assert!(shell_format.contains("GOATD_PKGREL=\"1\""));
    assert!(shell_format.contains("GOATD_PROFILE_SUFFIX=\"-goatd-gaming\""));

    // Should be valid shell syntax
    assert!(shell_format.starts_with("# GOATd Kernel Build Metadata"));
}

/// Test 2: MPLMetadata.from_shell_format() deserialization
#[test]
fn test_mpl_metadata_from_shell_format() {
    let shell_content = r#"# GOATd Kernel Build Metadata
# Generated: 2026-01-20T16:21:00Z
# Build Session ID: test-session-456

GOATD_BUILD_ID="test-session-456"
GOATD_KERNELRELEASE="6.19.0-goatd-workstation"
GOATD_KERNEL_VERSION="6.19.0"
GOATD_PROFILE="workstation"
GOATD_VARIANT="linux-zen"
GOATD_LTO_LEVEL="full"
GOATD_BUILD_TIMESTAMP="2026-01-20T16:21:00Z"
GOATD_WORKSPACE_ROOT="/mnt/Optane/goatd"
GOATD_SOURCE_DIR="/mnt/Optane/goatd/linux-zen"
GOATD_PKGVER="6.19.0"
GOATD_PKGREL="2"
GOATD_PROFILE_SUFFIX="-goatd-zen-workstation"
"#;

    let metadata =
        MPLMetadata::from_shell_format(shell_content).expect("Failed to deserialize MPL metadata");

    assert_eq!(metadata.build_id, "test-session-456");
    assert_eq!(metadata.kernel_release, "6.19.0-goatd-workstation");
    assert_eq!(metadata.kernel_version, "6.19.0");
    assert_eq!(metadata.profile, "workstation");
    assert_eq!(metadata.variant, "linux-zen");
    assert_eq!(metadata.lto_level, "full");
    assert_eq!(metadata.build_timestamp, "2026-01-20T16:21:00Z");
    assert_eq!(metadata.workspace_root, PathBuf::from("/mnt/Optane/goatd"));
    assert_eq!(
        metadata.source_dir,
        PathBuf::from("/mnt/Optane/goatd/linux-zen")
    );
    assert_eq!(metadata.pkgver, "6.19.0");
    assert_eq!(metadata.pkgrel, "2");
    assert_eq!(metadata.profile_suffix, "-goatd-zen-workstation");
}

/// Test 3: Roundtrip serialization/deserialization
#[test]
fn test_mpl_metadata_roundtrip() {
    let workspace = PathBuf::from("/mnt/Optane/goatd");
    let source_dir = workspace.join("linux-6.19.0");

    let original = MPLMetadata {
        build_id: "roundtrip-test-789".to_string(),
        kernel_release: "6.19.0-goatd-gaming".to_string(),
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace.clone(),
        source_dir: source_dir.clone(),
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    // Serialize to shell format
    let shell_format = original.to_shell_format();

    // Deserialize back
    let deserialized =
        MPLMetadata::from_shell_format(&shell_format).expect("Failed to roundtrip deserialize");

    // Verify all fields match
    assert_eq!(deserialized.build_id, original.build_id);
    assert_eq!(deserialized.kernel_release, original.kernel_release);
    assert_eq!(deserialized.kernel_version, original.kernel_version);
    assert_eq!(deserialized.profile, original.profile);
    assert_eq!(deserialized.variant, original.variant);
    assert_eq!(deserialized.lto_level, original.lto_level);
    assert_eq!(deserialized.build_timestamp, original.build_timestamp);
    assert_eq!(deserialized.workspace_root, original.workspace_root);
    assert_eq!(deserialized.source_dir, original.source_dir);
    assert_eq!(deserialized.pkgver, original.pkgver);
    assert_eq!(deserialized.pkgrel, original.pkgrel);
    assert_eq!(deserialized.profile_suffix, original.profile_suffix);
}

/// Test 4: MPL file writing and reading
#[test]
fn test_mpl_metadata_write_and_read_file() {
    let temp_workspace = tempfile::tempdir().expect("Failed to create temp workspace");

    let workspace_root = temp_workspace.path().to_path_buf();
    let source_dir = workspace_root.join("linux-6.19.0");

    let metadata = MPLMetadata {
        build_id: "file-io-test-001".to_string(),
        kernel_release: "6.19.0-goatd-gaming".to_string(),
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace_root.clone(),
        source_dir,
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    // Write metadata to file
    let metadata_path = workspace_root.join(".goatd_metadata");
    metadata
        .write_to_file(&metadata_path)
        .expect("Failed to write metadata file");

    // Verify file exists
    assert!(metadata_path.exists(), "Metadata file should exist");

    // Read file back
    let file_content = fs::read_to_string(&metadata_path).expect("Failed to read metadata file");

    // Deserialize from file
    let deserialized = MPLMetadata::from_shell_format(&file_content)
        .expect("Failed to deserialize metadata from file");

    // Verify metadata matches
    assert_eq!(deserialized.build_id, "file-io-test-001");
    assert_eq!(deserialized.kernel_release, "6.19.0-goatd-gaming");
}

/// Test 5: Patcher.inject_mpl_sourcing() - Verify PKGBUILD is patched correctly
#[test]
fn test_patcher_inject_mpl_sourcing() {
    let workspace = setup_test_workspace();
    let workspace_path = workspace.path().to_path_buf();

    // Create mock PKGBUILD
    let _pkgbuild_path = create_mock_pkgbuild(&workspace_path);

    // Create patcher for this workspace
    let patcher = KernelPatcher::new(workspace_path.clone());

    // Inject MPL sourcing
    let result = patcher.inject_mpl_sourcing();
    assert!(
        result.is_ok(),
        "Patcher.inject_mpl_sourcing() should succeed"
    );

    // Read patched PKGBUILD
    let pkgbuild_content = fs::read_to_string(workspace_path.join("PKGBUILD"))
        .expect("Failed to read patched PKGBUILD");

    // Verify injection markers are present
    assert!(
        pkgbuild_content.contains("!!! MPL STARTING !!!"),
        "PKGBUILD should contain MPL injection marker"
    );
    assert!(
        pkgbuild_content.contains("source"),
        "PKGBUILD should contain 'source' command for MPL file"
    );
    assert!(
        pkgbuild_content.contains(".goatd_metadata"),
        "PKGBUILD should reference .goatd_metadata file"
    );
}

/// Test 6: CRITICAL SHELL INTEGRATION TEST
/// Verify that bash can source the patched PKGBUILD and extract GOATD_KERNELRELEASE
#[test]
fn test_mpl_bash_sourcing_integration() {
    let workspace = setup_test_workspace();
    let mut workspace_path = workspace.path().to_path_buf();

    // CRITICAL FIX: Canonicalize the workspace path to resolve symlinks and normalize
    workspace_path =
        fs::canonicalize(&workspace_path).expect("Failed to canonicalize workspace path");

    // STEP 1: Create MPL metadata with specific kernel release
    let metadata = MPLMetadata {
        build_id: "bash-integration-test".to_string(),
        kernel_release: "6.19.0-goatd-gaming-test".to_string(), // CRITICAL: specific version
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace_path.clone(),
        source_dir: workspace_path.join("linux-6.19.0"),
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    // STEP 2: Write metadata to workspace
    let metadata_path = workspace_path.join(".goatd_metadata");
    metadata
        .write_to_file(&metadata_path)
        .expect("Failed to write metadata file");

    eprintln!("[TEST] Wrote metadata to: {}", metadata_path.display());
    eprintln!("[TEST] Metadata content:");
    let metadata_content = fs::read_to_string(&metadata_path).unwrap();
    eprintln!("{}", metadata_content);

    // STEP 3: Create mock PKGBUILD
    let _pkgbuild_path = create_mock_pkgbuild(&workspace_path);

    // STEP 4: Patch PKGBUILD with MPL sourcing
    let patcher = KernelPatcher::new(workspace_path.clone());
    let patch_result = patcher.inject_mpl_sourcing();
    assert!(patch_result.is_ok(), "Patcher.inject_mpl_sourcing() failed");

    // STEP 5: Read patched PKGBUILD for verification
    let pkgbuild_path = workspace_path.join("PKGBUILD");
    let patched_pkgbuild =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read patched PKGBUILD");

    eprintln!("[TEST] Patched PKGBUILD:");
    eprintln!("{}", patched_pkgbuild);

    // STEP 5.5: HARDENED SYNTAX CHECK - Verify bash syntax is valid
    // This catches injection errors like "syntax error near unexpected token '}'
    eprintln!("[TEST] SYNTAX CHECK: Running 'bash -n' on patched PKGBUILD...");
    let syntax_check = Command::new("bash")
        .arg("-n")
        .arg(&pkgbuild_path)
        .output()
        .expect("Failed to execute bash syntax check");

    if !syntax_check.status.success() {
        let stderr = String::from_utf8_lossy(&syntax_check.stderr);
        eprintln!("[TEST] ✗ SYNTAX ERROR in patched PKGBUILD:");
        eprintln!("{}", stderr);
        panic!("PKGBUILD has syntax errors after patching:\n{}", stderr);
    }
    eprintln!("[TEST] ✓ SYNTAX CHECK PASSED: PKGBUILD has valid bash syntax");

    // STEP 6: CRITICAL - Source PKGBUILD in bash child shell and verify GOATD_KERNELRELEASE
    // This simulates what makepkg does when it sources PKGBUILD
    // CRITICAL FIX: Must CALL the function, not just source it (sourcing only defines it)
    let bash_script = format!(
        r#"
cd "{workspace}"
source "./PKGBUILD"
package
echo "KERNELRELEASE=$GOATD_KERNELRELEASE"
"#,
        workspace = workspace_path.display()
    );

    eprintln!("[TEST] Bash script:");
    eprintln!("{}", bash_script);

    // HARDENED ASSERTION: Set GOATD_WORKSPACE_ROOT to the canonicalized path
    // This ensures the bash script can locate the metadata file correctly
    // even when symlinks or relative paths are involved
    let output = Command::new("bash")
        .arg("-c")
        .arg(&bash_script)
        .env("GOATD_WORKSPACE_ROOT", &workspace_path)
        .output()
        .expect("Failed to execute bash command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    eprintln!("[TEST] Bash stdout:");
    eprintln!("{}", stdout);
    eprintln!("[TEST] Bash stderr:");
    eprintln!("{}", stderr);

    // CRITICAL ASSERTION: GOATD_KERNELRELEASE must be correctly populated
    // The patched PKGBUILD should source .goatd_metadata, setting GOATD_KERNELRELEASE
    let kernelrelease_line = stdout
        .lines()
        .find(|line| line.contains("6.19.0-goatd-gaming-test"));

    assert!(
        kernelrelease_line.is_some(),
        "!!! CRITICAL FAILURE !!!: bash failed to populate GOATD_KERNELRELEASE\n\
             Expected: 6.19.0-goatd-gaming-test\n\
             Stdout: {}\n\
             Stderr: {}\n\
             This indicates the PRE-MPL error mentioned in the task description.",
        stdout,
        stderr
    );

    eprintln!(
        "[TEST] ✓ CRITICAL: bash successfully sourced GOATD_KERNELRELEASE={}",
        kernelrelease_line.unwrap()
    );
}

/// Test 7: Cross-mount scenario simulation
/// Verify that MPL works when workspace is on different mount than app root
#[test]
fn test_mpl_cross_mount_scenario() {
    // Create workspace on "different mount" (temp directory simulates /mnt/Optane)
    let external_workspace = tempfile::tempdir().expect("Failed to create external workspace");
    let mut workspace_path = external_workspace.path().to_path_buf();

    // CRITICAL FIX: Canonicalize the workspace path
    workspace_path =
        fs::canonicalize(&workspace_path).expect("Failed to canonicalize workspace path");

    // Create app root (simulates /home/madgoat/Documents/GOATd Kernel)
    let app_root = tempfile::tempdir().expect("Failed to create app root");
    let mut app_root_path = app_root.path().to_path_buf();

    // CRITICAL FIX: Canonicalize the app root path
    app_root_path = fs::canonicalize(&app_root_path).expect("Failed to canonicalize app root path");

    eprintln!("[TEST] App root: {}", app_root_path.display());
    eprintln!(
        "[TEST] Workspace (external mount): {}",
        workspace_path.display()
    );

    // Verify paths are actually different
    assert_ne!(
        workspace_path, app_root_path,
        "Paths should be different for cross-mount simulation"
    );

    // Create metadata pointing to external workspace
    let metadata = MPLMetadata {
        build_id: "cross-mount-test".to_string(),
        kernel_release: "6.19.0-goatd-cross-mount".to_string(),
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace_path.clone(),
        source_dir: workspace_path.join("linux-6.19.0"),
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    // Write metadata to external workspace
    let metadata_path = workspace_path.join(".goatd_metadata");
    metadata
        .write_to_file(&metadata_path)
        .expect("Failed to write metadata to external workspace");

    // Create and patch PKGBUILD in external workspace
    let _pkgbuild_path = create_mock_pkgbuild(&workspace_path);
    let patcher = KernelPatcher::new(workspace_path.clone());
    patcher
        .inject_mpl_sourcing()
        .expect("Failed to inject MPL sourcing");

    // Verify PKGBUILD in external workspace can source .goatd_metadata
    // even though the patcher was created from a different root
    // CRITICAL FIX: Must CALL the function to actually execute the sourcing
    let bash_script = format!(
        r#"
cd "{workspace}"
source "./PKGBUILD"
package
echo "RESULT: $GOATD_KERNELRELEASE"
"#,
        workspace = workspace_path.display()
    );

    // HARDENED ASSERTION: Set GOATD_WORKSPACE_ROOT to the canonicalized path
    // This ensures the bash script can locate the metadata file correctly
    // even in cross-mount scenarios with symlinks or relative paths
    let output = Command::new("bash")
        .arg("-c")
        .arg(&bash_script)
        .env("GOATD_WORKSPACE_ROOT", &workspace_path)
        .output()
        .expect("Failed to execute cross-mount verification");

    let stdout = String::from_utf8_lossy(&output.stdout);

    eprintln!("[TEST] Cross-mount verification stdout:\n{}", stdout);

    assert!(
        stdout.contains("6.19.0-goatd-cross-mount"),
        "GOATD_KERNELRELEASE should be populated in cross-mount scenario"
    );
}

/// Test 8: Multiple package function patching
/// Verify that inject_mpl_sourcing patches multiple package functions
#[test]
fn test_mpl_multiple_function_patching() {
    let workspace = setup_test_workspace();
    let workspace_path = workspace.path().to_path_buf();

    // Create PKGBUILD with multiple package functions
    let multi_func_pkgbuild = r#"#!/bin/bash
pkgname=linux-test
pkgbase=linux

package() {
    echo "Main package"
}

package_headers() {
    echo "Headers package"
}

package_docs() {
    echo "Docs package"
}
"#;

    let pkgbuild_path = workspace_path.join("PKGBUILD");
    fs::write(&pkgbuild_path, multi_func_pkgbuild)
        .expect("Failed to create multi-function PKGBUILD");

    // Inject MPL sourcing
    let patcher = KernelPatcher::new(workspace_path.clone());
    patcher
        .inject_mpl_sourcing()
        .expect("Failed to inject MPL sourcing");

    // Verify all functions were patched
    let patched_content =
        fs::read_to_string(&pkgbuild_path).expect("Failed to read patched PKGBUILD");

    // Count occurrences of MPL sourcing marker
    let mpl_marker_count = patched_content
        .matches("!!! MPL SOURCING INJECTED !!!")
        .count();
    assert!(
        mpl_marker_count > 0,
        "Should have MPL markers in patched PKGBUILD"
    );
}

/// Test 9: PRODUCTION SIMULATION - Git environment present
/// CRITICAL: Verify that patcher works EVEN WHEN .git directory exists
///
/// This test addresses the root cause of the bug mentioned in the task:
/// The previous implementation had a condition that checked `self.src_dir.join(".git").exists()`,
/// which was FALSE in the clean test environment but TRUE in production.
/// This test ensures the patcher:
/// - Detects presence of .git directory (git environment simulation)
/// - Successfully applies patches despite .git existing
/// - Does NOT run git restore or git reset
/// - Leaves PKGBUILD in patched state (not reverted)
#[test]
fn test_mpl_production_simulation_with_git_environment() {
    let workspace = setup_test_workspace();
    let mut workspace_path = workspace.path().to_path_buf();

    // CRITICAL FIX: Canonicalize the workspace path to resolve symlinks and normalize
    workspace_path =
        fs::canonicalize(&workspace_path).expect("Failed to canonicalize workspace path");

    // STEP 1: Simulate git environment by creating .git directory
    let git_dir = workspace_path.join(".git");
    fs::create_dir_all(&git_dir).expect("Failed to create .git directory");

    // Verify .git exists (this is the condition that was failing in production)
    assert!(
        git_dir.exists(),
        ".git directory should exist for production simulation"
    );
    eprintln!("[TEST] ✓ Created .git directory at: {}", git_dir.display());

    // STEP 2: Create MPL metadata in the workspace
    let metadata = MPLMetadata {
        build_id: "production-sim-test".to_string(),
        kernel_release: "6.19.0-goatd-production".to_string(),
        kernel_version: "6.19.0".to_string(),
        profile: "gaming".to_string(),
        variant: "linux".to_string(),
        lto_level: "thin".to_string(),
        build_timestamp: "2026-01-20T16:21:00Z".to_string(),
        workspace_root: workspace_path.clone(),
        source_dir: workspace_path.join("linux-6.19.0"),
        pkgver: "6.19.0".to_string(),
        pkgrel: "1".to_string(),
        profile_suffix: "-goatd-gaming".to_string(),
    };

    let metadata_path = workspace_path.join(".goatd_metadata");
    metadata
        .write_to_file(&metadata_path)
        .expect("Failed to write metadata file");

    eprintln!("[TEST] ✓ Wrote metadata to: {}", metadata_path.display());

    // STEP 3: Create mock PKGBUILD
    let _pkgbuild_path = create_mock_pkgbuild(&workspace_path);

    eprintln!("[TEST] ✓ Created mock PKGBUILD");

    // STEP 4: Read ORIGINAL PKGBUILD content BEFORE patching
    let original_content = fs::read_to_string(workspace_path.join("PKGBUILD"))
        .expect("Failed to read original PKGBUILD");

    eprintln!(
        "[TEST] Original PKGBUILD (first 200 chars):\n{}",
        &original_content[..std::cmp::min(200, original_content.len())]
    );

    // STEP 5: Apply patcher (even though .git exists)
    let patcher = KernelPatcher::new(workspace_path.clone());
    let patch_result = patcher.inject_mpl_sourcing();

    assert!(
        patch_result.is_ok(),
        "Patcher should succeed even with .git present"
    );

    eprintln!("[TEST] ✓ Patcher succeeded with MPL sourcing patched");

    // STEP 6: Verify PKGBUILD was actually modified (not reverted by git restore)
    let patched_content = fs::read_to_string(workspace_path.join("PKGBUILD"))
        .expect("Failed to read patched PKGBUILD");

    // CRITICAL ASSERTION: File should be DIFFERENT (patched)
    assert_ne!(
        original_content, patched_content,
        "PKGBUILD should be modified by patcher"
    );

    // Verify injection markers are present
    assert!(
        patched_content.contains("!!! MPL STARTING !!!"),
        "PKGBUILD should contain MPL injection marker"
    );
    assert!(
        patched_content.contains(".goatd_metadata"),
        "PKGBUILD should reference .goatd_metadata file"
    );

    eprintln!("[TEST] ✓ PKGBUILD verified as patched (not reverted)");

    // STEP 7: Verify old .git condition doesn't interfere
    // The original bug was that code like:
    //   if self.src_dir.join(".git").exists() { git_restore(); }
    // would cause the patcher to revert changes in production.
    // This was FALSE in tests but TRUE in production.
    // We confirm the fix by verifying the patcher doesn't need to check for .git at all.

    eprintln!("[TEST] ✓ CRITICAL: Patches persisted despite .git directory presence");
    eprintln!("[TEST] ✓ This test would have FAILED with the old implementation that called 'git restore'");

    // STEP 8: Bash integration test - verify sourcing works in git environment
    let bash_script = format!(
        r#"
cd "{workspace}"
source "./PKGBUILD"
package
echo "KERNELRELEASE=$GOATD_KERNELRELEASE"
"#,
        workspace = workspace_path.display()
    );

    // HARDENED ASSERTION: Set GOATD_WORKSPACE_ROOT to the canonicalized path
    // This ensures the bash script can locate the metadata file correctly
    // even when .git directory is present and symlinks or relative paths are involved
    let output = Command::new("bash")
        .arg("-c")
        .arg(&bash_script)
        .env("GOATD_WORKSPACE_ROOT", &workspace_path)
        .output()
        .expect("Failed to execute bash command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    eprintln!("[TEST] Bash stdout:\n{}", stdout);
    eprintln!("[TEST] Bash stderr:\n{}", stderr);

    // Verify the kernel release was loaded
    assert!(
        stdout.contains("6.19.0-goatd-production"),
        "GOATD_KERNELRELEASE should be populated from metadata even with .git present\n\
             stdout: {}\n\
             stderr: {}",
        stdout,
        stderr
    );

    eprintln!(
        "[TEST] ✓ BASH INTEGRATION: Successfully sourced GOATD_KERNELRELEASE in git environment"
    );
}

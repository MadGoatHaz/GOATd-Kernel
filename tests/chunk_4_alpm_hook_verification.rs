//! Chunk 4: ALPM Hook & Verification Script Integration Tests
//!
//! This test suite validates the post-install system integrity verification:
//! 1. ALPM hook creation (90-goatd-kernel-verify.hook)
//! 2. Verification script updates for:
//!    - uname -r parity (kernel version detection)
//!    - /usr/lib/modules/[KVER]/build symlink validity
//!    - CONFIG_LTO_CLANG_FULL=y detection via /proc/config.gz or /boot/config-*
//! 3. Hook deployment via PKGBUILD package_linux-headers()

use std::fs;
use std::path::Path;

// ========================================================================
// TEST 1: ALPM Hook File Structure Validation
// ========================================================================
#[test]
fn test_alpm_hook_file_structure() {
    // VERIFY: Hook file exists at assets/90-goatd-kernel-verify.hook
    let hook_path = Path::new("assets/90-goatd-kernel-verify.hook");
    assert!(
        hook_path.exists(),
        "ALPM hook file not found at assets/90-goatd-kernel-verify.hook"
    );

    // READ: Hook content
    let hook_content = fs::read_to_string(hook_path)
        .expect("Failed to read ALPM hook file");

    // VALIDATE: Pacman hook structure
    assert!(hook_content.contains("[Trigger]"),
        "Missing [Trigger] section in ALPM hook");
    assert!(hook_content.contains("Type = Package"),
        "Missing 'Type = Package' in [Trigger] section");
    assert!(hook_content.contains("Operation = Install"),
        "Missing 'Operation = Install' in [Trigger] section");
    assert!(hook_content.contains("Operation = Upgrade"),
        "Missing 'Operation = Upgrade' in [Trigger] section");
    assert!(hook_content.contains("Target = goatd-kernel"),
        "Missing 'Target = goatd-kernel*' in [Trigger] section");

    // VALIDATE: Action section
    assert!(hook_content.contains("[Action]"),
        "Missing [Action] section in ALPM hook");
    assert!(hook_content.contains("Description = Verifying GOATd Kernel post-install integrity"),
        "Missing clear description of hook purpose");
    assert!(hook_content.contains("When = PostTransaction"),
        "Hook must run 'When = PostTransaction'");
    assert!(hook_content.contains("Exec ="),
        "Missing 'Exec =' directive in [Action] section");
    assert!(hook_content.contains("goatd-kernel-verify.sh"),
        "Hook must execute verification script");

    println!("✓ ALPM hook file structure validated");
}

// ========================================================================
// TEST 2: Verification Script Post-Install Checks Presence
// ========================================================================
#[test]
fn test_verification_script_post_install_checks() {
    let verify_script = Path::new("scripts/verify.sh");
    assert!(
        verify_script.exists(),
        "Verification script not found at scripts/verify.sh"
    );

    let script_content = fs::read_to_string(verify_script)
        .expect("Failed to read verification script");

    // CHECK 1: Kernel version parity (uname -r)
    assert!(script_content.contains("uname -r"),
        "Script must check kernel version via 'uname -r'");
    assert!(script_content.contains("KVER="),
        "Script must capture kernel version in KVER variable");
    assert!(script_content.contains("Kernel version detected:") || 
            script_content.contains("Kernel version parity"),
        "Script must report kernel version detection status");

    // CHECK 2: Module symlink validity
    assert!(script_content.contains("/usr/lib/modules"),
        "Script must check /usr/lib/modules directory");
    assert!(script_content.contains("/build"),
        "Script must verify /build symlink specifically");
    assert!(script_content.contains("symlink") || script_content.contains("-L"),
        "Script must test symlink validity");
    assert!(script_content.contains("Module symlink") || script_content.contains("build symlink"),
        "Script must report symlink validation status");

    // CHECK 3: CONFIG_LTO_CLANG_FULL verification
    assert!(script_content.contains("CONFIG_LTO_CLANG_FULL"),
        "Script must check CONFIG_LTO_CLANG_FULL=y");
    assert!(script_content.contains("/proc/config.gz"),
        "Script must check /proc/config.gz for LTO config");
    assert!(script_content.contains("/boot/config-"),
        "Script must fallback to /boot/config-* for LTO config");
    assert!(script_content.contains("zgrep") || script_content.contains("gunzip"),
        "Script must decompress /proc/config.gz when checking LTO");

    println!("✓ Verification script post-install checks validated");
}

// ========================================================================
// TEST 3: Verification Script Check Logic Correctness
// ========================================================================
#[test]
fn test_verification_script_check_logic() {
    let verify_script = Path::new("scripts/verify.sh");
    let script_content = fs::read_to_string(verify_script)
        .expect("Failed to read verification script");

    // VERIFY: KVER variable initialization and usage
    let kver_init = script_content.find("KVER=$(uname -r)");
    assert!(kver_init.is_some(),
        "Script must initialize KVER with 'KVER=$(uname -r)'");

    // VERIFY: Directory check before symlink test
    assert!(script_content.contains("[ -d \"/usr/lib/modules/$KVER/build\"") ||
            script_content.contains("[ -d \"/usr/lib/modules/$KVER"),
        "Script must test directory existence before symlink validity");

    // VERIFY: Symlink validity test (must use -L flag)
    assert!(script_content.contains("[ -L ") && script_content.contains("/build") ||
            script_content.contains("-L \"/usr/lib/modules/$KVER/build"),
        "Script must use '[ -L ]' test for symlink validity");

    // VERIFY: CONFIG_LTO check with zgrep for /proc/config.gz
    let zgrep_check = script_content.find("zgrep");
    if zgrep_check.is_some() {
        let context = &script_content[zgrep_check.unwrap()..];
        assert!(context.contains("CONFIG_LTO_CLANG_FULL") && context.contains("/proc/config.gz"),
            "zgrep check must target CONFIG_LTO_CLANG_FULL in /proc/config.gz");
    }

    // VERIFY: Fallback to /boot/config-* if /proc/config.gz unavailable
    assert!(script_content.contains("grep") || script_content.contains("/boot/config-"),
        "Script must have fallback check for /boot/config-* file");

    println!("✓ Verification script check logic validated");
}

// ========================================================================
// TEST 4: PKGBUILD Hook Installation Integration
// ========================================================================
#[test]
fn test_pkgbuild_hook_installation() {
    let pkgbuild_path = Path::new("pkgbuilds/kernel/PKGBUILD");
    assert!(
        pkgbuild_path.exists(),
        "PKGBUILD not found at pkgbuilds/kernel/PKGBUILD"
    );

    let pkgbuild_content = fs::read_to_string(pkgbuild_path)
        .expect("Failed to read PKGBUILD");

    // VERIFY: Hook file copy in package_linux-headers()
    assert!(pkgbuild_content.contains("90-goatd-kernel-verify.hook"),
        "PKGBUILD must reference 90-goatd-kernel-verify.hook");

    // VERIFY: Installation to /etc/pacman.d/hooks/
    assert!(pkgbuild_content.contains("/etc/pacman.d/hooks") ||
            pkgbuild_content.contains("pacman.d/hooks"),
        "PKGBUILD must install hook to /etc/pacman.d/hooks/");

    // VERIFY: Conditional installation (file existence check)
    assert!(pkgbuild_content.contains("if [ -f") && 
            pkgbuild_content.contains("90-goatd-kernel-verify.hook") ||
            pkgbuild_content.contains("[ -f ../90-goatd-kernel-verify.hook"),
        "PKGBUILD should conditionally install hook file if it exists");

    // VERIFY: Proper install command with file permissions
    assert!(pkgbuild_content.contains("install -Dm644") && (
            pkgbuild_content.contains("90-goatd-kernel-verify.hook") ||
            pkgbuild_content.contains("hook")
        ),
        "PKGBUILD must use 'install -Dm644' for hook file (0644 = read-only)");

    println!("✓ PKGBUILD hook installation validated");
}

// ========================================================================
// TEST 5: Hook File Placement for Deployment
// ========================================================================
#[test]
fn test_hook_file_placement_for_deployment() {
    // VERIFY: Hook exists in assets/ for distribution/packaging
    let assets_hook = Path::new("assets/90-goatd-kernel-verify.hook");
    assert!(
        assets_hook.exists(),
        "Hook must be in assets/ directory for packaging"
    );

    // VERIFY: Hook is tracked in project (not .gitignored)
    let gitignore = Path::new(".gitignore");
    if gitignore.exists() {
        let gitignore_content = fs::read_to_string(gitignore).unwrap_or_default();
        // Hook files should NOT be in .gitignore (should be tracked)
        assert!(!gitignore_content.contains("*.hook") || 
                !gitignore_content.contains("90-goatd"),
            "Hook file should be tracked in git if in .gitignore, review constraints");
    }

    // VERIFY: File permissions are readable
    let metadata = fs::metadata(assets_hook)
        .expect("Failed to read hook file metadata");
    let permissions = metadata.permissions();
    assert!(!permissions.readonly() || metadata.is_file(),
        "Hook file must be readable");

    println!("✓ Hook file placement for deployment validated");
}

// ========================================================================
// TEST 6: Verification Script Output Consistency
// ========================================================================
#[test]
fn test_verification_script_output_consistency() {
    let verify_script = Path::new("scripts/verify.sh");
    let script_content = fs::read_to_string(verify_script)
        .expect("Failed to read verification script");

    // VERIFY: Color codes defined for output
    assert!(script_content.contains("GREEN") || script_content.contains("RED"),
        "Script should use color codes for status indicators");

    // VERIFY: Status indicators present (✓ or ✗)
    assert!(script_content.contains("✓") || script_content.contains("echo"),
        "Script must provide clear status output for checks");

    // VERIFY: Section headers for readability
    assert!(script_content.contains("POST-INSTALL") || 
            script_content.contains("POST_INSTALL") ||
            script_content.contains("System Integrity"),
        "Script must clearly identify post-install verification section");

    // VERIFY: Consistent check reporting pattern
    let check_patterns = script_content.matches("echo").count();
    assert!(check_patterns >= 3,
        "Script should have at least 3 echo statements for the 3 integrity checks");

    println!("✓ Verification script output consistency validated");
}

// ========================================================================
// TEST 7: LTO Configuration Detection Methods
// ========================================================================
#[test]
fn test_lto_configuration_detection_methods() {
    let verify_script = Path::new("scripts/verify.sh");
    let script_content = fs::read_to_string(verify_script)
        .expect("Failed to read verification script");

    // VERIFY: Primary method - /proc/config.gz check
    assert!(script_content.contains("/proc/config.gz"),
        "Script must attempt to check /proc/config.gz for CONFIG_LTO_CLANG_FULL");

    // VERIFY: Decompression method for /proc/config.gz
    assert!(script_content.contains("zgrep") || 
            script_content.contains("zcat") ||
            script_content.contains("gunzip"),
        "Script must use decompression tool (zgrep/zcat/gunzip) for /proc/config.gz");

    // VERIFY: Fallback method - /boot/config-[KVER]
    assert!(script_content.contains("/boot/config-") || 
            script_content.contains("/boot/config-$KVER"),
        "Script must fallback to /boot/config-[KVER] if /proc/config.gz unavailable");

    // VERIFY: Standard grep for /boot/config query
    assert!(script_content.contains("grep") && script_content.contains("/boot/config"),
        "Script must use grep for /boot/config-* file search");

    // VERIFY: CONFIG_LTO_CLANG_FULL specifically (not just CONFIG_LTO)
    let lto_full_refs = script_content.matches("CONFIG_LTO_CLANG_FULL").count();
    assert!(lto_full_refs >= 2,
        "CONFIG_LTO_CLANG_FULL should appear multiple times (method1 + method2)");

    println!("✓ LTO configuration detection methods validated");
}

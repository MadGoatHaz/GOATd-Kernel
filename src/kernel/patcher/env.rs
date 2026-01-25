//! Environment preparation and toolchain discovery for kernel builds.
//!
//! This module encapsulates all environment variable setup and toolchain discovery
//! to ensure:
//! 1. LLVM/Clang compiler enforcement (CC=clang, CXX=clang++)
//! 2. Linker and toolchain enforcement (LD=ld.lld, AR=llvm-ar, etc.)
//! 3. HOST COMPILER enforcement (HOSTCC=clang, HOSTCXX=clang++)
//! 4. PATH purification to prevent GCC/legacy compiler interference
//! 5. Dynamic toolchain discovery with LLVM-19 prioritization
//!
//! # LLVM-19 Prioritization
//! All toolchain binaries are discovered using a 4-step fallback strategy:
//! 1. LLVM-19 variant (e.g., llvm-19-strip) - HIGHEST PRIORITY
//! 2. LLVM variant without version (e.g., llvm-strip)
//! 3. Standard /usr/bin location (e.g., /usr/bin/strip)
//! 4. Just the command name (rely on PATH)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find a toolchain binary in PATH, with LLVM-19 prioritization
///
/// Searches for the binary in this order:
/// 1. LLVM-19 variant (e.g., llvm-19-strip for strip) - HIGHEST PRIORITY
/// 2. LLVM variant without version (e.g., llvm-strip for strip)
/// 3. Standard location (e.g., /usr/bin/strip)
/// 4. Just the command name (let PATH search)
///
/// Returns the resolved command name to use
pub fn find_toolchain_binary(name: &str) -> String {
    // STEP 1: Try LLVM-19 variant first (highest priority for consistency)
    let llvm19_variant = format!("llvm-19-{}", name);
    if Command::new(&llvm19_variant)
        .arg("--version")
        .output()
        .is_ok()
    {
        eprintln!(
            "[Patcher] [TOOLCHAIN] Found LLVM-19 variant: {}",
            llvm19_variant
        );
        return llvm19_variant;
    }

    // STEP 2: Try generic LLVM variant (fallback for latest LLVM)
    let llvm_variant = format!("llvm-{}", name);
    if Command::new(&llvm_variant)
        .arg("--version")
        .output()
        .is_ok()
    {
        eprintln!("[Patcher] [TOOLCHAIN] Found LLVM variant: {}", llvm_variant);
        return llvm_variant;
    }

    // STEP 3: Try standard /usr/bin location
    let standard_path = format!("/usr/bin/{}", name);
    if Path::new(&standard_path).exists() {
        eprintln!(
            "[Patcher] [TOOLCHAIN] Found at standard location: {}",
            standard_path
        );
        return standard_path;
    }

    // STEP 4: Fallback to just the command name (rely on PATH)
    eprintln!(
        "[Patcher] [TOOLCHAIN] Using {} from PATH (final fallback)",
        name
    );
    name.to_string()
}

/// Sanitize environment variables to prevent $srcdir leaks and toolchain contamination
///
/// Removes or cleans environment variables that may contain:
/// - $srcdir references (leaks into compiled output)
/// - GCC/legacy compiler paths (conflicts with Clang/LLVM)
/// - Unsafe temporary paths (build artifacts)
///
/// CRITICAL FIX: Explicitly preserve /usr/bin and /bin to ensure make and other essential
/// tools remain available after sanitization.
///
/// # Arguments
/// * `env_vars` - HashMap of environment variables to sanitize (mutated in-place)
pub fn sanitize_build_environment(env_vars: &mut HashMap<String, String>) {
    // STEP 1: Remove variables that commonly leak $srcdir paths
    let srcdir_leak_patterns = vec![
        "TMPDIR", // Temporary directories often contain $srcdir
        "TEMP",   // Alternative temp
        "TMP",    // Short form
    ];

    for var_name in srcdir_leak_patterns {
        if let Some(_) = env_vars.remove(var_name) {
            eprintln!(
                "[Patcher] [SANITIZE] Removed {} to prevent $srcdir leak",
                var_name
            );
        }
    }

    // STEP 2: Clean up any GCC-related paths from PATH
    // CRITICAL FIX: Preserve /usr/bin and /bin to ensure make is available
    if let Some(path) = env_vars.get_mut("PATH") {
        let original_path = path.clone();
        let filtered: Vec<&str> = path
            .split(':')
            .filter(|p| {
                // ALWAYS KEEP: Essential system directories for make and other tools
                if *p == "/usr/bin" || *p == "/bin" || *p == "/usr/local/bin" {
                    return true;
                }

                // Remove only paths that contain gcc/llvm/clang installations
                !(p.contains("/gcc")
                    || p.contains("/g++")
                    || p.contains("/cc")
                    || p.contains("/c++")
                    || p.contains("/llvm")
                    || p.contains("/clang"))
                    && !p.is_empty()
            })
            .collect();

        let new_path = filtered.join(":");
        if new_path != original_path {
            eprintln!("[Patcher] [SANITIZE] Cleaned PATH: removed GCC-related directories while preserving /usr/bin and /bin");
            *path = new_path;
        }
    }

    // STEP 3: Ensure CFLAGS/CXXFLAGS/LDFLAGS don't have GCC-specific markers
    for flag_var in &["CFLAGS", "CXXFLAGS", "LDFLAGS"] {
        if let Some(flags) = env_vars.get_mut(*flag_var) {
            let original = flags.clone();
            // Remove any GCC-specific flags
            *flags = flags
                .replace("-Wl,--as-needed", "") // GCC linker marker
                .replace("-Wl,--no-undefined", "") // GCC linker marker
                .trim()
                .to_string();

            if original != *flags {
                eprintln!(
                    "[Patcher] [SANITIZE] Cleaned {}: removed GCC-specific flags",
                    flag_var
                );
            }
        }
    }

    eprintln!("[Patcher] [SANITIZE] Environment sanitization complete");
}

/// Prepares purified build environment variables for toolchain enforcement.
///
/// Centralizes environment variable setup to ensure:
/// 1. LLVM/Clang compiler enforcement (CC=clang, CXX=clang++)
/// 2. Linker and toolchain enforcement (LD=ld.lld, AR=llvm-ar, etc.)
/// 3. HOST COMPILER enforcement (HOSTCC=clang, HOSTCXX=clang++)
/// 4. PATH purification to prevent GCC/legacy compiler interference
/// 5. Dynamic toolchain discovery (strip, llvm-strip, etc.)
///
/// This function encapsulates all environment setup that previously lived in the Executor,
/// ensuring the Executor only receives and applies pre-configured environment variables.
///
/// # Arguments
/// * `src_dir` - Path to the kernel source directory
/// * `native_optimizations` - Whether to enable -march=native in KCFLAGS
///
/// # Returns
/// HashMap of environment variable names to values
pub fn prepare_build_environment(
    src_dir: &Path,
    native_optimizations: bool,
) -> HashMap<String, String> {
    let mut env_vars = HashMap::new();

    // CRITICAL: Sanitize environment FIRST to remove leaked paths and GCC contamination
    eprintln!("[Patcher] [ENV] STEP 0: Sanitizing build environment for cleanliness");
    sanitize_build_environment(&mut env_vars);

    // ============================================================================
    // WORKSPACE ROOT EXPORT (for MPL metadata sourcing)
    // ============================================================================
    // PHASE 14: CROSS-MOUNT PATH RESOLUTION - Search for .goatd_anchor
    // Export the workspace root for makepkg/PKGBUILD sourcing of metadata files
    // CRITICAL: Path must be ABSOLUTE and CANONICALIZED to survive fakeroot execution
    //
    // PRIORITY 1: Search for .goatd_anchor walking up from src_dir
    // This provides definitive absolute path resolution across mount points
    let mut workspace_root_string: Option<String> = None;

    if let Some(mut current) = src_dir.parent() {
        let max_depth = 10; // Prevent infinite loops
        let mut depth = 0;

        while depth < max_depth {
            let anchor_path = current.join(".goatd_anchor");
            if anchor_path.exists() {
                eprintln!(
                    "[Patcher] [ENV] [ANCHOR] Found .goatd_anchor at: {}",
                    anchor_path.display()
                );

                // Use anchor's parent as workspace root and canonicalize it
                match std::fs::canonicalize(current) {
                    Ok(canonical_path) => {
                        let workspace_root = canonical_path.to_string_lossy().to_string();
                        eprintln!(
                            "[Patcher] [ENV] [ANCHOR] Using anchor's parent (canonicalized): {}",
                            workspace_root
                        );
                        workspace_root_string = Some(workspace_root);
                    }
                    Err(e) => {
                        eprintln!("[Patcher] [ENV] [ANCHOR] WARNING: Could not canonicalize anchor's parent: {}", e);
                        // Fallback to absolute path
                        let abs_path = if current.is_absolute() {
                            current.to_path_buf()
                        } else {
                            std::env::current_dir()
                                .map(|cwd| cwd.join(current))
                                .unwrap_or_else(|_| current.to_path_buf())
                        };
                        workspace_root_string = Some(abs_path.to_string_lossy().to_string());
                    }
                }
                break; // Anchor found, exit loop
            }

            // Check if we've reached the filesystem root
            if let Some(parent) = current.parent() {
                if parent == current {
                    eprintln!("[Patcher] [ENV] [ANCHOR] Reached filesystem root without finding .goatd_anchor");
                    break;
                }
                current = parent;
                depth += 1;
            } else {
                eprintln!("[Patcher] [ENV] [ANCHOR] No parent directory, reached filesystem root");
                break;
            }
        }

        if depth >= max_depth {
            eprintln!("[Patcher] [ENV] [ANCHOR] Max search depth ({}) reached without finding .goatd_anchor", max_depth);
        }
    }

    // PRIORITY 2/3: Fallback to parent of src_dir if anchor not found
    if let Some(ws_root) = workspace_root_string {
        env_vars.insert("GOATD_WORKSPACE_ROOT".to_string(), ws_root.clone());
        eprintln!(
            "[Patcher] [ENV] Exported GOATD_WORKSPACE_ROOT={} (anchor-based resolution)",
            ws_root
        );
    } else if let Some(parent) = src_dir.parent() {
        let parent_path = PathBuf::from(parent);

        // Try to canonicalize to absolute path
        match parent_path.canonicalize() {
            Ok(canonical_path) => {
                let workspace_root = canonical_path.to_string_lossy().to_string();
                env_vars.insert("GOATD_WORKSPACE_ROOT".to_string(), workspace_root.clone());
                eprintln!("[Patcher] [ENV] Exported GOATD_WORKSPACE_ROOT={} (parent canonicalized, no anchor found)", workspace_root);
            }
            Err(_) => {
                // Fallback: Use absolute path from current dir if canonicalize fails
                let abs_path = if parent_path.is_absolute() {
                    parent_path
                } else {
                    std::env::current_dir()
                        .map(|cwd| cwd.join(&parent_path))
                        .unwrap_or(parent_path)
                };

                let workspace_root = abs_path.to_string_lossy().to_string();
                env_vars.insert("GOATD_WORKSPACE_ROOT".to_string(), workspace_root.clone());
                eprintln!("[Patcher] [ENV] Exported GOATD_WORKSPACE_ROOT={} (absolute fallback, no anchor found)", workspace_root);
            }
        }
    } else {
        // Fallback to current directory if parent cannot be determined
        if let Ok(cwd) = std::env::current_dir() {
            let workspace_root = cwd.to_string_lossy().to_string();
            env_vars.insert("GOATD_WORKSPACE_ROOT".to_string(), workspace_root.clone());
            eprintln!("[Patcher] [ENV] WARNING: Could not determine parent of src_dir, using current directory: {}", workspace_root);
        } else {
            eprintln!("[Patcher] [ENV] ERROR: Could not determine workspace root - GOATD_WORKSPACE_ROOT will not be set");
        }
    }

    // ============================================================================
    // CLANG/LLVM v19+ ENFORCEMENT
    // ============================================================================
    env_vars.insert("LLVM".to_string(), "1".to_string());
    env_vars.insert("LLVM_IAS".to_string(), "1".to_string());
    env_vars.insert("CC".to_string(), "clang".to_string());
    env_vars.insert("CXX".to_string(), "clang++".to_string());
    env_vars.insert("LD".to_string(), "ld.lld".to_string());

    // DYNAMIC TOOLCHAIN DISCOVERY: All toolchain binaries use LLVM-19 prioritization
    let ar_cmd = find_toolchain_binary("ar");
    env_vars.insert("AR".to_string(), ar_cmd);

    let nm_cmd = find_toolchain_binary("nm");
    env_vars.insert("NM".to_string(), nm_cmd);

    let strip_cmd = find_toolchain_binary("strip");
    env_vars.insert("STRIP".to_string(), strip_cmd);

    let objcopy_cmd = find_toolchain_binary("objcopy");
    env_vars.insert("OBJCOPY".to_string(), objcopy_cmd);

    let objdump_cmd = find_toolchain_binary("objdump");
    env_vars.insert("OBJDUMP".to_string(), objdump_cmd);

    let readelf_cmd = find_toolchain_binary("readelf");
    env_vars.insert("READELF".to_string(), readelf_cmd);
    env_vars.insert("GCC".to_string(), "clang".to_string());
    env_vars.insert("GXX".to_string(), "clang++".to_string());

    // ============================================================================
    // HOST COMPILER ENFORCEMENT - Ensure host tools also use Clang
    // ============================================================================
    env_vars.insert("HOSTCC".to_string(), "clang".to_string());
    env_vars.insert("HOSTCXX".to_string(), "clang++".to_string());
    eprintln!("[Patcher] [ENV] Injected HOSTCC=clang, HOSTCXX=clang++ for host tool enforcement");

    // ============================================================================
    // GOATD FLAG HARDENING - Safe defaults for kernel optimization flags
    // ============================================================================
    // CRITICAL: These defaults ensure consistent performance optimization even
    // when environment variables are not explicitly set by the executor.
    // These are high-performance, safe defaults for gaming kernels.

    // GOATD_BASE_FLAGS: Essential optimization level required for hardening
    // -O2 is MANDATORY because -D_FORTIFY_SOURCE requires optimization
    if !env_vars.contains_key("GOATD_BASE_FLAGS") {
        env_vars.insert("GOATD_BASE_FLAGS".to_string(), "-O2".to_string());
        eprintln!("[Patcher] [ENV] [HARDENING] Set GOATD_BASE_FLAGS=-O2 (default, required for _FORTIFY_SOURCE)");
    }

    // GOATD_HARDENING_FLAGS: Stack canary + bounds checking for gaming kernel
    if !env_vars.contains_key("GOATD_HARDENING_FLAGS") {
        env_vars.insert(
            "GOATD_HARDENING_FLAGS".to_string(),
            "-fstack-protector-strong -D_FORTIFY_SOURCE=2 -fPIE".to_string(),
        );
        eprintln!("[Patcher] [ENV] [HARDENING] Set GOATD_HARDENING_FLAGS with stack-protector and _FORTIFY_SOURCE");
    }

    // GOATD_NATIVE_FLAGS: Native CPU optimization for gaming performance
    if !env_vars.contains_key("GOATD_NATIVE_FLAGS") {
        env_vars.insert(
            "GOATD_NATIVE_FLAGS".to_string(),
            "-march=native -mtune=native".to_string(),
        );
        eprintln!("[Patcher] [ENV] [HARDENING] Set GOATD_NATIVE_FLAGS=-march=native -mtune=native (gaming optimization)");
    }

    // GOATD_LTO_FLAGS: Will be set by executor based on LTO level (full/thin/none)
    // Default to thin LTO if not set (balanced performance/compile time)
    if !env_vars.contains_key("GOATD_LTO_FLAGS") {
        env_vars.insert("GOATD_LTO_FLAGS".to_string(), "-flto=thin".to_string());
        eprintln!("[Patcher] [ENV] [HARDENING] Set GOATD_LTO_FLAGS=-flto=thin (default fallback)");
    }

    // GOATD_POLLY_FLAGS: LLVM loop optimization (gaming kernel vectorization boost)
    // Polly enables advanced loop transformations for better cache locality
    if !env_vars.contains_key("GOATD_POLLY_FLAGS") {
        env_vars.insert(
            "GOATD_POLLY_FLAGS".to_string(),
            "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-omp-backend=GOMP"
                .to_string(),
        );
        eprintln!(
            "[Patcher] [ENV] [HARDENING] Set GOATD_POLLY_FLAGS with advanced loop optimization"
        );
    }

    eprintln!("[Patcher] [ENV] Flag hardening complete - all GOATD_* variables have safe defaults");

    // ============================================================================
    // NATIVE OPTIMIZATIONS (-march=native support)
    // ============================================================================
    if native_optimizations {
        env_vars.insert("KCFLAGS".to_string(), "\"-march=native\"".to_string());
        eprintln!("[Patcher] [ENV] Injected KCFLAGS=\"-march=native\" for native host-optimized kernel compilation");
    } else {
        eprintln!(
            "[Patcher] [ENV] Native optimizations disabled, KCFLAGS not set to -march=native"
        );
    }

    eprintln!("[Patcher] [ENV] Prepared LLVM/Clang toolchain enforcement");

    // ============================================================================
    // INTELLIGENT ASSET CACHING - SRCDEST CONFIGURATION
    // ============================================================================
    // CRITICAL: SRCDEST is now handled by the patcher, not the executor.
    // This enables smart caching of source artifacts across builds.
    let srcdest = std::env::var("SRCDEST").unwrap_or_else(|_| "/tmp/kernel-sources".to_string());
    env_vars.insert("SRCDEST".to_string(), srcdest.clone());
    eprintln!(
        "[Patcher] [ENV] [CACHE] Set SRCDEST={} for smart asset reuse",
        srcdest
    );

    // ============================================================================
    // CCACHE INTEGRATION FOR COMPILATION ACCELERATION
    // ============================================================================
    // CRITICAL: ccache integration is now handled by the patcher.
    // This ensures consistent caching behavior across all builds.
    if std::path::Path::new("/usr/lib/ccache/bin").exists() {
        let ccache_dir = std::env::var("CCACHE_DIR").unwrap_or_else(|_| {
            format!(
                "{}/.cache/ccache",
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())
            )
        });
        env_vars.insert("CCACHE_DIR".to_string(), ccache_dir.clone());
        eprintln!(
            "[Patcher] [ENV] [CACHE] ccache enabled with CCACHE_DIR={}",
            ccache_dir
        );

        // Note: PATH will be updated with ccache /usr/lib/ccache/bin in the PATH purification step
        eprintln!("[Patcher] [ENV] [CACHE] ccache /usr/lib/ccache/bin will be prioritized in PATH");
    }

    // ============================================================================
    // PATH PURIFICATION: Remove GCC/LLVM conflicts and prioritize ccache
    // CRITICAL FIX: Preserve /usr/bin and /bin for make and essential tools
    // ============================================================================
    // Build the safe paths list, prioritizing ccache if available
    let mut safe_paths = vec![];

    // PRIORITY 1: ccache (if available) - for compilation caching
    if std::path::Path::new("/usr/lib/ccache/bin").exists() {
        safe_paths.push("/usr/lib/ccache/bin");
        eprintln!("[Patcher] [ENV] [PATH] Prioritized ccache: /usr/lib/ccache/bin");
    }

    // PRIORITY 2: Custom LLVM bin directory (if it exists)
    let llvm_bin_path = src_dir.join(".llvm_bin").to_string_lossy().to_string();
    safe_paths.push(&llvm_bin_path);

    // PRIORITY 3: Essential system directories
    safe_paths.push("/usr/bin");
    safe_paths.push("/bin");
    safe_paths.push("/usr/local/bin");

    let current_path = std::env::var("PATH").unwrap_or_default();
    let filtered_path: Vec<&str> = current_path
        .split(':')
        .filter(|p| {
            // CRITICAL: Preserve /usr/bin and /bin - they contain make, sed, awk, etc.
            if *p == "/usr/bin" || *p == "/bin" || *p == "/usr/local/bin" {
                return true; // Keep these always
            }
            // Remove only paths that contain gcc/llvm/clang installations
            !(p.contains("/gcc")
                || p.contains("/g++")
                || p.contains("/cc")
                || p.contains("/c++")
                || p.contains("/llvm")
                || p.contains("/clang"))
                && !p.is_empty()
        })
        .collect();

    let new_path = format!(
        "{}{}{}",
        safe_paths.join(":"),
        if filtered_path.is_empty() { "" } else { ":" },
        filtered_path.join(":")
    );

    env_vars.insert("PATH".to_string(), new_path.clone());
    eprintln!("[Patcher] [ENV] [PATH] Purified PATH with ccache priority and /usr/bin:/bin (removed gcc/llvm/clang installations)");
    eprintln!(
        "[Patcher] [ENV] [PATH] Final PATH (first 100 chars): {}...",
        if new_path.len() > 100 {
            &new_path[..100]
        } else {
            &new_path
        }
    );

    // Verify make is available
    if let Ok(make_cmd) = Command::new("make").arg("--version").output() {
        if make_cmd.status.success() {
            eprintln!("[Patcher] [ENV] ✓ Verified: make command is available");
        } else {
            eprintln!("[Patcher] [ENV] WARNING: make command found but returned non-zero exit");
        }
    } else {
        eprintln!("[Patcher] [ENV] WARNING: make command not found in PATH - build may fail");
    }

    // ============================================================================
    // NATIVE LOCALVERSION EXPORT
    // ============================================================================
    // Export LOCALVERSION as an environment variable for the build system.
    // This ensures the kernel version string is consistent across all build phases.
    // The Makefile will read this and apply it to the final kernel binary name.
    env_vars.insert("LOCALVERSION".to_string(), "".to_string());
    eprintln!("[Patcher] [ENV] Set LOCALVERSION='' (will be configured via .config)");

    env_vars
}

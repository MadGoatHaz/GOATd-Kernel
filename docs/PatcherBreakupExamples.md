examples:
-----
env.rs
-----
Below is some information that was provided from a third party source, do not copy this directly, use it as information only and then adapt it to our project:
-----
EXAMPLE:

//! Build Environment Management Module
//!
//! Responsible for:
//! - Discovering LLVM toolchain binaries (prioritizing LLVM-19)
//! - Sanitizing environment variables to prevent leakage (TMPDIR, etc.)
//! - Constructing the purified PATH for the build process
//! - Setting up strict compiler flags (CC, CXX, LD)

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::env;

/// Prepares purified build environment variables for toolchain enforcement.
///
/// Centralizes environment variable setup to ensure:
/// 1. LLVM/Clang compiler enforcement (CC=clang, CXX=clang++)
/// 2. Linker and toolchain enforcement (LD=ld.lld, AR=llvm-ar, etc.)
/// 3. HOST COMPILER enforcement (HOSTCC=clang, HOSTCXX=clang++)
/// 4. PATH purification to prevent GCC/legacy compiler interference
/// 5. Dynamic toolchain discovery (strip, llvm-strip, etc.)
///
/// # Arguments
/// * `src_dir` - The root directory of the kernel source (used to locate .llvm_bin)
/// * `native_optimizations` - Whether to enable -march=native in KCFLAGS
pub fn prepare_build_environment(src_dir: &Path, native_optimizations: bool) -> HashMap<String, String> {
    let mut env_vars = HashMap::new();
    
    // CRITICAL: Sanitize environment FIRST to remove leaked paths and GCC contamination
    eprintln!("[Patcher] [ENV] STEP 0: Sanitizing build environment for cleanliness");
    sanitize_build_environment(&mut env_vars);

    // ============================================================================
    // CLANG/LLVM v19+ ENFORCEMENT
    // ============================================================================
    env_vars.insert("LLVM".to_string(), "1".to_string());
    env_vars.insert("LLVM_IAS".to_string(), "1".to_string());
    env_vars.insert("CC".to_string(), "clang".to_string());
    env_vars.insert("CXX".to_string(), "clang++".to_string());
    env_vars.insert("LD".to_string(), "ld.lld".to_string());
    
    // DYNAMIC TOOLCHAIN DISCOVERY: All toolchain binaries use LLVM-19 prioritization
    env_vars.insert("AR".to_string(), find_toolchain_binary("ar"));
    env_vars.insert("NM".to_string(), find_toolchain_binary("nm"));
    env_vars.insert("STRIP".to_string(), find_toolchain_binary("strip"));
    env_vars.insert("OBJCOPY".to_string(), find_toolchain_binary("objcopy"));
    env_vars.insert("OBJDUMP".to_string(), find_toolchain_binary("objdump"));
    env_vars.insert("READELF".to_string(), find_toolchain_binary("readelf"));
    
    // Explicitly override GCC variables to Clang to prevent fallback
    env_vars.insert("GCC".to_string(), "clang".to_string());
    env_vars.insert("GXX".to_string(), "clang++".to_string());

    // ============================================================================
    // HOST COMPILER ENFORCEMENT - Ensure host tools also use Clang
    // ============================================================================
    env_vars.insert("HOSTCC".to_string(), "clang".to_string());
    env_vars.insert("HOSTCXX".to_string(), "clang++".to_string());
    eprintln!("[Patcher] [ENV] Injected HOSTCC=clang, HOSTCXX=clang++ for host tool enforcement");

    // ============================================================================
    // NATIVE OPTIMIZATIONS (-march=native support)
    // ============================================================================
    if native_optimizations {
        env_vars.insert("KCFLAGS".to_string(), "\"-march=native\"".to_string());
        eprintln!("[Patcher] [ENV] Injected KCFLAGS=\"-march=native\" for native host-optimized kernel compilation");
    } else {
        eprintln!("[Patcher] [ENV] Native optimizations disabled, KCFLAGS not set to -march=native");
    }

    eprintln!("[Patcher] [ENV] Prepared LLVM/Clang toolchain enforcement");

    // ============================================================================
    // PATH PURIFICATION: Remove LLVM-specific dirs to prevent interference
    // CRITICAL: Preserve /usr/bin and /bin for make and essential tools
    // ============================================================================
    let llvm_bin_path = src_dir.join(".llvm_bin").to_string_lossy().to_string();
    let safe_paths = vec![
        llvm_bin_path.as_str(),
        "/usr/bin",
        "/bin",
        "/usr/local/bin",
    ];

    let current_path = env::var("PATH").unwrap_or_default();
    let filtered_path: Vec<&str> = current_path
        .split(':')
        .filter(|p| {
            // CRITICAL: Preserve /usr/bin and /bin - they contain make, sed, awk, etc.
            if *p == "/usr/bin" || *p == "/bin" || *p == "/usr/local/bin" {
                return true; // Keep these always
            }
            // Remove only paths that contain gcc/llvm/clang installations
            !(p.contains("/gcc") ||
              p.contains("/g++") ||
              p.contains("/cc") ||
              p.contains("/c++") ||
              p.contains("/llvm") ||
              p.contains("/clang")) &&
            !p.is_empty()
        })
        .collect();

    let new_path = format!("{}{}{}",
        safe_paths.join(":"),
        if filtered_path.is_empty() { "" } else { ":" },
        filtered_path.join(":")
    );

    env_vars.insert("PATH".to_string(), new_path.clone());
    eprintln!("[Patcher] [ENV] Purified PATH: {} and /usr/bin:/bin (removed gcc/llvm/clang installations)", llvm_bin_path);
    
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
    // Export LOCALVERSION as an empty string to allow .config to handle it
    env_vars.insert("LOCALVERSION".to_string(), "".to_string());
    eprintln!("[Patcher] [ENV] Set LOCALVERSION='' (will be configured via .config)");

    env_vars
}

/// Sanitize environment variables to prevent $srcdir leaks and toolchain contamination
pub fn sanitize_build_environment(env_vars: &mut HashMap<String, String>) {
    // STEP 1: Remove variables that commonly leak $srcdir paths
    let srcdir_leak_patterns = vec![
        "TMPDIR",      // Temporary directories often contain $srcdir
        "TEMP",        // Alternative temp
        "TMP",         // Short form
    ];
    
    for var_name in srcdir_leak_patterns {
        if let Some(_) = env_vars.remove(var_name) {
            eprintln!("[Patcher] [SANITIZE] Removed {} to prevent $srcdir leak", var_name);
        }
    }

    // STEP 2: Clean up any GCC-related paths from PATH
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
                !(p.contains("/gcc") ||
                  p.contains("/g++") ||
                  p.contains("/cc") ||
                  p.contains("/c++") ||
                  p.contains("/llvm") ||
                  p.contains("/clang")) &&
                !p.is_empty()
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
                .replace("-Wl,--as-needed", "")  // GCC linker marker
                .replace("-Wl,--no-undefined", "")  // GCC linker marker
                .trim()
                .to_string();
            
            if original != *flags {
                eprintln!("[Patcher] [SANITIZE] Cleaned {}: removed GCC-specific flags", flag_var);
            }
        }
    }

    eprintln!("[Patcher] [SANITIZE] Environment sanitization complete");
}

/// Find a toolchain binary in PATH, with LLVM-19 prioritization
///
/// Searches for the binary in this order:
/// 1. LLVM-19 variant (e.g., llvm-19-strip for strip) - HIGHEST PRIORITY
/// 2. LLVM variant without version (e.g., llvm-strip for strip)
/// 3. Standard location (e.g., /usr/bin/strip)
/// 4. Just the command name (let PATH search)
pub fn find_toolchain_binary(name: &str) -> String {
    // STEP 1: Try LLVM-19 variant first (highest priority for consistency)
    let llvm19_variant = format!("llvm-19-{}", name);
    if Command::new(&llvm19_variant).arg("--version").output().is_ok() {
        eprintln!("[Patcher] [TOOLCHAIN] Found LLVM-19 variant: {}", llvm19_variant);
        return llvm19_variant;
    }
    
    // STEP 2: Try generic LLVM variant (fallback for latest LLVM)
    let llvm_variant = format!("llvm-{}", name);
    if Command::new(&llvm_variant).arg("--version").output().is_ok() {
        eprintln!("[Patcher] [TOOLCHAIN] Found LLVM variant: {}", llvm_variant);
        return llvm_variant;
    }
    
    // STEP 3: Try standard /usr/bin location
    let standard_path = format!("/usr/bin/{}", name);
    if Path::new(&standard_path).exists() {
        eprintln!("[Patcher] [TOOLCHAIN] Found at standard location: {}", standard_path);
        return standard_path;
    }
    
    // STEP 4: Fallback to just the command name (rely on PATH)
    eprintln!("[Patcher] [TOOLCHAIN] Using {} from PATH (final fallback)", name);
    name.to_string()
}

-----
kconfig.rs
-----
//! Kernel Configuration Management Module
//! 
//! Handles all interactions with the kernel .config file, including:
//! - Generating .config.override for KCONFIG_ALLCONFIG
//! - Applying Phase 5 LTO Hard Enforcement
//! - Injecting specific hardening and security options
//! - Baking in kernel command line parameters

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use regex::Regex;
use once_cell::sync::Lazy;

use crate::error::PatchError;
use crate::models::{LtoType, HardeningLevel};
use super::templates; // Assumes we created the templates module

// Regex for surgical LTO removal (Phase 5)
static LTO_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:CONFIG_LTO_|CONFIG_HAS_LTO_|# CONFIG_LTO_|# CONFIG_HAS_LTO_)[^\n]*$")
        .expect("Invalid LTO removal regex")
});

static SPACE_COLLAPSE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\n\n+").expect("Invalid space collapse regex")
});

/// Generate .config.override for native KConfig injection
pub fn generate_config_override(
    src_dir: &Path, 
    options: &HashMap<String, String>, 
    lto_type: LtoType
) -> Result<PathBuf, PatchError> {
    let override_path = src_dir.join(".config.override");
    
    // Get the base template string
    let mut content = String::from("# GOATd-generated .config.override\n");
    
    // 1. Inject LTO Settings
    content.push_str(templates::get_lto_override_config(lto_type));
    
    // 2. Inject Toolchain Enforcement
    content.push_str("\nCONFIG_CC_IS_CLANG=y\nCONFIG_CLANG_VERSION=190106\n");
    
    // 3. Inject User Options
    content.push_str("\n# User-provided options\n");
    for (key, value) in options {
        if !key.starts_with("_") { // Skip internal metadata
            content.push_str(&format!("{}={}\n", key, value));
        }
    }
    
    // 4. Inject MGLRU Options (Special handling for _MGLRU_ prefixes)
    for (key, value) in options {
        if key.starts_with("_MGLRU_CONFIG_") {
            if let Some(eq_pos) = value.find('=') {
                content.push_str(&format!("{}={}\n", &value[..eq_pos], &value[eq_pos + 1..]));
            }
        }
    }

    // Write file
    fs::write(&override_path, &content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config.override: {}", e)))?;

    // Compatibility Fix: Also write to ../config for PKGBUILDs that expect it
    let parent_config = src_dir.parent().map(|p| p.join("config")).unwrap_or_else(|| PathBuf::from("../config"));
    let _ = fs::write(parent_config, &content);

    Ok(override_path)
}

/// Apply Kconfig options and enforce Phase 5 LTO protections
pub fn apply_kconfig(
    src_dir: &Path,
    backup_dir: &Path,
    options: &HashMap<String, String>,
    lto_type: LtoType
) -> Result<(), PatchError> {
    let config_path = src_dir.join(".config");
    
    // 1. Read existing config or create empty
    let mut content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read .config: {}", e)))?
    } else {
        String::new()
    };

    // 2. Backup
    if config_path.exists() {
        let _ = fs::create_dir_all(backup_dir);
        let _ = fs::write(backup_dir.join(".config.bak"), &content);
    }

    // 3. Clean GCC artifacts
    content = remove_gcc_references(&content);

    // 4. Apply User Options (Merging logic)
    content = merge_options(content, options);

    // 5. PHASE 5: LTO HARD ENFORCER (Surgical Replacement)
    // Remove ALL existing LTO lines first to prevent conflicts
    content = LTO_REMOVAL_REGEX.replace_all(&content, "").to_string();
    content = SPACE_COLLAPSE_REGEX.replace_all(&content, "\n").to_string();

    // Inject the authoritative LTO block from templates
    content.push_str("\n");
    content.push_str(templates::get_phase5_lto_enforcer(lto_type));

    // 6. Write back
    fs::write(&config_path, content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config: {}", e)))?;

    // 7. Inject Cmdline (needs separate logic flow usually, but could be called here)
    // For this example, we assume `inject_baked_in_cmdline` is called separately 
    // or we can call it here if we had the params.

    Ok(())
}

/// Inject baked-in kernel parameters (CONFIG_CMDLINE)
pub fn inject_baked_in_cmdline(
    src_dir: &Path, 
    use_mglru: bool, 
    hardening: HardeningLevel
) -> Result<(), PatchError> {
    let config_path = src_dir.join(".config");
    let mut content = fs::read_to_string(&config_path)
        .map_err(|e| PatchError::PatchFailed(format!("Read error: {}", e)))?;

    // Logic to parse existing cmdline and append new flags
    let mut current_cmdline = extract_existing_cmdline(&content);
    
    // Append mandatory flags
    let mut new_flags = vec!["nowatchdog", "preempt=full"];
    if use_mglru { new_flags.push("lru_gen.enabled=7"); }
    if hardening == HardeningLevel::Minimal { new_flags.push("mitigations=off"); }

    for flag in new_flags {
        if !current_cmdline.contains(flag) {
            current_cmdline.push_str(" ");
            current_cmdline.push_str(flag);
        }
    }

    // Replace in file
    content = remove_config_line(&content, "CONFIG_CMDLINE");
    content = remove_config_line(&content, "CONFIG_CMDLINE_BOOL");
    content = remove_config_line(&content, "CONFIG_CMDLINE_OVERRIDE");

    content.push_str(&format!("\nCONFIG_CMDLINE=\"{}\"\n", current_cmdline.trim()));
    content.push_str("CONFIG_CMDLINE_BOOL=y\n");
    content.push_str("CONFIG_CMDLINE_OVERRIDE=n\n");

    fs::write(&config_path, content)
        .map_err(|e| PatchError::PatchFailed(format!("Write error: {}", e)))?;

    Ok(())
}

/// Helper: Merge user options into config content
fn merge_options(mut content: String, options: &HashMap<String, String>) -> String {
    for (key, value) in options {
        if key.starts_with("_") { continue; } // Skip metadata
        
        // Remove existing line
        content = content.lines()
            .filter(|l| !l.starts_with(&format!("{}=", key)))
            .collect::<Vec<_>>()
            .join("\n");
            
        // Append new
        content.push_str(&format!("\n{}={}", key, value));
    }
    content
}

/// Helper: Remove lines starting with key
fn remove_config_line(content: &str, key: &str) -> String {
    content.lines()
        .filter(|l| !l.starts_with(&format!("{}=", key)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Helper: Clean GCC refs
fn remove_gcc_references(content: &str) -> String {
    content.lines()
        .filter(|l| !l.contains("CONFIG_CC_IS_GCC") && !l.contains("CONFIG_GCC_VERSION"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Helper: Extract existing cmdline string
fn extract_existing_cmdline(content: &str) -> String {
    for line in content.lines() {
        if line.starts_with("CONFIG_CMDLINE=") {
            return line.split('"').nth(1).unwrap_or("").to_string();
        }
    }
    String::new()
}

-----
pkgbuild.rs
-----
//! PKGBUILD Modification Module
//!
//! Handles surgical injections into the Arch Linux PKGBUILD file.
//! Responsible for:
//! - Rebranding (pkgbase, function names)
//! - Injecting build phases (prepare, build, package)
//! - Validating syntax stability

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use regex::Regex;
use crate::error::PatchError;
use super::templates;

/// Helper to read PKGBUILD
fn read_pkgbuild(src_dir: &Path) -> Result<(PathBuf, String), PatchError> {
    let path = src_dir.join("PKGBUILD");
    if !path.exists() {
        return Err(PatchError::FileNotFound(format!("PKGBUILD not found at {}", path.display())));
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;
    Ok((path, content))
}

/// Inject Clang/LLVM toolchain exports into PKGBUILD
pub fn inject_clang_env(src_dir: &Path) -> Result<(), PatchError> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // 1. Aggressively replace GCC variable assignments with regex
    //    Matches: export CC=gcc, CC=gcc, etc.
    let substitutions = [
        (r"(?m)^\s*(?:export\s+)?CC\s*=\s*(?:gcc|cc)[^\n]*", "export CC=clang"),
        (r"(?m)^\s*(?:export\s+)?CXX\s*=\s*(?:g\+\+|c\+\+)[^\n]*", "export CXX=clang++"),
        (r"(?m)^\s*(?:export\s+)?LD\s*=\s*(?:ld)[^\n]*", "export LD=ld.lld"),
    ];

    for (pattern, replacement) in substitutions {
        let regex = Regex::new(pattern).map_err(|e| PatchError::RegexInvalid(e.to_string()))?;
        content = regex.replace_all(&content, replacement).to_string();
    }

    // 2. Inject Exports block at start of functions
    let clang_block = templates::get_clang_exports();
    content = inject_into_functions(&content, &["prepare", "build", "_package"], &clang_block);

    // 3. Force LLVM=1 in make commands
    let make_regex = Regex::new(r"\bmake\b").unwrap();
    let lines: Vec<String> = content.lines().map(|line| {
        if line.contains("make") && !line.contains("LLVM=1") && !line.trim().starts_with('#') {
            make_regex.replace(line, "make LLVM=1 LLVM_IAS=1").to_string()
        } else {
            line.to_string()
        }
    }).collect();
    content = lines.join("\n");

    fs::write(path, content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
    Ok(())
}

/// Inject Modprobed-db Localmodconfig Logic (Phase G2 Prep)
pub fn inject_modprobed_logic(src_dir: &Path, enabled: bool) -> Result<(), PatchError> {
    if !enabled { return Ok(()); }
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Inject into prepare()
    if let Some(start) = find_function_body_start(&content, "prepare") {
        // Try to inject after cd "$srcdir" if possible
        let injection_point = if let Some(cd_pos) = content[start..].find("cd \"$srcdir\"") {
            let offset = start + cd_pos;
            if let Some(newline) = content[offset..].find('\n') {
                offset + newline + 1
            } else {
                start
            }
        } else {
            start
        };

        if !content.contains("MODPROBED-DB AUTO-DISCOVERY") {
            let snippet = templates::get_modprobed_injection();
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        }
    }
    Ok(())
}

/// Inject Kernel Whitelist Protection (Crucial Logic Fix)
pub fn inject_kernel_whitelist(src_dir: &Path, enabled: bool) -> Result<(), PatchError> {
    if !enabled { return Ok(()); }
    let (path, mut content) = read_pkgbuild(src_dir)?;

    if let Some(prepare_start) = find_function_body_start(&content, "prepare") {
        // ROBUST ANCHOR LOGIC:
        // Instead of counting if/fi (which fails on comments), look for the end of the
        // modprobed block we inserted previously.
        let anchor = "# END MODPROBED-DB BLOCK";
        
        let injection_point = if let Some(anchor_pos) = content[prepare_start..].find(anchor) {
            // Found the anchor, insert AFTER it
            let absolute_pos = prepare_start + anchor_pos + anchor.len();
            absolute_pos
        } else {
            // Anchor not found (modprobed disabled?), insert at start of function
            prepare_start
        };

        if !content.contains("KERNEL WHITELIST PROTECTION") {
            let snippet = templates::get_whitelist_injection();
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        }
    }
    Ok(())
}

/// Inject Metadata Persistence Layer (MPL) Sourcing
pub fn inject_mpl_sourcing(src_dir: &Path, workspace_root: &Path) -> Result<u32, PatchError> {
    let (path, mut content) = read_pkgbuild(src_dir)?;
    
    let snippet = templates::get_mpl_sourcing(workspace_root);
    
    // Regex to match package(), _package(), package_linux(), etc.
    let func_regex = Regex::new(r"(?m)^(package|_package|package_\w+)\(\)\s*\{").unwrap();
    
    let mut count = 0;
    // We rebuild the string to handle multiple insertions
    let mut new_content = String::with_capacity(content.len() + 1000);
    let mut last_idx = 0;

    for caps in func_regex.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        let end_idx = m.end();
        
        new_content.push_str(&content[last_idx..end_idx]);
        new_content.push('\n');
        new_content.push_str(&snippet);
        new_content.push('\n');
        
        last_idx = end_idx;
        count += 1;
    }
    new_content.push_str(&content[last_idx..]);

    if count > 0 {
        fs::write(path, new_content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
    }
    
    Ok(count)
}

/// Inject Module Directory Creation (Phase E2)
/// **Contains Critical Fix for String Formatting**
pub fn inject_module_directory_creation(
    src_dir: &Path, 
    actual_version: Option<&str>
) -> Result<u32, PatchError> {
    let (path, mut content) = read_pkgbuild(src_dir)?;
    
    // Get templates (Priority 0 vs Legacy)
    let (headers_code, main_code) = templates::get_module_dir_creation(actual_version);

    let func_regex = Regex::new(r"(?m)^(package|_package|package_\w+)\(\)\s*\{").unwrap();
    let mut count = 0;
    let mut new_content = String::with_capacity(content.len() + 2000);
    let mut last_idx = 0;

    for caps in func_regex.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        let end_idx = m.end();
        let func_name = caps.get(1).unwrap().as_str();
        
        new_content.push_str(&content[last_idx..end_idx]);
        new_content.push('\n');

        // Choose template based on function name
        if func_name.contains("headers") {
             new_content.push_str(&headers_code);
        } else {
             new_content.push_str(&main_code);
        }
        new_content.push('\n');

        last_idx = end_idx;
        count += 1;
    }
    new_content.push_str(&content[last_idx..]);

    if count > 0 {
        fs::write(path, new_content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
    }
    Ok(count)
}

/// Rebranding: Rename pkgbase and functions
pub fn patch_rebranding(src_dir: &Path, profile: &str) -> Result<(), PatchError> {
    let (path, content) = read_pkgbuild(src_dir)?;
    
    let variant = detect_kernel_variant(&content)?;
    let profile_lower = profile.to_lowercase();
    
    // Logic to construct new names
    let master_identity = if variant == "linux" {
        format!("linux-goatd-{}", profile_lower)
    } else {
        // e.g. linux-zen -> linux-goatd-zen-gaming
        let suffix = variant.trim_start_matches("linux-");
        format!("linux-goatd-{}-{}", suffix, profile_lower)
    };
    
    let master_func_suffix = master_identity.replace("-", "_");
    
    // Perform Replacements
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    
    for line in &mut lines {
        // 1. pkgbase
        if line.starts_with("pkgbase=") {
            *line = format!("pkgbase='{}'", master_identity);
        }
        
        // 2. pkgname array
        // Replace "linux" or variant with master_identity
        if line.starts_with("pkgname=") {
            *line = line.replace(&variant, &master_identity);
        }
        
        // 3. Function names
        // e.g. package_linux() -> package_linux_goatd_gaming()
        // We use a simplified replace here assuming standard naming
        if line.starts_with("package_") && line.ends_with("() {") {
             // Basic heuristic replace for function definitions
             *line = line.replace(&variant.replace("-", "_"), &master_func_suffix);
        }
    }
    
    // Inject "provides"
    // Find pkgdesc to insert after
    if let Some(idx) = lines.iter().position(|l| l.starts_with("pkgdesc=")) {
        lines.insert(idx + 1, format!("provides=('{}')", variant));
    }

    fs::write(path, lines.join("\n") + "\n").map_err(|e| PatchError::PatchFailed(e.to_string()))?;
    Ok(())
}

// --- Internal Helpers ---

/// Finds the index immediately after the opening `{` of a function
fn find_function_body_start(content: &str, func_name: &str) -> Option<usize> {
    // Matches "func_name() {" or "func_name() {" with various spacing
    let pattern = format!(r"(?m)^{}\(\)\s*\{{", regex::escape(func_name));
    let regex = Regex::new(&pattern).ok()?;
    
    regex.find(content).map(|m| m.end())
}

/// Detect kernel variant from existing PKGBUILD content
fn detect_kernel_variant(content: &str) -> Result<String, PatchError> {
    for line in content.lines() {
        if line.starts_with("pkgbase=") {
             let val = line.split('=').nth(1).unwrap_or("").trim_matches(&['\'', '"'][..]);
             // Simple extraction for standard Arch kernels
             if val.starts_with("linux") {
                 return Ok(val.to_string());
             }
        }
    }
    // Fallback default
    Ok("linux".to_string())
}

/// Helper to inject code into multiple functions
fn inject_into_functions(content: &str, func_names: &[&str], code: &str) -> String {
    let mut new_content = content.to_string();
    for func in func_names {
        if let Some(pos) = find_function_body_start(&new_content, func) {
             // We need to handle shifting indices if we do this in a loop, 
             // but since we modify `new_content`, subsequent finds *might* work 
             // if we search from 0. However, regex finding on modified string is expensive.
             // For simplicity in this example, we just do one pass per function type
             // or use the Regex replace approach shown in inject_mpl_sourcing.
             
             // A simple insert:
             new_content.insert_str(pos, &format!("\n{}\n", code));
        }
    }
    new_content
}

-----
mod.rs
-----
//! Kernel Patcher Module (Facade)
//!
//! This module coordinates the kernel patching process by delegating responsibilities
//! to specialized sub-modules. It ensures the "Cooperative Kernel Build Pipeline"
//! flows correctly:
//! 1. Environment Setup (env.rs)
//! 2. PKGBUILD Surgical Injection (pkgbuild.rs)
//! 3. Configuration Management (kconfig.rs)

pub mod env;
pub mod kconfig;
pub mod pkgbuild;
pub mod templates; // The data warehouse for shell scripts

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use crate::error::PatchError;
use crate::models::LtoType;

// Re-export specific types if needed by the rest of the app
pub use self::env::prepare_build_environment;

/// High-level kernel patcher orchestrator
pub struct KernelPatcher {
    /// Source directory of the kernel
    pub src_dir: PathBuf,
    /// Backup directory for original files
    pub backup_dir: PathBuf,
}

impl KernelPatcher {
    /// Create a new kernel patcher
    pub fn new(src_dir: PathBuf) -> Self {
        let backup_dir = src_dir.join(".kernel_patcher_backup");
        KernelPatcher {
            src_dir,
            backup_dir,
        }
    }

    /// Cleans up old .pkg.tar.zst artifacts from the source directory
    pub fn cleanup_previous_artifacts(&self) -> Result<u32, PatchError> {
        let mut count = 0;
        if let Ok(entries) = fs::read_dir(&self.src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".pkg.tar.zst") {
                        let _ = fs::remove_file(path);
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }

    /// Main Entry Point: Execute the full patching sequence
    pub fn execute_full_patch_with_env(
        &self,
        _shield_modules: Vec<String>, // Kept for API compatibility
        config_options: HashMap<String, String>,
        build_env_vars: HashMap<String, String>,
    ) -> Result<(), PatchError> {
        eprintln!("[Patcher] Starting Full Orchestrated Patch Sequence");

        // 1. Determine LTO Type from Environment
        let lto_type = match build_env_vars.get("GOATD_LTO_LEVEL").map(|s| s.as_str()) {
            Some("full") => LtoType::Full,
            Some("none") => LtoType::None,
            _ => LtoType::Thin, // Default
        };

        // 2. Determine Hardening / Modprobed Flags
        let use_modprobed = build_env_vars.get("GOATD_USE_MODPROBED_DB").map(|v| v == "1").unwrap_or(false);
        let use_whitelist = build_env_vars.get("GOATD_USE_KERNEL_WHITELIST").map(|v| v == "1").unwrap_or(false);
        let profile_name = build_env_vars.get("GOATD_PROFILE_NAME").map(|s| s.as_str());

        // 3. PRIORITY 0: Version Injection (The Nuclear Option)
        // Retrieve explicit version from environment (Source of Truth) to bypass fragile discovery
        let env_version = build_env_vars.get("GOATD_KERNELRELEASE").cloned();
        let detected_version = self.detect_kernel_version().ok();
        let final_version = env_version.or(detected_version);

        if let Some(v) = &final_version {
            eprintln!("[Patcher] [PHASE-E2] Activating Priority 0 Version Injection: {}", v);
        }

        // --- PKGBUILD SURGERY ---
        
        // A. Inject Module Directory Creation (Phase E2)
        // CRITICAL: This uses the Priority 0 version to fix the "unknown version" error
        pkgbuild::inject_module_directory_creation(&self.src_dir, final_version.as_deref())?;

        // B. Inject MPL Sourcing (Metadata Persistence)
        if let Some(ws_root) = build_env_vars.get("GOATD_WORKSPACE_ROOT") {
            let _ = pkgbuild::inject_mpl_sourcing(&self.src_dir, Path::new(ws_root));
        }

        // C. Rebranding (Change pkgbase/pkgname)
        if let Some(profile) = profile_name {
            pkgbuild::patch_rebranding(&self.src_dir, profile)?;
        }

        // D. Inject Toolchain (Clang/LLVM exports)
        pkgbuild::inject_clang_env(&self.src_dir)?;

        // E. Inject Modprobed-db Logic
        pkgbuild::inject_modprobed_logic(&self.src_dir, use_modprobed)?;

        // F. Inject Kernel Whitelist
        pkgbuild::inject_kernel_whitelist(&self.src_dir, use_whitelist)?;

        // --- KERNEL CONFIGURATION ---

        // G. Apply KConfig (and Phase 5 LTO Hard Enforcer)
        kconfig::apply_kconfig(
            &self.src_dir, 
            &self.backup_dir, 
            &config_options, 
            lto_type
        )?;

        // H. Generate .config.override for safety
        kconfig::generate_config_override(
            &self.src_dir, 
            &config_options, 
            lto_type
        )?;

        // --- FINAL TOUCHES ---

        // I. Patch Root Makefile (Force LLVM=1)
        self.patch_root_makefile()?;

        eprintln!("[Patcher] Patch sequence completed successfully.");
        Ok(())
    }

    /// Patch root Makefile to enforce LLVM
    fn patch_root_makefile(&self) -> Result<(), PatchError> {
        let makefile_path = self.src_dir.join("Makefile");
        if !makefile_path.exists() { return Ok(()); }

        let content = fs::read_to_string(&makefile_path)
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;

        if !content.contains("GOATD Toolchain Enforcement") {
            let block = templates::get_makefile_enforcer();
            let new_content = format!("{}{}", block, content);
            fs::write(makefile_path, new_content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// Helper: Detect kernel version from Makefile (fallback for Priority 0)
    fn detect_kernel_version(&self) -> Result<String, PatchError> {
        let makefile_path = self.src_dir.join("Makefile");
        let content = fs::read_to_string(&makefile_path)
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;

        let mut v = String::new();
        let mut p = String::new();

        for line in content.lines() {
            if line.starts_with("VERSION =") { v = line.split('=').nth(1).unwrap_or("").trim().to_string(); }
            if line.starts_with("PATCHLEVEL =") { p = line.split('=').nth(1).unwrap_or("").trim().to_string(); }
        }

        if !v.is_empty() && !p.is_empty() {
            Ok(format!("{}.{}", v, p))
        } else {
            Err(PatchError::PatchFailed("Version detection failed".to_string()))
        }
    }
}

-----
templates.rs
-----
//! Kernel Patcher Templates
//!
//! This module acts as a "Data Warehouse" for all hardcoded shell scripts,
//! Makefile snippets, and configuration blocks used by the kernel patcher.
//! separating data from logic makes the codebase significantly more readable.

use std::path::Path;
use crate::models::LtoType;

// =============================================================================
// ROOT MAKEFILE TEMPLATES
// =============================================================================

pub fn get_makefile_enforcer() -> &'static str {
    r#"# GOATd Toolchain Enforcement
LLVM := 1
LLVM_IAS := 1
export LLVM LLVM_IAS

"#
}

// =============================================================================
// PKGBUILD SHELL INJECTIONS
// =============================================================================

/// Returns the export block for Clang/LLVM toolchain variables
pub fn get_clang_exports() -> &'static str {
    r#"    # ======================================================================
    # CLANG/LLVM TOOLCHAIN INJECTION (Rust-based patcher)
    # ======================================================================
    export LLVM=1
    export LLVM_IAS=1
    export CC=clang
    export CXX=clang++
    export LD=ld.lld
    export AR=llvm-ar
    export NM=llvm-nm
    export STRIP=llvm-strip
    export OBJCOPY=llvm-objcopy
    export OBJDUMP=llvm-objdump
    export READELF=llvm-readelf
    
    export HOSTCC=clang
    export HOSTCXX=clang++
    
    export CFLAGS="-O2 -march=native"
    export CXXFLAGS="-O2 -march=native"
    export LDFLAGS="-O2 -march=native"
    export KCFLAGS="-march=native"
"#
}

/// Returns the modprobed-db localmodconfig logic (Phase G2 Prep)
pub fn get_modprobed_injection() -> &'static str {
    r#"
    # =====================================================================
    # MODPROBED-DB AUTO-DISCOVERY
    # =====================================================================
    # Detects modprobed.db path and runs localmodconfig
    
    MODPROBED_DB_PATH=""
    for candidate in "$HOME/.config/modprobed.db" /root/.config/modprobed.db /home/*/.config/modprobed.db; do
        if [[ -f "$candidate" ]]; then
            MODPROBED_DB_PATH="$candidate"
            break
        fi
    done
    
    if [[ -n "$MODPROBED_DB_PATH" && -f "$MODPROBED_DB_PATH" ]]; then
        printf "[MODPROBED] Found db at $MODPROBED_DB_PATH\n" >&2
        
        # Locate kernel source (handles linux, linux-zen, etc folder names)
        KERNEL_SRC_DIR=""
        if [[ -d "$srcdir/linux" ]]; then KERNEL_SRC_DIR="$srcdir/linux";
        else KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1); fi
        
        if [[ -n "$KERNEL_SRC_DIR" ]]; then
            ( cd "$KERNEL_SRC_DIR" || exit 1; 
              yes "" | make LLVM=1 LLVM_IAS=1 LSMOD="$MODPROBED_DB_PATH" localmodconfig > /dev/null 2>&1
            )
            printf "[MODPROBED] localmodconfig complete.\n" >&2
        fi
    else
        printf "[MODPROBED] No database found. Skipping localmodconfig.\n" >&2
    fi
    # =====================================================================
    # END MODPROBED-DB BLOCK
    # =====================================================================
"#
}

/// Returns the Kernel Whitelist logic
pub fn get_whitelist_injection() -> &'static str {
    r#"
    # =====================================================================
    # KERNEL WHITELIST PROTECTION
    # =====================================================================
    if [[ -f ".config" ]]; then
        printf "[WHITELIST] Applying critical feature whitelist...\n" >&2
        cat >> ".config" << 'EOF'
CONFIG_SYSFS=y
CONFIG_PROC_FS=y
CONFIG_TMPFS=y
CONFIG_DEVTMPFS=y
CONFIG_BLK_DEV_INITRD=y
CONFIG_EXT4_FS=y
CONFIG_BTRFS_FS=y
CONFIG_FAT_FS=m
CONFIG_VFAT_FS=m
CONFIG_NVME=m
CONFIG_USB=m
CONFIG_USB_HID=m
EOF
    fi
"#
}

/// Returns the MPL Sourcing snippet (Absolute Path)
pub fn get_mpl_sourcing(workspace_root: &Path) -> String {
    format!(
        r#"    # =====================================================================
    # MPL SOURCING (Metadata Persistence Layer)
    # =====================================================================
    # Sources build metadata from the orchestrator to ensure consistency
    if [ -f "{}/.goatd_metadata" ]; then
        source "{}/.goatd_metadata"
        echo "[MPL] Loaded metadata: GOATD_KERNELRELEASE=${{GOATD_KERNELRELEASE}}" >&2
    fi
"#,
        workspace_root.display(), workspace_root.display()
    )
}

/// Returns the tuple (headers_injection, main_injection) for Phase E2
///
/// **CRITICAL FIX:** This template uses `{{_actual_ver}}` to ensure the 
/// generated Bash code contains `${_actual_ver}` (valid syntax) instead of 
/// `${}_actual_ver` (invalid syntax).
pub fn get_module_dir_creation(actual_version: Option<&str>) -> (String, String) {
    
    // 1. Define the Version Discovery Logic (Priority 0 vs Fallback)
    let version_logic = if let Some(ver) = actual_version {
        // Priority 0: Hardcoded Literal (The Nuclear Option)
        format!(
            r#"
    # PRIORITY 0: Hardcoded Version Injection
    _actual_ver="{}"
    echo "[PHASE-E2] Using Hardcoded Version: ${{_actual_ver}}" >&2
"#,
            ver
        )
    } else {
        // Legacy Fallback Discovery
        r#"
    # Fallback Version Discovery
    _actual_ver=""
    if [ -n "${GOATD_KERNELRELEASE}" ]; then
        _actual_ver="${GOATD_KERNELRELEASE}"
    elif [ -f .kernelrelease ]; then
        _actual_ver=$(cat .kernelrelease)
    fi
    [ -z "${_actual_ver}" ] && _actual_ver="${_kernver}"
"#
        .to_string()
    };

    // 2. Define the Action Logic (Headers vs Main)
    // Note the use of {{_actual_ver}} to escape braces for Rust format!
    
    let headers_action = r#"
    if [ -n "${_actual_ver}" ]; then
        echo "[PHASE-E2] Installing headers to: /usr/src/linux-${_actual_ver}" >&2
        mkdir -p "${pkgdir}/usr/src/linux-${_actual_ver}"
        mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"
        
        # Version Bridge
        _pretty_ver="${pkgver}-${pkgrel}-${pkgbase#linux-}"
        if [ "${_actual_ver}" != "${_pretty_ver}" ]; then
            (cd "${pkgdir}/usr/lib/modules" && ln -sf "${_actual_ver}" "${_pretty_ver}") 2>/dev/null
        fi
    fi
"#;

    let main_action = r#"
    if [ -n "${_actual_ver}" ]; then
        echo "[PHASE-E2] Creating module dir: /usr/lib/modules/${_actual_ver}" >&2
        mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"
    fi
"#;

    // Combine them
    let headers_code = format!("{}{}", version_logic, headers_action);
    let main_code = format!("{}{}", version_logic, main_action);

    (headers_code, main_code)
}

// =============================================================================
// KCONFIG / LTO TEMPLATES
// =============================================================================

/// Returns the configuration block for .config.override
pub fn get_lto_override_config(lto_type: LtoType) -> String {
    match lto_type {
        LtoType::Full => "CONFIG_LTO_CLANG=y\nCONFIG_LTO_CLANG_FULL=y\nCONFIG_HAS_LTO_CLANG=y\n".to_string(),
        LtoType::Thin => "CONFIG_LTO_CLANG=y\nCONFIG_LTO_CLANG_THIN=y\nCONFIG_HAS_LTO_CLANG=y\n".to_string(),
        LtoType::None => "# LTO Disabled\n".to_string(),
    }
}

/// Returns the Phase 5 Hard Enforcer block (Surgically injected into .config)
pub fn get_phase5_lto_enforcer(lto_type: LtoType) -> String {
    match lto_type {
        LtoType::Full => vec![
            "# PHASE 5 HARD ENFORCER: LTO CLANG FULL",
            "CONFIG_LTO_CLANG=y",
            "CONFIG_LTO_CLANG_FULL=y",
            "CONFIG_HAS_LTO_CLANG=y",
        ].join("\n"),
        
        LtoType::Thin => vec![
            "# PHASE 5 HARD ENFORCER: LTO CLANG THIN",
            "CONFIG_LTO_CLANG=y",
            "CONFIG_LTO_CLANG_THIN=y",
            "CONFIG_HAS_LTO_CLANG=y",
        ].join("\n"),
        
        LtoType::None => vec![
            "# PHASE 5: LTO DISABLED",
            "# No LTO configuration injected",
        ].join("\n"),
    }
}


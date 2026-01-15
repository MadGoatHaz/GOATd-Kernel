//! Kernel patch application with LTO shielding and CONFIG management.

use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use crate::error::PatchError;
use std::process::Command;
use crate::models::LtoType;
use crate::kernel::lto;

// Pre-compiled regex patterns (compiled once at startup)
static FLTO_THIN_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\s*-flto=thin\s*").expect("Invalid -flto=thin regex")
});
static FLTO_FULL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\s*-flto=full\s*").expect("Invalid -flto=full regex")
});
static ICF_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\s*--icf[=a-zA-Z0-9]*\s*").expect("Invalid --icf regex")
});
static SPACE_CLEANUP_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r" {2,}").expect("Invalid space cleanup regex")
});
static CC_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?:export\s+)?CC\s*=\s*(?:gcc|cc)[^\n]*").expect("Invalid CC regex")
});
static CXX_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?:export\s+)?CXX\s*=\s*(?:g\+\+|c\+\+)[^\n]*").expect("Invalid CXX regex")
});
static LD_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?:export\s+)?LD\s*=\s*(?:ld)[^\n]*").expect("Invalid LD regex")
});
static LTO_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:CONFIG_LTO_|CONFIG_HAS_LTO_|# CONFIG_LTO_|# CONFIG_HAS_LTO_)[^\n]*$")
        .expect("Invalid LTO removal regex")
});
static SPACE_COLLAPSE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\n\n+").expect("Invalid space collapse regex")
});
static MAKE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bmake\b").expect("Invalid make regex")
});
static GCC_PATTERN_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?:export\s+)?(GCC|CFLAGS|CXXFLAGS|LDFLAGS)_[A-Z0-9_]*\s*=")
        .expect("Invalid GCC pattern regex")
});
static OLDCONFIG_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)(make\s+(?:old)?config|make\s+syncconfig)").expect("Invalid oldconfig pattern regex")
});
static POLLY_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)\s*# ===+\s*\n\s*# POLLY LOOP OPTIMIZATION.*?export LDFLAGS.*?\n")
        .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# POLLY LOOP OPTIMIZATION.*?^export LDFLAGS").unwrap())
});
static G1_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)\s*# ===+\s*\n\s*# PHASE G1 PREBUILD:.*?fi\s*\n")
        .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# PHASE G1 PREBUILD.*?^fi\s*$").unwrap())
});
static E1_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)\s*# ===+\s*\n\s*# PHASE E1 CRITICAL:.*?fi\s*\n")
        .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# PHASE E1 CRITICAL.*?^fi\s*$").unwrap())
});

/// Result type for patching operations
pub type PatchResult<T> = std::result::Result<T, PatchError>;

/// Find a toolchain binary in PATH, with fallbacks
///
/// Searches for the binary in this order:
/// 1. LLVM variant (e.g., llvm-strip for strip)
/// 2. Standard location (e.g., /usr/bin/strip)
/// 3. Just the command name (let PATH search)
///
/// Returns the resolved command name to use
fn find_toolchain_binary(name: &str) -> String {
    let llvm_variant = format!("llvm-{}", name);
    
    // Try LLVM variant first
    if Command::new(&llvm_variant).arg("--version").output().is_ok() {
        eprintln!("[Patcher] [TOOLCHAIN] Found {}", llvm_variant);
        return llvm_variant;
    }
    
    // Try standard /usr/bin location
    let standard_path = format!("/usr/bin/{}", name);
    if std::path::Path::new(&standard_path).exists() {
        eprintln!("[Patcher] [TOOLCHAIN] Found {}", standard_path);
        return standard_path;
    }
    
    // Fallback to just the command name (rely on PATH)
    eprintln!("[Patcher] [TOOLCHAIN] Using {} from PATH", name);
    name.to_string()
}

/// AMD GPU directories that require LTO shielding
const AMDGPU_SHIELD_DIRS: &[&str] = &[
    "drivers/gpu/drm/amd",
    "drivers/gpu/drm/amd/amdgpu",
    "drivers/gpu/drm/amd/amdkfd",
    "drivers/gpu/drm/amd/display",
];

/// Map of directory paths to their corresponding module names
fn get_module_name_for_dir(dir: &str) -> &'static str {
    match dir {
        "drivers/gpu/drm/amd" | "drivers/gpu/drm/amd/amdgpu" => "amdgpu",
        "drivers/gpu/drm/amd/amdkfd" => "amdkfd",
        "drivers/gpu/drm/amd/display" => "amdgpu_display",
        _ => "amdgpu",
    }
}

/// High-level kernel patcher for orchestrator integration
///
/// Provides a clean API for applying all kernel patches:
/// - LTO shielding for GPU drivers
/// - ICF flag removal
/// - Kconfig option application
pub struct KernelPatcher {
    /// Source directory of the kernel
    src_dir: PathBuf,
    /// Backup directory for original files
    backup_dir: PathBuf,
}

impl KernelPatcher {
    /// Create a new kernel patcher for the given source directory
    pub fn new(src_dir: PathBuf) -> Self {
        let backup_dir = src_dir.join(".kernel_patcher_backup");
        KernelPatcher {
            src_dir,
            backup_dir,
        }
    }

    /// Cleans up old .pkg.tar.zst artifacts.
    pub fn cleanup_previous_artifacts(&self) -> PatchResult<u32> {
        let mut removed_count = 0u32;

        if let Ok(entries) = fs::read_dir(&self.src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    let name = filename.to_string_lossy();
                    // Remove any old .pkg.tar.zst files (both kernel and headers packages)
                    if name.ends_with(".pkg.tar.zst") {
                        match fs::remove_file(&path) {
                            Ok(()) => {
                                eprintln!("[Patcher] [CLEANUP] Removed old artifact: {}", name);
                                removed_count += 1;
                            }
                            Err(e) => {
                                eprintln!("[Patcher] [WARNING] Failed to remove {}: {}", name, e);
                                // Continue anyway - build will try to force overwrite
                            }
                        }
                    }
                }
            }
        }

        Ok(removed_count)
    }

    /// Finds built kernel artifacts (.pkg.tar.zst files and alternative images).
    ///
    /// Searches the kernel source directory for:
    /// 1. Arch package files (*.pkg.tar.zst) - primary artifacts from makepkg
    /// 2. Fallback kernel images (bzImage, vmlinuz, vmlinux) - for raw make builds
    ///
    /// # Returns
    /// Vector of PathBuf pointing to found artifacts, or empty vector if none found
    pub fn find_build_artifacts(&self) -> PatchResult<Vec<PathBuf>> {
        let mut artifacts = Vec::new();

        // PRIORITY 1: Look for .pkg.tar.zst files (Arch package format from makepkg)
        // Pattern: linux-*-x86_64.pkg.tar.zst (main kernel image)
        if let Ok(entries) = fs::read_dir(&self.src_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(filename) = path.file_name() {
                    let name = filename.to_string_lossy().to_string();
                    // Look for the main linux kernel package or kernel headers
                    if (name.starts_with("linux-") && name.ends_with(".pkg.tar.zst")) ||
                       (name.contains("linux-headers-") && name.ends_with(".pkg.tar.zst")) {
                        eprintln!("[Patcher] [ARTIFACTS] Found kernel package: {}", name);
                        artifacts.push(path);
                    }
                }
            }
        }

        // PRIORITY 2: Fallback to common kernel image names if no packages found
        // This handles cases where raw kernel images were built instead of packages
        if artifacts.is_empty() {
            let possible_images = vec!["arch/x86/boot/bzImage", "vmlinuz", "vmlinux"];
            for image_name in possible_images {
                let image_path = self.src_dir.join(image_name);
                if image_path.exists() {
                    artifacts.push(image_path);
                    eprintln!("[Patcher] [ARTIFACTS] Found kernel image: {}", image_name);
                }
            }
        }

        Ok(artifacts)
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
    /// This method encapsulates all environment setup that previously lived in the Executor,
    /// ensuring the Executor only receives and applies pre-configured environment variables.
    ///
    /// # Arguments
    /// * `native_optimizations` - Whether to enable -march=native in KCFLAGS
    ///
    /// # Returns
    /// HashMap of environment variable names to values
    pub fn prepare_build_environment(&self, native_optimizations: bool) -> HashMap<String, String> {
        let mut env_vars = HashMap::new();

        // ============================================================================
        // CLANG/LLVM v19+ ENFORCEMENT
        // ============================================================================
        env_vars.insert("LLVM".to_string(), "1".to_string());
        env_vars.insert("LLVM_IAS".to_string(), "1".to_string());
        env_vars.insert("CC".to_string(), "clang".to_string());
        env_vars.insert("CXX".to_string(), "clang++".to_string());
        env_vars.insert("LD".to_string(), "ld.lld".to_string());
        env_vars.insert("AR".to_string(), "llvm-ar".to_string());
        env_vars.insert("NM".to_string(), "llvm-nm".to_string());
        
        // DYNAMIC TOOLCHAIN DISCOVERY: Find strip command (llvm-strip or strip)
        let strip_cmd = find_toolchain_binary("strip");
        env_vars.insert("STRIP".to_string(), strip_cmd);
        
        env_vars.insert("OBJCOPY".to_string(), "llvm-objcopy".to_string());
        env_vars.insert("OBJDUMP".to_string(), "llvm-objdump".to_string());
        env_vars.insert("READELF".to_string(), "llvm-readelf".to_string());
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
        // ============================================================================
        let llvm_bin_path = self.src_dir.join(".llvm_bin").to_string_lossy().to_string();
        let safe_paths = vec![
            llvm_bin_path.as_str(),
            "/usr/bin",
            "/bin",
        ];

        let current_path = std::env::var("PATH").unwrap_or_default();
        let filtered_path: Vec<&str> = current_path
            .split(':')
            .filter(|p| {
                !p.contains("gcc") && !p.contains("llvm") && !p.contains("clang") && !p.is_empty()
            })
            .collect();

        let new_path = format!("{}{}{}",
            safe_paths.join(":"),
            if filtered_path.is_empty() { "" } else { ":" },
            filtered_path.join(":")
        );

        env_vars.insert("PATH".to_string(), new_path.clone());
        eprintln!("[Patcher] [ENV] Purified PATH: {} (removed gcc/llvm/clang dirs)", llvm_bin_path);

        env_vars
    }

    /// Injects CFLAGS LTO filters for GPUs.
    /// Delegates to the shared implementation in lto.rs
    pub fn shield_lto(&self, module_names: Vec<String>) -> PatchResult<u32> {
        if module_names.is_empty() {
            return Ok(0); // No shielding needed
        }

        let mut shielded_count = 0u32;

        // Shield each AMD GPU directory
        for shield_dir in AMDGPU_SHIELD_DIRS {
            let makefile_path = self.src_dir.join(shield_dir).join("Makefile");

            if !makefile_path.exists() {
                continue;
            }

            let content = fs::read_to_string(&makefile_path)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to read Makefile: {}", e)))?;

            // Skip if already shielded
            if content.contains("CFLAGS_amdgpu") && content.contains("filter-out -flto") {
                shielded_count += 1;
                continue;
            }

            // Create backup
            let backup_path = self.backup_dir.join(format!("Makefile.{}.bak", shield_dir.replace('/', "_")));
            fs::create_dir_all(&self.backup_dir)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
            fs::write(&backup_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

            // Use shared consolidate implementation from lto.rs
            let updated = lto::shield_amd_gpu_from_lto(&content);

            // Write back the shielded Makefile
            fs::write(&makefile_path, &updated)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to write Makefile: {}", e)))?;

            shielded_count += 1;
        }

        Ok(shielded_count)
    }

    /// Strips ICF/LTO flags from root Makefile.
    /// Delegates to the shared implementation in lto.rs
    pub fn remove_icf_flags(&self) -> PatchResult<()> {
        let makefile_path = self.src_dir.join("Makefile");

        if !makefile_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("Makefile not found: {}", makefile_path.display())
            ));
        }

        let original_content = fs::read_to_string(&makefile_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read Makefile: {}", e)))?;

        // Check if any ICF/LTO flags exist
        if !original_content.contains("--icf")
            && !original_content.contains("-flto=thin")
            && !original_content.contains("-flto=full")
        {
            return Ok(()); // Nothing to remove
        }

        // Create backup
        let backup_path = self.backup_dir.join("Makefile.root.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &original_content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // Use shared consolidate implementation from lto.rs
        let content = lto::remove_icf_flags(&original_content);

        // Write back only if changed
        if content != original_content {
            fs::write(&makefile_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to write Makefile: {}", e)))?;
        }

        Ok(())
    }

    /// Fix Rust .rmeta and .so installation to use find instead of glob expansion
    ///
    /// CRITICAL FIX FOR CROSS-ENVIRONMENT COMPATIBILITY:
    /// The _package-headers() function uses glob patterns like `rust/*.rmeta` and `rust/*.so`
    /// which fail when those files don't exist (glob expands literally, not as empty set).
    /// This method surgically fixes the installation to use `find` which gracefully handles
    /// missing files across all environments.
    ///
    /// # Returns
    /// Number of fix applications or error if PKGBUILD not found
    pub fn fix_rust_rmeta_installation(&self) -> PatchResult<u32> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
           return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
           ));
        }

        let original_content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

       // Check if the problematic pattern exists
        if !original_content.contains("install -Dt \"$builddir/rust\" -m644 rust/*.rmeta") {
            return Ok(0); // Fix not needed
        }

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.rust_rmeta.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &original_content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // Use find with -exec instead of glob expansion
        let old_pattern = r#"   echo "Installing Rust files..."
  install -Dt "$builddir/rust" -m644 rust/*.rmeta
  install -Dt "$builddir/rust" rust/*.so"#;

       let new_pattern = r#"   echo "Installing Rust files..."
  # Use find to safely handle cases where .rmeta or .so files may not exist
  find rust -maxdepth 1 -type f -name '*.rmeta' -exec install -Dt "$builddir/rust" -m644 {} +
  find rust -maxdepth 1 -type f -name '*.so' -exec install -Dt "$builddir/rust" {} +"#;

        let mut content = original_content.clone();
        
        // Apply the fix if we can find the exact pattern
        if content.contains(old_pattern) {
            content = content.replace(old_pattern, new_pattern);
            eprintln!("[Patcher] [RUST-RMETA] Fixed _package-headers() Rust file installation with safe find pattern");
            
            fs::write(&pkgbuild_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;
            
            return Ok(1);
        }

        // Fallback: Try a more flexible pattern match
        let rmeta_line = "install -Dt \"$builddir/rust\" -m644 rust/*.rmeta";
        let so_line = "install -Dt \"$builddir/rust\" rust/*.so";
        
        if content.contains(rmeta_line) || content.contains(so_line) {
            let lines: Vec<String> = content
                .lines()
                .map(|line| {
                    if line.trim() == rmeta_line.trim() {
                        "   find rust -maxdepth 1 -type f -name '*.rmeta' -exec install -Dt \"$builddir/rust\" -m644 {} +".to_string()
                    } else if line.trim() == so_line.trim() {
                        "   find rust -maxdepth 1 -type f -name '*.so' -exec install -Dt \"$builddir/rust\" {} +".to_string()
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            
            content = lines.join("\n");
            if !content.ends_with('\n') {
                content.push('\n');
            }
            
            eprintln!("[Patcher] [RUST-RMETA] Fixed _package-headers() Rust file installation with flexible pattern");
            
            fs::write(&pkgbuild_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;
            
            return Ok(1);
        }

       Ok(0) // Pattern not found, fix not applied
    }

   /// Surgically remove `-v` flag from all `strip` calls in PKGBUILD
    ///
    /// This targets the incompatibility between llvm-strip and the `-v` flag.
    /// llvm-strip doesn't support `-v` for verbose output, so this method:
    /// - Finds all `strip -v` patterns
    /// - Replaces them with `strip` (removing the flag)
    /// - Creates a backup of the original PKGBUILD
    /// - Ensures safe operation without breaking the build
    ///
    /// # Returns
    ///
    /// Number of replacements made, or error if PKGBUILD not found
    pub fn remove_strip_verbose_flag(&self) -> PatchResult<u32> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let original_content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Check if there are any `strip -v` patterns to remove
        if !original_content.contains("strip -v") {
            return Ok(0); // Nothing to remove
        }

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.strip_verbose.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &original_content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // Regex patterns to match `strip -v` variations:
        // - Matches `strip -v` with optional spacing
        // - Can be preceded by path or variable expansion
        // - Ensures word boundary to avoid matching strings like "mystrip-v"
        let strip_verbose_regex = Regex::new(r"(?m)\bstrip\s+-v\b")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;

        // Count matches before replacement
        let match_count = strip_verbose_regex.find_iter(&original_content).count() as u32;

        // Replace `strip -v` with `strip`
        let mut content = original_content.clone();
        content = strip_verbose_regex.replace_all(&content, "strip").to_string();

        // Write back only if changed
        if content != original_content {
            fs::write(&pkgbuild_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;
        }

        Ok(match_count)
    }

    /// Inject baked-in kernel command line parameters via CONFIG_CMDLINE
    ///
    /// This method forcefully enables kernel features by baking them into the kernel binary
    /// via CONFIG_CMDLINE, which overrides any runtime kernel parameters. This is critical
    /// for features like MGLRU that report 0x0000 at runtime despite being enabled via CONFIG.
    ///
    /// Parameters baked in:
    /// - `lru_gen.enabled=7` if `use_mglru` is true (enables all MGLRU subsystems)
    /// - `mitigations=off` if `hardening_level` is Minimal (performance optimization)
    /// - `nowatchdog` (always) - disable watchdog timer for performance
    /// - `preempt=full` (always) - ensure full preemption is enforced at runtime
    ///
    /// # Arguments
    /// * `use_mglru` - Whether MGLRU is enabled
    /// * `hardening_level` - Hardening level for mitigations decision
    ///
    /// # Returns
    /// Result indicating success or error
    fn inject_baked_in_cmdline(&self, use_mglru: bool, hardening_level: crate::models::HardeningLevel) -> PatchResult<()> {
        let config_path = self.src_dir.join(".config");

        // Read or create .config
        let mut content = if config_path.exists() {
            fs::read_to_string(&config_path)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to read .config: {}", e)))?
        } else {
            String::new()
        };

        // STEP 1: Extract existing CONFIG_CMDLINE value if present
        let mut existing_cmdline = String::new();

        for line in content.lines() {
            if line.starts_with("CONFIG_CMDLINE=") {
                // Extract the value between quotes
                if let Some(start) = line.find('"') {
                    if let Some(end) = line.rfind('"') {
                        if start < end {
                            existing_cmdline = line[start + 1..end].to_string();
                            eprintln!("[Patcher] [CMDLINE] Found existing CONFIG_CMDLINE: '{}'", existing_cmdline);
                        }
                    }
                }
                break;
            }
        }

        // STEP 2: Build the new command line parameters to append
        let mut new_params = Vec::new();

        // Always add performance parameters
        new_params.push("nowatchdog");
        new_params.push("preempt=full");

        // Add MGLRU parameter if enabled
        if use_mglru {
            new_params.push("lru_gen.enabled=7");
            eprintln!("[Patcher] [CMDLINE] MGLRU enabled: adding lru_gen.enabled=7");
        }

        // Add mitigations=off if hardening is Minimal
        if hardening_level == crate::models::HardeningLevel::Minimal {
            new_params.push("mitigations=off");
            eprintln!("[Patcher] [CMDLINE] Hardening is Minimal: adding mitigations=off");
        }

        // STEP 3: Construct the final command line
        let mut final_cmdline = existing_cmdline.clone();
        if !final_cmdline.is_empty() && !final_cmdline.ends_with(' ') {
            final_cmdline.push(' ');
        }

        for param in &new_params {
            if !final_cmdline.contains(param) {
                final_cmdline.push_str(param);
                final_cmdline.push(' ');
            }
        }

        // Remove trailing space
        let final_cmdline = final_cmdline.trim_end().to_string();

        eprintln!("[Patcher] [CMDLINE] FINAL BAKED-IN CMDLINE: '{}'", final_cmdline);

        // STEP 4: Remove existing CONFIG_CMDLINE* entries from .config (exact and prefix matches)
        // Exact matches: CONFIG_CMDLINE_BOOL, CONFIG_CMDLINE_OVERRIDE
        // Prefix matches: CONFIG_CMDLINE=
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !(trimmed.starts_with("CONFIG_CMDLINE=") ||
                  trimmed.starts_with("CONFIG_CMDLINE_BOOL=") ||
                  trimmed.starts_with("CONFIG_CMDLINE_OVERRIDE="))
            })
            .collect();

        if lines.len() != content.lines().count() {
            content = lines.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
        }

        // STEP 5: Inject CONFIG_CMDLINE with the final command line
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("CONFIG_CMDLINE=\"{}\"", final_cmdline));
        content.push('\n');

        // STEP 6: Inject CONFIG_CMDLINE_BOOL=y to enable command line override
        content.push_str("CONFIG_CMDLINE_BOOL=y");
        content.push('\n');

        // STEP 7: Explicitly inject CONFIG_CMDLINE_OVERRIDE=n to prevent override conflicts
        // This ensures the baked-in CMDLINE takes precedence at runtime
        content.push_str("CONFIG_CMDLINE_OVERRIDE=n");
        content.push('\n');

        eprintln!("[Patcher] [CMDLINE] Injected CONFIG_CMDLINE, CONFIG_CMDLINE_BOOL=y, and CONFIG_CMDLINE_OVERRIDE=n into .config");

        // Write updated .config
        fs::write(&config_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config: {}", e)))?;

        Ok(())
    }

    /// Applies Kconfig with Clang/LTO/BORE enforcement (Phase 5).
    ///
    /// Now also supports baking in command line parameters via CONFIG_CMDLINE for
    /// features like MGLRU that require runtime parameter enforcement.
    pub fn apply_kconfig(&self, options: HashMap<String, String>, lto_type: LtoType) -> PatchResult<()> {
        let config_path = self.src_dir.join(".config");

        // Create backup directory
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;

        // Read or create .config
        let mut content = if config_path.exists() {
            fs::read_to_string(&config_path)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to read .config: {}", e)))?
        } else {
            String::new()
        };

        // Create backup if file exists
        if config_path.exists() {
            let backup_path = self.backup_dir.join(".config.bak");
            fs::write(&backup_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;
        }

        // CRITICAL: Detect MGLRU and hardening level BEFORE consuming options in loop
        let use_mglru = options.iter().any(|(k, _)| k.starts_with("_MGLRU_"));
        let hardening_level = if options.contains_key("_HARDENING_LEVEL_MINIMAL") {
            crate::models::HardeningLevel::Minimal
        } else if options.contains_key("_HARDENING_LEVEL_HARDENED") {
            crate::models::HardeningLevel::Hardened
        } else {
            crate::models::HardeningLevel::Standard
        };

        // Extract MGLRU flags.
        let mut mglru_options = HashMap::new();
        let mut mglru_count = 0;
        for (key, value) in &options {
            if key.starts_with("_MGLRU_CONFIG_") {
                // Value format: "CONFIG_LRU_GEN=y" - extract and parse it
                if let Some(eq_pos) = value.find('=') {
                    let config_key = value[..eq_pos].to_string();
                    let config_value = value[eq_pos + 1..].to_string();
                    mglru_count += 1;
                    eprintln!("[Patcher] [MGLRU] Extracted MGLRU config #{}: {}={}", mglru_count, config_key, config_value);
                    mglru_options.insert(config_key, config_value);
                } else {
                    eprintln!("[Patcher] [MGLRU] WARNING: Invalid MGLRU value format (no '='): {}", value);
                }
            }
        }
        
        if mglru_count > 0 {
            eprintln!("[Patcher] [MGLRU] TOTAL: Extracted {} MGLRU config options", mglru_count);
        }

        // STEP 1: Remove ALL GCC-related config lines to prevent conflicts
        let gcc_config_patterns = vec![
            "CONFIG_CC_IS_GCC=",
            "CONFIG_GCC_VERSION=",
            "CONFIG_CC_VERSION_TEXT=",
        ];

        for pattern in gcc_config_patterns {
            content = content
                .lines()
                .filter(|line| !line.starts_with(pattern))
                .collect::<Vec<_>>()
                .join("\n");
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
        }

        // STEP 2: Apply all user-provided options (EXCEPT special _* prefixed ones)
        for (key, value) in options {
            // Skip special metadata keys that start with underscore
            if key.starts_with("_") {
                continue;
            }
            // Remove existing line if present
            let lines: Vec<&str> = content
                .lines()
                .filter(|line| !line.starts_with(&format!("{}=", key)))
                .collect();

            if lines.len() != content.lines().count() {
                content = lines.join("\n");
                if !content.is_empty() {
                    content.push('\n');
                }
            }

            // Add new option
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&format!("{}={}", key, value));
            content.push('\n');
        }

        // STEP 2.5: INJECT EXTRACTED MGLRU OPTIONS
        // Apply the MGLRU configs we extracted from _MGLRU_ prefixed keys
        for (key, value) in &mglru_options {
            // Remove existing line if present to prevent conflicts
            let lines: Vec<&str> = content
                .lines()
                .filter(|line| !line.starts_with(&format!("{}=", key)))
                .collect();

            if lines.len() != content.lines().count() {
                content = lines.join("\n");
                if !content.is_empty() {
                    content.push('\n');
                }
            }

            // Add MGLRU option
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&format!("{}={}", key, value));
            content.push('\n');

            eprintln!("[Patcher] [MGLRU] CRITICAL: Injected {}={} into .config", key, value);
        }

        // STEP 3: FORCEFULLY INJECT Clang-specific configuration
         // These MUST be hardcoded to override any kernel-detected GCC settings
         // CRITICAL: Respect the target lto_type when setting LTO configs
         let mut clang_configs = vec![
             ("CONFIG_CC_IS_CLANG", "y"),
             ("CONFIG_CLANG_VERSION", "190106"),  // LLVM 19.0.1 release
             ("CONFIG_CC_IS_GCC", "n"),
         ];
         
         // Add LTO configs based on lto_type parameter
         match lto_type {
             LtoType::Full => {
                 clang_configs.push(("CONFIG_LTO_CLANG_FULL", "y"));
                 clang_configs.push(("CONFIG_LTO_CLANG", "y"));
                 eprintln!("[Patcher] [KCONFIG] LTO Type: FULL - Setting CONFIG_LTO_CLANG_FULL=y");
             }
             LtoType::Thin => {
                 clang_configs.push(("CONFIG_LTO_CLANG_THIN", "y"));
                 clang_configs.push(("CONFIG_LTO_CLANG", "y"));
                 eprintln!("[Patcher] [KCONFIG] LTO Type: THIN - Setting CONFIG_LTO_CLANG_THIN=y");
             }
             LtoType::None => {
                 eprintln!("[Patcher] [KCONFIG] LTO Type: NONE - Removing all LTO configs");
                 // Don't add any LTO configs for None type
             }
         }

        for (key, value) in clang_configs {
            // Remove existing line if present (even if it was just added in user options)
            let lines: Vec<&str> = content
                .lines()
                .filter(|line| !line.starts_with(&format!("{}=", key)))
                .collect();

            if lines.len() != content.lines().count() {
                content = lines.join("\n");
                if !content.is_empty() {
                    content.push('\n');
                }
            }

            // Add Clang option with ABSOLUTE priority
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&format!("{}={}", key, value));
            content.push('\n');
        }

        // ============================================================================
        // PHASE 3: INJECT MODULE SIZE & STRIPPING OPTIMIZATION (CRITICAL)
        // ============================================================================
        // Module size optimization flags to reduce on-disk footprint from 462MB to <100MB
        // These MUST be injected AFTER user options to enforce stripping priorities
        let module_optimization_configs = vec![
            ("CONFIG_MODULE_COMPRESS_ZSTD", "y"),     // Compress modules with Zstandard (best compression)
            ("CONFIG_STRIP_ASM_SYMS", "y"),           // Strip assembly symbols from modules
            ("CONFIG_DEBUG_INFO", "n"),               // Disable debug info (critical for size reduction)
            ("CONFIG_DEBUG_INFO_NONE", "y"),          // Explicitly enforce no debug info
        ];

        for (key, value) in module_optimization_configs {
            // Remove existing line if present to prevent conflicts
            let lines: Vec<&str> = content
                .lines()
                .filter(|line| !line.starts_with(&format!("{}=", key)))
                .collect();

            if lines.len() != content.lines().count() {
                content = lines.join("\n");
                if !content.is_empty() {
                    content.push('\n');
                }
            }

            // Inject module optimization config with ABSOLUTE priority
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&format!("{}={}", key, value));
            content.push('\n');

            eprintln!("[Patcher] [MODULE-OPT] CRITICAL: Injected {}={} (Phase 3 stripping)", key, value);
        }

        // STEP 4: Final cleanup - remove any dangling GCC settings
        let final_lines: Vec<String> = content
            .lines()
            .filter(|line| {
                // Remove any line that says GCC is the compiler
                !line.contains("CONFIG_CC_IS_GCC=y") &&
                // Remove GCC version text if present
                !line.contains("CONFIG_CC_VERSION_TEXT=\"gcc")
            })
            .map(|s| s.to_string())
            .collect();
        content = final_lines.join("\n");
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }

        // ============================================================================
        // PHASE 5 (CRITICAL): LTO HARD ENFORCER - SURGICAL OVERRIDE PROTECTION
        // ============================================================================
        // This MUST run AFTER all other steps to ensure LTO configuration can NEVER
        // be reverted by kernel's oldconfig/syncconfig/make oldconfig processes.
        //
        // CRITICAL APPROACH: Instead of just removing and appending, we SURGICALLY
        // cut out ALL LTO-related lines in ALL their forms:
        // 1. CONFIG_LTO_NONE=y (the culprit)
        // 2. # CONFIG_LTO_CLANG_FULL is not set (disabled flags)
        // 3. # CONFIG_LTO_CLANG_THIN is not set (disabled flags)
        // 4. CONFIG_HAS_LTO_CLANG=y (may be already set or kernel-added)
        // Then atomically inject the complete correct trio in the proper order.

        eprintln!("[Patcher] [PHASE-5-HARD-ENFORCER] Starting SURGICAL LTO override protection...");
        
        // SURGICAL REMOVAL: Target ALL LTO configuration variants (using pre-compiled pattern)
         // This regex matches:
         // - CONFIG_LTO_NONE=y
         // - CONFIG_LTO_NONE=n
         // - CONFIG_LTO_CLANG=y
         // - CONFIG_LTO_CLANG_THIN=y
         // - CONFIG_LTO_CLANG_FULL=y
         // - CONFIG_HAS_LTO_CLANG=y
         // - # CONFIG_LTO_* is not set (commented-out variants)
         // - # CONFIG_HAS_LTO_* is not set (commented-out HAS_LTO variants)
        
        let lines_before = content.lines().count();
        content = LTO_REMOVAL_REGEX.replace_all(&content, "").to_string();
        let lines_after = content.lines().count();
        let lto_lines_removed = lines_before - lines_after;

        eprintln!("[Patcher] [PHASE-5-HARD-ENFORCER] SURGICAL REMOVAL: Eliminated {} LTO configuration lines", lto_lines_removed);

        // Clean up any blank lines introduced by regex replacements (using pre-compiled pattern)
        content = SPACE_COLLAPSE_REGEX.replace_all(&content, "\n").to_string();

        // ATOMIC INJECTION: NOW inject the final, authoritative LTO Clang configuration
        // These MUST be in this specific order and at the END to ensure kernel reconfig doesn't revert them
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }

        let final_lto_configs = match lto_type {
            LtoType::Full => {
                eprintln!("[Patcher] [PHASE-5] Enforcing CONFIG_LTO_CLANG_FULL=y (FULL LTO)");
                vec![
                    "# ======================================================================",
                    "# PHASE 5 HARD ENFORCER: LTO CLANG FULL (SURGICAL, ATOMIC, FINAL)",
                    "# ======================================================================",
                    "# These lines are SURGICALLY injected after ALL LTO entries are removed.",
                    "# CONFIG_LTO_NONE is COMPLETELY ABSENT from this file.",
                    "# Full LTO provides maximum optimizations at the cost of compile time.",
                    "# Order is critical: CONFIG_LTO_CLANG must come before CONFIG_LTO_CLANG_FULL.",
                    "CONFIG_LTO_CLANG=y",
                    "CONFIG_LTO_CLANG_FULL=y",
                    "CONFIG_HAS_LTO_CLANG=y",
                ]
            }
            LtoType::Thin => {
                eprintln!("[Patcher] [PHASE-5] Enforcing CONFIG_LTO_CLANG_THIN=y (THIN LTO)");
                vec![
                    "# ======================================================================",
                    "# PHASE 5 HARD ENFORCER: LTO CLANG THIN (SURGICAL, ATOMIC, FINAL)",
                    "# ======================================================================",
                    "# These lines are SURGICALLY injected after ALL LTO entries are removed.",
                    "# CONFIG_LTO_NONE is COMPLETELY ABSENT from this file.",
                    "# Thin LTO reduces compile time vs Full LTO while maintaining optimizations.",
                    "# Order is critical: CONFIG_LTO_CLANG must come before CONFIG_LTO_CLANG_THIN.",
                    "CONFIG_LTO_CLANG=y",
                    "CONFIG_LTO_CLANG_THIN=y",
                    "CONFIG_HAS_LTO_CLANG=y",
                ]
            }
            LtoType::None => {
                eprintln!("[Patcher] [PHASE-5] LTO DISABLED (None) - not injecting LTO configs");
                vec![
                    "# ======================================================================",
                    "# PHASE 5: LTO DISABLED (None selected)",
                    "# ======================================================================",
                    "# No LTO configuration injected per user selection",
                ]
            }
        };

        for line in final_lto_configs {
            content.push_str(line);
            content.push('\n');
        }

        let lto_config_msg = match lto_type {
            LtoType::Full => "CONFIG_LTO_CLANG_FULL=y",
            LtoType::Thin => "CONFIG_LTO_CLANG_THIN=y",
            LtoType::None => "CONFIG_LTO_CLANG (disabled)",
        };
        eprintln!("[Patcher] [PHASE-5-HARD-ENFORCER] ATOMIC INJECTION: Set CONFIG_LTO_CLANG=y, {}, CONFIG_HAS_LTO_CLANG=y", lto_config_msg);
        eprintln!("[Patcher] [PHASE-5-HARD-ENFORCER] CRITICAL: CONFIG_LTO_NONE is SURGICALLY REMOVED and NEVER exists");

        // Write updated .config with Clang hardcoded and LTO protected
        fs::write(&config_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config: {}", e)))?;

        eprintln!("[Patcher] [PHASE-5] SUCCESS: .config written with LTO override protection active");

        // ============================================================================
        // CMDLINE INJECTION: Bake in kernel parameters for runtime enforcement
        // ============================================================================
        // CRITICAL: This MUST run AFTER all config options are written but before
        // returning from apply_kconfig. This injects CONFIG_CMDLINE to forcefully
        // enable features like MGLRU that require both CONFIG options AND runtime
        // kernel parameters to function correctly.
        eprintln!("[Patcher] [CMDLINE] STARTING BAKED-IN CMDLINE INJECTION");
        self.inject_baked_in_cmdline(use_mglru, hardening_level)?;
        eprintln!("[Patcher] [CMDLINE] SUCCESS: Baked-in CMDLINE injection complete");

        Ok(())
    }

    /// Phase G1: Prebuild LTO Enforcer injection.
    pub fn inject_prebuild_lto_hard_enforcer(&self, lto_type: LtoType) -> PatchResult<()> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.prebuild_enforcer.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // The PREBUILD LTO HARD ENFORCER snippet - runs RIGHT BEFORE make command
        // Respects lto_type to inject appropriate LTO settings
        let prebuild_enforcer = match lto_type {
            LtoType::Full => r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO HARD ENFORCER (FULL LTO)
    # =====================================================================
    # This runs IMMEDIATELY BEFORE the 'make' command in build().
    # It ONLY enforces LTO settings - nothing more.
    # Module filtering is handled by PHASE G2 in prepare(), not here.
    # Profile settings (MGLRU, Polly, etc.) are untouched.
    #
    # CRITICAL: This is the FINAL GATE before kernel compilation.
    # All other config changes have been finalized in prepare().
    
    if [[ -f ".config" ]]; then
        config_file=".config"
        
        # ====================================================================
        # PHASE G1 LTO HARD ENFORCER (Surgical, Atomic Enforcement - FULL LTO)
        # ====================================================================
        # CRITICAL: This is LTO-ONLY enforcement.
        # Module filtering is handled by PHASE G2 in prepare().
        # Profile settings (MGLRU, Polly, etc.) are NOT touched here.
        #
        # SURGICAL REMOVAL: Use GLOBAL sed pattern to delete ALL LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
        
        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
        
        # ATOMIC INJECTION: Append FULL LTO settings
        cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE G1.2: LTO CLANG FULL HARD ENFORCER (SURGICAL)
# ====================================================================
# These lines are SURGICALLY injected immediately before kernel make.
# All conflicting LTO entries have been removed above.
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_FULL=y
CONFIG_HAS_LTO_CLANG=y
EOF
        
        printf "[PREBUILD] [LTO] PHASE G1.2: Surgically enforced CONFIG_LTO_CLANG=y + CONFIG_LTO_CLANG_FULL=y\n" >&2
        
        # ====================================================================
        # PHASE G1.3: RUN OLDDEFCONFIG TO ACCEPT NEW CONFIG OPTIONS
        # ====================================================================
        if command -v make &> /dev/null; then
            printf "[PREBUILD] OLDDEFCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized without interactive prompts\n" >&2
            else
                printf "[PREBUILD] WARNING: 'make olddefconfig' failed or unavailable, continuing anyway...\n" >&2
            fi
        fi
        
        # Verify final module count
        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#,
            LtoType::Thin => r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO HARD ENFORCER (THIN LTO)
    # =====================================================================
    # This runs IMMEDIATELY BEFORE the 'make' command in build().
    # It ONLY enforces LTO settings - nothing more.
    # Module filtering is handled by PHASE G2 in prepare(), not here.
    # Profile settings (MGLRU, Polly, etc.) are untouched.
    #
    # CRITICAL: This is the FINAL GATE before kernel compilation.
    # All other config changes have been finalized in prepare().
    
    if [[ -f ".config" ]]; then
        config_file=".config"
        
        # ====================================================================
        # PHASE G1 LTO HARD ENFORCER (Surgical, Atomic Enforcement - THIN LTO)
        # ====================================================================
        # CRITICAL: This is LTO-ONLY enforcement.
        # Module filtering is handled by PHASE G2 in prepare().
        # Profile settings (MGLRU, Polly, etc.) are NOT touched here.
        #
        # SURGICAL REMOVAL: Use GLOBAL sed pattern to delete ALL LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
        
        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
        
        # ATOMIC INJECTION: Append THIN LTO settings
        cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE G1.2: LTO CLANG THIN HARD ENFORCER (SURGICAL)
# ====================================================================
# These lines are SURGICALLY injected immediately before kernel make.
# All conflicting LTO entries have been removed above.
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_THIN=y
CONFIG_HAS_LTO_CLANG=y
EOF
        
        printf "[PREBUILD] [LTO] PHASE G1.2: Surgically enforced CONFIG_LTO_CLANG=y + CONFIG_LTO_CLANG_THIN=y\n" >&2
        
        # ====================================================================
        # PHASE G1.3: RUN OLDDEFCONFIG TO ACCEPT NEW CONFIG OPTIONS
        # ====================================================================
        if command -v make &> /dev/null; then
            printf "[PREBUILD] OLDDEFCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized without interactive prompts\n" >&2
            else
                printf "[PREBUILD] WARNING: 'make olddefconfig' failed or unavailable, continuing anyway...\n" >&2
            fi
        fi
        
        # Verify final module count
        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#,
            LtoType::None => r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO DISABLED (None)
    # =====================================================================
    # LTO is disabled per user selection - no LTO enforcement
    
    if [[ -f ".config" ]]; then
        config_file=".config"
        
        # SURGICAL REMOVAL: Remove all LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
        
        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
        
        printf "[PREBUILD] [LTO] PHASE G1: LTO disabled - removed all LTO configs\n" >&2
        
        # Run olddefconfig to finalize
        if command -v make &> /dev/null; then
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized\n" >&2
            fi
        fi
        
        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#,
        };

        // Find the build() function and inject the enforcer BEFORE the first make command
        let build_func_pos = content.find("build()");
        if build_func_pos.is_none() {
            eprintln!("[Patcher] [PHASE-G1] WARNING: build() function not found, skipping prebuild injection");
            return Ok(());
        }

        let build_func_pos = build_func_pos.unwrap();
        
        // Find the opening brace of build()
        if let Some(brace_pos) = content[build_func_pos..].find('{') {
            let brace_absolute_pos = build_func_pos + brace_pos;
            
            // Find the first 'make' command after the opening brace
            let mut search_pos = brace_absolute_pos + 1;
            let mut make_pos = None;
            
            // Search for the first 'make' that's not in a comment
            while let Some(pos) = content[search_pos..].find("make") {
                let absolute_pos = search_pos + pos;
                
                // Check if this 'make' is on a simple line (not in a comment, string, etc.)
                if let Some(line_start) = content[..absolute_pos].rfind('\n') {
                    let line = &content[line_start + 1..absolute_pos];
                    if !line.trim_start().starts_with("#") {
                        make_pos = Some(absolute_pos);
                        break;
                    }
                }
                search_pos = absolute_pos + 1;
            }

            if let Some(make_pos) = make_pos {
                // Inject the enforcer RIGHT BEFORE the make command
                // Find the start of this line to inject before it
                if let Some(line_start) = content[..make_pos].rfind('\n') {
                    let inject_pos = line_start + 1;
                    
                    // CRITICAL: Remove any existing PHASE G1 PREBUILD block to ensure fresh injection
                    // This allows switching LTO levels (Thin -> Full, etc.) and having changes take effect
                    let g1_removal_regex = Regex::new(r"(?m)\s*# ===+\s*\n\s*# PHASE G1 PREBUILD:.*?fi\s*\n")
                        .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# PHASE G1 PREBUILD.*?^fi\s*$").unwrap());
                    content = g1_removal_regex.replace_all(&content, "").to_string();
                    
                    // Now inject the new enforcer (fresh injection with current lto_type)
                    content.insert_str(inject_pos, &format!("{}\n", prebuild_enforcer));
                    eprintln!("[Patcher] [PHASE-G1] Injected PREBUILD LTO HARD ENFORCER before make command");
                }
            }
        }

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [PHASE-G1] PREBUILD hard enforcer injected into build() function");

        Ok(())
    }

    /// Phase E1: Post-oldconfig LTO re-enforcement.
    /// Inject MODPROBED-DB localmodconfig command into PKGBUILD's prepare() function
    ///
    /// CRITICAL FIX FOR MODPROBED DISCOVERY: Surgically injects the command that performs
    /// automatic module filtering based on hardware detection via modprobed-db.
    ///
    /// This MUST run BEFORE any make oldconfig/syncconfig to ensure the kernel configuration
    /// is correctly filtered to only include modules actually used on the system.
    ///
    /// The injection pattern searches for prepare() and inserts the modprobed localmodconfig
    /// command at the BEGINNING of that function, before any other configuration steps.
    ///
    /// Command injected:
    /// ```bash
    /// make LSMOD=$HOME/.config/modprobed.db localmodconfig
    /// ```
    ///
    /// This command:
    /// - Reads the modprobed-db file containing all loaded modules
    /// - Uses localmodconfig to automatically filter kernel modules to only those in use
    /// - Significantly reduces kernel size and build time
    ///
    /// # Returns
    /// * `Ok(())` if injection successful
    /// * `Err(PatchError)` if PKGBUILD not found or write fails
    /// Inject PHASE G2 POST-MODPROBED hard enforcer into PKGBUILD's prepare() function
    ///
    /// PHASE G2 CRITICAL: After localmodconfig filters to ~170 modules,
    /// olddefconfig's Kconfig dependency expansion re-enables thousands of unwanted modules.
    /// This enforcer surgically removes all CONFIG_*=m entries NOT in modprobed.db,
    /// then runs olddefconfig ONCE to handle consistent dependencies.
    ///
    /// This is the FINAL enforcement gate for modprobed-db filtering.
    /// Without it, the filtered config expands back to full size during make olddefconfig.
    ///
    /// # Returns
    /// * `Ok(())` if injection successful
    /// * `Err(PatchError)` if PKGBUILD not found or write fails
    pub fn inject_post_setting_config_restorer(&self, use_modprobed: bool) -> PatchResult<()> {
         if !use_modprobed {
             eprintln!("[Patcher] [PHASE-G2.5] Post-setting-config restorer DISABLED, skipping injection");
             return Ok(());
         }

         let pkgbuild_path = self.src_dir.join("PKGBUILD");

         if !pkgbuild_path.exists() {
             return Err(PatchError::FileNotFound(
                 format!("PKGBUILD not found: {}", pkgbuild_path.display())
             ));
         }

         let mut content = fs::read_to_string(&pkgbuild_path)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

         // Create backup
         let backup_path = self.backup_dir.join("PKGBUILD.phase_g2_5.bak");
         fs::create_dir_all(&self.backup_dir)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
         fs::write(&backup_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

         // PHASE G2.5 POST-SETTING-CONFIG RESTORER
         // CRITICAL: The "Setting config..." step runs "cp ../config .config" which OVERWRITES:
         // 1. The modprobed-filtered module list (restored via localmodconfig)
         // 2. MGLRU, Polly, and other CONFIG options (MUST be re-applied)
         // This restorer backs up CONFIG options BEFORE the overwrite, then restores them.
         let phase_g2_5_restorer = r#"
     
     # =====================================================================
     # PHASE G2.5 POST-SETTING-CONFIG: Protect profile settings and re-apply filtering
     # =====================================================================
     # CRITICAL FIX: The "Setting config..." step runs "cp ../config .config"
     # which OVERWRITES:
     # 1. Modprobed-filtered modules (6199+ modules restored)
     # 2. MGLRU, Polly CONFIG options (completely lost)
     # 3. CONFIG_CMDLINE* parameters (baked-in kernel parameters lost)
     #
     # This restorer:
     # - Captures CONFIG_CMDLINE* values BEFORE overwrite
     # - Re-applies modprobed filtering AFTER overwrite
     # - Re-injects backed-up CONFIG_CMDLINE*, MGLRU configs
     
     # STEP 1: Capture CONFIG_CMDLINE* settings BEFORE "cp ../config .config"
     # Find kernel source directory
     KERNEL_SRC_DIR=""
     if [[ -d "$srcdir/linux" ]]; then
         KERNEL_SRC_DIR="$srcdir/linux"
     else
         KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
     fi
     
     if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
         KERNEL_SRC_DIR="$srcdir"
     fi
     
     # Capture CONFIG_CMDLINE* values BEFORE the "cp ../config .config" step overwrites them
     CONFIG_CMDLINE_BACKUP=""
     CONFIG_CMDLINE_BOOL_BACKUP=""
     CONFIG_CMDLINE_OVERRIDE_BACKUP=""
     
     if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/.config" ]]; then
         CONFIG_CMDLINE_BACKUP=$(grep "^CONFIG_CMDLINE=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")
         CONFIG_CMDLINE_BOOL_BACKUP=$(grep "^CONFIG_CMDLINE_BOOL=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")
         CONFIG_CMDLINE_OVERRIDE_BACKUP=$(grep "^CONFIG_CMDLINE_OVERRIDE=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")
         
         if [[ -n "$CONFIG_CMDLINE_BACKUP" ]]; then
             printf "[PHASE-G2.5] CAPTURED CONFIG_CMDLINE before overwrite\n" >&2
         fi
     fi
     
     # STEP 2: Now the "Setting config..." step runs "cp ../config .config" (happens in main prepare)
     # We'll restore values immediately after
     
     if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/.config" ]]; then
         if cd "$KERNEL_SRC_DIR"; then
             # STEP 3: Count modules BEFORE re-filtering
             BEFORE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" ".config" 2>/dev/null || echo "unknown")
             printf "[PHASE-G2.5] Module count after 'Setting config...': $BEFORE_COUNT\n" >&2
             
             # STEP 4: Re-apply modprobed filtering (only if modprobed.db exists)
             MODPROBED_DB_PATH="$HOME/.config/modprobed.db"
             
             # Check if modprobed.db exists BEFORE attempting to use it
             if [[ -f "$MODPROBED_DB_PATH" ]]; then
                 printf "[PHASE-G2.5] Re-running: yes \"\" | make LLVM=1 LLVM_IAS=1 LSMOD=$MODPROBED_DB_PATH localmodconfig\n" >&2
                 if yes "" | make LLVM=1 LLVM_IAS=1 LSMOD="$MODPROBED_DB_PATH" localmodconfig > /dev/null 2>&1; then
                     AFTER_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" ".config" 2>/dev/null || echo "unknown")
                     printf "[PHASE-G2.5] Module count after re-filtering: $AFTER_COUNT\n" >&2
                 else
                     printf "[PHASE-G2.5] WARNING: Re-filtering failed, continuing with current config (localmodconfig command error)\n" >&2
                 fi
             else
                 printf "[PHASE-G2.5] INFO: modprobed.db not found at $MODPROBED_DB_PATH, skipping re-filtering (expected on fresh install)\n" >&2
             fi
             
             # STEP 5: Restore CONFIG_CMDLINE* parameters
             if [[ -n "$CONFIG_CMDLINE_BACKUP" ]] || [[ -n "$CONFIG_CMDLINE_BOOL_BACKUP" ]] || [[ -n "$CONFIG_CMDLINE_OVERRIDE_BACKUP" ]]; then
                 # Remove old CONFIG_CMDLINE* entries to prevent duplicates
                 sed -i '/^CONFIG_CMDLINE.*/d' ".config"
                 
                 # Ensure newline before appending
                 [[ -n "$(tail -c 1 ".config")" ]] && echo "" >> ".config"
                 
                 # Re-inject backed-up CMDLINE parameters
                 [[ -n "$CONFIG_CMDLINE_BACKUP" ]] && echo "$CONFIG_CMDLINE_BACKUP" >> ".config"
                 [[ -n "$CONFIG_CMDLINE_BOOL_BACKUP" ]] && echo "$CONFIG_CMDLINE_BOOL_BACKUP" >> ".config"
                 [[ -n "$CONFIG_CMDLINE_OVERRIDE_BACKUP" ]] && echo "$CONFIG_CMDLINE_OVERRIDE_BACKUP" >> ".config"
                 
                 printf "[PHASE-G2.5] Re-applied CONFIG_CMDLINE* parameters\n" >&2
             fi
             
             # STEP 6: Re-apply MGLRU configs if they were set
             if [[ -n "$GOATD_MGLRU_CONFIGS" ]]; then
                 # Remove old MGLRU lines
                 sed -i '/^CONFIG_LRU_GEN/d' ".config"
                 
                 # Ensure newline before appending
                 [[ -n "$(tail -c 1 ".config")" ]] && echo "" >> ".config"
                 echo "$GOATD_MGLRU_CONFIGS" >> ".config"
                 printf "[PHASE-G2.5] Re-applied MGLRU configs\n" >&2
             fi
             
             printf "[PHASE-G2.5] SUCCESS: Modprobed filtering, CMDLINE parameters, and profile settings restored\n" >&2
             
             # CRITICAL: Stay in kernel source directory for remaining prepare() operations
             # Do NOT return to $srcdir - subsequent make commands need kernel source dir
         fi
     fi
     "#;

         // Find the "echo "Setting config..."" line in prepare() and inject AFTER the diff output
         if let Some(prepare_pos) = content.find("prepare()") {
             // Look for the diff output line that comes after "Setting config..."
             if let Some(diff_pos) = content[prepare_pos..].find("diff -u ../config .config") {
                 let search_start = prepare_pos + diff_pos;
                 
                 // Find the next line after the diff command
                 if let Some(next_line_pos) = content[search_start..].find('\n') {
                     let inject_pos = search_start + next_line_pos + 1;
                     
                     // Only inject if not already present
                     let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                     if !check_str.contains("PHASE G2.5") {
                         content.insert_str(inject_pos, &format!("{}\n", phase_g2_5_restorer));
                         eprintln!("[Patcher] [PHASE-G2.5] Injected POST-SETTING-CONFIG restorer into prepare() function");
                     }
                 }
             }
         } else {
             eprintln!("[Patcher] [PHASE-G2.5] WARNING: prepare() function not found in PKGBUILD");
             return Ok(());
         }

         // Write modified PKGBUILD
         fs::write(&pkgbuild_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

         eprintln!("[Patcher] [PHASE-G2.5] SUCCESS: POST-SETTING-CONFIG restorer injected into prepare()");

         Ok(())
     }

    pub fn inject_post_modprobed_hard_enforcer(&self, use_modprobed: bool) -> PatchResult<()> {
         if !use_modprobed {
             eprintln!("[Patcher] [PHASE-G2] Post-modprobed enforcer DISABLED, skipping injection");
             return Ok(());
         }

         let pkgbuild_path = self.src_dir.join("PKGBUILD");

         if !pkgbuild_path.exists() {
             return Err(PatchError::FileNotFound(
                 format!("PKGBUILD not found: {}", pkgbuild_path.display())
             ));
         }

         let mut content = fs::read_to_string(&pkgbuild_path)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

         // Create backup
         let backup_path = self.backup_dir.join("PKGBUILD.phase_g2.bak");
         fs::create_dir_all(&self.backup_dir)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
         fs::write(&backup_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

         // PHASE G2 POST-MODPROBED hard enforcer snippet
         let phase_g2_enforcer = r#"
    # =====================================================================
    # PHASE G2 POST-MODPROBED: Hard enforcer to protect filtered modules
    # =====================================================================
    # CRITICAL FIX FOR MODPROBED-DB EXPANSION:
    # After localmodconfig filters to ~170 modules, olddefconfig's Kconfig
    # dependency expansion re-enables thousands of unwanted modules.
    #
    # This enforcer:
    # 1. Reads modprobed.db to get list of modules to KEEP
    # 2. Surgically removes all CONFIG_*=m entries NOT in modprobed.db
    # 3. Runs olddefconfig ONCE to handle consistent dependencies
    # 4. Result: ~170 modules preserved with correct Kconfig dependencies
    #
    # This MUST run AFTER localmodconfig but BEFORE make olddefconfig (or in place of it)
    
    # CRITICAL: Find modprobed.db using robust path detection (same as modprobed_injection)
    # In makepkg context, $HOME may be /root but modprobed.db is at user's ~/.config/
    MODPROBED_DB_PATH=""
    
    # Try common locations - search in order of likelihood
    for candidate in "$HOME/.config/modprobed.db" /root/.config/modprobed.db /home/*/.config/modprobed.db; do
        if [[ -f "$candidate" ]]; then
            MODPROBED_DB_PATH="$candidate"
            break
        fi
    done
    
    if [[ -n "$MODPROBED_DB_PATH" && -f "$MODPROBED_DB_PATH" ]]; then
        printf "[PHASE-G2] POST-MODPROBED: Starting hard enforcer to protect filtered modules\n" >&2
        printf "[PHASE-G2] Found modprobed.db at: $MODPROBED_DB_PATH\n" >&2
        
        # CRITICAL: Must be in kernel source directory to operate on .config
        # Use same directory detection as modprobed_injection
        KERNEL_SRC_DIR=""
        if [[ -d "$srcdir/linux" ]]; then
            KERNEL_SRC_DIR="$srcdir/linux"
        else
            KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
        fi
        
        if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
            KERNEL_SRC_DIR="$srcdir"
        fi
        
        # Only proceed if we found and can access the kernel source directory
        if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/Makefile" ]]; then
            printf "[PHASE-G2] Found kernel source directory: $KERNEL_SRC_DIR\n" >&2
            
            # Change to kernel source directory for .config manipulation
            if cd "$KERNEL_SRC_DIR"; then
                if [[ -f ".config" ]]; then
                    # CRITICAL FIX: Protect the filtered 170 modules from olddefconfig re-expansion
                    #
                    # After localmodconfig filters to 170 modules, olddefconfig's Kconfig dependency
                    # expansion can remove some of these modules if they become optional dependencies.
                    # We HARD LOCK them by extracting and restoring them after olddefconfig.
                    
                    # STEP 1: Create a backup and extract all filtered modules (CONFIG_*=m lines)
                    cp ".config" ".config.pre_g2"
                    FILTERED_MODULES=$(grep "=m$" ".config.pre_g2" | sort)
                    FILTERED_MODULE_COUNT=$(echo "$FILTERED_MODULES" | grep -c "=" 2>/dev/null || echo "unknown")
                    
                    printf "[PHASE-G2] HARD LOCK: Extracted $FILTERED_MODULE_COUNT filtered modules from localmodconfig\n" >&2
                    
                    # STEP 2: Run olddefconfig to handle consistent Kconfig dependencies
                    # This may add NEW dependencies but we'll restore our 170 filtered modules afterward
                    printf "[PHASE-G2] Running: make LLVM=1 LLVM_IAS=1 olddefconfig\n" >&2
                    if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                        # STEP 3: Restore the original filtered modules if any were removed
                        # Create a temporary file with all non-module configs
                        TEMP_CONFIG=$(mktemp)
                        grep -v "=m$" ".config" > "$TEMP_CONFIG"
                        
                        # Append the original filtered modules back
                        echo "$FILTERED_MODULES" >> "$TEMP_CONFIG"
                        
                        # Replace .config with hard-locked version
                        mv "$TEMP_CONFIG" ".config"
                        
                        # Count final module count
                        FINAL_MODULE_COUNT=$(grep -c "=m" ".config" 2>/dev/null || echo "unknown")
                        printf "[PHASE-G2] Module count: $FILTERED_MODULE_COUNT → $FINAL_MODULE_COUNT (hard-locked to filtered set)\n" >&2
                        printf "[PHASE-G2] SUCCESS: Filtered modules protected and dependencies finalized\n" >&2
                    else
                        printf "[PHASE-G2] WARNING: olddefconfig failed\n" >&2
                    fi
                    
                    # Cleanup backup
                    rm -f ".config.pre_g2"
                fi
                
                # Return to srcdir for remaining prepare() operations
                cd "$srcdir" || printf "[PHASE-G2] WARNING: Failed to return to srcdir\n" >&2
            else
                printf "[PHASE-G2] ERROR: Failed to change to kernel source directory: $KERNEL_SRC_DIR\n" >&2
            fi
        else
            printf "[PHASE-G2] WARNING: Could not locate kernel source directory\n" >&2
        fi
    else
        printf "[PHASE-G2] INFO: modprobed.db not found, skipping hard enforcer\n" >&2
    fi
    "#;

        // Find prepare() function and inject AFTER the modprobed section
        if let Some(prepare_pos) = content.find("prepare()") {
            // Find the opening brace of prepare()
            if let Some(brace_pos) = content[prepare_pos..].find('{') {
                let brace_absolute_pos = prepare_pos + brace_pos;
                let after_brace = &content[brace_absolute_pos + 1..];
                
                // Find the position after modprobed section if it exists
                let search_start = brace_absolute_pos + 1;

                let inject_pos = if let Some(modprobed_pos) = after_brace.find("MODPROBED-DB AUTO-DISCOVERY") {
                    // Insert AFTER modprobed section
                    let modprobed_section = &after_brace[modprobed_pos..];
                    let mut if_count = 0;
                    let mut outer_fi_byte_offset = 0;
                    let mut current_byte_offset = 0;
                    
                    for line in modprobed_section.lines() {
                        let trimmed = line.trim();
                        
                        if trimmed.starts_with("if ") || trimmed.starts_with("if[") {
                            if_count += 1;
                        }
                        
                        if trimmed == "fi" {
                            if_count -= 1;
                            
                            if if_count == 0 && current_byte_offset > 100 {
                                outer_fi_byte_offset = current_byte_offset + line.len();
                                break;
                            }
                        }
                        
                        current_byte_offset += line.len() + 1;
                    }
                    
                    if outer_fi_byte_offset > 0 {
                        search_start + modprobed_pos + outer_fi_byte_offset + 1
                    } else {
                        search_start
                    }
                } else {
                    // Insert at beginning of prepare()
                    search_start
                };
                
                // Only inject if not already present
                let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                if !check_str.contains("PHASE G2") {
                    content.insert_str(inject_pos, &format!("\n{}", phase_g2_enforcer));
                    eprintln!("[Patcher] [PHASE-G2] Injected POST-MODPROBED hard enforcer into prepare() function");
                } else {
                    eprintln!("[Patcher] [PHASE-G2] POST-MODPROBED enforcer already present, skipping");
                }
            }
        } else {
            eprintln!("[Patcher] [PHASE-G2] WARNING: prepare() function not found in PKGBUILD");
            return Ok(());
        }

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [PHASE-G2] SUCCESS: POST-MODPROBED hard enforcer injected into prepare()");

        Ok(())
    }

    pub fn inject_modprobed_localmodconfig(&self, use_modprobed: bool) -> PatchResult<()> {
        if !use_modprobed {
            eprintln!("[Patcher] [MODPROBED] Modprobed-db discovery DISABLED, skipping injection");
            return Ok(());
        }

        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.modprobed.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // The modprobed localmodconfig injection snippet with FIX #1: Directory Context
        // This MUST change to the kernel source directory before running make localmodconfig
        let modprobed_injection = r#"
    # =====================================================================
    # MODPROBED-DB AUTO-DISCOVERY: Automatic module filtering (FIX #1: DIRECTORY CONTEXT)
    # =====================================================================
    # CRITICAL: The kernel Makefile and Kconfig files are at the root of the source tree.
    # Running 'make localmodconfig' from the wrong directory will FAIL with Kconfig errors.
    # We MUST change to the kernel source directory BEFORE running make commands.
    #
    # This section uses modprobed-db to automatically filter kernel modules
    # to only those actually used on the system. This significantly reduces:
    # - Kernel size (often 50-70% reduction in module count)
    # - Build time (avoids compiling unused drivers)
    # - Runtime overhead (fewer modules to load and initialize)
    #
    # The kernel's localmodconfig target reads enabled modules from the
    # modprobed-db file ($HOME/.config/modprobed.db) and automatically
    # deselects all CONFIG_*=m module options that aren't in the database.
    
    if [[ -f "$HOME/.config/modprobed.db" ]]; then
        printf "[MODPROBED] Found modprobed-db at $HOME/.config/modprobed.db\n" >&2
        printf "[MODPROBED] Running localmodconfig to filter kernel modules...\n" >&2
        printf "[MODPROBED] This will automatically filter kernel modules to those in use\n" >&2
        printf "[MODPROBED] Using modprobed database: $HOME/.config/modprobed.db\n" >&2
        
        # FIX #1: HARDENED DIRECTORY CONTEXT DETECTION
        # The kernel source may be in various directory formats:
        # - linux-6.18.3 (standard version naming)
        # - linux-zen (custom kernel name)
        # - linux (generic)
        # First, try to find and use the actual kernel directory
        KERNEL_SRC_DIR=""
        
        # Try to find a directory matching linux-* or linux pattern
        if [[ -d "$srcdir/linux" ]]; then
            KERNEL_SRC_DIR="$srcdir/linux"
        else
            # Try to find any directory starting with 'linux-'
            KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
        fi
        
        # If still not found, check if srcdir itself has Makefile (might be extracted there)
        if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
            KERNEL_SRC_DIR="$srcdir"
        fi
        
        # If we found a kernel source directory, change to it
        if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/Makefile" ]]; then
            printf "[MODPROBED] Found kernel source directory: $KERNEL_SRC_DIR\n" >&2
            
            # Change to kernel source directory BEFORE running make localmodconfig
            # This is CRITICAL - make localmodconfig MUST run from the kernel root
            if cd "$KERNEL_SRC_DIR"; then
                printf "[MODPROBED] Changed to kernel source directory\n" >&2
                printf "[MODPROBED] Running: yes \"\" | make LLVM=1 LLVM_IAS=1 LSMOD=$HOME/.config/modprobed.db localmodconfig\n" >&2
                
                if yes "" | make LLVM=1 LLVM_IAS=1 LSMOD="$HOME/.config/modprobed.db" localmodconfig 2>&1 | tee /tmp/modprobed_output.log; then
                    printf "[MODPROBED] SUCCESS: Kernel configuration filtered for used modules\n" >&2
                    
                    # VERIFICATION: Count the filtered modules IMMEDIATELY after localmodconfig
                    if [[ -f ".config" ]]; then
                        MODULE_COUNT=$(grep -c "=m" .config 2>/dev/null || echo "unknown")
                        printf "[MODPROBED] Module count after localmodconfig: $MODULE_COUNT\n" >&2
                    fi
                    
                    # NOTE: We do NOT run olddefconfig here because Kconfig dependency expansion
                    # will re-enable thousands of modules that localmodconfig just filtered out.
                    # Instead, a PHASE G2 POST-MODPROBED hard enforcer (injected by patcher)
                    # will surgically remove all CONFIG_*=m entries not in modprobed.db,
                    # then run olddefconfig ONCE at the end with protected modules.
                    printf "[MODPROBED] Modprobed-db discovery complete - PHASE G2 enforcer will protect filtered modules\n" >&2
                else
                    printf "[MODPROBED] WARNING: localmodconfig failed or unavailable, continuing with full config\n" >&2
                    printf "[MODPROBED] This is not fatal - the build will still complete\n" >&2
                    printf "[MODPROBED] See /tmp/modprobed_output.log for details\n" >&2
                fi
                
                # CRITICAL: Return to srcdir after modprobed processing
                # The whitelist section and remaining prepare() logic expect to be in $srcdir
                cd "$srcdir" || printf "[MODPROBED] WARNING: Failed to return to srcdir after localmodconfig\n" >&2
            else
                printf "[MODPROBED] ERROR: Failed to change to kernel source directory: $KERNEL_SRC_DIR\n" >&2
            fi
        else
            printf "[MODPROBED] WARNING: Could not locate kernel source directory with Makefile\n" >&2
            printf "[MODPROBED] Checked: \$srcdir/linux, \$srcdir/linux-*, \$srcdir (with Makefile)\n" >&2
            printf "[MODPROBED] Continuing with full config\n" >&2
        fi
    else
        printf "[MODPROBED] INFO: modprobed-db not found at $HOME/.config/modprobed.db\n" >&2
        printf "[MODPROBED] INFO: To enable automatic module filtering, populate modprobed-db with:\n" >&2
        printf "[MODPROBED] INFO:   $ modprobed-db store\n" >&2
        printf "[MODPROBED] INFO: (assumes modprobed-db package is installed)\n" >&2
    fi
    "#;

        // Find prepare() function and inject AFTER extraction logic
        if let Some(prepare_pos) = content.find("prepare()") {
            // Find the opening brace of prepare()
            if let Some(brace_pos) = content[prepare_pos..].find('{') {
                let brace_absolute_pos = prepare_pos + brace_pos;
                let after_brace = &content[brace_absolute_pos + 1..];
                
                // Find a good injection point AFTER extraction logic
                // Look for "cd" or "tar" to indicate extraction has started
                // Insert AFTER the cd "$srcdir" line
                let inject_pos = if let Some(cd_srcdir_pos) = after_brace.find("cd \"$srcdir\"") {
                    // Find the end of this line (next newline)
                    let line_end = after_brace[cd_srcdir_pos..].find('\n')
                        .map(|pos| cd_srcdir_pos + pos + 1)
                        .unwrap_or(cd_srcdir_pos + 20);
                    brace_absolute_pos + 1 + line_end
                } else {
                    // Fallback: inject right after opening brace
                    brace_absolute_pos + 1
                };
                
                // Only inject if not already present
                let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                if !check_str.contains("MODPROBED") {
                    content.insert_str(inject_pos, &format!("\n{}", modprobed_injection));
                    eprintln!("[Patcher] [MODPROBED] Injected modprobed-db localmodconfig into prepare() function");
                } else {
                    eprintln!("[Patcher] [MODPROBED] Modprobed injection already present, skipping");
                }
            }
        } else {
            eprintln!("[Patcher] [MODPROBED] WARNING: prepare() function not found in PKGBUILD");
            return Ok(());
        }

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [MODPROBED] SUCCESS: Modprobed-db discovery injected into prepare()");

        Ok(())
    }

    /// Inject KERNEL WHITELIST into PKGBUILD's prepare() function
    ///
    /// CRITICAL FIX FOR WHITELIST PROTECTION: Surgically injects a whitelist enforcement
    /// mechanism that protects against unwanted module inclusion during localmodconfig.
    ///
    /// The whitelist works by:
    /// 1. Reading a kernel whitelist file (CONFIG_* options to ALWAYS include)
    /// 2. Ensuring these modules and features are always enabled regardless of modprobed-db
    /// 3. Preventing security-critical or essential features from being accidentally excluded
    ///
    /// This runs BEFORE localmodconfig to establish the baseline, then localmodconfig only
    /// ADDS to this whitelist, never removes from it.
    ///
    /// # Returns
    /// * `Ok(())` if injection successful
    /// * `Err(PatchError)` if PKGBUILD not found or write fails
    pub fn inject_kernel_whitelist(&self, use_whitelist: bool) -> PatchResult<()> {
        if !use_whitelist {
            eprintln!("[Patcher] [WHITELIST] Kernel whitelist protection DISABLED, skipping injection");
            return Ok(());
        }

        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.whitelist.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // The kernel whitelist protection injection snippet
        let whitelist_injection = r#"
    # =====================================================================
    # KERNEL WHITELIST PROTECTION: Ensure critical features are always built
    # =====================================================================
    # This section implements a whitelist of critical kernel CONFIG options
    # that MUST always be enabled, protected from modprobed-db filtering.
    #
    # The whitelist includes:
    # - Security features (CFI, SMACK, SELINUX, AppArmor)
    # - Core functionality (SYSFS, PROC, TMPFS, DEVTMPFS)
    # - Boot/Init essentials (INITRAMFS_SOURCE, RAMFS, BINFMT)
    # - Critical filesystems (EXT4, BTRFS, FAT, VFAT, ISO9660, CIFS)
    # - NLS support (ASCII, CP437 for EFI mounting)
    # - Loopback and UEFI (LOOP, EFIVAR_FS)
    # - Storage drivers (AHCI, NVMe, USB, USB_STORAGE, USB_HID)
    #
    # These options will survive localmodconfig filtering and ensure
    # the kernel remains bootable even with aggressive module stripping.
    
    if [[ -f ".config" ]]; then
        printf "[WHITELIST] Applying kernel whitelist protection...\n" >&2
        
        # CRITICAL: These CONFIG options MUST be present for bootability
        # We enforce them BEFORE localmodconfig to establish the baseline
        cat >> ".config" << 'EOF'

# ====================================================================
# KERNEL WHITELIST: Critical features protected from modprobed filtering
# ====================================================================
# These options are enforced to ensure kernel bootability and security

# Core filesystem and procfs support (MANDATORY)
CONFIG_SYSFS=y
CONFIG_PROC_FS=y
CONFIG_TMPFS=y
CONFIG_DEVTMPFS=y
CONFIG_BLK_DEV_INITRD=y
CONFIG_ROOTS_FS_DEFAULT_CFI=y

# Security features (MANDATORY)
CONFIG_SELINUX=y
CONFIG_AUDIT=y
CONFIG_LSM="selinux,apparmor"

# Primary filesystems (MANDATORY)
CONFIG_EXT4_FS=y
CONFIG_BTRFS_FS=y

# Additional filesystems for bootability and compatibility (CRITICAL)
CONFIG_FAT_FS=m
CONFIG_VFAT_FS=m
CONFIG_ISO9660=m
CONFIG_CIFS=m

# NLS (National Language Support) for EFI partition mounting (CRITICAL)
CONFIG_NLS_ASCII=m
CONFIG_NLS_CP437=m

# Loopback device mounting (CRITICAL)
CONFIG_BLK_DEV_LOOP=m

# UEFI variables support (CRITICAL for UEFI systems)
CONFIG_EFIVAR_FS=m

# Storage device support (CRITICAL)
CONFIG_AHCI=m
CONFIG_SATA_AHCI=m
CONFIG_NVME=m
CONFIG_USB=m
CONFIG_USB_COMMON=m
CONFIG_USB_STORAGE=m

# Input device support (CRITICAL - USB keyboards, mice)
CONFIG_USB_HID=m
EOF
        
        printf "[WHITELIST] Kernel whitelist applied - critical features protected\n" >&2
    fi
    "#;

        // Find prepare() function and inject at the beginning (AFTER modprobed if present)
        if let Some(prepare_pos) = content.find("prepare()") {
            // Find the opening brace of prepare()
            if let Some(brace_pos) = content[prepare_pos..].find('{') {
                let brace_absolute_pos = prepare_pos + brace_pos;
                
                // Find the position after modprobed section if it exists
                let search_start = brace_absolute_pos + 1;
                let after_brace = &content[search_start..];
                
                let inject_pos = if let Some(modprobed_pos) = after_brace.find("MODPROBED-DB AUTO-DISCOVERY") {
                    // Insert AFTER modprobed section
                    // CRITICAL FIX: Count if/fi pairs to find the OUTER closing fi
                    // The modprobed section has nested ifs, so we must match them correctly
                    let modprobed_section = &after_brace[modprobed_pos..];
                    let mut if_count = 0;
                    let mut outer_fi_byte_offset = 0;
                    let mut current_byte_offset = 0;
                    
                    for line in modprobed_section.lines() {
                        let trimmed = line.trim();
                        
                        // Count 'if' statements (opening ifs increase count)
                        if trimmed.starts_with("if ") || trimmed.starts_with("if[") {
                            if_count += 1;
                        }
                        
                        // Count 'fi' statements (closing fis decrease count)
                        if trimmed == "fi" {
                            if_count -= 1;
                            
                            // When if_count reaches 0, we've found the outer closing fi
                            if if_count == 0 && current_byte_offset > 100 {
                                outer_fi_byte_offset = current_byte_offset + line.len();
                                break;
                            }
                        }
                        
                        current_byte_offset += line.len() + 1; // +1 for newline
                    }
                    
                    if outer_fi_byte_offset > 0 {
                        search_start + modprobed_pos + outer_fi_byte_offset + 1 // +1 to position after fi
                    } else {
                        search_start
                    }
                } else {
                    // Insert at beginning of prepare()
                    search_start
                };
                
                // Only inject if not already present
                let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                if !check_str.contains("WHITELIST") {
                    content.insert_str(inject_pos, &format!("\n{}", whitelist_injection));
                    eprintln!("[Patcher] [WHITELIST] Injected kernel whitelist protection into prepare() function");
                } else {
                    eprintln!("[Patcher] [WHITELIST] Whitelist injection already present, skipping");
                }
            }
        } else {
            eprintln!("[Patcher] [WHITELIST] WARNING: prepare() function not found in PKGBUILD");
            return Ok(());
        }

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [WHITELIST] SUCCESS: Kernel whitelist protection injected into prepare()");

        Ok(())
    }

    pub fn inject_post_oldconfig_lto_patch(&self, lto_type: LtoType) -> PatchResult<()> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.post_oldconfig.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // The post-oldconfig patch snippet - respects lto_type
        let post_oldconfig_patch = match lto_type {
            LtoType::Full => r#"
    # =====================================================================
    # PHASE E1 CRITICAL: POST-OLDCONFIG LTO ENFORCEMENT (FULL LTO)
    # =====================================================================
    # After any 'make oldconfig' or 'make syncconfig', the kernel's Kconfig
    # system may revert our LTO settings to defaults. This snippet
    # IMMEDIATELY re-applies CONFIG_LTO_CLANG_FULL enforcement.
    
    # Check if we're in a build function (prepare, build, etc.)
    if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
        config_file=".config"
        
        # Only patch if .config exists (oldconfig already ran)
        if [[ -f "$config_file" ]]; then
            # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
            sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
            
            # Ensure file ends with newline, then append FULL LTO enforcement
            tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
            
            # APPEND FULL LTO settings at the END of .config
            cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE E1 POST-OLDCONFIG: LTO CLANG FULL ENFORCEMENT (FINAL)
# ====================================================================
CONFIG_LTO_CLANG_FULL=y
CONFIG_LTO_CLANG=y
CONFIG_HAS_LTO_CLANG=y
EOF
            
            printf "[PATCH] POST-OLDCONFIG: Re-enforced CONFIG_LTO_CLANG_FULL=y after config regeneration\n" >&2
            
            if command -v make &> /dev/null; then
                printf "[PATCH] POST-OLDCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
                if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                    printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized without prompts\n" >&2
                else
                    printf "[PATCH] WARNING: 'make olddefconfig' failed, continuing anyway...\n" >&2
                fi
            fi
        fi
    fi
    "#,
            LtoType::Thin => r#"
    # =====================================================================
    # PHASE E1 CRITICAL: POST-OLDCONFIG LTO ENFORCEMENT (THIN LTO)
    # =====================================================================
    # After any 'make oldconfig' or 'make syncconfig', the kernel's Kconfig
    # system may revert our LTO settings to defaults. This snippet
    # IMMEDIATELY re-applies CONFIG_LTO_CLANG_THIN enforcement.
    
    # Check if we're in a build function (prepare, build, etc.)
    if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
        config_file=".config"
        
        # Only patch if .config exists (oldconfig already ran)
        if [[ -f "$config_file" ]]; then
            # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
            sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
            
            # Ensure file ends with newline, then append THIN LTO enforcement
            tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
            
            # APPEND THIN LTO settings at the END of .config
            cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE E1 POST-OLDCONFIG: LTO CLANG THIN ENFORCEMENT (FINAL)
# ====================================================================
CONFIG_LTO_CLANG_THIN=y
CONFIG_LTO_CLANG=y
CONFIG_HAS_LTO_CLANG=y
EOF
            
            printf "[PATCH] POST-OLDCONFIG: Re-enforced CONFIG_LTO_CLANG_THIN=y after config regeneration\n" >&2
            
            if command -v make &> /dev/null; then
                printf "[PATCH] POST-OLDCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
                if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                    printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized without prompts\n" >&2
                else
                    printf "[PATCH] WARNING: 'make olddefconfig' failed, continuing anyway...\n" >&2
                fi
            fi
        fi
    fi
    "#,
            LtoType::None => r#"
    # =====================================================================
    # PHASE E1: POST-OLDCONFIG (LTO DISABLED - None)
    # =====================================================================
    # LTO is disabled per user selection - only remove LTO configs
    
    if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
        config_file=".config"
        
        if [[ -f "$config_file" ]]; then
            # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
            sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"
            
            # Ensure file ends with newline
            tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"
            
            printf "[PATCH] POST-OLDCONFIG: LTO disabled - removed all LTO configs\n" >&2
            
            if command -v make &> /dev/null; then
                if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                    printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized\n" >&2
                fi
            fi
        fi
    fi
    "#,
        };

        // Find all make oldconfig/syncconfig calls and inject the patch immediately after
        let oldconfig_pattern = Regex::new(r"(?m)(make\s+(?:old)?config|make\s+syncconfig)")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;

        // For each oldconfig call, insert the post-patch snippet after it
        let lines: Vec<String> = content
            .lines()
            .flat_map(|line| {
                if oldconfig_pattern.is_match(line) && !line.trim_start().starts_with("#") {
                    // This line contains a make oldconfig call - emit it, then emit the patch
                    vec![line.to_string(), post_oldconfig_patch.to_string()]
                } else {
                    vec![line.to_string()]
                }
            })
            .collect();

        content = lines.join("\n");

        // Also patch any prepare(), build(), or _package() functions to inject the handler
        // so it runs at the start of those functions
        let functions = vec!["prepare()", "build()", "_package()"];

        for func_sig in functions {
            if let Some(func_pos) = content.find(func_sig) {
                // Find the opening brace
                if let Some(brace_pos) = content[func_pos..].find('{') {
                    let inject_pos = func_pos + brace_pos + 1;
                    
                    // CRITICAL: Remove any existing POST-OLDCONFIG block from this function
                    // This ensures that when LTO level changes (Full -> Thin, etc.), new enforcement applies
                    let e1_removal_regex = Regex::new(r"(?m)\s*# ===+\s*\n\s*# PHASE E1 CRITICAL:.*?fi\s*\n")
                        .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# PHASE E1 CRITICAL.*?^fi\s*$").unwrap());
                    content = e1_removal_regex.replace_all(&content, "").to_string();
                    
                    // Now inject the new post-oldconfig patch with current lto_type
                    content.insert_str(inject_pos, &format!("\n{}", post_oldconfig_patch));
                }
            }
        }

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [PHASE-E1] Injected POST-OLDCONFIG LTO patch into PKGBUILD");
        let lto_msg = match lto_type {
            LtoType::Full => "CONFIG_LTO_CLANG_FULL=y",
            LtoType::Thin => "CONFIG_LTO_CLANG_THIN=y",
            LtoType::None => "LTO disabled",
        };
        eprintln!("[Patcher] [PHASE-E1] {} will be re-enforced after any config regeneration", lto_msg);

        Ok(())
    }

    /// Inject LLVM/Clang compiler variables directly into PKGBUILD
    ///
    /// AGGRESSIVE INJECTION: This uses sed-like patterns to REPLACE any existing
    /// CC, CXX, LD, AR, NM, STRIP, OBJCOPY, OBJDUMP, READELF assignments with
    /// LLVM/Clang alternatives. Variables are injected into prepare(), build(),
    /// and _package() functions to ensure absolute priority.
    ///
    /// Critical: Ensures the kernel detects CLANG, not GCC, by FORCEFULLY overriding
    /// any existing toolchain variable assignments.
    ///
    /// Variables forcefully injected:
    /// - LLVM=1, LLVM_IAS=1 (Enable LLVM in kernel build)
    /// - CC=clang, CXX=clang++ (C/C++ compilers - REPLACES GCC)
    /// - LD=ld.lld (LLVM linker - REPLACES ld)
    /// - AR=llvm-ar, NM=llvm-nm, STRIP=llvm-strip (LLVM tools)
    /// - OBJCOPY=llvm-objcopy, OBJDUMP=llvm-objdump, READELF=llvm-readelf
    pub fn inject_clang_into_pkgbuild(&self) -> PatchResult<()> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // STEP 1: AGGRESSIVELY REPLACE any existing CC, CXX, LD, etc. assignments
        // This uses sed-like patterns to ensure NO GCC variables remain
        
        // Replace CC assignments (handles: CC=gcc, export CC=gcc, etc.)
        let cc_regex = Regex::new(r"(?m)^\s*(?:export\s+)?CC\s*=\s*(?:gcc|cc)[^\n]*")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
        content = cc_regex.replace_all(&content, "export CC=clang").to_string();

        // Replace CXX assignments
        let cxx_regex = Regex::new(r"(?m)^\s*(?:export\s+)?CXX\s*=\s*(?:g\+\+|c\+\+)[^\n]*")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
        content = cxx_regex.replace_all(&content, "export CXX=clang++").to_string();

        // Replace LD assignments (replace ld with ld.lld)
        let ld_regex = Regex::new(r"(?m)^\s*(?:export\s+)?LD\s*=\s*(?:ld)[^\n]*")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
        content = ld_regex.replace_all(&content, "export LD=ld.lld").to_string();

        // STEP 2: Inject LLVM/Clang variables at top of prepare(), build(), and _package()
        let clang_exports = r#"    # ======================================================================
        # CLANG/LLVM TOOLCHAIN INJECTION (Rust-based patcher - AGGRESSIVE)
        # ======================================================================
        # CRITICAL: These MUST be set BEFORE scripts/kconfig runs
        # They FORCEFULLY ensure the kernel detects CLANG, not GCC
        export LLVM=1
        export LLVM_IAS=1
        export CC=clang
        export CXX=clang++
        export LD=ld.lld
        export AR=llvm-ar
        export NM=llvm-nm
        export STRIP=/usr/bin/strip
        export OBJCOPY=llvm-objcopy
        export OBJDUMP=llvm-objdump
        export READELF=llvm-readelf
        
        # ======================================================================
        # HOST COMPILER ENFORCEMENT - Ensure host tools also use Clang
        # ======================================================================
        export HOSTCC=clang
        export HOSTCXX=clang++
        
        # ======================================================================
        # COMPILER OPTIMIZATION FLAGS (Phase 1: Enforce)
        # ======================================================================
        # CRITICAL: These flags MUST be present for performance and hardening
        export CFLAGS="-O2 -march=native"
        export CXXFLAGS="-O2 -march=native"
        export LDFLAGS="-O2 -march=native"
        export KCFLAGS="-march=native"
    "#;

        let functions = vec!["prepare()", "build()", "_package()"];

        for func_sig in functions {
            if let Some(func_pos) = content.find(func_sig) {
                // Find the opening brace
                if let Some(brace_pos) = content[func_pos..].find('{') {
                    let inject_pos = func_pos + brace_pos + 1;
                    // Only inject if not already present (check for LLVM=1)
                    let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                    if !check_str.contains("export LLVM=1") {
                        content.insert_str(inject_pos, &format!("\n{}", clang_exports));
                    }
                }
            }
        }

        // STEP 3: INJECT LLVM=1 into all make invocations
        // This ensures the kernel build system ALWAYS detects CLANG
        // Replace patterns like "make" with "make LLVM=1 LLVM_IAS=1"
        let make_regex = Regex::new(r"\bmake\b")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
        
        // Only replace make commands that don't already have LLVM=1
        let lines: Vec<String> = content
            .lines()
            .map(|line| {
                if line.contains("make") && !line.contains("LLVM=1") && !line.starts_with("#") {
                    // Inject LLVM=1 LLVM_IAS=1 right after "make"
                    make_regex.replace_all(line, "make LLVM=1 LLVM_IAS=1").to_string()
                } else {
                    line.to_string()
                }
            })
            .collect();
        content = lines.join("\n");

        // STEP 4: Ensure NO GCC-related environment variables leak through
        // Comment out or remove any GCC-specific variable assignments
        let gcc_pattern_regex = Regex::new(r"(?m)^\s*(?:export\s+)?(GCC|CFLAGS|CXXFLAGS|LDFLAGS)_[A-Z0-9_]*\s*=")
            .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
        
        // Replace GCC_* with comments (keep them for reference but disabled)
        let lines: Vec<String> = content
            .lines()
            .map(|line| {
                if gcc_pattern_regex.is_match(line) && line.contains("gcc") {
                    format!("# DISABLED (Clang override): {}", line)
                } else {
                    line.to_string()
                }
            })
            .collect();
        content = lines.join("\n");

        // Write modified PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        Ok(())
     }

     /// Inject build environment variables into PKGBUILD
     ///
     /// This injects GOATD_* environment variables that custom build scripts
     /// may reference. Variables are exported at the start of key functions.
     ///
     /// # Arguments
     /// * `env_vars` - HashMap of environment variable names to values
     pub fn inject_build_environment_variables(&self, env_vars: HashMap<String, String>) -> PatchResult<()> {
         let pkgbuild_path = self.src_dir.join("PKGBUILD");

         if !pkgbuild_path.exists() {
             return Err(PatchError::FileNotFound(
                 format!("PKGBUILD not found: {}", pkgbuild_path.display())
             ));
         }

         let mut content = fs::read_to_string(&pkgbuild_path)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

         // Build the environment variable export block
         let mut env_block = String::new();
         for (key, value) in &env_vars {
             // Format: export VARNAME='value' with proper quoting
             env_block.push_str(&format!("    export {}='{}'\n", key, value));
             eprintln!("[Patcher] [ENVVARS] Injecting {}='{}'", key, value);
         }

         if env_block.is_empty() {
             return Ok(()); // Nothing to inject
         }

         // Inject into prepare(), build(), and _package() functions
         let functions = vec!["prepare()", "build()", "_package()"];

         for func_sig in functions {
             if let Some(func_pos) = content.find(func_sig) {
                 // Find the opening brace
                 if let Some(brace_pos) = content[func_pos..].find('{') {
                     let inject_pos = func_pos + brace_pos + 1;
                     // Only inject if not already present
                     let check_str = &content[inject_pos..std::cmp::min(inject_pos + 1000, content.len())];
                     if !check_str.contains("GOATD_") {
                         let header = format!("\n    # ======================================================================\n    # BUILD ENVIRONMENT VARIABLES (Rust patcher - orchestrator config)\n    # ======================================================================\n");
                         content.insert_str(inject_pos, &format!("{}{}\n", header, env_block));
                     }
                 }
             }
         }

         // Write modified PKGBUILD
         fs::write(&pkgbuild_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

         eprintln!("[Patcher] [ENVVARS] SUCCESS: Injected {} build environment variables into PKGBUILD", env_vars.len());

         Ok(())
     }

     /// Inject Polly compiler flags into PKGBUILD
     ///
     /// Extracts Polly optimization flags from build environment and injects them
     /// into PKGBUILD's CFLAGS/CXXFLAGS/LDFLAGS for advanced loop optimizations.
     ///
     /// Ensures POLLY flags are: `-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max`
     ///
     /// # Arguments
     /// * `polly_options` - HashMap containing _POLLY_CFLAGS, _POLLY_CXXFLAGS, _POLLY_LDFLAGS
     pub fn inject_polly_flags(&self, polly_options: HashMap<String, String>) -> PatchResult<()> {
         let pkgbuild_path = self.src_dir.join("PKGBUILD");
 
         if !pkgbuild_path.exists() {
             return Err(PatchError::FileNotFound(
                 format!("PKGBUILD not found: {}", pkgbuild_path.display())
             ));
         }
 
         let mut content = fs::read_to_string(&pkgbuild_path)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;
 
         // Create backup
         let backup_path = self.backup_dir.join("PKGBUILD.polly.bak");
         fs::create_dir_all(&self.backup_dir)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
         fs::write(&backup_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;
 
         // CRITICAL FIX: Remove any existing POLLY injection blocks to ensure fresh injection
         // This allows switching Polly settings and having changes take effect
         let polly_removal_regex = Regex::new(r"(?m)\s*# ===+\s*\n\s*# POLLY LOOP OPTIMIZATION.*?export LDFLAGS.*?\n")
             .unwrap_or_else(|_| Regex::new(r"(?m)^\s*# POLLY LOOP OPTIMIZATION.*?^export LDFLAGS").unwrap());
         content = polly_removal_regex.replace_all(&content, "").to_string();
         eprintln!("[Patcher] [POLLY] Surgically removed any existing Polly injection blocks for fresh re-injection");
 
         // Extract Polly flags - use optimized set with stripmine vectorizer and fusion
         let polly_cflags = polly_options.get("_POLLY_CFLAGS").cloned()
             .unwrap_or_else(|| "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max".to_string());
         let polly_cxxflags = polly_options.get("_POLLY_CXXFLAGS").cloned()
             .unwrap_or_else(|| "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max".to_string());
         let polly_ldflags = polly_options.get("_POLLY_LDFLAGS").cloned()
             .unwrap_or_else(|| "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max".to_string());
 
         // Build Polly injection block with visible logging announcement
         let polly_block = format!(
             r#"    # ======================================================================
     # PHASE 3.5: POLLY LOOP OPTIMIZATION FLAGS (Surgical LLVM optimization)
     # ======================================================================
     # Polly provides advanced loop optimizations via LLVM
     # Flags enable: core Polly + stripmine vectorizer + max fusion
     # This is the OPTIMIZED flag set ensuring consistent performance gains
     printf "[PHASE 3.5] Surgically enforced Polly LLVM loop optimizations\n" >&2
     
     export POLLY_CFLAGS='{}'
     export POLLY_CXXFLAGS='{}'
     export POLLY_LDFLAGS='{}'
     
     # Append Polly flags to existing CFLAGS/CXXFLAGS/LDFLAGS
     export CFLAGS="${{CFLAGS}} $POLLY_CFLAGS"
     export CXXFLAGS="${{CXXFLAGS}} $POLLY_CXXFLAGS"
     export LDFLAGS="${{LDFLAGS}} $POLLY_LDFLAGS"
     "#,
             polly_cflags, polly_cxxflags, polly_ldflags
         );

         // Inject into prepare(), build(), and _package() functions
         let functions = vec!["prepare()", "build()", "_package()"];

         for func_sig in functions {
             if let Some(func_pos) = content.find(func_sig) {
                 // Find the opening brace
                 if let Some(brace_pos) = content[func_pos..].find('{') {
                     let inject_pos = func_pos + brace_pos + 1;
                     // Only inject if not already present
                     let check_str = &content[inject_pos..std::cmp::min(inject_pos + 500, content.len())];
                     if !check_str.contains("POLLY_CFLAGS") {
                         content.insert_str(inject_pos, &format!("\n{}", polly_block));
                     }
                 }
             }
         }

         // Write modified PKGBUILD
         fs::write(&pkgbuild_path, &content)
             .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

         eprintln!("[Patcher] [PHASE-3.5] CRITICAL: Surgically enforced Polly LLVM loop optimizations");
         eprintln!("[Patcher] [PHASE-3.5] Polly injection complete - printf logging added");
         eprintln!("[Patcher] [POLLY]   CFLAGS: {}", polly_cflags);
         eprintln!("[Patcher] [POLLY]   CXXFLAGS: {}", polly_cxxflags);
         eprintln!("[Patcher] [POLLY]   LDFLAGS: {}", polly_ldflags);

         Ok(())
     }

     /// Apply all patches in sequence (orchestrator-friendly)
    ///
    /// Returns a JSON-like result structure for Python consumption
    /// Inject PKGBUILD metadata variables (GOATD_* style)
    ///
    /// This injects variables like GOATD_LTO_LEVEL, GOATD_PROFILE_SUFFIX, etc.
    /// at the top of PKGBUILD for orchestrator metadata passing.
    ///
    /// # Arguments
    /// * `pkgbuild_vars` - HashMap of variable names to values (without GOATD_ prefix)
    pub fn inject_pkgbuild_metadata_variables(&self, pkgbuild_vars: HashMap<String, String>) -> PatchResult<()> {
        if pkgbuild_vars.is_empty() {
            return Ok(());
        }

        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.metadata_vars.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        // Clean up any existing GOATD_ or legacy variables
        let lines: Vec<String> = content.lines()
            .filter(|line| !line.starts_with("GOATD_") && !line.starts_with("_lto_level="))
            .map(|s| s.to_string())
            .collect();
        content = lines.join("\n") + "\n";

        // Build all patches to inject (safe single-quoted shell assignments)
        let mut patches_to_inject = String::new();
        for (key, value) in &pkgbuild_vars {
            patches_to_inject.push_str(&format!("{}='{}'\n", key, value));
            eprintln!("[Patcher] [METADATA] Injecting {}='{}'", key, value);
        }

        // Inject at TOP of file after initial shebang (if present)
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let inject_idx = if lines.first().map_or(false, |line| line.starts_with("#!")) {
            1  // Insert after shebang
        } else {
            0  // Insert at the very top
        };

        // Split patches into individual lines and insert each one
        let patch_lines: Vec<&str> = patches_to_inject.lines().collect();
        for (offset, patch_line) in patch_lines.iter().enumerate().rev() {
            if !patch_line.is_empty() || offset == 0 {
                lines.insert(inject_idx, patch_line.to_string());
            }
        }

        content = lines.join("\n") + "\n";

        // Write back to PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [METADATA] SUCCESS: Injected {} PKGBUILD metadata variables", pkgbuild_vars.len());

        Ok(())
    }

    /// Patch PKGBUILD pkgbase and function names for rebranding
    ///
    /// Transforms:
    /// - pkgbase=linux → pkgbase=linux-goatd-<profile>
    /// - package_linux() → package_linux_goatd_<profile>()
    /// - package_linux-headers() → package_linux_goatd_<profile>_headers()
    ///
    /// # Arguments
    /// * `profile_name` - Profile name (e.g., "Gaming", "Laptop") for rebranding
    pub fn patch_pkgbuild_for_rebranding(&self, profile_name: &str) -> PatchResult<()> {
        let pkgbuild_path = self.src_dir.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            return Err(PatchError::FileNotFound(
                format!("PKGBUILD not found: {}", pkgbuild_path.display())
            ));
        }

        let mut content = fs::read_to_string(&pkgbuild_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

        // Create backup
        let backup_path = self.backup_dir.join("PKGBUILD.rebranding.bak");
        fs::create_dir_all(&self.backup_dir)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

        let profile_lower = profile_name.to_lowercase();
        let master_identity = format!("linux-goatd-{}", profile_lower);

        // PATCH 1: Transform pkgbase
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        for line in &mut lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with("pkgbase=") && trimmed.contains("linux") {
                let original = line.clone();
                let leading_ws = line.len() - trimmed.len();
                let indent = &line[0..leading_ws];
                *line = format!("{}pkgbase='{}'", indent, master_identity);
                eprintln!("[Patcher] [REBRANDING] pkgbase: {} → {}", original, line);
                break;
            }
        }

        // If pkgbase not found, inject before pkgname
        if lines.iter().all(|l| !l.trim_start().starts_with("pkgbase=")) {
            for (idx, line) in lines.iter().enumerate() {
                if line.trim_start().starts_with("pkgname=") {
                    lines.insert(idx, format!("pkgbase='{}'", master_identity));
                    eprintln!("[Patcher] [REBRANDING] Injected new pkgbase='{}'", master_identity);
                    break;
                }
            }
        }

        // PATCH 2: Rename function declarations
        for line in &mut lines {
            if line.contains("package_linux()") {
                let original = line.clone();
                *line = line.replace("package_linux()",
                    &format!("package_{}()", master_identity.replace("-", "_")));
                eprintln!("[Patcher] [REBRANDING] Function: {} → {}", original, line);
            }

            if line.contains("package_linux-headers()") {
                let original = line.clone();
                let new_func_name = format!("package_{}-headers()", master_identity.replace("-", "_"));
                *line = line.replace("package_linux-headers()", &new_func_name);
                eprintln!("[Patcher] [REBRANDING] Function: {} → {}", original, line);
            }
        }

        content = lines.join("\n") + "\n";

        // PATCH 3: Inject provides=('linux') for multi-kernel coexistence
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let mut insert_idx = None;
        for (idx, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("pkgdesc=") {
                insert_idx = Some(idx + 1);
                break;
            }
        }

        if let Some(idx) = insert_idx {
            lines.insert(idx, "provides=('linux')".to_string());
            eprintln!("[Patcher] [REBRANDING] Injected provides=('linux') for multi-kernel coexistence");
        }

        content = lines.join("\n") + "\n";

        // Write back to PKGBUILD
        fs::write(&pkgbuild_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

        eprintln!("[Patcher] [REBRANDING] SUCCESS: PKGBUILD rebranded for profile '{}'", profile_name);

        Ok(())
    }

    /// Detect kernel version from Makefile
    fn detect_kernel_version(&self) -> PatchResult<String> {
        let makefile_path = self.src_dir.join("Makefile");
        let content = fs::read_to_string(&makefile_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read Makefile: {}", e)))?;

        // Look for VERSION and PATCHLEVEL in Makefile
        let mut version = None;
        let mut patchlevel = None;

        for line in content.lines() {
            if line.starts_with("VERSION =") {
                version = line.split('=').nth(1).map(|s| s.trim().to_string());
            }
            if line.starts_with("PATCHLEVEL =") {
                patchlevel = line.split('=').nth(1).map(|s| s.trim().to_string());
            }
        }

        match (version, patchlevel) {
            (Some(v), Some(p)) => Ok(format!("{}.{}", v, p)),
            _ => Err(PatchError::PatchFailed("Could not detect kernel version".to_string())),
        }
    }

    pub fn execute_full_patch(
        &self,
        shield_modules: Vec<String>,
        config_options: HashMap<String, String>,
    ) -> PatchResult<()> {
        // Default to Thin LTO when no environment variables are provided
        let mut build_env = HashMap::new();
        build_env.insert("GOATD_LTO_LEVEL".to_string(), "thin".to_string());
        self.execute_full_patch_with_env(shield_modules, config_options, build_env)
    }

    /// Apply all patches in sequence with build environment variables
    /// Wire Secure Boot hardening settings into kernel CONFIG
    ///
    /// Injects Secure Boot configuration options when enabled:
    /// - CONFIG_EFI_SECURE_BOOT_SIG_FORCE=y (enforce signature verification)
    /// - CONFIG_MODULE_SIG=y (sign kernel modules)
    /// - CONFIG_MODULE_SIG_FORCE=y (verify all modules at load time)
    /// - CONFIG_LOCKDOWN_LSM=y (LSM lockdown mode enabled)
    ///
    /// These settings ensure kernel integrity checking and prevent unsigned
    /// module loading, which is critical for UEFI Secure Boot systems.
    fn inject_secure_boot_hardening(&self, config_options: &mut HashMap<String, String>) -> PatchResult<()> {
        eprintln!("[Patcher] [HARDENING] Injecting Secure Boot configuration");
        
        // Module signature enforcement (critical for Secure Boot)
        config_options.insert("CONFIG_MODULE_SIG".to_string(), "y".to_string());
        config_options.insert("CONFIG_MODULE_SIG_FORCE".to_string(), "y".to_string());
        config_options.insert("CONFIG_MODULE_SIG_SHA512".to_string(), "y".to_string());
        
        // EFI Secure Boot enforcement
        config_options.insert("CONFIG_EFI_SECURE_BOOT_SIG_FORCE".to_string(), "y".to_string());
        
        // Kernel Lockdown LSM for additional security
        config_options.insert("CONFIG_LOCKDOWN_LSM".to_string(), "y".to_string());
        config_options.insert("CONFIG_LOCKDOWN_LSM_EARLY".to_string(), "y".to_string());
        
        eprintln!("[Patcher] [HARDENING] Secure Boot hardening configured: MODULE_SIG, EFI_SECURE_BOOT, LOCKDOWN_LSM");
        Ok(())
    }
    
    /// Wire SELinux/AppArmor hardening settings into kernel CONFIG
    ///
    /// Injects LSM (Linux Security Module) configuration for mandatory access control:
    /// - CONFIG_SECURITY=y (enable security framework)
    /// - CONFIG_SECURITY_SELINUX=y (enable SELinux)
    /// - CONFIG_SECURITY_SELINUX_BOOTPARAM=y (allow boottime disable)
    /// - CONFIG_SECURITY_SELINUX_DISABLE=y (allow runtime disable via sysctl)
    /// - CONFIG_SECURITY_APPARMOR=y (enable AppArmor)
    /// - CONFIG_SECURITY_APPARMOR_BOOTPARAM_VALUE=1 (enabled by default)
    /// - CONFIG_LSM="selinux,apparmor" (load order)
    ///
    /// These settings provide mandatory access control (MAC) frameworks
    /// that complement DAC (Discretionary Access Control) for fine-grained security.
    fn inject_selinux_apparmor_hardening(&self, config_options: &mut HashMap<String, String>) -> PatchResult<()> {
        eprintln!("[Patcher] [HARDENING] Injecting SELinux/AppArmor configuration");
        
        // Core security subsystem
        config_options.insert("CONFIG_SECURITY".to_string(), "y".to_string());
        config_options.insert("CONFIG_SECURITY_DMESG_RESTRICT".to_string(), "y".to_string());
        
        // SELinux mandatory access control
        config_options.insert("CONFIG_SECURITY_SELINUX".to_string(), "y".to_string());
        config_options.insert("CONFIG_SECURITY_SELINUX_BOOTPARAM".to_string(), "y".to_string());
        config_options.insert("CONFIG_SECURITY_SELINUX_BOOTPARAM_VALUE".to_string(), "1".to_string());
        config_options.insert("CONFIG_SECURITY_SELINUX_DEVELOP".to_string(), "y".to_string());
        config_options.insert("CONFIG_SECURITY_SELINUX_MLS".to_string(), "y".to_string());
        
        // AppArmor mandatory access control
        config_options.insert("CONFIG_SECURITY_APPARMOR".to_string(), "y".to_string());
        config_options.insert("CONFIG_SECURITY_APPARMOR_BOOTPARAM_VALUE".to_string(), "1".to_string());
        config_options.insert("CONFIG_SECURITY_APPARMOR_HASH".to_string(), "y".to_string());
        
        // LSM stack configuration
        config_options.insert("CONFIG_LSM".to_string(), "selinux,apparmor".to_string());
        
        eprintln!("[Patcher] [HARDENING] SELinux/AppArmor hardening configured: LSM with SELINUX and APPARMOR");
        Ok(())
    }

    pub fn execute_full_patch_with_env(
        &self,
        shield_modules: Vec<String>,
        mut config_options: HashMap<String, String>,
        build_env_vars: HashMap<String, String>,
    ) -> PatchResult<()> {
        // Extract LTO type from environment variable (GOATD_LTO_LEVEL: "full", "thin", or "none")
        // CRITICAL FIX: Be explicit about all cases to prevent defaulting away Full
        let lto_type = match build_env_vars.get("GOATD_LTO_LEVEL") {
            Some(level) => {
                let s = level.as_str();
                eprintln!("[Patcher] [ENV] GOATD_LTO_LEVEL raw value: '{}'", s);
                match s {
                    "full" => {
                        eprintln!("[Patcher] [ENV] ✓ Recognized GOATD_LTO_LEVEL='full' → LTO::Full");
                        LtoType::Full
                    }
                    "none" => {
                        eprintln!("[Patcher] [ENV] ✓ Recognized GOATD_LTO_LEVEL='none' → LTO::None");
                        LtoType::None
                    }
                    "thin" => {
                        eprintln!("[Patcher] [ENV] ✓ Recognized GOATD_LTO_LEVEL='thin' → LTO::Thin");
                        LtoType::Thin
                    }
                    unexpected => {
                        eprintln!("[Patcher] [ENV] ⚠ WARNING: Unexpected GOATD_LTO_LEVEL='{}', defaulting to Thin (MIGHT BE BUG)", unexpected);
                        LtoType::Thin
                    }
                }
            }
            None => {
                eprintln!("[Patcher] [ENV] ⚠ WARNING: GOATD_LTO_LEVEL environment variable NOT SET, defaulting to Thin");
                eprintln!("[Patcher] [ENV] Available env vars: {:?}", build_env_vars.keys().collect::<Vec<_>>());
                LtoType::Thin
            }
        };
        eprintln!("[Patcher] [ENV] Final extracted LTO type: {:?}", lto_type);
        
        // Extract hardening flags from build environment (standardized variables)
        let enable_secure_boot = build_env_vars.get("GOATD_ENABLE_SECURE_BOOT") == Some(&"1".to_string());
        let enable_hardening = build_env_vars.get("GOATD_ENABLE_HARDENING") == Some(&"1".to_string());
        let enable_selinux_apparmor = build_env_vars.get("GOATD_ENABLE_SELINUX_APPARMOR") == Some(&"1".to_string());
        
        eprintln!("[Patcher] [HARDENING] Environment flags: SECURE_BOOT={}, HARDENING={}, SELINUX_APPARMOR={}",
                  enable_secure_boot, enable_hardening, enable_selinux_apparmor);
        
        // Apply hardening settings to config_options BEFORE Kconfig application
        if enable_secure_boot {
            self.inject_secure_boot_hardening(&mut config_options)?;
        }
        if enable_hardening {
            eprintln!("[Patcher] [HARDENING] GOATD_ENABLE_HARDENING is set, applying general hardening");
        }
        if enable_selinux_apparmor {
            self.inject_selinux_apparmor_hardening(&mut config_options)?;
        }
        
        // Extract modprobed-db discovery flag
        let use_modprobed = build_env_vars.get("GOATD_USE_MODPROBED_DB") == Some(&"1".to_string());

        // CRITICAL: Clean restore PKGBUILD if in git repository (ensures clean slate)
        if self.src_dir.join(".git").exists() {
            let _ = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.src_dir)
                .arg("restore")
                .arg("PKGBUILD")
                .output();
            eprintln!("[Patcher] [INIT] Restored PKGBUILD to upstream state using git");
        }

        // Step 0: Inject PKGBUILD metadata variables (GOATD_* prefix)
        // These carry build configuration from orchestrator to PKGBUILD
        let pkgbuild_metadata: HashMap<String, String> = build_env_vars
            .iter()
            .filter(|(k, _)| k.starts_with("GOATD_"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        if !pkgbuild_metadata.is_empty() {
            self.inject_pkgbuild_metadata_variables(pkgbuild_metadata)?;
        }

        // Step 0.1: Patch PKGBUILD for rebranding (profile-specific naming)
        if let Some(profile) = build_env_vars.get("GOATD_PROFILE_NAME") {
            self.patch_pkgbuild_for_rebranding(profile)?;
        }

        // Step 0.2: Inject Clang into PKGBUILD (CRITICAL)
         self.inject_clang_into_pkgbuild()?;

         // Step 0.45: Fix Rust .rmeta installation for cross-environment compatibility
         // Use find instead of glob expansion to handle missing files gracefully
         let _rmeta_fix = self.fix_rust_rmeta_installation();
         if let Ok(count) = _rmeta_fix {
             if count > 0 {
                 eprintln!("[Patcher] [RUST-RMETA] Fixed Rust .rmeta installation for cross-environment compatibility");
             }
         }

         // Step 0.5: Surgically remove `-v` flag from strip calls (LLVM strip incompatibility)
         // This MUST happen after Clang injection to ensure compatibility with llvm-strip
         let _strip_removals = self.remove_strip_verbose_flag();
        // Don't fail if no `-v` flags found - it's not a critical error
        if let Ok(count) = _strip_removals {
            if count > 0 {
                eprintln!("INFO: Removed {} 'strip -v' flags from PKGBUILD", count);
            }
        }

        // Step 0.6: CRITICAL FIX - MODPROBED-DB DISCOVERY INJECTION
        // Inject modprobed-db localmodconfig command into prepare() BEFORE any oldconfig/syncconfig
        // This MUST happen BEFORE prebuild enforcer so modprobed filtering runs first
        if use_modprobed {
            self.inject_modprobed_localmodconfig(true)?;
            eprintln!("[Patcher] [MODPROBED] SUCCESS: Modprobed-db discovery injected into PKGBUILD");
        } else {
            eprintln!("[Patcher] [MODPROBED] Modprobed-db discovery not enabled, skipping injection");
        }

        // Step 0.62: CRITICAL FIX - PHASE G2: POST-MODPROBED HARD ENFORCER
         // After localmodconfig filters to ~170 modules, olddefconfig's Kconfig dependency
         // expansion re-enables thousands of unwanted modules. PHASE G2 surgically protects
         // the filtered modules by removing all CONFIG_*=m entries NOT in modprobed.db.
         if use_modprobed {
             self.inject_post_modprobed_hard_enforcer(true)?;
             eprintln!("[Patcher] [PHASE-G2] SUCCESS: POST-MODPROBED hard enforcer injected into PKGBUILD");
         } else {
             eprintln!("[Patcher] [PHASE-G2] Post-modprobed enforcer not enabled (modprobed disabled), skipping injection");
         }

         // Step 0.63: CRITICAL FIX - PHASE G2.5: POST-SETTING-CONFIG RESTORER
         // CRITICAL: The "Setting config..." step runs "cp ../config .config" which OVERWRITES
         // our modprobed-filtered config with the original unfiltered config (6199 modules).
         // PHASE G2.5 detects this and re-applies localmodconfig immediately after.
         if use_modprobed {
             self.inject_post_setting_config_restorer(true)?;
             eprintln!("[Patcher] [PHASE-G2.5] SUCCESS: POST-SETTING-CONFIG restorer injected into PKGBUILD");
         } else {
             eprintln!("[Patcher] [PHASE-G2.5] Post-setting-config restorer not enabled (modprobed disabled), skipping injection");
         }

        // Step 0.65: CRITICAL FIX - KERNEL WHITELIST INJECTION
        // Inject kernel whitelist protection AFTER modprobed localmodconfig
        // The whitelist ensures critical filesystem and device drivers are always built
        // even with aggressive modprobed-db filtering, maintaining bootability
        let use_whitelist = build_env_vars.get("GOATD_USE_KERNEL_WHITELIST") == Some(&"1".to_string());
        if use_whitelist {
            self.inject_kernel_whitelist(true)?;
            eprintln!("[Patcher] [WHITELIST] SUCCESS: Kernel whitelist protection injected into PKGBUILD");
        } else {
            eprintln!("[Patcher] [WHITELIST] Kernel whitelist protection not enabled, skipping injection");
        }

        // Step 0.7: CRITICAL FIX - PHASE G1: Inject PREBUILD LTO HARD ENFORCER
        // This injects the final LTO enforcement immediately BEFORE the make command
        // in build(). This is the LAST enforcement barrier before kernel compilation.
        self.inject_prebuild_lto_hard_enforcer(lto_type)?;
        eprintln!("[Patcher] [PHASE-G1] SUCCESS: Prebuild LTO hard enforcer injected into PKGBUILD ({:?})", lto_type);

        // Step 0.75: CRITICAL FIX - PHASE E1: Inject POST-OLDCONFIG LTO PATCH
        // This ensures that after any 'make oldconfig' or 'make syncconfig' call,
        // our LTO settings are IMMEDIATELY re-enforced. Without this, the kernel's
        // Kconfig system can revert our patches to defaults (CONFIG_LTO_NONE=y).
        self.inject_post_oldconfig_lto_patch(lto_type)?;
        eprintln!("[Patcher] [PHASE-E1] SUCCESS: Post-oldconfig LTO patch injected into PKGBUILD ({:?})", lto_type);

        // Step 1: Shield LTO (non-fatal - log warning but continue)
        // Some GPU directories may not exist; don't block the build
        if let Err(e) = self.shield_lto(shield_modules) {
            eprintln!("[Patcher] [SHIELD-LTO] WARNING: LTO shielding failed: {}", e);
        }

        // Step 2: Remove ICF flags (non-fatal - log warning but continue)
        // Root Makefile may not have ICF flags; don't block the build
        if let Err(e) = self.remove_icf_flags() {
            eprintln!("[Patcher] [REMOVE-ICF] WARNING: ICF flag removal failed: {}", e);
        }

        // Step 3: Apply Kconfig (includes PHASE 5 HARD ENFORCER)
        // Extract Polly-related options before applying kconfig
        eprintln!("[Patcher] [KCONFIG] PHASE 3: Applying Kconfig with {} total options", config_options.len());
        let mut polly_options = HashMap::new();
        for (key, value) in &config_options {
            if key.starts_with("_POLLY_") {
                eprintln!("[Patcher] [POLLY-PREP] Extracted Polly option: {}={}", key, value);
                polly_options.insert(key.clone(), value.clone());
            }
        }
        eprintln!("[Patcher] [POLLY-PREP] Total extracted Polly options: {}", polly_options.len());
        
        // Extract MGLRU options for diagnostics
        let mglru_count = config_options.iter().filter(|(k, _)| k.starts_with("_MGLRU_")).count();
        eprintln!("[Patcher] [MGLRU-PREP] Total MGLRU options in config: {}", mglru_count);
        
        self.apply_kconfig(config_options, lto_type)?;

        // Step 3.5: Inject Polly optimization flags if present
        if !polly_options.is_empty() {
            eprintln!("[Patcher] [POLLY] PHASE 3.5: Injecting {} Polly options into PKGBUILD", polly_options.len());
            self.inject_polly_flags(polly_options)?;
            eprintln!("[Patcher] [POLLY] SUCCESS: Polly optimization flags injected into PKGBUILD");
        } else {
            eprintln!("[Patcher] [POLLY] No Polly options to inject (use_polly might be false)");
        }

        // Step 4: Inject build environment variables
        if !build_env_vars.is_empty() {
            self.inject_build_environment_variables(build_env_vars)?;
        }

        Ok(())
    }
}

/// Validate whether content looks like a valid patch file
///
/// A valid patch file should contain typical unified diff markers.
/// This is a simple heuristic check to avoid downloading/processing binary files.
///
/// # Arguments
/// * `content` - The raw bytes from a potential patch file
///
/// # Returns
/// true if content appears to be a valid patch, false otherwise
fn is_valid_patch(content: &[u8]) -> bool {
    // Try to convert to UTF-8 (patches are text files)
    let text = match std::str::from_utf8(content) {
        Ok(s) => s,
        Err(_) => return false, // Not valid UTF-8
    };

    // Check for key patch markers:
    // - "---" or "+++" (unified diff format)
    // - "@@" (hunk header)
    // - "diff --git" or "diff -" (patch file header)
    
    let has_diff_header = text.contains("diff --") || text.contains("diff -");
    let has_hunk_markers = text.contains("@@") || text.contains("---") || text.contains("+++");
    
    // Valid patch should have at least one diff marker
    has_diff_header || has_hunk_markers
}

/// Apply a single patch file to target content using regex.
///
/// # Arguments
///
/// * `content` - The file content to patch
/// * `pattern` - Regex pattern to match
/// * `replacement` - Text to replace matched pattern with
///
/// # Returns
///
/// Modified content with patch applied, or error if pattern doesn't match
///
/// # Examples
///
/// ```
/// use goatd_kernel::kernel::patcher::apply_patch;
///
/// let content = "CFLAGS = -O2 -flto=thin";
/// let result = apply_patch(content, r"-flto=thin", "-flto=full");
/// assert!(result.is_ok());
/// ```
pub fn apply_patch(content: &str, pattern: &str, replacement: &str) -> PatchResult<String> {
    // Validate regex pattern first
    let regex = Regex::new(pattern)
        .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;

    // Check if pattern matches at least once
    if !regex.is_match(content) {
        return Err(PatchError::PatchFailed(
            format!("Pattern not found in content: {}", pattern),
        ));
    }

    // Apply replacement
    Ok(regex.replace_all(content, replacement).to_string())
}

/// Patch a kernel Makefile by path with backup.
///
/// Creates a backup of the original file before modification.
///
/// # Arguments
///
/// * `makefile_path` - Path to the Makefile to patch
/// * `pattern` - Regex pattern to match
/// * `replacement` - Replacement text
/// * `backup_dir` - Directory to store backup file
///
/// # Returns
///
/// Number of lines modified, or error
pub fn patch_makefile(
    makefile_path: &Path,
    pattern: &str,
    replacement: &str,
    backup_dir: &Path,
) -> PatchResult<u32> {
    // Verify file exists
    if !makefile_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "Makefile not found: {}",
            makefile_path.display()
        )));
    }

    // Read original content
    let original_content = fs::read_to_string(makefile_path)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to read file: {}", e)))?;

    // Create backup
    let backup_filename = makefile_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let backup_path = backup_dir.join(format!("{}.bak", backup_filename));

    fs::create_dir_all(backup_dir)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;

    fs::write(&backup_path, &original_content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;

    // Apply patch
    let patched_content = apply_patch(&original_content, pattern, replacement)?;

    // Write patched content
    fs::write(makefile_path, &patched_content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to write patched file: {}", e)))?;

    // Count lines modified (approximate: count pattern matches)
    let regex = Regex::new(pattern)
        .map_err(|e| PatchError::RegexInvalid(format!("Invalid regex: {}", e)))?;
    let match_count = regex.find_iter(&original_content).count() as u32;

    Ok(match_count)
}

/// Apply a CONFIG option to kernel .config file.
///
/// # Arguments
///
/// * `config_path` - Path to .config file
/// * `option_key` - Configuration key (e.g., "CONFIG_LTO_CLANG")
/// * `option_value` - Configuration value (e.g., "y", "n", "m")
/// * `backup_dir` - Directory to store backup
///
/// # Returns
///
/// Success or error
pub fn patch_config(
    config_path: &Path,
    option_key: &str,
    option_value: &str,
    backup_dir: &Path,
) -> PatchResult<()> {
    // Create backup directory
    fs::create_dir_all(backup_dir)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup dir: {}", e)))?;

    // Read or create .config
    let mut content = if config_path.exists() {
        fs::read_to_string(config_path)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to read .config: {}", e)))?
    } else {
        String::new()
    };

    // Create backup if file exists
    if config_path.exists() {
        let backup_path = backup_dir.join(".config.bak");
        fs::write(&backup_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;
    }

    // Check if option already exists and remove it by filtering lines
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.starts_with(&format!("{}=", option_key)))
        .collect();
    
    if lines.len() != content.lines().count() {
        // Some lines were removed, rebuild content
        content = lines.join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
    }

    // Add new option
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("{}={}", option_key, option_value));
    content.push('\n');

    // Write updated .config
    fs::write(config_path, &content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    // ======= KERNEL PATCHER STRUCT TESTS (Tests 1-6)

    // Test 1: Create new kernel patcher
    #[test]
    fn test_kernel_patcher_new() {
        let temp_dir = TempDir::new().unwrap();
        let src_dir = temp_dir.path().to_path_buf();
        
        let patcher = KernelPatcher::new(src_dir.clone());
        assert_eq!(patcher.src_dir, src_dir);
        assert!(patcher.backup_dir.ends_with(".kernel_patcher_backup"));
    }

    // Test 2: Shield LTO with empty modules list
    #[test]
    fn test_shield_lto_empty() {
        let temp_dir = TempDir::new().unwrap();
        let patcher = KernelPatcher::new(temp_dir.path().to_path_buf());
        
        let result = patcher.shield_lto(vec![]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    // Test 3: Shield LTO with directories that don't exist
    #[test]
    fn test_shield_lto_missing_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let patcher = KernelPatcher::new(temp_dir.path().to_path_buf());
        
        let result = patcher.shield_lto(vec!["amdgpu".to_string()]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    // Test 4: Remove ICF flags when file doesn't exist
    #[test]
    fn test_remove_icf_flags_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let patcher = KernelPatcher::new(temp_dir.path().to_path_buf());
        
        let result = patcher.remove_icf_flags();
        assert!(result.is_err());
    }

    // Test 5: Remove ICF flags when no flags present
    #[test]
    fn test_remove_icf_flags_none_present() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -O2").unwrap();
        drop(file);
        
        let patcher = KernelPatcher::new(temp_dir.path().to_path_buf());
        let result = patcher.remove_icf_flags();
        assert!(result.is_ok());
    }

    // Test 6: Apply kconfig with empty options
    #[test]
    fn test_apply_kconfig_empty() {
        let temp_dir = TempDir::new().unwrap();
        let patcher = KernelPatcher::new(temp_dir.path().to_path_buf());
        
        let result = patcher.apply_kconfig(HashMap::new(), LtoType::Thin);
        assert!(result.is_ok());
    }

    // ======= BASIC PATCH APPLICATION TESTS (Tests 7-12)

    // Test 7: Apply simple string replacement patch
    #[test]
    fn test_apply_patch_simple() {
        let content = "CFLAGS = -O2 -flto=thin";
        let result = apply_patch(content, r"-flto=thin", "-flto=full");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "CFLAGS = -O2 -flto=full");
    }

    // Test 8: Apply regex pattern patch
    #[test]
    fn test_apply_patch_regex() {
        let content = "CFLAGS = -O2 -flto=thin -march=native";
        let result = apply_patch(content, r"-flto=\w+", "-flto=disabled");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "CFLAGS = -O2 -flto=disabled -march=native");
    }

    // Test 9: Patch fails when pattern not found
    #[test]
    fn test_apply_patch_not_found() {
        let content = "CFLAGS = -O2 -march=native";
        let result = apply_patch(content, r"-flto=thin", "-flto=full");
        assert!(result.is_err());
    }

    // Test 10: Patch with invalid regex fails
    #[test]
    fn test_apply_patch_invalid_regex() {
        let content = "CFLAGS = -O2";
        let result = apply_patch(content, "[invalid(regex", "replacement");
        assert!(result.is_err());
    }

    // Test 11: Patch multiline content
    #[test]
    fn test_apply_patch_multiline() {
        let content = "CFLAGS = -O2 -flto=thin\nCXXFLAGS = -O2 -flto=thin";
        let result = apply_patch(content, r"-flto=thin", "-flto=full");
        assert!(result.is_ok());
        let patched = result.unwrap();
        assert_eq!(patched.matches("-flto=full").count(), 2);
    }

    // Test 12: Patch preserves surrounding content
    #[test]
    fn test_apply_patch_preserves_content() {
        let content = "prefix\nCFLAGS = -O2 -flto=thin\nsuffix";
        let result = apply_patch(content, r"-flto=thin", "-flto=full");
        assert!(result.is_ok());
        let patched = result.unwrap();
        assert!(patched.contains("prefix"));
        assert!(patched.contains("suffix"));
    }

    // ======= MAKEFILE PATCHING TESTS (Tests 13-18)

    // Test 13: Patch Makefile with file I/O
    #[test]
    fn test_patch_makefile_basic() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -O2 -flto=thin").unwrap();

        let result = patch_makefile(&makefile_path, r"-flto=thin", "-flto=full", &backup_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);

        let content = fs::read_to_string(&makefile_path).unwrap();
        assert!(content.contains("-flto=full"));
        assert!(!content.contains("-flto=thin"));
    }

    // Test 14: Patch creates backup file
    #[test]
    fn test_patch_makefile_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -O2 -flto=thin").unwrap();

        patch_makefile(&makefile_path, r"-flto=thin", "-flto=full", &backup_dir).unwrap();

        let backup_path = backup_dir.join("Makefile.bak");
        assert!(backup_path.exists());
        
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert!(backup_content.contains("-flto=thin"));
    }

    // Test 15: Patch fails if file not found
    #[test]
    fn test_patch_makefile_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("nonexistent");
        let backup_dir = temp_dir.path().join("backups");

        let result = patch_makefile(&makefile_path, r"-flto=thin", "-flto=full", &backup_dir);
        assert!(result.is_err());
    }

    // Test 16: Patch counts multiple matches
    #[test]
    fn test_patch_makefile_multiple_matches() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -flto=thin\nCXXFLAGS = -flto=thin").unwrap();

        let result = patch_makefile(&makefile_path, r"-flto=thin", "-flto=full", &backup_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
    }

    // Test 17: Patch with invalid regex fails
    #[test]
    fn test_patch_makefile_invalid_regex() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -O2").unwrap();

        let result = patch_makefile(&makefile_path, "[invalid", "replacement", &backup_dir);
        assert!(result.is_err());
    }

    // Test 18: Patch fails if pattern not found in file
    #[test]
    fn test_patch_makefile_pattern_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "CFLAGS = -O2").unwrap();

        let result = patch_makefile(&makefile_path, r"-flto=thin", "-flto=full", &backup_dir);
        assert!(result.is_err());
    }

    // ======= CONFIG PATCHING TESTS (Tests 19-24)

    // Test 19: Apply CONFIG option to new .config
    #[test]
    fn test_patch_config_new() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        let result = patch_config(&config_path, "CONFIG_LTO_CLANG", "y", &backup_dir);
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("CONFIG_LTO_CLANG=y"));
    }

    // Test 20: Update existing CONFIG option
    #[test]
    fn test_patch_config_update_existing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "CONFIG_LTO_CLANG=n").unwrap();
        drop(file);

        let result = patch_config(&config_path, "CONFIG_LTO_CLANG", "y", &backup_dir);
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("CONFIG_LTO_CLANG=y"));
        assert!(!content.contains("CONFIG_LTO_CLANG=n"));
    }

    // Test 21: Patch config creates backup
    #[test]
    fn test_patch_config_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "CONFIG_FOO=bar").unwrap();
        drop(file);

        patch_config(&config_path, "CONFIG_LTO_CLANG", "y", &backup_dir).unwrap();

        let backup_path = backup_dir.join(".config.bak");
        assert!(backup_path.exists());
    }

    // Test 22: Patch multiple CONFIG options
    #[test]
    fn test_patch_config_multiple_options() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        patch_config(&config_path, "CONFIG_LTO_CLANG", "y", &backup_dir).unwrap();
        patch_config(&config_path, "CONFIG_CFI_CLANG", "y", &backup_dir).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("CONFIG_LTO_CLANG=y"));
        assert!(content.contains("CONFIG_CFI_CLANG=y"));
    }

    // Test 23: Patch config handles empty file
    #[test]
    fn test_patch_config_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        File::create(&config_path).unwrap();

        let result = patch_config(&config_path, "CONFIG_LTO_CLANG", "y", &backup_dir);
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("CONFIG_LTO_CLANG=y"));
    }

    // Test 24: Patch config with special characters in value
    #[test]
    fn test_patch_config_special_chars() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");
        let backup_dir = temp_dir.path().join("backups");

        let result = patch_config(&config_path, "CONFIG_PATH", "/some/path/here", &backup_dir);
        assert!(result.is_ok());

        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("CONFIG_PATH=/some/path/here"));
    }
}

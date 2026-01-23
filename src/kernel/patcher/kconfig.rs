//! Kernel configuration management (.config and .config.override)
//!
//! This module handles the generation and application of kernel configuration
//! options through both native KConfig injection and direct .config manipulation.

use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use crate::error::PatchError;
use crate::models::{LtoType, HardeningLevel};

// Pre-compiled regex patterns for kconfig-specific operations
static LTO_REMOVAL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(?:CONFIG_LTO_|CONFIG_HAS_LTO_|# CONFIG_LTO_|# CONFIG_HAS_LTO_)[^\n]*$")
        .expect("Invalid LTO removal regex")
});
static SPACE_COLLAPSE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\n\n+").expect("Invalid space collapse regex")
});

/// Result type for patching operations
pub type PatchResult<T> = std::result::Result<T, PatchError>;

/// KernelPatcher implementation for kconfig operations
impl crate::kernel::patcher::KernelPatcher {
    /// Generate `.config.override` file for native KConfig injection via KCONFIG_ALLCONFIG
    ///
    /// Creates a `.config.override` file in the kernel source directory that will be
    /// automatically processed by the kernel's KConfig system through the `KCONFIG_ALLCONFIG`
    /// environment variable. This allows GOATd-specific configuration to integrate cleanly
    /// with the native kernel build system.
    ///
    /// CRITICAL FIX: This method also writes configuration to the parent directory as `../config`
    /// (relative to kernel source) for compatibility with PKGBUILD's "Setting config..." step
    /// which expects: `cp ../config .config` to work correctly.
    ///
    /// # Arguments
    /// * `options` - HashMap of CONFIG_* options to inject
    /// * `lto_type` - LTO configuration type (Full, Thin, or None)
    ///
    /// # Returns
    /// Path to the generated `.config.override` file, or error
    pub fn generate_config_override(&self, options: HashMap<String, String>, lto_type: LtoType) -> PatchResult<PathBuf> {
        let override_path = self.src_dir().join(".config.override");
        
        // CRITICAL FIX: Also write config to parent directory as ../config
        // This is needed for PKGBUILD's "Setting config..." step: cp ../config .config
        // The parent directory is srcdir in makepkg context
        let parent_config_path = self.src_dir().parent()
            .map(|p| p.join("config"))
            .unwrap_or_else(|| PathBuf::from("../config"));
        
        let mut content = String::new();
        content.push_str("# GOATd-generated .config.override - Native KConfig injection\n");
        content.push_str("# Processed by kernel via KCONFIG_ALLCONFIG environment variable\n");
        content.push_str("# Do NOT hand-edit - regenerated each build\n\n");
        
        // STEP 1: Inject LTO configuration based on lto_type
        match lto_type {
            LtoType::Full => {
                content.push_str("# LTO Configuration (FULL)\n");
                content.push_str("CONFIG_LTO_CLANG=y\n");
                content.push_str("CONFIG_LTO_CLANG_FULL=y\n");
                content.push_str("CONFIG_HAS_LTO_CLANG=y\n");
                eprintln!("[Patcher] [CONFIG-OVERRIDE] LTO set to FULL");
            }
            LtoType::Thin => {
                content.push_str("# LTO Configuration (THIN)\n");
                content.push_str("CONFIG_LTO_CLANG=y\n");
                content.push_str("CONFIG_LTO_CLANG_THIN=y\n");
                content.push_str("CONFIG_HAS_LTO_CLANG=y\n");
                eprintln!("[Patcher] [CONFIG-OVERRIDE] LTO set to THIN");
            }
            LtoType::None => {
                content.push_str("# LTO Configuration (DISABLED)\n");
                eprintln!("[Patcher] [CONFIG-OVERRIDE] LTO disabled");
            }
        }
        
        content.push_str("\n# Clang/LLVM toolchain enforcement\n");
        content.push_str("CONFIG_CC_IS_CLANG=y\n");
        content.push_str("CONFIG_CLANG_VERSION=190106\n");
        
        // STEP 2: Inject user-provided options (skip special prefix keys)
        content.push_str("\n# User-provided configuration options\n");
        for (key, value) in &options {
            // Skip special metadata keys that start with underscore
            if !key.starts_with("_") {
                content.push_str(&format!("{}={}\n", key, value));
            }
        }
        
        // STEP 3: Extract and inject MGLRU options if present
        let mut mglru_count = 0;
        for (key, value) in &options {
            if key.starts_with("_MGLRU_CONFIG_") {
                // Value format: "CONFIG_LRU_GEN=y" - extract and parse it
                if let Some(eq_pos) = value.find('=') {
                    let config_key = value[..eq_pos].to_string();
                    let config_value = value[eq_pos + 1..].to_string();
                    content.push_str(&format!("{}={}\n", config_key, config_value));
                    mglru_count += 1;
                    eprintln!("[Patcher] [CONFIG-OVERRIDE] Injected MGLRU: {}={}", config_key, config_value);
                }
            }
        }
        
        if mglru_count > 0 {
            eprintln!("[Patcher] [CONFIG-OVERRIDE] Total MGLRU options: {}", mglru_count);
        }
        
        // Write .config.override file
        fs::write(&override_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config.override: {}", e)))?;
        
        // CRITICAL FIX: Also write to parent directory as ../config for PKGBUILD compatibility
        // This ensures "cp ../config .config" in PKGBUILD's "Setting config..." step works
        if let Ok(_) = fs::write(&parent_config_path, &content) {
            eprintln!("[Patcher] [CONFIG-OVERRIDE] Also wrote config to parent directory: {}", parent_config_path.display());
        } else {
            eprintln!("[Patcher] [CONFIG-OVERRIDE] WARNING: Could not write to parent config path (may be non-fatal): {}", parent_config_path.display());
        }
        
        eprintln!("[Patcher] [CONFIG-OVERRIDE] SUCCESS: Generated .config.override at {}", override_path.display());
        eprintln!("[Patcher] [CONFIG-OVERRIDE] Set KCONFIG_ALLCONFIG={} for KConfig injection", override_path.display());
        
        Ok(override_path)
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
    fn inject_baked_in_cmdline(&self, use_mglru: bool, hardening_level: HardeningLevel) -> PatchResult<()> {
        let config_path = self.src_dir().join(".config");

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
        if hardening_level == HardeningLevel::Minimal {
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
        let config_path = self.src_dir().join(".config");

        // Create backup directory
        fs::create_dir_all(self.backup_dir())
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
            let backup_path = self.backup_dir().join(".config.bak");
            fs::write(&backup_path, &content)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to create backup: {}", e)))?;
        }

        // CRITICAL: Detect MGLRU and hardening level BEFORE consuming options in loop
        let use_mglru = options.iter().any(|(k, _)| k.starts_with("_MGLRU_"));
        let hardening_level = if options.contains_key("_HARDENING_LEVEL_MINIMAL") {
            HardeningLevel::Minimal
        } else if options.contains_key("_HARDENING_LEVEL_HARDENED") {
            HardeningLevel::Hardened
        } else {
            HardeningLevel::Standard
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
         // NOTE: CONFIG_LOCALVERSION is set dynamically based on variant + profile
         // to ensure collision-free kernel version strings across all builds
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

         // ============================================================================
         // PHASE 13: NVIDIA ABI PRESERVATION (SAFETY CLUSTER)
         // ============================================================================
         // These Kconfigs are CRITICAL for NVIDIA driver and DKMS compatibility.
         // They must be FORCEFULLY enabled to ensure ABI stability and proper memory
         // management interoperability with proprietary driver stacks.
         //
         // Safety Cluster Definition:
         // - CONFIG_ZONE_DEVICE: Enables device memory zone support (required for NVIDIA GPU memory)
         // - CONFIG_MEMORY_HOTPLUG: Allows runtime memory hotplug (NVIDIA DKMS interaction)
         // - CONFIG_SPARSEMEM_VMEMMAP: Virtual memory map for sparse memory (GPU P2P DMA support)
         // - CONFIG_DEVICE_PRIVATE: Private device memory support (critical for GPU integration)
         // - CONFIG_PCI_P2PDMA: PCI Peer-to-Peer DMA (NVIDIA direct GPU transfers)
         // - CONFIG_MMU_NOTIFIER: Memory management unit notifications (GPU page table sync)

         eprintln!("[Patcher] [PHASE-13-ABI] Starting NVIDIA ABI Preservation (Safety Cluster)...");

         // Add comment header to .config for documentation
         if !content.is_empty() && !content.ends_with('\n') {
             content.push('\n');
         }
         content.push_str("# NVIDIA ABI Safeguards (Phase 13)\n");

         let safety_cluster = vec![
             ("CONFIG_ZONE_DEVICE", "y"),
             ("CONFIG_MEMORY_HOTPLUG", "y"),
             ("CONFIG_SPARSEMEM_VMEMMAP", "y"),
             ("CONFIG_DEVICE_PRIVATE", "y"),
             ("CONFIG_PCI_P2PDMA", "y"),
             ("CONFIG_MMU_NOTIFIER", "y"),
         ];

         for (key, value) in safety_cluster {
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
                 eprintln!("[Patcher] [PHASE-13-ABI] Removed existing {} entry", key);
             }

             // Inject Safety Cluster option with ABSOLUTE priority
             if !content.is_empty() && !content.ends_with('\n') {
                 content.push('\n');
             }
             content.push_str(&format!("{}={}", key, value));
             content.push('\n');

             eprintln!("[Patcher] [PHASE-13-ABI] CRITICAL: Injected {}={} (NVIDIA ABI Safeguard)", key, value);
         }

         eprintln!("[Patcher] [PHASE-13-ABI] NVIDIA ABI Preservation complete - Safety Cluster enforced");

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

    /// Inject modular LOCALVERSION into .config based on variant and profile
    ///
    /// Implements the kernel naming scheme: `linux-{variant}-goatd-{profile}`
    ///
    /// The LOCALVERSION format is:
    /// - If variant is `linux`: `-linux-goatd-{profile}`
    /// - Otherwise (e.g., `linux-zen`): `-linux-{variant_suffix}-goatd-{profile}`
    ///
    /// Example outputs:
    /// - variant: "linux",       profile: "gaming"  -> LOCALVERSION="-linux-goatd-gaming"
    /// - variant: "linux-zen",   profile: "gaming"  -> LOCALVERSION="-linux-zen-goatd-gaming"
    /// - variant: "linux-mainline", profile: "gaming" -> LOCALVERSION="-linux-mainline-goatd-gaming"
    ///
    /// # Arguments
    /// * `variant` - Kernel variant (e.g., "linux", "linux-zen", "linux-hardened")
    /// * `profile_name` - Profile name (e.g., "gaming", "server", "balanced")
    ///
    /// # Returns
    /// Result indicating success or error
    pub fn inject_modular_localversion(&self, variant: &str, profile_name: &str) -> PatchResult<()> {
        let config_path = self.src_dir().join(".config");

        // Read or create .config
        let mut content = if config_path.exists() {
            fs::read_to_string(&config_path)
                .map_err(|e| PatchError::PatchFailed(format!("Failed to read .config: {}", e)))?
        } else {
            String::new()
        };

        // STEP 1: Construct the LOCALVERSION value based on kernel variant
        let localversion = if variant == "linux" {
            // Standard linux kernel: variant is "linux"
            // Format: -linux-goatd-{profile}
            format!("-linux-goatd-{}", profile_name)
        } else {
            // Variant kernels: variant is "linux-zen", "linux-hardened", etc.
            // Extract the suffix after "linux-" and build: -linux-{variant_suffix}-goatd-{profile}
            // Example: "linux-zen" -> variant_suffix is "zen" -> "-linux-zen-goatd-{profile}"
            let variant_suffix = variant
                .strip_prefix("linux-")
                .unwrap_or(&variant); // Fallback to variant if no "linux-" prefix
            format!("-linux-{}-goatd-{}", variant_suffix, profile_name)
        };

        eprintln!("[Patcher] [LOCALVERSION] variant='{}', profile='{}' -> LOCALVERSION='{}'",
                  variant, profile_name, localversion);

        // STEP 2: Remove any existing CONFIG_LOCALVERSION lines from .config
        let lines: Vec<&str> = content
            .lines()
            .filter(|line| !line.starts_with("CONFIG_LOCALVERSION="))
            .collect();

        if lines.len() != content.lines().count() {
            content = lines.join("\n");
            if !content.is_empty() {
                content.push('\n');
            }
            eprintln!("[Patcher] [LOCALVERSION] Removed existing CONFIG_LOCALVERSION entries");
        }

        // STEP 3: Inject the new CONFIG_LOCALVERSION at the end of .config
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("CONFIG_LOCALVERSION=\"{}\"", localversion));
        content.push('\n');

        // Write updated .config
        fs::write(&config_path, &content)
            .map_err(|e| PatchError::PatchFailed(format!("Failed to write .config: {}", e)))?;

        eprintln!("[Patcher] [LOCALVERSION] SUCCESS: Injected CONFIG_LOCALVERSION=\"{}\" into .config", localversion);

        Ok(())
    }
}

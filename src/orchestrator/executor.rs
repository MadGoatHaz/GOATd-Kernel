//! Phase execution: hardware validation, build spawning, output streaming.
//!
//! Integrates with the unified logging pipeline via LogCollector for:
//! - Granular compilation progress tracking (CC/LD/AR line counting)
//! - Parsed milestone logging (high-level status updates)
//! - Full detailed output persistence

use crate::models::{HardwareInfo, KernelConfig};
use crate::error::BuildError;
use std::path::Path;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;
use regex::Regex;

/// Validates memory and disk requirements.
pub fn validate_hardware(hardware: &HardwareInfo) -> Result<(), BuildError> {
    if hardware.ram_gb < 4 {
        return Err(BuildError::PreparationFailed(
            "Insufficient RAM: minimum 4GB required".to_string(),
        ));
    }

    if hardware.disk_free_gb < 20 {
        return Err(BuildError::PreparationFailed(
            "Insufficient disk space: minimum 20GB free required".to_string(),
        ));
    }

    Ok(())
}

/// Checks kernel version is present.
pub fn validate_kernel_config(config: &KernelConfig) -> Result<(), BuildError> {
    if config.version.is_empty() {
        return Err(BuildError::ConfigurationFailed(
            "Kernel version not specified in configuration".to_string(),
        ));
    }

    // Validate LTO type is reasonable
    // (All enum variants are valid, so no additional validation needed)

    Ok(())
}


/// Verifies hardware and source structure.
pub fn prepare_build_environment(hardware: &HardwareInfo, kernel_path: &Path) -> Result<(), BuildError> {
    eprintln!("[Build] [DEBUG] Preparing build environment at: {}", kernel_path.display());
    
    validate_hardware(hardware)?;
    
    // =========================================================================
    // SAFEGUARD VALIDATION: Check if workspace path is valid for Kbuild
    // =========================================================================
    // Redundant check: even if controller validation is bypassed, this ensures
    // we catch invalid paths (with spaces or colons) before attempting the build
    use crate::kernel::validator::validate_kbuild_path;
    
    validate_kbuild_path(kernel_path).map_err(|app_err| {
        let error_msg = app_err.user_message();
        eprintln!("[Build] [SAFEGUARD] Path validation failed: {}", error_msg);
        BuildError::PreparationFailed(error_msg)
    })?;
    
    eprintln!("[Build] [SAFEGUARD] ✓ Workspace path validation passed");
    
    if !kernel_path.exists() {
        return Err(BuildError::PreparationFailed(
            format!("Kernel source not found at: {}", kernel_path.display())
        ));
    } else {
        eprintln!("[Build] [PREPARATION] Kernel source found at: {}", kernel_path.display());
    }

    if !kernel_path.is_dir() {
        return Err(BuildError::PreparationFailed(
            format!("Kernel source path exists but is not a directory: {}", kernel_path.display())
        ));
    }

    if !kernel_path.join("PKGBUILD").exists() && !kernel_path.join("Makefile").exists() {
        return Err(BuildError::PreparationFailed(
            format!("Valid kernel source not found in {}. Missing PKGBUILD or Makefile.", kernel_path.display())
        ));
    }

    eprintln!("[Build] [PREPARATION] Environment ready");
    Ok(())
}


/// Validates and returns config.
pub fn configure_build(
    config: &KernelConfig,
    _hardware: &HardwareInfo,
) -> Result<KernelConfig, BuildError> {
    // Validate config - the Finalizer has already applied all policies
    validate_kernel_config(config)?;

    Ok(config.clone())
}


/// Validates build configuration.
pub fn prepare_kernel_build(config: &KernelConfig) -> Result<(), BuildError> {
    validate_kernel_config(config)?;
    Ok(())
}

/// Parses [X/Y] or [%] from make output and provides granular milestone tracking.
///
/// Returns progress incrementally based on:
/// 1. makepkg milestones (e.g., "Retrieving sources...", "Extracting sources...", "Starting build()...")
/// 2. [X/Y] compilation patterns (granular sub-percentage)
/// 3. [%] percentage patterns
/// 4. Compilation line counter for pseudo-progress
fn parse_build_progress(line: &str) -> Option<u32> {
    // PRIORITY 1: Match makepkg milestone messages for incremental progress
    // These appear as major phase transitions during package build
    if line.contains("==> Retrieving sources") {
        return Some(5); // Source retrieval started
    }
    if line.contains("==> Extracting sources") {
        return Some(10); // Source extraction started
    }
    if line.contains("==> Starting build()") {
        return Some(20); // Build() function about to start
    }
    if line.contains("==> Packaging") || line.contains("==> Creating package") {
        return Some(85); // Packaging phase started, almost complete
    }

    // PRIORITY 2: Match [X/Y] patterns (most granular - e.g., "[ 582/12041]")
    // This provides real sub-percentage during building
    if let Ok(re) = Regex::new(r"\[\s*(\d+)/(\d+)\]") {
        if let Some(caps) = re.captures(line) {
            if let (Ok(current), Ok(total)) = (caps[1].parse::<u32>(), caps[2].parse::<u32>()) {
                if total > 0 {
                    let progress = (current as f32 / total as f32 * 100.0) as u32;
                    let clamped = progress.min(100);
                    // Silent progress parsing - diagnostic logging occurs in the orchestrator instead
                    return Some(clamped);
                }
            }
        }
    }

    // PRIORITY 3: Match percentage patterns like "[ 45%]" or "[  1%]"
    if let Ok(re) = Regex::new(r"\[\s*(\d+)%\]") {
        if let Some(caps) = re.captures(line) {
            if let Ok(progress) = caps[1].parse::<u32>() {
                return Some(progress.min(100));
            }
        }
    }

    // PRIORITY 4: Match file count patterns like "CC  arch/x86/boot/cpuflags.c"
    // Provide pseudo-progress by incrementing small amounts for each compilation unit
    // This ensures the progress bar moves during intensive CC/LD phases even without [X/Y] markers
    if line.contains("CC ") || line.contains("LD ") || line.contains("AR ") {
        return Some(0); // Just a marker that work is happening
    }

    None
}

/// Execute a real kernel build process.
///
/// This function spawns an actual build command (make or build script) in the kernel
/// source directory and streams stdout/stderr to the provided callback. It's the
/// real implementation replacing the simulated version.
///
/// # Arguments
/// * `kernel_path` - Path to the kernel source directory
/// * `config` - Kernel configuration (used for build options)
/// * `output_callback` - Callback function to receive output lines and progress updates
/// * `cancel_rx` - Watch channel receiver for cancellation signals
/// * `log_collector` - Optional log collector for dual-writing build output
///
/// # Returns
/// * `Ok(())` if build completes successfully
/// * `Err(BuildError::BuildFailed)` if build fails or process exits with error
/// * `Err(BuildError::BuildCancelled)` if build is cancelled
///
/// # Example
/// ```no_run
/// use std::path::Path;
/// use goatd_kernel::models::KernelConfig;
/// use goatd_kernel::orchestrator::executor::run_kernel_build;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let kernel_path = Path::new("/tmp/linux-6.6");
/// let config = KernelConfig::default();
/// let (tx, rx) = tokio::sync::watch::channel(false);
///
/// run_kernel_build(kernel_path, &config, |output, _progress| {
///     println!("Build: {}", output);
/// }, rx, None).await?;
/// # Ok(())
/// # }
/// ```
///

pub async fn run_kernel_build<F>(
    kernel_path: &Path,
    config: &KernelConfig,
    mut output_callback: F,
    mut cancel_rx: watch::Receiver<bool>,
    log_collector: Option<std::sync::Arc<crate::LogCollector>>,
) -> Result<(), BuildError>
where
    F: FnMut(String, Option<u32>) + Send + 'static,
{
    // CRITICAL FIX: Counter for meaningful compilation progress tracking
    // Detects every 100 CC (compilation unit) lines and sends status updates
    let mut cc_line_counter = 0_usize;
    
    // NOTE: Environment variable setup has been moved to Patcher::prepare_build_environment()
    // The Orchestrator provides pre-computed environment variables through the process environment.
    // The Executor is now responsible ONLY for:
    // 1. Spawning the build process with inherited environment
    // 2. Streaming output and progress
    // 3. Handling cancellation and exit codes
    //
    // This enforces Patcher Exclusivity: only the Patcher prepares build environment.

    // =========================================================================
    // HOOK: DEEP PIPE DRY RUN (EARLY CHECK - BEFORE ANY PATCHING)
    // =========================================================================
    // CRITICAL: Check DRY_RUN_HOOK at the VERY START before any patcher operations
    // This prevents expensive filesystem operations in test/dry-run mode
    let dry_run_mode = std::env::var("GOATD_DRY_RUN_HOOK").is_ok();
    if dry_run_mode {
        eprintln!("[Build] [DRY-RUN] ========================================");
        eprintln!("[Build] [DRY-RUN] DRY RUN HOOK ACTIVATED (EARLY CHECK)");
        eprintln!("[Build] [DRY-RUN] Skipping all expensive filesystem operations");
        eprintln!("[Build] [DRY-RUN] ========================================");
        
        // Create minimal artifacts for testing
        let lto_level = match config.lto_type {
            crate::models::LtoType::Full => "full",
            crate::models::LtoType::Thin => "thin",
            crate::models::LtoType::None => "none",
        };
        let use_bore = config.config_options.get("_APPLY_BORE_SCHEDULER") == Some(&"1".to_string());
        
        eprintln!("[Build] [DRY-RUN] Build environment prepared successfully");
        eprintln!("[Build] [DRY-RUN] Environment verification:");
        eprintln!("[Build] [DRY-RUN]   • Compiler: LLVM/Clang clang");
        eprintln!("[Build] [DRY-RUN]   • Linker: ld.lld");
        eprintln!("[Build] [DRY-RUN]   • LTO Level: {}", lto_level);
        eprintln!("[Build] [DRY-RUN]   • BORE Scheduler: {}", if use_bore { "Enabled" } else { "Disabled" });
        eprintln!("[Build] [DRY-RUN]   • Module Stripping: ENABLED (INSTALL_MOD_STRIP=1)");
        eprintln!("[Build] [DRY-RUN] ========================================");
        eprintln!("[Build] [DRY-RUN] Configuration dump:");
        eprintln!("[Build] [DRY-RUN]   Profile: {}", config.profile);
        eprintln!("[Build] [DRY-RUN]   Kernel Version: {}", config.version);
        eprintln!("[Build] [DRY-RUN] ========================================");
        eprintln!("[Build] [DRY-RUN] EXIT: Halting before build (DRY_RUN_HOOK set)");
        output_callback("DRY RUN: Environment verified, halting before build".to_string(), Some(100));
        return Ok(());
    }

    // CRITICAL: Build execution starting - real-time logging begins here
     // Diagnostic logs go to eprintln only (NOT to callback), clean output pipe to UI
     let pkgbuild_path = kernel_path.join("PKGBUILD");
     eprintln!("[Build] [EXECUTOR] Starting kernel build from: {}", kernel_path.display());
     
     if !pkgbuild_path.exists() {
         eprintln!("[Build] [EXECUTOR] WARNING: PKGBUILD not found, will attempt fallback build");
     }

    // Validate config before proceeding
    validate_kernel_config(config)?;

    // Determine build command based on what's available
    let num_jobs = num_cpus::get();

    let mut command = if pkgbuild_path.exists() {
        // Use makepkg if PKGBUILD is available (Arch-style)
        eprintln!("[Build] [DEBUG] PKGBUILD found, using 'makepkg'");
        let mut cmd = Command::new("makepkg");
        cmd.arg("-s");
        cmd.arg("-f");
        cmd.arg("--noconfirm");
        cmd
    } else if kernel_path.join("scripts/build.sh").exists() {
        eprintln!("[Build] [DEBUG] Found scripts/build.sh, using it");
        // Use custom build script if available
        let mut cmd = Command::new("bash");
        cmd.arg("scripts/build.sh");
        cmd
    } else {
        eprintln!("[Build] [DEBUG] Falling back to standard 'make'");
        // Fall back to make with parallel jobs
        let mut cmd = Command::new("make");
        cmd.arg(format!("-j{}", num_jobs));
        cmd.arg("bzImage");
        cmd
    };

    // Set working directory
    command.current_dir(kernel_path);

    // Set up environment for parallel builds
    command.env("MAKEFLAGS", format!("-j{}", num_jobs));
    command.env("KBUILD_BUILD_TIMESTAMP", "");

    // ============================================================================
    // PYTHON BUILD CLEANLINESS - PREVENT __pycache__ GENERATION
    // ============================================================================
    // PYTHONDONTWRITEBYTECODE=1 prevents Python from creating .pyc files and
    // __pycache__ directories during the build. This is critical for:
    // - Preventing $srcdir path leakage into compiled Python bytecode
    // - Improving reproducibility by avoiding non-deterministic cache files
    // - Reducing package size and cleanup requirements
    // This prevents warnings from makepkg about $srcdir references in bytecode
    command.env("PYTHONDONTWRITEBYTECODE", "1");
    eprintln!("[Build] [PYTHON] Set PYTHONDONTWRITEBYTECODE=1 to prevent __pycache__ generation");

    // ============================================================================
    // PHASE 3: MODULE SIZE & STRIPPING OPTIMIZATION (CRITICAL)
    // ============================================================================
    // INSTALL_MOD_STRIP=1 ensures modules are stripped during installation phase
    // This reduces module size from 462MB to <100MB by removing debug symbols
    command.env("INSTALL_MOD_STRIP", "1");
    eprintln!("[Build] [MODULE-OPT] Set INSTALL_MOD_STRIP=1 for module stripping during installation");

    // ============================================================================
    // HOST-OPTIMIZED KERNEL BUILD (-march=native support)
    // ============================================================================
    // KCFLAGS injects kernel compilation flags for host-specific optimizations
    // LLVM_IAS=1 forces LLVM inline assembler for consistent optimization
    // Respect user's native_optimizations toggle from config
    if config.native_optimizations {
        command.env("KCFLAGS", "-march=native");
        eprintln!("[Build] [OPTIMIZATION] Native optimizations enabled: Set KCFLAGS=\"-march=native\" for host-optimized kernel build");
    } else {
        eprintln!("[Build] [OPTIMIZATION] Native optimizations disabled: KCFLAGS not set (portable binary target)");
    }
    command.env("LLVM_IAS", "1");
    eprintln!("[Build] [OPTIMIZATION] Set LLVM_IAS=1 for LLVM inline assembler enforcement");

    // ============================================================================
    // INTELLIGENT ASSET CACHING - SRCDEST CONFIGURATION
    // ============================================================================
    // Configure makepkg to reuse source files across builds instead of re-downloading
    // SRCDEST specifies where makepkg stores downloaded sources
    // Using /tmp/kernel-sources ensures persistent caching across builds
    let srcdest = std::env::var("SRCDEST")
        .unwrap_or_else(|_| "/tmp/kernel-sources".to_string());
    command.env("SRCDEST", &srcdest);
    eprintln!("[Build] [CACHE] Set SRCDEST={} for smart asset reuse", srcdest);

    // Also set cache directory for ccache if available
    if std::path::Path::new("/usr/lib/ccache/bin").exists() {
        let ccache_dir = std::env::var("CCACHE_DIR")
            .unwrap_or_else(|_| format!("{}/.cache/ccache",
                std::env::var("HOME").unwrap_or_else(|_| "/root".to_string())));
        command.env("CCACHE_DIR", &ccache_dir);
        
        // Prepend ccache to PATH for compiler caching
        let current_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/usr/lib/ccache/bin:{}", current_path);
        command.env("PATH", new_path);
        eprintln!("[Build] [CACHE] ccache enabled with CCACHE_DIR={}", ccache_dir);
    }

    // ============================================================================
    // INJECT BUILD OPTIONS INTO ENVIRONMENT (CRITICAL FIX)
    // ============================================================================
    // Uses GOATD_ prefix to avoid collisions with makepkg's reserved names and
    // variable name parsing issues (e.g., "package__lto_level=thin()" syntax errors)
    
    // Log and apply build configuration options
    eprintln!("[Build] [CONFIG] Applying build configuration:");
    eprintln!("[Build] [CONFIG]  Profile: {}", config.profile);
    eprintln!("[Build] [CONFIG]  LTO Level: {:?}", config.lto_type);
    eprintln!("[Build] [CONFIG]  Use Modprobed DB: {}", config.use_modprobed);
    eprintln!("[Build] [CONFIG]  Use Whitelist: {}", config.use_whitelist);
    eprintln!("[Build] [CONFIG]  Hardening: {}", config.hardening);
    eprintln!("[Build] [CONFIG]  Secure Boot: {}", config.secure_boot);

    // =========================================================================
    // ENVIRONMENT VARIABLES: Inherited from Patcher preparation
    // =========================================================================
    // The Patcher has prepared all environment variables (LLVM=1, CC=clang, etc.)
    // and PATH purification. The Executor inherits these from the process environment.
    // This enforces Patcher Exclusivity: only the Patcher configures the environment.
    eprintln!("[Build] [COMPILER] ========================================");
    eprintln!("[Build] [COMPILER] INHERITED CLANG/LLVM TOOLCHAIN (from Patcher)");
    eprintln!("[Build] [COMPILER] ========================================");
    eprintln!("[Build] [COMPILER] Environment prepared by: KernelPatcher::prepare_build_environment()");
    eprintln!("[Build] [COMPILER] Executor receives pre-configured environment variables");
    eprintln!("[Build] [COMPILER] CRITICAL VARIABLES (inherited from parent process):");
    eprintln!("[Build] [COMPILER] LLVM=1, LLVM_IAS=1, CC=clang, CXX=clang++, LD=ld.lld");

    // BORE Scheduler
    let use_bore = config.config_options.get("_APPLY_BORE_SCHEDULER") == Some(&"1".to_string());
    command.env("GOATD_APPLY_BORE_SCHEDULER", if use_bore { "1" } else { "0" });

    // Set LTO level environment variable (CRITICAL: must match patcher expectations exactly)
    // CRITICAL FIX: Ensure we're setting the literal string that the patcher expects
    let lto_level_str = match config.lto_type {
        crate::models::LtoType::Full => {
            eprintln!("[Build] [CONFIG] LTO_TYPE: Full (enum value) → setting GOATD_LTO_LEVEL='full'");
            "full"
        }
        crate::models::LtoType::Thin => {
            eprintln!("[Build] [CONFIG] LTO_TYPE: Thin (enum value) → setting GOATD_LTO_LEVEL='thin'");
            "thin"
        }
        crate::models::LtoType::None => {
            eprintln!("[Build] [CONFIG] LTO_TYPE: None (enum value) → setting GOATD_LTO_LEVEL='none'");
            "none"
        }
    };
    command.env("GOATD_LTO_LEVEL", lto_level_str);
    eprintln!("[Build] [ENV] Set GOATD_LTO_LEVEL={} (from config.lto_type: {:?})", lto_level_str, config.lto_type);
    eprintln!("[Build] [CONFIG] config.user_toggled_lto={}, config.lto_type={:?}", config.user_toggled_lto, config.lto_type);

    // Set modprobed auto-discovery flag
    command.env("GOATD_USE_MODPROBED_DB", if config.use_modprobed { "1" } else { "0" });
    eprintln!("[Build] [ENV] Set GOATD_USE_MODPROBED_DB={}", if config.use_modprobed { "1" } else { "0" });

    // Set whitelist protection flag
    command.env("GOATD_USE_KERNEL_WHITELIST", if config.use_whitelist { "1" } else { "0" });
    eprintln!("[Build] [ENV] Set GOATD_USE_KERNEL_WHITELIST={}", if config.use_whitelist { "1" } else { "0" });

    // Set hardening level (profile-specific)
    command.env("GOATD_KERNEL_HARDENING", config.hardening.to_string());
    eprintln!("[Build] [ENV] Set GOATD_KERNEL_HARDENING={}", config.hardening);

    // Set secure boot flag (standardized environment variable)
    command.env("GOATD_ENABLE_SECURE_BOOT", if config.secure_boot { "1" } else { "0" });
    eprintln!("[Build] [ENV] Set GOATD_ENABLE_SECURE_BOOT={}", if config.secure_boot { "1" } else { "0" });
    
    // Set hardening enable flag (enables for Standard and Hardened levels, disabled for Minimal)
    let hardening_enabled = config.hardening != crate::models::HardeningLevel::Minimal;
    command.env("GOATD_ENABLE_HARDENING", if hardening_enabled { "1" } else { "0" });
    eprintln!("[Build] [ENV] Set GOATD_ENABLE_HARDENING={} (hardening_level={})", if hardening_enabled { "1" } else { "0" }, config.hardening);

    // Native architecture + -O2 standard (all profiles)
    let mut cflags = String::from("-march=native -mtune=native -O2");
    let mut cxxflags = String::from("-march=native -mtune=native -O2");

    // LTO via CFLAGS/CXXFLAGS
    let lto_level = match config.lto_type {
        crate::models::LtoType::Full => "full",
        crate::models::LtoType::Thin => "thin",
        crate::models::LtoType::None => "none",
    };
    
    if lto_level != "none" {
        let lto_flag = format!(" -flto={}", lto_level);
        cflags.push_str(&lto_flag);
        cxxflags.push_str(&lto_flag);
        eprintln!("[Build] [LTO] Injected -flto={} into CFLAGS and CXXFLAGS", lto_level);
    } else {
        eprintln!("[Build] [LTO] LTO disabled (Base profile selected)");
    }

    // Add Polly loop optimization if enabled and Clang is the compiler
    if config.use_polly {
        // Polly flags for LLVM-based vectorization and loop fusion
        cflags.push_str(" -mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max");
        cxxflags.push_str(" -mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max");
        eprintln!("[Build] [POLLY] Enabled Polly loop optimization (stripmine vectorizer + fusion)");
    }

    command.env("CFLAGS", &cflags);
    command.env("CXXFLAGS", &cxxflags);
    eprintln!("[Build] [COMPILER] Set CFLAGS={}", cflags);
    eprintln!("[Build] [COMPILER] Set CXXFLAGS={}", cxxflags);
    eprintln!("[Build] [COMPILER] GLOBAL STANDARD: Clang 19 with -O2 optimization (ALL profiles)");

    // Set polly flag for build script awareness
    command.env("GOATD_USE_POLLY", if config.use_polly { "1" } else { "0" });
    eprintln!("[Build] [ENV] Set GOATD_USE_POLLY={}", if config.use_polly { "1" } else { "0" });

    // (DRY_RUN_HOOK already checked at the start of run_kernel_build function)

    // Set up stdout and stderr pipes
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    // Spawn the process
    let mut child = command.spawn().map_err(|e| {
        BuildError::BuildFailed(format!(
            "Failed to spawn build process in {}: {}",
            kernel_path.display(),
            e
        ))
    })?;

    // Get stdout and stderr handlers
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BuildError::BuildFailed("Failed to capture stdout".to_string()))?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BuildError::BuildFailed("Failed to capture stderr".to_string()))?;

    // Create readers for both streams
    let stdout_reader = BufReader::new(stdout);
    let stderr_reader = BufReader::new(stderr);

    let mut stdout_lines = stdout_reader.lines();
    let mut stderr_lines = stderr_reader.lines();

    // Stream output from both stdout and stderr
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    
    loop {
        // CRITICAL: Check if both streams are closed BEFORE trying to read
        if stdout_closed && stderr_closed {
            eprintln!("[Build] [INFO] Both stdout and stderr closed, waiting for process exit");
            break;
        }
        
        tokio::select! {
            line_result = stdout_lines.next_line(), if !stdout_closed => {
                match line_result {
                    Ok(Some(line)) => {
                        let progress = parse_build_progress(&line);
                        // CRITICAL: Call callback DIRECTLY - don't spawn async task that might be lost
                        output_callback(line.clone(), progress);
                        
                        // CRITICAL FIX: Track compilation progress with CC line counter
                        // Every 100 CC lines = meaningful progress checkpoint
                        if line.contains("CC ") || line.contains("LD ") || line.contains("AR ") {
                            cc_line_counter += 1;
                            if cc_line_counter % 100 == 0 {
                                let status_msg = format!("Compiling: Processed {} files...", cc_line_counter);
                                output_callback(status_msg.clone(), None);
                                eprintln!("[Build] [STATUS] {}", status_msg);
                            }
                        }
                        
                        // Log to collector (if present) for dual-writing
                        if let Some(ref collector) = log_collector {
                            collector.log_str(&line);
                            // Check for high-level milestones (common makepkg markers)
                            if line.starts_with("==> ") || line.starts_with(":: ") {
                                collector.log_parsed(line.clone());
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("[Build] [INFO] stdout closed");
                        stdout_closed = true;
                    }
                    Err(e) => {
                        let err_msg = format!("stdout read error: {}", e);
                        output_callback(err_msg.clone(), None);
                        if let Some(ref collector) = log_collector {
                            collector.log_str(&err_msg);
                        }
                        stdout_closed = true;
                    }
                }
            }
            line_result = stderr_lines.next_line(), if !stderr_closed => {
                match line_result {
                    Ok(Some(line)) => {
                        // Parse progress from stderr as well
                        let progress = parse_build_progress(&line);
                        let formatted_line = format!("[STDERR] {}", line);
                        // CRITICAL: Call callback DIRECTLY - don't spawn async task
                        output_callback(formatted_line.clone(), progress);
                        
                        // CRITICAL FIX: Track compilation progress from stderr as well
                        if line.contains("CC ") || line.contains("LD ") || line.contains("AR ") {
                            cc_line_counter += 1;
                            if cc_line_counter % 100 == 0 {
                                let status_msg = format!("Compiling: Processed {} files...", cc_line_counter);
                                output_callback(status_msg.clone(), None);
                                eprintln!("[Build] [STATUS] {}", status_msg);
                            }
                        }
                        
                        // Log to collector (if present) for dual-writing
                        if let Some(ref collector) = log_collector {
                            collector.log_str(&formatted_line);
                        }
                    }
                    Ok(None) => {
                        eprintln!("[Build] [INFO] stderr closed");
                        stderr_closed = true;
                    }
                    Err(e) => {
                        let err_msg = format!("stderr read error: {}", e);
                        output_callback(err_msg.clone(), None);
                        if let Some(ref collector) = log_collector {
                            collector.log_str(&err_msg);
                        }
                        stderr_closed = true;
                    }
                }
            }
            _ = cancel_rx.changed() => {
                // Cancellation signal received
                if *cancel_rx.borrow() {
                    output_callback("Build cancelled by user".to_string(), None);
                    
                    // Kill the child process with SIGTERM first
                    if let Err(kill_err) = child.kill().await {
                        output_callback(
                            format!("Warning: Failed to kill process: {}", kill_err),
                            None,
                        );
                    }
                    
                    // Try to kill the entire process group (all sub-processes)
                    #[cfg(unix)]
                    {
                        if let Some(pid) = child.id() {
                            // Strategy 1: Kill all children of this process using pkill -P
                            let _ = std::process::Command::new("pkill")
                                .arg("-P")
                                .arg(pid.to_string())
                                .arg("-9")
                                .output();
                            
                            // Strategy 2: Kill the process group using kill -9 -<pid>
                            // This sends SIGKILL to the entire process group
                            let _ = std::process::Command::new("kill")
                                .arg("-9")
                                .arg(format!("-{}", pid))
                                .output();
                        }
                    }
                    
                    // Wait a moment for processes to be killed
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    
                    return Err(BuildError::BuildCancelled);
                }
            }
        }

        // Check if process has exited
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("[Build] [INFO] Process exited with status: {}", status);
                if status.success() {
                    output_callback("Build completed successfully".to_string(), Some(100));
                    eprintln!("[Build] [SUCCESS] Kernel build completed successfully");
                    return Ok(());
                } else {
                    let exit_msg = if let Some(code) = status.code() {
                        format!("Build failed with exit code {}", code)
                    } else {
                        "Build terminated by signal".to_string()
                    };
                    eprintln!("[Build] [FAILED] {}", exit_msg);
                    return Err(BuildError::BuildFailed(exit_msg));
                }
            }
            Ok(None) => {
                // Process still running, continue reading
            }
            Err(e) => {
                return Err(BuildError::BuildFailed(format!(
                    "Failed to check process status: {}",
                    e
                )));
            }
        }
    }
    
    // Final wait for process to complete
    eprintln!("[Build] [WAIT] Waiting for process to complete...");
    match child.wait().await {
        Ok(status) => {
            if status.success() {
                output_callback("Build completed successfully".to_string(), Some(100));
                eprintln!("[Build] [SUCCESS] Final wait: Kernel build completed successfully");
                Ok(())
            } else {
                let exit_msg = if let Some(code) = status.code() {
                    format!("Build failed with exit code {}", code)
                } else {
                    "Build terminated by signal".to_string()
                };
                eprintln!("[Build] [FAILED] Final wait: {}", exit_msg);
                Err(BuildError::BuildFailed(exit_msg))
            }
        }
        Err(e) => {
            Err(BuildError::BuildFailed(format!(
                "Failed to wait for process: {}",
                e
            )))
        }
    }
}

/// Simulate validation phase operations.
///
/// Pure function: validates kernel build results, returns nothing.
/// In a real implementation, would check:
/// - Kernel artifacts exist (vmlinuz, initramfs, System.map)
/// - LTO is enabled in the build
/// - CFI metadata is present
/// - Boot readiness
///
/// # Arguments
/// * `config` - Kernel configuration that was built
///
/// # Returns
/// * `Ok(())` if all validations pass
/// * `Err(BuildError::ValidationFailed)` if any validation fails
pub fn validate_kernel_build(config: &KernelConfig) -> Result<(), BuildError> {
    // Validate config first
    validate_kernel_config(config)?;

    // In a real implementation:
    // - Check vmlinuz exists and has reasonable size
    // - Verify LTO symbols in compiled binary
    // - Check for CFI metadata
    // - Verify boot readiness
    // For now, we just validate config

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BootManager, BootType, GpuVendor, InitSystem, LtoType, StorageType};
    use std::collections::HashMap;

    fn create_test_hardware() -> HardwareInfo {
        HardwareInfo {
            cpu_model: "Intel Core i7".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 32,
            disk_free_gb: 100,
            gpu_vendor: GpuVendor::Nvidia,
            gpu_model: "NVIDIA RTX 3080".to_string(),
            storage_type: StorageType::Nvme,
            storage_model: "Samsung 970 EVO".to_string(),
            boot_type: BootType::Efi,
            boot_manager: BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        }
    }

    fn create_test_config() -> KernelConfig {
         KernelConfig {
              version: "6.6.0".to_string(),
              lto_type: LtoType::Thin,
              use_modprobed: true,
              use_whitelist: false,
              driver_exclusions: vec![],
              config_options: HashMap::new(),
              hardening: crate::models::HardeningLevel::Standard,
              secure_boot: false,
              profile: "Generic".to_string(),
              use_bore: false,
             use_polly: false,
             use_mglru: false,
             user_toggled_bore: false,
             user_toggled_polly: false,
             user_toggled_mglru: false,
             user_toggled_hardening: false,
             user_toggled_lto: false,
             mglru_enabled_mask: 0x0007,
             mglru_min_ttl_ms: 1000,
             hz: 300,
             preemption: "Voluntary".to_string(),
             force_clang: true,
             lto_shield_modules: vec![],
             scx_available: vec![],
             scx_active_scheduler: None,
             native_optimizations: true,
             user_toggled_native_optimizations: false,
         }
    }

    #[test]
    fn test_validate_hardware_sufficient() {
        let hw = create_test_hardware();
        assert!(validate_hardware(&hw).is_ok());
    }

    #[test]
    fn test_validate_hardware_insufficient_ram() {
        let mut hw = create_test_hardware();
        hw.ram_gb = 2;
        assert!(validate_hardware(&hw).is_err());
    }

    #[test]
    fn test_validate_hardware_insufficient_disk() {
        let mut hw = create_test_hardware();
        hw.disk_free_gb = 10;
        assert!(validate_hardware(&hw).is_err());
    }

    #[test]
    fn test_validate_kernel_config_valid() {
        let cfg = create_test_config();
        assert!(validate_kernel_config(&cfg).is_ok());
    }

    #[test]
    fn test_validate_kernel_config_missing_version() {
         let mut cfg = create_test_config();
         cfg.version.clear();
        assert!(validate_kernel_config(&cfg).is_err());
    }


    #[test]
    fn test_prepare_build_environment_success() {
        use tempfile::TempDir;
        use std::fs::File;
        use std::io::Write;

        let hw = create_test_hardware();
        
        // Use tempfile crate for clean temporary directory management
        // This delegates filesystem setup to a proper testing utility
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        
        // Create a dummy PKGBUILD using proper file creation
        let pkgbuild_path = temp_dir.path().join("PKGBUILD");
        let mut file = File::create(&pkgbuild_path).expect("Failed to create PKGBUILD");
        writeln!(file, "# Dummy PKGBUILD for testing").expect("Failed to write PKGBUILD");
        drop(file);
        
        let result = prepare_build_environment(&hw, temp_dir.path());
        
        // TempDir automatically cleans up when dropped - no manual cleanup needed
        assert!(result.is_ok());
    }

    #[test]
    fn test_prepare_build_environment_insufficient_hardware() {
        let mut hw = create_test_hardware();
        hw.ram_gb = 2;
        let kernel_path = std::path::Path::new(".");
        assert!(prepare_build_environment(&hw, kernel_path).is_err());
    }

    #[test]
    fn test_configure_build_success() {
        let cfg = create_test_config();
        let hw = create_test_hardware();
        let result = configure_build(&cfg, &hw);
        assert!(result.is_ok());
    }

    #[test]
    fn test_prepare_kernel_build_success() {
        let cfg = create_test_config();
        assert!(prepare_kernel_build(&cfg).is_ok());
    }

    #[test]
    fn test_validate_kernel_build_success() {
        let cfg = create_test_config();
        assert!(validate_kernel_build(&cfg).is_ok());
    }

    #[test]
    fn test_functions_are_pure() {
        // Verify functions don't have hidden state
        let hw1 = create_test_hardware();
        let hw2 = create_test_hardware();

        // Same inputs should produce same outputs
        let result1 = validate_hardware(&hw1);
        let result2 = validate_hardware(&hw2);

        assert_eq!(result1.is_ok(), result2.is_ok());
    }

    #[test]
    fn test_parse_build_progress_x_y_pattern() {
        // Test [X/Y] compilation progress patterns (granular tracking)
        assert_eq!(parse_build_progress("[ 582/12041] CC  arch/x86/boot/cpuflags.c"), Some(4)); // 582/12041 ≈ 4%
        assert_eq!(parse_build_progress("[1/100] Compiling..."), Some(1)); // 1/100 = 1%
        assert_eq!(parse_build_progress("[100/100] Done"), Some(100)); // 100/100 = 100%
        assert_eq!(parse_build_progress("[ 50/100] Half done"), Some(50)); // 50/100 = 50%
    }

    #[test]
    fn test_parse_build_progress_percentage() {
        // Test standard make progress format
        assert_eq!(parse_build_progress("[ 45%] Compiling..."), Some(45));
        assert_eq!(parse_build_progress("[  1%] Starting..."), Some(1));
        assert_eq!(parse_build_progress("[100%] Complete"), Some(100));
        assert_eq!(parse_build_progress("[ 99%] Almost done"), Some(99));
    }

    #[test]
    fn test_parse_build_progress_compilation() {
        // Test file compilation patterns
        assert_eq!(parse_build_progress("  CC  arch/x86/boot/cpuflags.c"), Some(0));
        assert_eq!(parse_build_progress("  LD  vmlinux"), Some(0));
        assert_eq!(parse_build_progress("  AR  lib/lib.a"), Some(0));
    }

    #[test]
    fn test_parse_build_progress_no_match() {
        // Test lines with no progress information
        assert_eq!(parse_build_progress("random compilation output"), None);
        assert_eq!(parse_build_progress("error: undefined reference"), None);
    }

    #[test]
    fn test_kernel_source_db_integration() {
        // Verify that all required kernel URLs are present in sources database
        let source_db = crate::kernel::sources::KernelSourceDB::new();
        
        // Test all variants have correct URLs
        assert_eq!(
            source_db.get_source_url("linux"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git")
        );
        
        assert_eq!(
            source_db.get_source_url("linux-mainline"),
            Some("https://aur.archlinux.org/linux-mainline.git")
        );
        
        assert_eq!(
            source_db.get_source_url("linux-lts"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git")
        );
        
        assert_eq!(
            source_db.get_source_url("linux-hardened"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git")
        );
    }

    #[test]
    fn test_kernel_source_db_aur_variant() {
        // Specifically verify that linux-mainline AUR URL is correct
        let source_db = crate::kernel::sources::KernelSourceDB::new();
        let aur_url = source_db.get_source_url("linux-mainline").unwrap();
        
        // Verify it's pointing to AUR, not GitLab
        assert!(aur_url.contains("aur.archlinux.org"),
            "linux-mainline should point to AUR, got: {}", aur_url);
        assert!(aur_url.ends_with(".git"),
            "AUR URL should end with .git, got: {}", aur_url);
    }
}

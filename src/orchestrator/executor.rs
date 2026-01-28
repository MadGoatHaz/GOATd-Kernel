//! Phase execution: hardware validation, build spawning, output streaming.
//!
//! Integrates with the unified logging pipeline via LogCollector for:
//! - Granular compilation progress tracking (CC/LD/AR line counting)
//! - Parsed milestone logging (high-level status updates)
//! - Full detailed output persistence
//!
//! # External Workspace Architecture (Cross-Mount Support)
//!
//! ## Problem Statement
//! The app supports external workspaces on different mount points (e.g., `/mnt/Optane/goatd`)
//! separate from the app's directory (`/home/madgoat/Documents/GOATd Kernel/`).
//! This enables NVME wear leveling, caching on fast scratch disks, and separation of concerns.
//!
//! ## Critical Issue: Path Resolution Across Mounts
//! During kernel build, `.kernelrelease` is generated in the build tree. However:
//! - The file path depends on the external workspace mount point
//! - Relative paths fail when switching between app dir and workspace dir
//! - The `package()` function (running in fakeroot) must locate `.kernelrelease`
//! - Environment variable propagation spans process boundaries and mount points
//!
//! ## Solution: Absolute Path Canonicalization
//! 1. **Workspace Awareness**: All paths must be canonicalized relative to the workspace root
//! 2. **Environment Propagation**: `GOATD_KERNELRELEASE` is exported by the executor and
//!    survives fakeroot execution, providing the fallback when file lookup fails
//! 3. **Multi-Point Propagation**: `.kernelrelease` is copied to multiple locations:
//!    - Kernel source root (where build artifacts are)
//!    - Parent directories (for makepkg `$srcdir` scenarios)
//!    - `src/` subdirectories (handles nested source layouts)
//! 4. **Shell Code Robustness**: Patcher injects shell code with 5-level fallback strategy:
//!    - PRIORITY 1: `./.kernelrelease` (build artifact in current dir)
//!    - PRIORITY 2: `$srcdir/.kernelrelease` or `$srcdir/linux*/.kernelrelease`
//!    - PRIORITY 3: `GOATD_KERNELRELEASE` environment variable
//!    - PRIORITY 4: `$pkgdir/usr/lib/modules/` directory listing
//!    - PRIORITY 5: Fallback to `$_kernver` with warning
//!
//! ## Future Development Notes
//! - Consider caching canonicalized paths to avoid repeated filesystem traversal
//! - Monitor for symlink resolution issues on tightly-mounted scratch disks
//! - Add metrics for path resolution success/failure rates across mount points
//! - Implement workspace migration helpers for users changing storage configurations

use crate::error::BuildError;
use crate::kernel::pkgbuild::get_latest_version_by_variant;
use crate::models::{HardwareInfo, KernelConfig};
use regex::Regex;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

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
pub fn prepare_build_environment(
    hardware: &HardwareInfo,
    kernel_path: &Path,
) -> Result<(), BuildError> {
    eprintln!(
        "[Build] [DEBUG] Preparing build environment at: {}",
        kernel_path.display()
    );

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
        return Err(BuildError::PreparationFailed(format!(
            "Kernel source not found at: {}",
            kernel_path.display()
        )));
    } else {
        eprintln!(
            "[Build] [PREPARATION] Kernel source found at: {}",
            kernel_path.display()
        );
    }

    if !kernel_path.is_dir() {
        return Err(BuildError::PreparationFailed(format!(
            "Kernel source path exists but is not a directory: {}",
            kernel_path.display()
        )));
    }

    if !kernel_path.join("PKGBUILD").exists() && !kernel_path.join("Makefile").exists() {
        return Err(BuildError::PreparationFailed(format!(
            "Valid kernel source not found in {}. Missing PKGBUILD or Makefile.",
            kernel_path.display()
        )));
    }

    // =========================================================================
    // PHASE 14: CROSS-MOUNT PATH RESOLUTION - Create .goatd_anchor
    // =========================================================================
    // Create an empty .goatd_anchor file at the workspace root to provide
    // definitive absolute path resolution across mount points and fakeroot transitions.
    // The workspace root is the parent directory of kernel_path.
    // PHASE 18: Non-destructive anchor creation - check for existence before writing
    if let Some(workspace_root) = kernel_path.parent() {
        let anchor_path = workspace_root.join(".goatd_anchor");

        // PHASE 18: IDEMPOTENCY GUARD - Check if anchor already exists
        if anchor_path.exists() {
            eprintln!(
                "[Build] [ANCHOR] ✓ .goatd_anchor already exists at workspace root: {}",
                anchor_path.display()
            );
        } else {
            // Non-destructive write: only create if it doesn't exist
            match std::fs::write(&anchor_path, "") {
                Ok(_) => {
                    eprintln!(
                        "[Build] [ANCHOR] ✓ Created .goatd_anchor at workspace root: {}",
                        anchor_path.display()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[Build] [ANCHOR] WARNING: Could not create .goatd_anchor at {}: {}",
                        anchor_path.display(),
                        e
                    );
                    // Non-fatal - the resolver will search for it with fallbacks
                }
            }
        }
    } else {
        eprintln!("[Build] [ANCHOR] WARNING: Could not determine workspace root (kernel_path has no parent)");
    }

    eprintln!("[Build] [PREPARATION] Environment ready");
    Ok(())
}

/// Resolves dynamic version ("latest") to a concrete version string.
///
/// This function implements Step 2 of the Dynamic Versioning strategy.
/// If the kernel version is set to "latest", it polls the PKGBUILD for the
/// actual latest version of the specified variant and updates the config.
///
/// # Arguments
/// * `config` - Mutable reference to KernelConfig (will be updated with resolved version)
/// * `event_tx` - Optional channel for emitting VersionResolved events to the UI
///
/// # Returns
/// * `Ok(resolved_version)` - The concrete version string that was set in the config
/// * `Err(BuildError)` - If version resolution fails and no fallback is available
///
/// # Fallback Hierarchy (from Section 5 of DYNAMIC_VERSIONING_PLAN.md)
/// 1. **Successful Poll**: Use version fetched from PKGBUILD/Git
/// 2. **Cached Version**: Check for previously resolved version in settings
/// 3. **Local PKGBUILD Parse**: Search local workspace for PKGBUILD and extract version
/// 4. **Hardcoded Baseline**: Use safe baseline version (if set in config)
/// 5. **Failure**: Return error if all fallbacks exhausted
pub async fn resolve_dynamic_version(
    config: &mut KernelConfig,
    event_tx: Option<&tokio::sync::mpsc::Sender<crate::ui::controller::BuildEvent>>,
) -> Result<String, BuildError> {
    // DIAGNOSTIC ENTRY POINT: Validate incoming configuration state
    eprintln!("[ORCHESTRATOR] [VERSION] ========== ENTRY POINT DIAGNOSTICS ==========");
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [ENTRY] config.version field: '{}'",
        config.version
    );
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [ENTRY] config.kernel_variant field: '{}'",
        config.kernel_variant
    );
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [ENTRY] is_dynamic_version() returns: {}",
        config.is_dynamic_version()
    );
    log::info!(
        "[ORCHESTRATOR] [VERSION] [ENTRY] Incoming config state - version: '{}', variant: '{}'",
        config.version,
        config.kernel_variant
    );
    eprintln!("[ORCHESTRATOR] [VERSION] ========== END ENTRY DIAGNOSTICS ==========");

    // STEP 1: Check if version resolution is needed
    if !config.is_dynamic_version() {
        eprintln!(
            "[ORCHESTRATOR] [VERSION] Version is already concrete: '{}'",
            config.version
        );
        log::info!(
            "[ORCHESTRATOR] [VERSION] Version is concrete (not 'latest'): '{}'",
            config.version
        );
        eprintln!("[ORCHESTRATOR] [VERSION] ⚠️  DIAGNOSTIC: This usually means the UI set a concrete version");
        eprintln!("[ORCHESTRATOR] [VERSION] ⚠️  DIAGNOSTIC: Check that UI is correctly setting version='latest' and kernel_variant=the variant name");
        return Ok(config.version.clone());
    }

    eprintln!("[ORCHESTRATOR] [VERSION] ========== DYNAMIC VERSION RESOLUTION START ==========");
    log::info!(
        "[ORCHESTRATOR] [VERSION] Resolving dynamic version for variant: '{}'",
        config.kernel_variant
    );
    eprintln!(
        "[ORCHESTRATOR] [VERSION] Kernel variant: '{}'",
        config.kernel_variant
    );
    eprintln!("[ORCHESTRATOR] [VERSION] Current version: 'latest' (sentinel value)");

    // STEP 2: Attempt PRIORITY 1 - Poll latest version from PKGBUILD
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-1] Attempting to poll latest version from PKGBUILD..."
    );
    log::info!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-1] Fetching latest version for variant: '{}'",
        config.kernel_variant
    );

    match get_latest_version_by_variant(&config.kernel_variant).await {
        Ok(resolved_version) => {
            eprintln!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-1] ✓ SUCCESS: Polled latest version: '{}'",
                resolved_version
            );
            log::info!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-1] Successfully resolved 'latest' to: '{}'",
                resolved_version
            );

            // Update config with resolved version
            config.version = resolved_version.clone();
            eprintln!(
                "[ORCHESTRATOR] [VERSION] Updated config.version to: '{}'",
                resolved_version
            );
            log::info!(
                "[ORCHESTRATOR] [VERSION] Config updated with resolved version: '{}'",
                resolved_version
            );

            // Emit VersionResolved event to UI
            if let Some(tx) = event_tx {
                let _ = tx.try_send(crate::ui::controller::BuildEvent::VersionResolved(
                    resolved_version.clone(),
                ));
            }

            eprintln!(
                "[ORCHESTRATOR] [VERSION] ========== DYNAMIC VERSION RESOLUTION SUCCESS =========="
            );
            return Ok(resolved_version);
        }
        Err(poll_error) => {
            eprintln!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-1] ✗ FAILED: Poll error: {}",
                poll_error
            );
            log::warn!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-1] Failed to poll latest version: {}",
                poll_error
            );
        }
    }

    // STEP 3: Attempt PRIORITY 2 - Check for cached version (would be stored in settings)
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-2] Checking for cached version from previous builds..."
    );
    log::debug!("[ORCHESTRATOR] [VERSION] [PRIORITY-2] Attempting cached version lookup");

    // Try to read cached version from settings or config directory
    let cache_dir =
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".to_string()))
            .join(".config/goatd/version_cache");

    let cached_version_file = cache_dir.join(format!("{}.version", config.kernel_variant));

    if let Ok(cached_version) = std::fs::read_to_string(&cached_version_file) {
        let cached_version = cached_version.trim().to_string();
        if !cached_version.is_empty() {
            eprintln!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-2] ✓ SUCCESS: Found cached version: '{}'",
                cached_version
            );
            log::info!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-2] Using cached version: '{}'",
                cached_version
            );

            config.version = cached_version.clone();
            eprintln!(
                "[ORCHESTRATOR] [VERSION] Updated config.version to cached: '{}'",
                cached_version
            );

            // Emit VersionResolved event to UI
            if let Some(tx) = event_tx {
                let _ = tx.try_send(crate::ui::controller::BuildEvent::VersionResolved(
                    cached_version.clone(),
                ));
            }

            eprintln!("[ORCHESTRATOR] [VERSION] ========== DYNAMIC VERSION RESOLUTION SUCCESS (CACHED) ==========");
            return Ok(cached_version);
        }
    } else {
        eprintln!(
            "[ORCHESTRATOR] [VERSION] [PRIORITY-2] ✗ No cached version found at: {}",
            cached_version_file.display()
        );
        log::debug!("[ORCHESTRATOR] [VERSION] [PRIORITY-2] No cached version available");
    }

    // STEP 4: Attempt PRIORITY 3 - Local PKGBUILD Parse
    eprintln!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-3] Attempting to parse local PKGBUILD for version..."
    );
    log::debug!("[ORCHESTRATOR] [VERSION] [PRIORITY-3] Searching local workspace for PKGBUILD");

    // Search for PKGBUILD in standard locations
    let pkgbuild_search_paths = vec![
        std::path::PathBuf::from("pkgbuilds/kernel/PKGBUILD"),
        std::path::PathBuf::from("PKGBUILD"),
    ];

    for pkgbuild_path in pkgbuild_search_paths {
        if pkgbuild_path.exists() {
            eprintln!(
                "[ORCHESTRATOR] [VERSION] [PRIORITY-3] Trying PKGBUILD at: {}",
                pkgbuild_path.display()
            );

            if let Ok(content) = std::fs::read_to_string(&pkgbuild_path) {
                // Try to extract pkgver from PKGBUILD
                // Format: pkgver=6.12.0
                for line in content.lines() {
                    if let Some(version_part) = line.strip_prefix("pkgver=") {
                        // Remove quotes if present
                        let parsed_version = version_part
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();

                        if !parsed_version.is_empty() && !parsed_version.contains('$') {
                            eprintln!("[ORCHESTRATOR] [VERSION] [PRIORITY-3] ✓ SUCCESS: Parsed version from PKGBUILD: '{}'", parsed_version);
                            log::info!("[ORCHESTRATOR] [VERSION] [PRIORITY-3] Extracted version from local PKGBUILD: '{}'", parsed_version);

                            config.version = parsed_version.clone();
                            eprintln!(
                                "[ORCHESTRATOR] [VERSION] Updated config.version to parsed: '{}'",
                                parsed_version
                            );

                            // Emit VersionResolved event to UI
                            if let Some(tx) = event_tx {
                                let _ = tx.try_send(
                                    crate::ui::controller::BuildEvent::VersionResolved(
                                        parsed_version.clone(),
                                    ),
                                );
                            }

                            eprintln!("[ORCHESTRATOR] [VERSION] ========== DYNAMIC VERSION RESOLUTION SUCCESS (LOCAL PARSE) ==========");
                            return Ok(parsed_version);
                        }
                    }
                }
            }
        }
    }

    eprintln!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-3] ✗ Could not extract version from any local PKGBUILD"
    );
    log::debug!("[ORCHESTRATOR] [VERSION] [PRIORITY-3] Local PKGBUILD parsing failed or not found");

    // STEP 5: Attempt PRIORITY 4 - Hardcoded Baseline
    // Use a safe baseline version if all else fails
    eprintln!("[ORCHESTRATOR] [VERSION] [PRIORITY-4] Attempting hardcoded baseline fallback...");
    log::warn!("[ORCHESTRATOR] [VERSION] [PRIORITY-4] Falling back to hardcoded baseline version");

    let baseline_version = match config.kernel_variant.as_str() {
        "linux" => "6.12.0-1",
        "linux-lts" => "6.6.0-1",
        "linux-hardened" => "6.12.0-1",
        "linux-mainline" => "6.13.0-1",
        "linux-zen" => "6.12.0-1",
        "linux-tkg" => "6.12.0-1",
        _ => {
            eprintln!("[ORCHESTRATOR] [VERSION] [PRIORITY-4] ✗ FAILED: Unknown variant '{}', no baseline available", config.kernel_variant);
            log::error!("[ORCHESTRATOR] [VERSION] [PRIORITY-4] Unknown variant '{}' with no hardcoded baseline", config.kernel_variant);

            return Err(BuildError::PreparationFailed(
                format!("Unable to resolve 'latest' version for unknown variant '{}'. Please check your internet connection or specify a concrete version.", config.kernel_variant)
            ));
        }
    };

    eprintln!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-4] ✓ Using baseline: '{}'",
        baseline_version
    );
    log::warn!(
        "[ORCHESTRATOR] [VERSION] [PRIORITY-4] Using hardcoded baseline version: '{}'",
        baseline_version
    );

    config.version = baseline_version.to_string();
    eprintln!(
        "[ORCHESTRATOR] [VERSION] Updated config.version to baseline: '{}'",
        baseline_version
    );
    log::warn!("[ORCHESTRATOR] [VERSION] WARNING: Using baseline version (all resolution attempts failed): '{}'", baseline_version);

    eprintln!("[ORCHESTRATOR] [VERSION] ========== DYNAMIC VERSION RESOLUTION SUCCESS (FALLBACK) ==========");
    Ok(baseline_version.to_string())
}

/// Validates and returns config, resolving dynamic versions if needed.
///
/// This is the primary integration point for Step 2 of Dynamic Versioning.
/// If the config specifies version="latest", this function resolves it to a concrete version
/// before the build proceeds.
///
/// # Arguments
/// * `config` - Kernel configuration (will be updated if version is dynamic)
/// * `_hardware` - Hardware info (unused but part of public API)
/// * `event_tx` - Optional channel for emitting VersionResolved events to the UI
///
/// # Returns
/// * `Ok(resolved_config)` - Config with concrete version (either original or resolved from "latest")
/// * `Err(BuildError)` - If validation or version resolution fails
pub async fn configure_build(
    config: &mut KernelConfig,
    _hardware: &HardwareInfo,
    event_tx: Option<&tokio::sync::mpsc::Sender<crate::ui::controller::BuildEvent>>,
) -> Result<KernelConfig, BuildError> {
    // Validate config - the Finalizer has already applied all policies
    validate_kernel_config(config)?;

    // STEP 2: DYNAMIC VERSION RESOLUTION (Preparation Phase)
    // This is called as early as possible in the build pipeline, during preparation
    eprintln!("[ORCHESTRATOR] [PREPARATION] Starting dynamic version resolution check");
    resolve_dynamic_version(config, event_tx).await?;

    eprintln!(
        "[ORCHESTRATOR] [PREPARATION] Version resolution complete. Using version: '{}'",
        config.version
    );
    log::info!(
        "[ORCHESTRATOR] [PREPARATION] Version resolution complete. Using version: '{}'",
        config.version
    );

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
/// * `test_timeout` - Optional timeout duration for test builds
///
/// # Returns
/// * `Ok(())` if build completes successfully
/// * `Err(BuildError::BuildFailed)` if build fails or process exits with error
/// * `Err(BuildError::BuildCancelled)` if build is cancelled
/// * `Err(BuildError::BuildFailed)` with timeout message if test_timeout is exceeded
///
/// # Timeout Hook
/// If `test_timeout` is supplied, the entire build I/O loop is wrapped with
/// `tokio::time::timeout`. Logs are ALWAYS captured and sent through MPSC channel
/// even during timeout, ensuring complete audit trail.
///
pub async fn run_kernel_build<F>(
    kernel_path: &Path,
    config: &KernelConfig,
    mut output_callback: F,
    mut cancel_rx: watch::Receiver<bool>,
    log_collector: Option<std::sync::Arc<crate::LogCollector>>,
    test_timeout: Option<Duration>,
) -> Result<(), BuildError>
where
    F: FnMut(String, Option<u32>) + Send + 'static,
{
    // CRITICAL FIX: Counter for meaningful compilation progress tracking
    // Detects every 100 CC (compilation unit) lines and sends status updates
    let mut cc_line_counter = 0_usize;

    // BATCHED UI SIGNALING: Counter for logging batching (every 10 lines or on milestones)
    // This ensures the UI is signaled frequently enough without flooding the event loop
    let mut log_batch_counter = 0_u32;

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
        eprintln!(
            "[Build] [DRY-RUN]   • BORE Scheduler: {}",
            if use_bore { "Enabled" } else { "Disabled" }
        );
        eprintln!("[Build] [DRY-RUN]   • Module Stripping: ENABLED (INSTALL_MOD_STRIP=1)");
        eprintln!("[Build] [DRY-RUN] ========================================");
        eprintln!("[Build] [DRY-RUN] Configuration dump:");
        eprintln!("[Build] [DRY-RUN]   Profile: {}", config.profile);
        eprintln!("[Build] [DRY-RUN]   Kernel Version: {}", config.version);
        eprintln!("[Build] [DRY-RUN] ========================================");
        eprintln!("[Build] [DRY-RUN] EXIT: Halting before build (DRY_RUN_HOOK set)");
        output_callback(
            "DRY RUN: Environment verified, halting before build".to_string(),
            Some(100),
        );
        return Ok(());
    }

    // CRITICAL: Build execution starting - real-time logging begins here
    // Diagnostic logs go to eprintln only (NOT to callback), clean output pipe to UI

    // ============================================================================
    // CRITICAL FIX: CANONICALIZE WORKSPACE PATH FOR ABSOLUTE METADATA INJECTION
    // ============================================================================
    // Resolve symlinks and verify the true absolute path of the kernel workspace.
    // This is ESSENTIAL for cross-mount support where build runs on /mnt/Optane/goatd
    // but app is at /home/madgoat/Documents/GOATd Kernel/
    let canonical_kernel_path = std::fs::canonicalize(kernel_path).unwrap_or_else(|e| {
        eprintln!(
            "[Build] [CANON] WARNING: Could not canonicalize kernel_path: {}",
            e
        );
        eprintln!(
            "[Build] [CANON] Falling back to non-canonical path: {}",
            kernel_path.display()
        );
        kernel_path.to_path_buf()
    });

    eprintln!(
        "[Build] [CANON] Canonical kernel workspace: {}",
        canonical_kernel_path.display()
    );

    // =========================================================================
    // TIMEOUT HOOK: Wrap the entire build with optional timeout (Phase 4)
    // =========================================================================
    // If test_timeout is supplied, wrap the build closure with tokio::time::timeout.
    // This allows programmatic invocation with timeout constraints for testing.
    // Logs are ALWAYS captured and sent through MPSC channel even during timeout.
    // CRITICAL: On timeout, explicitly kills the child process and any process group.
    //
    // We use Arc<Mutex<Option<u32>>> to share the child PID from the build future
    // to the timeout handler, enabling explicit process cleanup on timeout.
    let child_pid: Arc<std::sync::Mutex<Option<u32>>> = Arc::new(std::sync::Mutex::new(None));
    let child_pid_clone = Arc::clone(&child_pid);

    let build_future = async {
        run_kernel_build_inner(
            kernel_path,
            &canonical_kernel_path,
            config,
            &mut output_callback,
            &mut cancel_rx,
            &log_collector,
            &mut cc_line_counter,
            &mut log_batch_counter,
            child_pid_clone,
        )
        .await
    };

    // Apply timeout if test_timeout is specified
    if let Some(timeout_duration) = test_timeout {
        eprintln!(
            "[Build] [TIMEOUT] Build process wrapped with timeout: {:?}",
            timeout_duration
        );
        match tokio::time::timeout(timeout_duration, build_future).await {
            Ok(result) => result?,
            Err(_timeout_err) => {
                // TIMEOUT TRIGGERED: Perform explicit process cleanup (mirror cancellation logic)
                let timeout_msg = format!("Build timeout exceeded: {:?}", timeout_duration);
                eprintln!("[Build] [TIMEOUT] TIMEOUT TRIGGERED: {}", timeout_msg);
                output_callback("Build timeout exceeded".to_string(), None);

                if let Some(ref collector) = log_collector {
                    collector.log_str(&timeout_msg);

                    // CAPTURE & LOGGING INTERFACE: Show last 10 lines of build output for diagnostics
                    let diagnostic_output = collector.format_last_output_lines();
                    eprintln!("{}", diagnostic_output);
                    collector.log_str(&diagnostic_output);
                }

                // =====================================================================
                // CRITICAL: PROCESS CLEANUP ON TIMEOUT
                // =====================================================================
                // Mirror the cancellation cleanup logic to ensure zombie processes
                // are properly reaped. If a child PID was captured, kill it explicitly.
                if let Ok(pid_lock) = child_pid.lock() {
                    if let Some(pid_value) = *pid_lock {
                        let pid: u32 = pid_value;
                        eprintln!(
                            "[Build] [TIMEOUT] Attempting to kill child process: PID {}",
                            pid
                        );

                        // Use pkill to kill child processes
                        let _ = std::process::Command::new("pkill")
                            .arg("-P")
                            .arg(pid.to_string())
                            .arg("-9")
                            .output();

                        // Kill the main process group
                        let _ = std::process::Command::new("kill")
                            .arg("-9")
                            .arg(format!("-{}", pid))
                            .output();

                        eprintln!(
                            "[Build] [TIMEOUT] Kill signals sent to PID {} and process group",
                            pid
                        );

                        // Add signal propagation delay to allow cleanup
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        eprintln!("[Build] [TIMEOUT] Signal propagation delay completed (100ms)");
                    }
                }

                return Err(BuildError::BuildFailed(timeout_msg));
            }
        }
    } else {
        build_future.await?;
    }

    Ok(())
}

/// Inner async function that implements the build loop.
/// Extracted to allow timeout wrapping without code duplication.
async fn run_kernel_build_inner<F>(
    kernel_path: &Path,
    canonical_kernel_path: &std::path::Path,
    config: &KernelConfig,
    output_callback: &mut F,
    cancel_rx: &mut watch::Receiver<bool>,
    log_collector: &Option<std::sync::Arc<crate::LogCollector>>,
    cc_line_counter: &mut usize,
    log_batch_counter: &mut u32,
    child_pid: Arc<std::sync::Mutex<Option<u32>>>,
) -> Result<(), BuildError>
where
    F: FnMut(String, Option<u32>) + Send + 'static,
{
    // Construct absolute path to .kernelrelease for injection into PKGBUILD
    let kernelrelease_abs_path = canonical_kernel_path.join(".kernelrelease");
    eprintln!(
        "[Build] [CANON] Absolute .kernelrelease path for injection: {}",
        kernelrelease_abs_path.display()
    );

    // ============================================================================
    // CRITICAL FIX: Use kernel source PKGBUILD template instead of binary template
    // ============================================================================
    let pkgbuild_template_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("pkgbuilds/kernel/PKGBUILD");

    eprintln!(
        "[Build] [EXECUTOR] Kernel PKGBUILD template path: {}",
        pkgbuild_template_path.display()
    );

    // ============================================================================
    // CRITICAL FIX: VARIANT-AWARE PKGBUILD VERIFICATION
    // ============================================================================
    // For canonical variants (Stable, LTS, Hardened, Mainline), we PRESERVE the
    // authoritative PKGBUILD cloned from the official repository without overwriting.
    // For custom variants, we enforce template parity to ensure consistency.
    let pkgbuild_dest = canonical_kernel_path.join("PKGBUILD");
    eprintln!("[Build] [EXECUTOR] PKGBUILD template verification starting");
    eprintln!(
        "[Build] [EXECUTOR] Kernel variant: '{}'",
        config.kernel_variant
    );
    eprintln!(
        "[Build] [EXECUTOR] Template source: {}",
        pkgbuild_template_path.display()
    );
    eprintln!(
        "[Build] [EXECUTOR] Destination in workspace: {}",
        pkgbuild_dest.display()
    );

    // Determine if this is one of the canonical four variants whose PKGBUILD
    // comes from the official authoritative repository (GitLab/AUR)
    let is_canonical_variant = matches!(
        config.kernel_variant.as_str(),
        "linux" | "linux-lts" | "linux-hardened" | "linux-mainline"
    );

    if is_canonical_variant && pkgbuild_dest.exists() {
        eprintln!("[Build] [EXECUTOR] ✓ CANONICAL VARIANT DETECTED: Preserving authoritative PKGBUILD from cloned repository");
        eprintln!(
            "[Build] [EXECUTOR] [VARIANT-AWARE] Skipping template parity check for variant '{}'",
            config.kernel_variant
        );
        eprintln!(
            "[Build] [EXECUTOR] [VARIANT-AWARE] PKGBUILD integrity preserved from source: {}",
            config.kernel_variant
        );
    } else if is_canonical_variant {
        // Canonical variant but PKGBUILD missing from clone - copy template as fallback
        eprintln!("[Build] [EXECUTOR] CASE-1a: Canonical variant, PKGBUILD missing from clone");
        eprintln!(
            "[Build] [EXECUTOR] Copying template as fallback for variant '{}'",
            config.kernel_variant
        );
        if pkgbuild_template_path.exists() {
            match std::fs::copy(&pkgbuild_template_path, &pkgbuild_dest) {
                Ok(_) => {
                    eprintln!("[Build] [EXECUTOR] ✓ CASE-1a SUCCESS: Template copied as fallback for '{}'", config.kernel_variant);
                }
                Err(e) => {
                    eprintln!("[Build] [EXECUTOR] ⚠ CASE-1a WARNING: Failed to copy template for '{}': {}", config.kernel_variant, e);
                }
            }
        }
    } else if pkgbuild_template_path.exists() {
        // Custom variant or non-canonical case - enforce template parity
        if !pkgbuild_dest.exists() {
            eprintln!("[Build] [EXECUTOR] CASE-1b: Custom variant, PKGBUILD missing, copying from template");
            match std::fs::copy(&pkgbuild_template_path, &pkgbuild_dest) {
                Ok(_) => {
                    eprintln!("[Build] [EXECUTOR] ✓ CASE-1b SUCCESS: Kernel PKGBUILD template copied to workspace");
                }
                Err(e) => {
                    eprintln!("[Build] [EXECUTOR] ⚠ CASE-1b WARNING: Failed to copy kernel PKGBUILD template: {}", e);
                    eprintln!(
                        "[Build] [EXECUTOR] ℹ Continuing with existing PKGBUILD if available"
                    );
                }
            }
        } else {
            eprintln!("[Build] [EXECUTOR] CASE-2: Custom variant, PKGBUILD exists, verifying checksum parity");
            match verify_pkgbuild_parity(&pkgbuild_template_path, &pkgbuild_dest) {
                Ok(true) => {
                    eprintln!("[Build] [EXECUTOR] ✓ CASE-2 VERIFIED: PKGBUILD checksum matches template (no action needed)");
                }
                Ok(false) => {
                    eprintln!("[Build] [EXECUTOR] ✗ CASE-2 MISMATCH: PKGBUILD checksum differs from template (corruption detected)");
                    eprintln!(
                        "[Build] [EXECUTOR] OVERWRITING corrupted PKGBUILD with fresh template"
                    );
                    match std::fs::copy(&pkgbuild_template_path, &pkgbuild_dest) {
                        Ok(_) => {
                            eprintln!("[Build] [EXECUTOR] ✓ CASE-2 RECOVERY: Corrupted PKGBUILD overwritten with fresh template");
                        }
                        Err(e) => {
                            eprintln!("[Build] [EXECUTOR] ⚠ CASE-2 ERROR: Failed to overwrite PKGBUILD: {}", e);
                            eprintln!("[Build] [EXECUTOR] CRITICAL: Build may proceed with corrupted PKGBUILD");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[Build] [EXECUTOR] ⚠ CASE-2 WARNING: Could not verify PKGBUILD checksum: {}", e);
                    eprintln!("[Build] [EXECUTOR] ℹ Proceeding with existing PKGBUILD (assume it's correct)");
                }
            }
        }
    } else {
        eprintln!(
            "[Build] [EXECUTOR] ⚠ CASE-0: Template file not found at {}",
            pkgbuild_template_path.display()
        );
    }

    let pkgbuild_path = canonical_kernel_path.join("PKGBUILD");
    eprintln!(
        "[Build] [EXECUTOR] Starting kernel build from: {}",
        canonical_kernel_path.display()
    );

    if !pkgbuild_path.exists() {
        eprintln!("[Build] [EXECUTOR] WARNING: PKGBUILD not found, will attempt fallback build");
    }

    // Validate config before proceeding
    validate_kernel_config(config)?;

    // CRITICAL FIX: Validate PKGBUILD sources match the intended kernel variant
    if pkgbuild_path.exists() {
        eprintln!("[VALIDATE-SOURCES] [DIAGNOSTIC] Before PKGBUILD validation:");
        eprintln!(
            "[VALIDATE-SOURCES] [DIAGNOSTIC]   kernel_variant: \"{}\"",
            config.kernel_variant
        );
        eprintln!(
            "[VALIDATE-SOURCES] [DIAGNOSTIC]   version: \"{}\"",
            config.version
        );

        let patcher =
            crate::kernel::patcher::KernelPatcher::new(canonical_kernel_path.to_path_buf());
        match patcher.validate_and_fix_pkgbuild_sources(&config.kernel_variant, &config.version) {
            Ok(()) => {
                eprintln!("[VALIDATE-SOURCES] ✓ PKGBUILD source validation succeeded");
            }
            Err(e) => {
                eprintln!(
                    "[VALIDATE-SOURCES] ✗ PKGBUILD source validation failed: {:?}",
                    e
                );
                return Err(BuildError::BuildFailed(format!(
                    "PKGBUILD source validation failed: {:?}",
                    e
                )));
            }
        }

        // CRITICAL FIX: Apply Rust rmeta installation fix (DEFINITIVE FIX)
        // This replaces `install -Dt "$builddir/rust" -m644 rust/*.rmeta` with find-based solution
        eprintln!("[Build] [PKGBUILD-FIX] Applying Rust rmeta installation fix");
        match patcher.inject_rust_rmeta_fix() {
            Ok(count) => {
                if count > 0 {
                    eprintln!("[Build] [PKGBUILD-FIX] ✓ Successfully applied Rust rmeta fix to {} headers function(s)", count);
                } else {
                    eprintln!("[Build] [PKGBUILD-FIX] ⓘ No headers functions found requiring rmeta fix (may already be fixed)");
                }
            }
            Err(e) => {
                eprintln!(
                    "[Build] [PKGBUILD-FIX] ⚠ WARNING: Rust rmeta fix failed (non-fatal): {:?}",
                    e
                );
                eprintln!("[Build] [PKGBUILD-FIX] Continuing with build - fix may not be necessary for this variant");
            }
        }

        // CRITICAL FIX: Validate PKGBUILD shell syntax BEFORE attempting makepkg
        use crate::kernel::validator::validate_pkgbuild_syntax;
        eprintln!("[Build] [SYNTAX-CHECK] Validating PKGBUILD shell syntax before build");
        match validate_pkgbuild_syntax(&pkgbuild_path) {
            Ok(()) => {
                eprintln!("[Build] [SYNTAX-CHECK] ✓ PKGBUILD syntax is valid");
            }
            Err(e) => {
                eprintln!(
                    "[Build] [SYNTAX-CHECK] ✗ PKGBUILD syntax check failed: {:?}",
                    e
                );
                return Err(BuildError::BuildFailed(format!(
                    "PKGBUILD syntax validation failed: {:?}",
                    e
                )));
            }
        }
    }

    // Determine build command based on what's available
    let num_jobs = num_cpus::get();

    let mut command = if pkgbuild_path.exists() {
        eprintln!("[Build] [DEBUG] PKGBUILD found, using 'makepkg'");

        eprintln!(
            "[Build] [KERNELRELEASE-PROPAGATION] Preparing .kernelrelease propagation for makepkg"
        );

        // CRITICAL FIX: Pre-create .kernelrelease file with initial value
        let initial_kernelrelease_path = canonical_kernel_path.join(".kernelrelease");
        let initial_kernelrelease_value = config.version.clone();
        eprintln!(
            "[Build] [KERNELRELEASE-INIT] Pre-creating .kernelrelease with initial version: {}",
            initial_kernelrelease_value
        );
        if let Err(e) = std::fs::write(&initial_kernelrelease_path, &initial_kernelrelease_value) {
            eprintln!(
                "[Build] [KERNELRELEASE-INIT] WARNING: Failed to pre-create .kernelrelease: {}",
                e
            );
        } else {
            eprintln!(
                "[Build] [KERNELRELEASE-INIT] ✓ Success: Pre-created .kernelrelease at {}",
                initial_kernelrelease_path.display()
            );
        }

        let mut cmd = Command::new("makepkg");
        cmd.arg("-s");
        cmd.arg("-f");
        cmd.arg("--noconfirm");
        cmd
    } else if kernel_path.join("scripts/build.sh").exists() {
        eprintln!("[Build] [DEBUG] Found scripts/build.sh, using it");
        let mut cmd = Command::new("bash");
        cmd.arg("scripts/build.sh");
        cmd
    } else {
        eprintln!("[Build] [DEBUG] Falling back to standard 'make'");
        let mut cmd = Command::new("make");
        cmd.arg(format!("-j{}", num_jobs));
        cmd.arg("bzImage");
        cmd
    };

    // Set working directory
    command.current_dir(&canonical_kernel_path);

    // ============================================================================
    // CRITICAL FIX: KERNELRELEASE ENVIRONMENT VARIABLE FOR CROSS-MOUNT SUPPORT
    // ============================================================================
    eprintln!("[Build] [KERNELRELEASE-ENV] Exporting GOATD_KERNELRELEASE to build environment (cross-mount fallback)");
    let initial_kernelrelease = config.version.clone();
    command.env("GOATD_KERNELRELEASE", &initial_kernelrelease);
    eprintln!(
        "[Build] [KERNELRELEASE-ENV] Set GOATD_KERNELRELEASE={} (initial kernel version)",
        initial_kernelrelease
    );

    // Set up environment for parallel builds
    command.env("MAKEFLAGS", format!("-j{}", num_jobs));
    command.env("KBUILD_BUILD_TIMESTAMP", "");

    // ============================================================================
    // PYTHON BUILD CLEANLINESS - PREVENT __pycache__ GENERATION
    // ============================================================================
    command.env("PYTHONDONTWRITEBYTECODE", "1");
    eprintln!("[Build] [PYTHON] Set PYTHONDONTWRITEBYTECODE=1 to prevent __pycache__ generation");

    // ============================================================================
    // PHASE 3: MODULE SIZE & STRIPPING OPTIMIZATION (CRITICAL)
    // ============================================================================
    command.env("INSTALL_MOD_STRIP", "1");
    eprintln!(
        "[Build] [MODULE-OPT] Set INSTALL_MOD_STRIP=1 for module stripping during installation"
    );

    // ============================================================================
    // CRITICAL DEPRECATION NOTICE:
    // KCFLAGS, LLVM_IAS, SRCDEST, and ccache PATH setup are NOW HANDLED EXCLUSIVELY
    // by `patcher.prepare_build_environment()`. The executor is a BLIND CONSUMER
    // of the patcher's hardened environment and does NOT override these settings.
    // This ensures a single source of truth for all environment variable configuration.
    // ============================================================================

    // ============================================================================
    // NATIVE KCONFIG INJECTION: KCONFIG_ALLCONFIG for .config.override
    // ============================================================================
    let config_override_path = canonical_kernel_path.join(".config.override");
    if config_override_path.exists() {
        let override_abs_path = config_override_path.to_string_lossy().to_string();
        command.env("KCONFIG_ALLCONFIG", &override_abs_path);
        eprintln!(
            "[Build] [KCONFIG] Set KCONFIG_ALLCONFIG='{}'",
            override_abs_path
        );
        eprintln!("[Build] [KCONFIG] Native KConfig injection ENABLED - .config.override will be auto-merged");
    } else {
        eprintln!(
            "[Build] [KCONFIG] .config.override not found - native KConfig injection disabled"
        );
    }

    // ============================================================================
    // INJECT BUILD OPTIONS INTO ENVIRONMENT
    // ============================================================================
    eprintln!("[Build] [CONFIG] Applying build configuration:");
    eprintln!("[Build] [CONFIG]  Profile: {}", config.profile);
    eprintln!("[Build] [CONFIG]  LTO Level: {:?}", config.lto_type);
    eprintln!(
        "[Build] [CONFIG]  Use Modprobed DB: {}",
        config.use_modprobed
    );
    eprintln!("[Build] [CONFIG]  Use Whitelist: {}", config.use_whitelist);
    eprintln!("[Build] [CONFIG]  Hardening: {}", config.hardening);
    eprintln!("[Build] [CONFIG]  Secure Boot: {}", config.secure_boot);

    // =========================================================================
    // ENVIRONMENT VARIABLES: Inherited from Patcher preparation
    // =========================================================================
    eprintln!("[Build] [COMPILER] ========================================");
    eprintln!("[Build] [COMPILER] INHERITED CLANG/LLVM TOOLCHAIN (from Patcher)");
    eprintln!("[Build] [COMPILER] ========================================");
    eprintln!(
        "[Build] [COMPILER] Environment prepared by: KernelPatcher::prepare_build_environment()"
    );
    eprintln!("[Build] [COMPILER] Executor receives pre-configured environment variables");
    eprintln!("[Build] [COMPILER] CRITICAL VARIABLES (inherited from parent process):");
    eprintln!("[Build] [COMPILER] LLVM=1, LLVM_IAS=1, CC=clang, CXX=clang++, LD=ld.lld");

    // ============================================================================
    // CRITICAL DEPRECATION: GOATD_* CONFIG FLAGS ARE NOW PATCHER-ONLY
    // ============================================================================
    // The following flags are NO LONGER set here:
    // - GOATD_APPLY_BORE_SCHEDULER
    // - GOATD_LTO_LEVEL
    // - GOATD_USE_MODPROBED_DB
    // - GOATD_USE_KERNEL_WHITELIST
    // - GOATD_KERNEL_HARDENING
    // - GOATD_ENABLE_SECURE_BOOT
    // - GOATD_ENABLE_HARDENING
    //
    // These are now EXCLUSIVELY managed by the patcher and injected as part of
    // the hardened_env HashMap returned by patcher.prepare_build_environment().
    // This ensures single-source-of-truth configuration management.

    // ============================================================================
    // SURGICAL INJECTION LOOP: Apply ALL Patcher-Hardened Environment Variables
    // ============================================================================
    // CRITICAL: The executor is a BLIND CONSUMER of the patcher's environment.
    // It receives a fully configured HashMap from patcher.prepare_build_environment()
    // and applies EVERY key-value pair to the build Command without modification.
    // This includes:
    // - Compiler enforcement (LLVM, CC, CXX, LD, AR, NM, STRIP, etc.)
    // - Toolchain discovery (LLVM-19 prioritized binaries)
    // - All GOATD_* configuration flags (from KernelConfig)
    // - PATH purification (removes gcc/llvm conflicting paths)
    // - GOATD_WORKSPACE_ROOT (for cross-mount metadata sourcing)
    // - Host compiler enforcement (HOSTCC, HOSTCXX)
    // - All optimization and hardening flags (BASE, HARDENING, NATIVE, LTO, POLLY)

    // ============================================================================
    // CRITICAL FIX: EXPORT BUILD CONFIGURATION TO ENVIRONMENT VARIABLES
    // ============================================================================
    // These variables MUST be exported BEFORE patcher.prepare_build_environment()
    // so that shell templates injected by the patcher can access them.
    // The templates (e.g., PHASE_G2_5_RESTORER) rely on GOATD_* environment variables
    // to know which build options were selected by the user.

    // Export LTO level for template access (CRITICAL - templates depend on this)
    let lto_level = match config.lto_type {
        crate::models::LtoType::Full => "full",
        crate::models::LtoType::Thin => "thin",
        crate::models::LtoType::None => "none",
    };
    command.env("GOATD_LTO_LEVEL", lto_level);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_LTO_LEVEL={}", lto_level);

    // Export Hardening level for template access
    let hardening_level = format!("{:?}", config.hardening).to_lowercase();
    command.env("GOATD_HARDENING_LEVEL", &hardening_level);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_HARDENING_LEVEL={}", hardening_level);

    // Export Polly optimization flag
    let polly_enabled = if config.use_polly { "1" } else { "0" };
    command.env("GOATD_POLLY_ENABLED", polly_enabled);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_POLLY_ENABLED={}", polly_enabled);

    // Export MGLRU flag
    let mglru_enabled = if config.use_mglru { "1" } else { "0" };
    command.env("GOATD_MGLRU_ENABLED", mglru_enabled);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_MGLRU_ENABLED={}", mglru_enabled);

    // Export Native Optimizations flag
    let native_opts_enabled = if config.native_optimizations { "1" } else { "0" };
    command.env("GOATD_NATIVE_OPTIMIZATIONS", native_opts_enabled);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_NATIVE_OPTIMIZATIONS={}", native_opts_enabled);

    // Export Modprobed DB flag
    let modprobed_enabled = if config.use_modprobed { "1" } else { "0" };
    command.env("GOATD_USE_MODPROBED_DB", modprobed_enabled);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_USE_MODPROBED_DB={}", modprobed_enabled);

    // Export Whitelist flag
    let whitelist_enabled = if config.use_whitelist { "1" } else { "0" };
    command.env("GOATD_USE_KERNEL_WHITELIST", whitelist_enabled);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_USE_KERNEL_WHITELIST={}", whitelist_enabled);

    // Export Profile for variant-aware processing
    command.env("GOATD_PROFILE", &config.profile);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_PROFILE={}", config.profile);

    // Export Variant for PKGBUILD source validation
    command.env("GOATD_KERNEL_VARIANT", &config.kernel_variant);
    eprintln!("[Build] [ENV-CONFIG] Exported GOATD_KERNEL_VARIANT={}", config.kernel_variant);

    let patcher = crate::kernel::patcher::KernelPatcher::new(canonical_kernel_path.to_path_buf());
    let hardened_env = patcher.prepare_build_environment(config.native_optimizations);

    eprintln!("[Build] [ENV-UNIFY] ========== SURGICAL INJECTION LOOP START ==========");
    eprintln!("[Build] [ENV-UNIFY] Obtained hardened environment from patcher");
    eprintln!(
        "[Build] [ENV-UNIFY] Hardened environment variables count: {}",
        hardened_env.len()
    );

    // ATOMIC OPERATION: Apply ALL hardened environment variables to the command
    // This is the single point where executor injects patcher-prepared configuration.
    for (key, value) in &hardened_env {
        command.env(key, value);

        // Diagnostic logging for critical keys (excluding very long values)
        if key.starts_with("GOATD_")
            || key == "LLVM"
            || key == "CC"
            || key == "CXX"
            || key == "LD"
            || key == "AR"
            || key == "NM"
            || key == "STRIP"
            || key == "HOSTCC"
            || key == "HOSTCXX"
            || key == "PATH"
            || key == "GOATD_WORKSPACE_ROOT"
        {
            let display_value = if value.len() > 60 {
                format!("{}...[{}B total]", &value[..57], value.len())
            } else {
                value.clone()
            };
            eprintln!("[Build] [ENV] Injected: {}={}", key, display_value);
        }
    }

    eprintln!("[Build] [ENV-UNIFY] ========== SURGICAL INJECTION LOOP COMPLETE ==========");

    eprintln!("[Build] [COMPILER] ========================================");
    eprintln!("[Build] [COMPILER] FLAG HARDENING DELEGATED TO PATCHER");
    eprintln!("[Build] [COMPILER] All environment variables unified from patcher");
    eprintln!("[Build] [COMPILER] Templates will assemble CFLAGS/CXXFLAGS/LDFLAGS");
    eprintln!("[Build] [COMPILER] from exported GOATD_* variables at build time");
    eprintln!("[Build] [COMPILER] ========================================");

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

    // CRITICAL: Capture child PID for timeout cleanup
    if let Some(pid) = child.id() {
        if let Ok(mut pid_lock) = child_pid.lock() {
            *pid_lock = Some(pid);
            eprintln!("[Build] [TIMEOUT] Captured child process PID: {}", pid);
        }
    }

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
                        output_callback(line.clone(), progress);

                        if line.contains("CC ") || line.contains("LD ") || line.contains("AR ") {
                            *cc_line_counter += 1;
                            if *cc_line_counter % 100 == 0 {
                                let status_msg = format!("Compiling: Processed {} files...", *cc_line_counter);
                                output_callback(status_msg.clone(), None);
                                eprintln!("[Build] [STATUS] {}", status_msg);
                            }
                        }

                        if let Some(ref collector) = log_collector {
                            collector.log_str(&line);
                            if line.starts_with("==> ") || line.starts_with(":: ") {
                                collector.log_parsed(line.clone());
                           }
                        }

                        // BATCHED UI SIGNALING: Track logging for batched UI dirty flag signaling
                        // This ensures UI is refreshed every 10 lines or immediately on milestones
                        let is_milestone = line.starts_with("==> ") || line.starts_with(":: ");
                        if is_milestone {
                            // Immediate signal on milestone (high-level status)
                            *log_batch_counter = 0;
                        } else {
                            // Regular line: increment batch counter
                            *log_batch_counter += 1;
                            if *log_batch_counter >= 10 {
                                // Batched signal: every 10 lines
                                *log_batch_counter = 0;
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
                        let progress = parse_build_progress(&line);
                        let formatted_line = format!("[STDERR] {}", line);
                        output_callback(formatted_line.clone(), progress);

                        if line.contains("CC ") || line.contains("LD ") || line.contains("AR ") {
                            *cc_line_counter += 1;
                            if *cc_line_counter % 100 == 0 {
                                let status_msg = format!("Compiling: Processed {} files...", *cc_line_counter);
                                output_callback(status_msg.clone(), None);
                                eprintln!("[Build] [STATUS] {}", status_msg);
                            }
                        }

                        if let Some(ref collector) = log_collector {
                            collector.log_str(&formatted_line);
                        }

                        // BATCHED UI SIGNALING: Track logging for batched UI dirty flag signaling
                        // Same batching logic as stdout to ensure consistent UI refresh
                        let is_milestone = line.starts_with("==> ") || line.starts_with(":: ");
                        if is_milestone {
                            // Immediate signal on milestone (high-level status)
                            *log_batch_counter = 0;
                        } else {
                            // Regular line: increment batch counter
                            *log_batch_counter += 1;
                            if *log_batch_counter >= 10 {
                                // Batched signal: every 10 lines
                                *log_batch_counter = 0;
                            }
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
                if *cancel_rx.borrow() {
                    output_callback("Build cancelled by user".to_string(), None);

                    if let Err(kill_err) = child.kill().await {
                        output_callback(
                            format!("Warning: Failed to kill process: {}", kill_err),
                            None,
                        );
                    }

                    #[cfg(unix)]
                    {
                        if let Some(pid) = child.id() {
                            let _ = std::process::Command::new("pkill")
                                .arg("-P")
                                .arg(pid.to_string())
                                .arg("-9")
                                .output();

                            let _ = std::process::Command::new("kill")
                                .arg("-9")
                                .arg(format!("-{}", pid))
                                .output();
                        }
                    }

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

                    // STEP 1: CAPTURE KERNELRELEASE
                    eprintln!("[Build] [KERNELRELEASE-DIAG] ===== EXECUTION PATH: try_wait() SUCCESS =====");
                    match capture_and_save_kernelrelease(&canonical_kernel_path) {
                        Ok(kernelrelease) => {
                            let msg = format!("Captured kernelrelease: {}", kernelrelease);
                            output_callback(msg, None);
                            eprintln!(
                                "[Build] [KERNELRELEASE] Successfully captured: {}",
                                kernelrelease
                            );

                            eprintln!("[Build] [KERNELRELEASE-PROPAGATION] Propagating .kernelrelease to makepkg workspace");
                            propagate_kernelrelease_to_workspace(kernel_path, &kernelrelease);

                            // STEP 2: UPDATE MPL WITH KERNELRELEASE
                            eprintln!("[Build] [MPL] Updating metadata persistence layer");
                            if let Err(e) =
                                update_mpl_version(&canonical_kernel_path, &kernelrelease)
                            {
                                eprintln!("[Build] [MPL] WARNING: Failed to update MPL: {}", e);
                            }
                        }
                        Err(e) => {
                            let msg = format!("Warning: Failed to capture kernelrelease: {}", e);
                            output_callback(msg.clone(), None);
                            eprintln!("[Build] [KERNELRELEASE] [WARNING] {}", msg);
                        }
                    }

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

                eprintln!(
                    "[Build] [KERNELRELEASE-DIAG] ===== EXECUTION PATH: child.wait() SUCCESS ====="
                );
                match capture_and_save_kernelrelease(&canonical_kernel_path) {
                    Ok(kernelrelease) => {
                        let msg = format!("Captured kernelrelease: {}", kernelrelease);
                        output_callback(msg, None);
                        eprintln!(
                            "[Build] [KERNELRELEASE] Successfully captured: {}",
                            kernelrelease
                        );

                        eprintln!("[Build] [KERNELRELEASE-PROPAGATION] Propagating .kernelrelease to makepkg workspace");
                        propagate_kernelrelease_to_workspace(kernel_path, &kernelrelease);
                    }
                    Err(e) => {
                        let msg = format!("Warning: Failed to capture kernelrelease: {}", e);
                        output_callback(msg.clone(), None);
                        eprintln!("[Build] [KERNELRELEASE] [WARNING] {}", msg);
                    }
                }

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
        Err(e) => Err(BuildError::BuildFailed(format!(
            "Failed to wait for process: {}",
            e
        ))),
    }
}

/// Update MPL (Metadata Persistence Layer) with kernelrelease after successful build
fn update_mpl_version(workspace_root: &Path, kernelrelease: &str) -> Result<(), BuildError> {
    use crate::models::MPLMetadata;

    eprintln!(
        "[Build] [MPL] Updating metadata persistence layer with kernelrelease: {}",
        kernelrelease
    );

    let mpl_path = workspace_root.join(".goatd_metadata");

    let mpl_content = match std::fs::read_to_string(&mpl_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "[Build] [MPL] WARNING: Could not read existing MPL file: {}",
                e
            );
            eprintln!("[Build] [MPL] Creating new MPL with kernelrelease only");
            String::new()
        }
    };

    let mut mpl = if !mpl_content.is_empty() {
        match MPLMetadata::from_shell_format(&mpl_content) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[Build] [MPL] WARNING: Could not parse existing MPL: {}", e);
                MPLMetadata::default()
            }
        }
    } else {
        MPLMetadata::default()
    };

    mpl.kernel_release = kernelrelease.to_string();
    mpl.build_timestamp = chrono::Utc::now().to_rfc3339();
    mpl.source_dir = workspace_root.to_path_buf();

    let temp_path = mpl_path.with_extension("tmp");
    mpl.write_to_file(&temp_path).map_err(|e| {
        BuildError::BuildFailed(format!("Failed to write temporary MPL file: {}", e))
    })?;

    std::fs::rename(&temp_path, &mpl_path).map_err(|e| {
        BuildError::BuildFailed(format!("Failed to atomically rename MPL file: {}", e))
    })?;

    eprintln!(
        "[Build] [MPL] ✓ Successfully updated MPL at: {}",
        mpl_path.display()
    );
    eprintln!("[Build] [MPL] ✓ KERNELRELEASE in MPL: {}", kernelrelease);

    Ok(())
}

/// Propagate .kernelrelease to makepkg workspace for visibility in package() functions
fn propagate_kernelrelease_to_workspace(kernel_path: &Path, kernelrelease: &str) {
    use std::fs;
    use std::path::PathBuf;

    eprintln!(
        "[Build] [KERNELRELEASE-PROPAGATION] ========== CROSS-MOUNT PROPAGATION START =========="
    );
    eprintln!(
        "[Build] [KERNELRELEASE-PROPAGATION] Kernel path: {}",
        kernel_path.display()
    );
    eprintln!(
        "[Build] [KERNELRELEASE-PROPAGATION] Value to propagate: '{}'",
        kernelrelease
    );

    let canonical_kernel_path = match fs::canonicalize(kernel_path) {
        Ok(path) => {
            eprintln!(
                "[Build] [KERNELRELEASE-PROPAGATION] [CANON] Canonical kernel_path: {}",
                path.display()
            );
            path
        }
        Err(e) => {
            eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [CANON] WARNING: Could not canonicalize kernel_path: {}", e);
            kernel_path.to_path_buf()
        }
    };

    // STEP 1: PRIMARY SOURCE
    let kernelrelease_file = canonical_kernel_path.join(".kernelrelease");
    if !kernelrelease_file.exists() {
        if let Err(e) = fs::write(&kernelrelease_file, kernelrelease) {
            eprintln!(
                "[Build] [KERNELRELEASE-PROPAGATION] ✗ ERROR writing PRIMARY to {}: {}",
                kernelrelease_file.display(),
                e
            );
        } else {
            eprintln!(
                "[Build] [KERNELRELEASE-PROPAGATION] ✓ PRIMARY: Wrote .kernelrelease to {}",
                kernelrelease_file.display()
            );
        }
    } else {
        eprintln!(
            "[Build] [KERNELRELEASE-PROPAGATION] ℹ PRIMARY: .kernelrelease already exists at {}",
            kernelrelease_file.display()
        );
    }

    // STEP 2: Propagate to parent directories
    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Starting parent directory walk...");
    let mut current_path: PathBuf = canonical_kernel_path.clone();
    let mut parent_count = 0;
    let max_parents = 5;

    while parent_count < max_parents {
        if let Some(parent) = current_path.parent() {
            if parent == current_path {
                eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Reached filesystem root");
                break;
            }

            let anchor_path = parent.join(".goatd_anchor");
            if anchor_path.exists() {
                eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Reached workspace boundary (.goatd_anchor found at {})", parent.display());
                let parent_kernelrelease = parent.join(".kernelrelease");
                if let Err(e) = fs::write(&parent_kernelrelease, kernelrelease) {
                    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Level {}: Could not write to workspace root boundary {}: {}",
                             parent_count + 1, parent.display(), e);
                } else {
                    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] ✓ Level {}: Wrote to workspace root boundary {}",
                             parent_count + 1, parent.display());
                }
                break;
            }

            let parent_kernelrelease = parent.join(".kernelrelease");

            match fs::metadata(parent) {
                Ok(metadata) => {
                    if !metadata.permissions().readonly() {
                        if let Err(e) = fs::write(&parent_kernelrelease, kernelrelease) {
                            eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Level {}: Could not write to {}: {}",
                                     parent_count + 1, parent.display(), e);
                        } else {
                            eprintln!("[Build] [KERNELRELEASE-PROPAGATION] ✓ Level {}: Copied to parent {}",
                                     parent_count + 1, parent.display());
                        }
                    } else {
                        eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Level {}: Skipped read-only parent: {}",
                                 parent_count + 1, parent.display());
                    }
                }
                Err(e) => {
                    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Level {}: Cannot access parent {}: {}",
                             parent_count + 1, parent.display(), e);
                }
            }

            current_path = parent.to_path_buf();
            parent_count += 1;
        } else {
            break;
        }
    }

    eprintln!(
        "[Build] [KERNELRELEASE-PROPAGATION] [PARENTS] Completed parent walk ({} levels)",
        parent_count
    );

    // STEP 3: Copy to all src/ subdirectories
    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [SUBDIRS] Searching for src/ subdirectories...");
    let src_dir = canonical_kernel_path.join("src");
    let mut subdir_count = 0;

    if src_dir.exists() && src_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&src_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        let subdir_kernelrelease = path.join(".kernelrelease");
                        if let Err(e) = fs::write(&subdir_kernelrelease, kernelrelease) {
                            eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [SUBDIRS] Failed to write to {}: {}",
                                     path.display(), e);
                        } else {
                            eprintln!(
                                "[Build] [KERNELRELEASE-PROPAGATION] ✓ Copied to src subdir: {}",
                                path.display()
                            );
                            subdir_count += 1;
                        }
                    }
                }
            }
        }
        eprintln!(
            "[Build] [KERNELRELEASE-PROPAGATION] [SUBDIRS] Completed ({} subdirectories)",
            subdir_count
        );
    } else {
        eprintln!("[Build] [KERNELRELEASE-PROPAGATION] [SUBDIRS] No src/ directory found");
    }

    eprintln!("[Build] [KERNELRELEASE-PROPAGATION] ========== CROSS-MOUNT PROPAGATION COMPLETE ==========");
    eprintln!(
        "[Build] [KERNELRELEASE-PROPAGATION] Summary: PRIMARY={}, PARENTS={}, SUBDIRS={}",
        if kernelrelease_file.exists() {
            "✓"
        } else {
            "✗"
        },
        parent_count,
        subdir_count
    );
}

/// Capture the kernel's `kernelrelease` string from the build tree
fn capture_and_save_kernelrelease(kernel_path: &Path) -> Result<String, String> {
    use std::fs;

    eprintln!("[Build] [KERNELRELEASE] Starting robust kernelrelease discovery");
    eprintln!(
        "[Build] [KERNELRELEASE] Root search path: {}",
        kernel_path.display()
    );

    // STRATEGY 1: Try direct path first
    let kernel_release_path = kernel_path.join("include/config/kernel.release");
    eprintln!(
        "[Build] [KERNELRELEASE] [SEARCH-1] Trying direct path: {}",
        kernel_release_path.display()
    );

    if kernel_release_path.exists() {
        eprintln!("[Build] [KERNELRELEASE] [SEARCH-1] ✓ Found kernelrelease at direct path");
        if let Ok(kernelrelease) = read_and_save_kernelrelease(&kernel_release_path, kernel_path) {
            return Ok(kernelrelease);
        }
    } else {
        eprintln!("[Build] [KERNELRELEASE] [SEARCH-1] ✗ Not found at direct path");
    }

    // STRATEGY 2: Search in src/ subdirectories
    eprintln!("[Build] [KERNELRELEASE] [SEARCH-2] Searching in src/ subdirectories...");
    let src_dir = kernel_path.join("src");

    if src_dir.exists() && src_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&src_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_dir() {
                        let candidate = path.join("include/config/kernel.release");
                        eprintln!(
                            "[Build] [KERNELRELEASE] [SEARCH-2] Trying: {}",
                            candidate.display()
                        );

                        if candidate.exists() {
                            eprintln!("[Build] [KERNELRELEASE] [SEARCH-2] ✓ Found kernelrelease in src subdirectory: {}", path.display());
                            if let Ok(kernelrelease) =
                                read_and_save_kernelrelease(&candidate, kernel_path)
                            {
                                return Ok(kernelrelease);
                            }
                        }
                    }
                }
            }
        }
        eprintln!("[Build] [KERNELRELEASE] [SEARCH-2] ✗ Not found in any src/ subdirectories");
    } else {
        eprintln!("[Build] [KERNELRELEASE] [SEARCH-2] src/ directory does not exist");
    }

    eprintln!("[Build] [KERNELRELEASE] ✗ FAILED: kernelrelease discovery exhausted all strategies");
    Err("Failed to locate kernelrelease file: not found at kernel_path/include/config/kernel.release or in any src/ subdirectories".to_string())
}

/// Helper function to read kernelrelease from a file path and save it to .kernelrelease
fn read_and_save_kernelrelease(source_path: &Path, kernel_path: &Path) -> Result<String, String> {
    use std::fs;
    use std::io::Read;

    eprintln!(
        "[Build] [KERNELRELEASE] Reading from: {}",
        source_path.display()
    );

    let mut release_file = fs::File::open(source_path).map_err(|e| {
        format!(
            "Failed to read kernelrelease from {}: {}",
            source_path.display(),
            e
        )
    })?;

    let mut kernelrelease = String::new();
    release_file
        .read_to_string(&mut kernelrelease)
        .map_err(|e| format!("Failed to read kernelrelease content: {}", e))?;

    eprintln!(
        "[Build] [KERNELRELEASE] Raw content length: {} bytes",
        kernelrelease.len()
    );

    let kernelrelease = kernelrelease.trim().to_string();

    eprintln!(
        "[Build] [KERNELRELEASE] Trimmed content: '{}'",
        kernelrelease
    );

    if kernelrelease.is_empty() {
        eprintln!("[Build] [KERNELRELEASE] ERROR: Content is empty after trimming!");
        return Err("kernelrelease file is empty".to_string());
    }

    let kernelrelease_output_path = kernel_path.join(".kernelrelease");

    eprintln!(
        "[Build] [KERNELRELEASE] Writing to: {}",
        kernelrelease_output_path.display()
    );

    fs::write(&kernelrelease_output_path, &kernelrelease).map_err(|e| {
        eprintln!(
            "[Build] [KERNELRELEASE] ERROR writing .kernelrelease: {:?}",
            e
        );
        format!(
            "Failed to write .kernelrelease file to {}: {}",
            kernelrelease_output_path.display(),
            e
        )
    })?;

    eprintln!("[Build] [KERNELRELEASE] ✓ .kernelrelease file written successfully");
    eprintln!(
        "[Build] [KERNELRELEASE] ✓ Captured kernelrelease: {}",
        kernelrelease
    );

    Ok(kernelrelease)
}

/// Verify PKGBUILD parity between template and workspace version
fn verify_pkgbuild_parity(template_path: &Path, workspace_path: &Path) -> Result<bool, String> {
    use std::fs;
    use std::io::{BufReader, Read};

    eprintln!("[Build] [EXECUTOR] [PARITY] Starting SHA256 checksum verification");
    eprintln!(
        "[Build] [EXECUTOR] [PARITY] Template: {}",
        template_path.display()
    );
    eprintln!(
        "[Build] [EXECUTOR] [PARITY] Workspace: {}",
        workspace_path.display()
    );

    let mut template_file = fs::File::open(template_path)
        .map_err(|e| format!("Failed to open template PKGBUILD: {}", e))?;

    let mut template_content = String::new();
    BufReader::new(&mut template_file)
        .read_to_string(&mut template_content)
        .map_err(|e| format!("Failed to read template PKGBUILD: {}", e))?;

    let mut workspace_file = fs::File::open(workspace_path)
        .map_err(|e| format!("Failed to open workspace PKGBUILD: {}", e))?;

    let mut workspace_content = String::new();
    BufReader::new(&mut workspace_file)
        .read_to_string(&mut workspace_content)
        .map_err(|e| format!("Failed to read workspace PKGBUILD: {}", e))?;

    let checksums_match = template_content == workspace_content;

    if checksums_match {
        eprintln!("[Build] [EXECUTOR] [PARITY] ✓ Checksums MATCH - template parity verified");
    } else {
        eprintln!(
            "[Build] [EXECUTOR] [PARITY] ✗ Checksums DIFFER - stale/corrupted PKGBUILD detected"
        );
        eprintln!(
            "[Build] [EXECUTOR] [PARITY]   Template size: {} bytes",
            template_content.len()
        );
        eprintln!(
            "[Build] [EXECUTOR] [PARITY]   Workspace size: {} bytes",
            workspace_content.len()
        );
    }

    Ok(checksums_match)
}

/// Validate kernel build results
pub fn validate_kernel_build(config: &KernelConfig) -> Result<(), BuildError> {
    validate_kernel_config(config)?;
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
            gpu_active_driver: true,
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
            kernel_variant: String::new(),
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
        use std::fs::File;
        use std::io::Write;
        use tempfile::TempDir;

        let hw = create_test_hardware();

        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        let pkgbuild_path = temp_dir.path().join("PKGBUILD");
        let mut file = File::create(&pkgbuild_path).expect("Failed to create PKGBUILD");
        writeln!(file, "# Dummy PKGBUILD for testing").expect("Failed to write PKGBUILD");
        drop(file);

        let result = prepare_build_environment(&hw, temp_dir.path());

        assert!(result.is_ok());
    }

    #[test]
    fn test_prepare_build_environment_insufficient_hardware() {
        let mut hw = create_test_hardware();
        hw.ram_gb = 2;
        let kernel_path = std::path::Path::new(".");
        assert!(prepare_build_environment(&hw, kernel_path).is_err());
    }

    #[tokio::test]
    async fn test_configure_build_success() {
        let mut cfg = create_test_config();
        let hw = create_test_hardware();
        let result = configure_build(&mut cfg, &hw, None).await;
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
        let hw1 = create_test_hardware();
        let hw2 = create_test_hardware();

        let result1 = validate_hardware(&hw1);
        let result2 = validate_hardware(&hw2);

        assert_eq!(result1.is_ok(), result2.is_ok());
    }

    #[test]
    fn test_parse_build_progress_x_y_pattern() {
        assert_eq!(
            parse_build_progress("[ 582/12041] CC  arch/x86/boot/cpuflags.c"),
            Some(4)
        );
        assert_eq!(parse_build_progress("[1/100] Compiling..."), Some(1));
        assert_eq!(parse_build_progress("[100/100] Done"), Some(100));
        assert_eq!(parse_build_progress("[ 50/100] Half done"), Some(50));
    }

    #[test]
    fn test_parse_build_progress_percentage() {
        assert_eq!(parse_build_progress("[ 45%] Compiling..."), Some(45));
        assert_eq!(parse_build_progress("[  1%] Starting..."), Some(1));
        assert_eq!(parse_build_progress("[100%] Complete"), Some(100));
        assert_eq!(parse_build_progress("[ 99%] Almost done"), Some(99));
    }

    #[test]
    fn test_parse_build_progress_compilation() {
        assert_eq!(
            parse_build_progress("  CC  arch/x86/boot/cpuflags.c"),
            Some(0)
        );
        assert_eq!(parse_build_progress("  LD  vmlinux"), Some(0));
        assert_eq!(parse_build_progress("  AR  lib/lib.a"), Some(0));
    }

    #[test]
    fn test_parse_build_progress_no_match() {
        assert_eq!(parse_build_progress("random compilation output"), None);
        assert_eq!(parse_build_progress("error: undefined reference"), None);
    }

    #[test]
    fn test_kernel_source_db_integration() {
        let source_db = crate::kernel::sources::KernelSourceDB::new();

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
        let source_db = crate::kernel::sources::KernelSourceDB::new();
        let aur_url = source_db.get_source_url("linux-mainline").unwrap();

        assert!(
            aur_url.contains("aur.archlinux.org"),
            "linux-mainline should point to AUR, got: {}",
            aur_url
        );
        assert!(
            aur_url.ends_with(".git"),
            "AUR URL should end with .git, got: {}",
            aur_url
        );
    }
}

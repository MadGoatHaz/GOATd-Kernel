//! Build Orchestration: 5-phase kernel build pipeline (Preparation -> Configuration -> Patching -> Building -> Validation).

pub mod checkpoint;
pub mod executor;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

pub use executor::{
    configure_build, prepare_build_environment, prepare_kernel_build, validate_hardware,
    validate_kernel_build, validate_kernel_config,
};

pub use state::{BuildPhaseState, OrchestrationState};

use crate::error::Result;
use crate::models::{HardwareInfo, KernelConfig};
use crate::LogCollector;
use eframe::egui;

/// Manages 5-phase kernel build orchestration with progress tracking.
#[derive(Clone)]
pub struct AsyncOrchestrator {
    /// Shared mutable state protected by RwLock for thread safety
    state: Arc<RwLock<OrchestrationState>>,

    /// Directory for storing checkpoints and state snapshots
    checkpoint_dir: PathBuf,

    /// Path to the kernel source directory
    kernel_path: PathBuf,

    /// Whether recovery from previous checkpoint is enabled
    recovery_enabled: bool,

    /// Channel for sending build events to UI
    build_tx: Option<tokio::sync::mpsc::Sender<crate::ui::controller::BuildEvent>>,

    /// Channel for receiving cancellation signals from UI
    cancel_rx: tokio::sync::watch::Receiver<bool>,

    /// Log collector for dual-writing build output to logs and UI
    pub log_collector: Option<Arc<LogCollector>>,

    /// Optional timeout for the build process (test mode support)
    test_timeout: Option<Duration>,

    /// Optional egui context handle for requesting repaints from background threads
    ctx_handle: Option<egui::Context>,
}

impl AsyncOrchestrator {
    /// Create a new AsyncOrchestrator with the given hardware and config.
    ///
    /// Initializes the orchestrator in the Preparation phase.
    ///
    /// # Arguments
    /// * `hardware` - System hardware information
    /// * `config` - Kernel build configuration
    /// * `checkpoint_dir` - Directory for checkpoint files
    /// * `kernel_path` - Path to the kernel source directory
    /// * `build_tx` - Channel for sending build events to UI
    /// * `log_collector` - Optional log collector for build output persistence
    /// * `test_timeout` - Optional timeout for build process (test mode)
    /// * `ctx_handle` - Optional egui context for requesting repaints from background threads
    ///
    /// # Examples
    /// ```ignore
    /// let orch = AsyncOrchestrator::new(hardware, config, "/tmp/checkpoints".into(), "/tmp/kernel-src".into(), tx, log_collector, None, ctx_handle).await;
    /// ```
    pub async fn new(
        hardware: HardwareInfo,
        config: KernelConfig,
        checkpoint_dir: PathBuf,
        kernel_path: PathBuf,
        build_tx: Option<tokio::sync::mpsc::Sender<crate::ui::controller::BuildEvent>>,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
        log_collector: Option<Arc<LogCollector>>,
        test_timeout: Option<Duration>,
        ctx_handle: Option<egui::Context>,
    ) -> Result<Self> {
        let state = OrchestrationState::new(hardware, config);

        // RESTORED: Create build directory structure
        // App allows empty workspaces - auto-initialization (cloning/fetching sources) will trigger
        std::fs::create_dir_all(&kernel_path)
            .map_err(|e| format!("Failed to create kernel directory: {}", e))?;

        // =========================================================================
        // INITIALIZE WORKSPACE ANCHOR - Centralized Path Registry
        // =========================================================================
        // Deploy .goatd_anchor file at workspace root to establish canonical path resolution.
        // This eliminates reliance on fragile parent() calls and provides absolute verification.
        let workspace_root = kernel_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));

        let anchor_path = workspace_root.join(".goatd_anchor");
        
        // Create or verify the anchor file
        match std::fs::write(&anchor_path, "") {
            Ok(_) => {
                eprintln!(
                    "[Build] [ANCHOR] ✓ Workspace anchor deployed at: {}",
                    anchor_path.display()
                );
            }
            Err(e) => {
                eprintln!(
                    "[Build] [ANCHOR] ⚠ Warning: Could not deploy workspace anchor: {}",
                    e
                );
                // Non-fatal: continue with build even if anchor creation fails
                // The PathRegistry will verify existence later
            }
        }

        Ok(AsyncOrchestrator {
            state: Arc::new(RwLock::new(state)),
            checkpoint_dir,
            kernel_path,
            recovery_enabled: true,
            build_tx,
            cancel_rx,
            log_collector,
            test_timeout,
            ctx_handle,
        })
    }

    /// Get the current build phase.
    pub async fn current_phase(&self) -> BuildPhaseState {
        self.state.read().await.phase
    }

    /// Get the current progress percentage (0-100).
    pub async fn current_progress(&self) -> u32 {
        self.state.read().await.progress
    }

    /// Update the progress percentage and emit Progress event to UI.
    ///
    /// # Arguments
    /// * `percent` - Progress percentage (0-100), will be clamped to 100
    pub async fn set_progress(&self, percent: u32) {
        self.state.write().await.set_progress(percent);

        // Emit Progress event to UI channel
        if let Some(ref tx) = self.build_tx {
            let progress_f32 = (percent as f32) / 100.0;
            let _ = tx.try_send(crate::ui::controller::BuildEvent::Progress(progress_f32));
        }

        // Request repaint to ensure UI updates immediately
        if let Some(ref ctx) = self.ctx_handle {
            ctx.request_repaint();
        }
    }

    /// Transition to the next build phase and emit PhaseChanged event to UI.
    ///
    /// Validates the transition is legal before applying it.
    ///
    /// # Arguments
    /// * `next_phase` - The phase to transition to
    ///
    /// # Errors
    /// Returns an error if the transition is not valid from the current phase
    pub async fn transition_phase(&self, next_phase: BuildPhaseState) -> Result<()> {
        let mut state = self.state.write().await;
        state.transition_to(next_phase)?;

        // Emit PhaseChanged event to UI channel
        if let Some(ref tx) = self.build_tx {
            let phase_name = format!("{:?}", next_phase);
            let _ = tx.try_send(crate::ui::controller::BuildEvent::PhaseChanged(phase_name));
        }

        // Log phase transition to parsed log
        if let Some(ref collector) = self.log_collector {
            collector.log_parsed(format!("PHASE TRANSITION: {:?}", next_phase));
        }

        // Request repaint to ensure UI updates immediately
        if let Some(ref ctx) = self.ctx_handle {
            ctx.request_repaint();
        }

        Ok(())
    }

    /// Record a patch application result.
    pub async fn record_patch_result(&self, success: bool) {
        self.state.write().await.record_patch_applied(success);
    }

    /// Record an error and transition to the Failed phase.
    pub async fn record_error(&self, error: String) {
        self.state.write().await.record_error(error);
    }

    /// Send a log event to the UI
    pub async fn send_log_event(&self, message: String) {
        if let Some(ref tx) = self.build_tx {
            let _ = tx
                .send(crate::ui::controller::BuildEvent::Log(message))
                .await;
        }
        // Request repaint to ensure UI updates immediately
        if let Some(ref ctx) = self.ctx_handle {
            ctx.request_repaint();
        }
    }

    /// Send a granular status update from major phase start
    pub async fn send_status(&self, status: String) {
        if let Some(ref tx) = self.build_tx {
            let _ = tx
                .send(crate::ui::controller::BuildEvent::StatusUpdate(status))
                .await;
        }
        // Request repaint to ensure UI updates immediately
        if let Some(ref ctx) = self.ctx_handle {
            ctx.request_repaint();
        }
    }

    /// Send a status event to the UI
    pub async fn send_status_event(&self, message: String) {
        if let Some(ref tx) = self.build_tx {
            let _ = tx
                .send(crate::ui::controller::BuildEvent::Status(message))
                .await;
        }
        // Request repaint to ensure UI updates immediately
        if let Some(ref ctx) = self.ctx_handle {
            ctx.request_repaint();
        }
    }

    /// Validates hardware and kernel source, cleans old artifacts.
    pub async fn prepare(&self) -> Result<()> {
        // Send status update at phase start
        self.send_status(
            "Preparation: Validating hardware and acquiring kernel sources...".to_string(),
        )
        .await;

        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Preparation {
            return Err("Not in Preparation phase".into());
        }

        let hardware = state.hardware.clone();

        // Determine kernel variant from config with explicit fallback chain
        // Priority 1: Use state.config.kernel_variant if not empty (set by UI)
        // Priority 2: Use state.config.version if not empty
        // Priority 3: Default to "linux"
        let kernel_variant = if !state.config.kernel_variant.is_empty() {
            eprintln!(
                "[Build] [PREPARATION] ✓ Using kernel_variant from config (set by UI): {}",
                state.config.kernel_variant
            );
            state.config.kernel_variant.clone()
        } else if !state.config.version.is_empty() {
            eprintln!(
                "[Build] [PREPARATION] ⚠ kernel_variant empty, falling back to version: {}",
                state.config.version
            );
            state.config.version.clone()
        } else {
            eprintln!("[Build] [PREPARATION] ⚠ Both kernel_variant and version empty, defaulting to 'linux'");
            "linux".to_string()
        };
        eprintln!(
            "[Build] [PREPARATION] Final kernel variant: {}",
            kernel_variant
        );

        drop(state);

        // Store kernel variant in state for access by other phases
        {
            let mut state_mut = self.state.write().await;
            state_mut.config.kernel_variant = kernel_variant.clone();
        }

        // =========================================================================
        // PHASE 1a: SOURCE AUTO-ACQUISITION - Check and fetch missing sources
        // =========================================================================
        // Before any other preparation, ensure kernel sources are available
        let pkgbuild_path = self.kernel_path.join("PKGBUILD");

        if !pkgbuild_path.exists() {
            self.send_log_event(
                "Kernel sources missing. Initializing source acquisition...".to_string(),
            )
            .await;
            eprintln!(
                "[Build] [PREPARATION] PKGBUILD not found at: {:?}",
                pkgbuild_path
            );

            // Get source URL from the kernel sources database
            use crate::kernel::sources::KernelSourceDB;
            let source_db = KernelSourceDB::new();

            let source_url = source_db
                .get_source_url(&kernel_variant)
                .ok_or_else(|| format!("Unknown kernel variant: {}", kernel_variant))?;

            eprintln!(
                "[Build] [PREPARATION] Cloning kernel source from: {}",
                source_url
            );
            self.send_log_event(format!("Cloning from: {}", source_url))
                .await;

            // Perform async git clone operation
            use crate::kernel::git::GitManager;

            match GitManager::clone(source_url, &self.kernel_path) {
                Ok(_) => {
                    self.send_log_event("Sources successfully acquired.".to_string())
                        .await;
                    eprintln!("[Build] [PREPARATION] ✓ Kernel sources cloned successfully");
                }
                Err(e) => {
                    let err_msg = format!("Failed to acquire kernel sources: {:?}", e);
                    eprintln!("[Build] [PREPARATION] ✗ {}", err_msg);
                    return Err(err_msg.into());
                }
            }
        } else {
            eprintln!(
                "[Build] [PREPARATION] PKGBUILD found at: {:?}",
                pkgbuild_path
            );

            // =========================================================================
            // PURGE ON VERSION MISMATCH - Validate source version against expected
            // =========================================================================
            // If PKGBUILD exists, validate its version matches expected variant version
            // If mismatch detected, purge the directory and force re-clone
            eprintln!(
                "[Build] [PREPARATION] Validating source version for variant: {}",
                kernel_variant
            );
            self.send_status(
                "Validation: Checking if source version matches expected version...".to_string(),
            )
            .await;

            // Fetch expected version from remote (latest version for the variant)
            let expected_version =
                match crate::kernel::pkgbuild::get_latest_version_by_variant(&kernel_variant).await
                {
                    Ok(version) => {
                        eprintln!(
                            "[Build] [PREPARATION] ✓ Fetched expected version for '{}': {}",
                            kernel_variant, version
                        );
                        version
                    }
                    Err(e) => {
                        eprintln!(
                            "[Build] [PREPARATION] ⚠ Failed to fetch expected version: {}",
                            e
                        );
                        self.send_log_event(format!(
                        "Warning: Could not fetch expected version - proceeding with caution: {}",
                        e
                    ))
                        .await;
                        // Continue without version check if fetch fails (non-fatal)
                        String::new()
                    }
                };

            // Validate source version against expected
            if !expected_version.is_empty() {
                use crate::kernel::git::validate_source_version;

                match validate_source_version(&self.kernel_path, &expected_version) {
                    Ok(true) => {
                        eprintln!(
                            "[Build] [PREPARATION] ✓ Source version matches expected version"
                        );
                        self.send_log_event("Source version validated successfully.".to_string())
                            .await;
                    }
                    Ok(false) => {
                        // Version mismatch detected - purge and re-clone
                        eprintln!("[Build] [PREPARATION] ✗ Version mismatch detected! Purging old source and re-cloning...");
                        self.send_log_event("Version mismatch detected. Purging old kernel sources and re-cloning...".to_string()).await;

                        // Remove old kernel source directory
                        match std::fs::remove_dir_all(&self.kernel_path) {
                            Ok(()) => {
                                eprintln!(
                                    "[Build] [PREPARATION] ✓ Removed old kernel source directory"
                                );
                                self.send_log_event("Removed old kernel sources.".to_string())
                                    .await;
                            }
                            Err(e) => {
                                eprintln!(
                                    "[Build] [PREPARATION] ⚠ Failed to remove old directory: {}",
                                    e
                                );
                                self.send_log_event(format!(
                                    "Warning: Failed to remove old sources: {}",
                                    e
                                ))
                                .await;
                                // Attempt to continue anyway
                            }
                        }

                        // Recreate the directory for fresh clone
                        std::fs::create_dir_all(&self.kernel_path)
                            .map_err(|e| format!("Failed to recreate kernel directory: {}", e))?;

                        // Perform fresh clone
                        use crate::kernel::sources::KernelSourceDB;
                        let source_db = KernelSourceDB::new();
                        let source_url = source_db
                            .get_source_url(&kernel_variant)
                            .ok_or_else(|| format!("Unknown kernel variant: {}", kernel_variant))?;

                        eprintln!(
                            "[Build] [PREPARATION] Re-cloning kernel source from: {}",
                            source_url
                        );
                        self.send_log_event(format!("Re-cloning from: {}", source_url))
                            .await;

                        use crate::kernel::git::GitManager;
                        match GitManager::clone(source_url, &self.kernel_path) {
                            Ok(_) => {
                                self.send_log_event("Fresh kernel sources acquired.".to_string())
                                    .await;
                                eprintln!(
                                    "[Build] [PREPARATION] ✓ Kernel sources re-cloned successfully"
                                );
                            }
                            Err(e) => {
                                let err_msg = format!("Failed to re-clone kernel sources: {:?}", e);
                                eprintln!("[Build] [PREPARATION] ✗ {}", err_msg);
                                return Err(err_msg.into());
                            }
                        }
                    }
                    Err(e) => {
                        // Validation failed (I/O or parsing error) - log warning and continue
                        eprintln!("[Build] [PREPARATION] ⚠ Version validation failed: {:?}", e);
                        self.send_log_event(format!("Warning: Could not validate source version: {:?}. Proceeding with caution.", e)).await;
                        // Non-fatal: continue with build
                    }
                }
            }
        }

        // =========================================================================
        // CLEANUP OLD ARTIFACTS - Delegate to KernelPatcher
        // =========================================================================
        // Use Patcher's cleanup method for old .pkg.tar.zst files
        use crate::kernel::patcher::KernelPatcher;
        let patcher = KernelPatcher::new(self.kernel_path.clone());
        let _ = patcher.cleanup_previous_artifacts();

        // =========================================================================
        // INITIALIZE MODPROBED-DB - Database for module detection
        // =========================================================================
        {
            match tokio::process::Command::new("which")
                .arg("modprobed-db")
                .status()
                .await
            {
                Ok(status) if status.success() => {
                    // modprobed-db is available, initialize the database
                    self.send_log_event("Initializing modprobed-db database...".to_string())
                        .await;
                    eprintln!("[Build] [MODPROBED] Starting modprobed-db initialization");

                    // First execution: create/initialize the database
                    match tokio::process::Command::new("modprobed-db")
                        .arg("store")
                        .status()
                        .await
                    {
                        Ok(status) if status.success() => {
                            eprintln!("[Build] [MODPROBED] ✓ First modprobed-db store completed");

                            // Wait 500ms before second run
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                            // Second execution: populate/update the database
                            match tokio::process::Command::new("modprobed-db")
                                .arg("store")
                                .status()
                                .await
                            {
                                Ok(status) if status.success() => {
                                    eprintln!(
                                        "[Build] [MODPROBED] ✓ Second modprobed-db store completed"
                                    );
                                    self.send_status(
                                        "modprobed-db database initialized successfully"
                                            .to_string(),
                                    )
                                    .await;
                                }
                                _ => {
                                    eprintln!("[Build] [MODPROBED] ⚠ Second modprobed-db store failed, continuing anyway");
                                    // Non-fatal: continue with build
                                }
                            }
                        }
                        _ => {
                            eprintln!("[Build] [MODPROBED] ⚠ First modprobed-db store failed, continuing anyway");
                            // Non-fatal: continue with build
                        }
                    }
                }
                _ => {
                    eprintln!(
                        "[Build] [MODPROBED] ⚠ modprobed-db not found, skipping initialization"
                    );
                    self.send_log_event(
                        "modprobed-db not installed - skipping database initialization".to_string(),
                    )
                    .await;
                    // Non-fatal: continue with build
                }
            }
        }

        // =========================================================================
        // PHASE 1b: HARDWARE VALIDATION - After source acquisition, validate hardware
        // =========================================================================
        // Validate hardware meets minimum requirements and kernel source exists
        executor::prepare_build_environment(&hardware, &self.kernel_path)?;

        // Update progress: Preparation phase is 0-5%
        let progress = 5;
        self.set_progress(progress).await;
        eprintln!("[Build] [PROGRESS] Preparation complete: {}%", progress);

        // Transition to Configuration phase
        self.transition_phase(BuildPhaseState::Configuration).await
    }

    /// Finalizes config via Rule Engine, applies GPU/driver policies.
    pub async fn configure(&self) -> Result<()> {
        // Send status update at phase start
        self.send_status(
            "Configuration: Finalizing kernel configuration via Rule Engine...".to_string(),
        )
        .await;

        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Configuration {
            return Err("Not in Configuration phase".into());
        }

        let hardware = state.hardware.clone();
        let config = state.config.clone();
        drop(state);

        // =========================================================================
        // FINALIZE CONFIGURATION (Rule Engine) - AUTHORITATIVE
        // =========================================================================
        // The Finalizer is the ONLY place where all configuration rules are applied:
        // 1. Load profile data (from pure data provider)
        // 2. Apply profile defaults to first-class fields (hz, preemption, force_clang)
        // 3. Set MGLRU tuning parameters
        // 4. Apply GPU-aware driver exclusions
        // 5. Determine LTO shielding modules
        // 6. Generate derived config_options strings
        //
        // This ensures a single source of truth for all configuration logic.
        use crate::config::finalizer;

        let mut finalized_config = finalizer::finalize_kernel_config(config, &hardware)
            .map_err(|e| format!("Failed to finalize configuration: {}", e))?;

        eprintln!("[Build] [CONFIG] ✓ Configuration finalized via Rule Engine");
        eprintln!("[Build] [CONFIG]   - Profile: {}", finalized_config.profile);
        eprintln!("[Build] [CONFIG]   - HZ: {}", finalized_config.hz);
        eprintln!(
            "[Build] [CONFIG]   - Preemption: {}",
            finalized_config.preemption
        );
        eprintln!(
            "[Build] [CONFIG]   - Clang: {}",
            finalized_config.force_clang
        );
        eprintln!(
            "[Build] [CONFIG]   - LTO Type: {:?}",
            finalized_config.lto_type
        );
        eprintln!(
            "[Build] [CONFIG]   - LTO Shield Modules: {:?}",
            finalized_config.lto_shield_modules
        );
        eprintln!(
            "[Build] [CONFIG]   - Driver Exclusions: {}",
            finalized_config.driver_exclusions.len()
        );

        // Apply GPU policy and resolve dynamic version (STEP 2: Dynamic Versioning)
        let configured =
            executor::configure_build(&mut finalized_config, &hardware, self.build_tx.as_ref())
                .await?;

        // Update internal state with configured kernel config
        {
            let mut state = self.state.write().await;
            state.config = configured;
        }

        // Update progress: Configuration phase is 5-8%
        let progress = 8;
        self.set_progress(progress).await;
        eprintln!("[Build] [PROGRESS] Configuration complete: {}%", progress);

        // Transition to Patching phase
        self.transition_phase(BuildPhaseState::Patching).await
    }

    /// Delegates PKGBUILD and .config patching to KernelPatcher.
    pub async fn patch(&self) -> Result<()> {
        // Send status update at phase start
        self.send_status(
            "Patching: Applying PKGBUILD and kernel configuration patches...".to_string(),
        )
        .await;

        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Patching {
            return Err("Not in Patching phase".into());
        }

        let config = state.config.clone();
        drop(state);

        // =========================================================================
        // UNIFIED SURGICAL ENGINE: Delegate all patching to KernelPatcher
        // =========================================================================
        // Build environment variables for patcher (includes GOATD_* metadata)
        let mut build_env_vars = std::collections::HashMap::new();

        // CRITICAL FIX: Pass workspace root to patcher for MPL injection
        // This enables the patcher to inject .goatd_metadata sourcing into PKGBUILD
        let workspace_root = self
            .kernel_path
            .canonicalize()
            .unwrap_or_else(|_| self.kernel_path.clone());
        build_env_vars.insert(
            "GOATD_WORKSPACE_ROOT".to_string(),
            workspace_root.to_string_lossy().to_string(),
        );
        eprintln!(
            "[Build] [ORCHESTRATOR] Set GOATD_WORKSPACE_ROOT={}",
            workspace_root.display()
        );

        // LTO configuration
        let lto_level = match config.lto_type {
            crate::models::LtoType::Full => "full",
            crate::models::LtoType::Thin => "thin",
            crate::models::LtoType::None => "none",
        };
        build_env_vars.insert("GOATD_LTO_LEVEL".to_string(), lto_level.to_string());

        // Modprobed and whitelist flags
        build_env_vars.insert(
            "GOATD_USE_MODPROBED_DB".to_string(),
            if config.use_modprobed { "1" } else { "0" }.to_string(),
        );
        build_env_vars.insert(
            "GOATD_USE_KERNEL_WHITELIST".to_string(),
            if config.use_whitelist { "1" } else { "0" }.to_string(),
        );

        // Profile name for rebranding
        build_env_vars.insert("GOATD_PROFILE_NAME".to_string(), config.profile.clone());
        build_env_vars.insert(
            "GOATD_PROFILE_SUFFIX".to_string(),
            format!("-GOATd-{}", config.profile),
        );

        // Additional configuration metadata
        build_env_vars.insert(
            "GOATD_KERNEL_HARDENING".to_string(),
            config.hardening.to_string(),
        );
        build_env_vars.insert(
            "GOATD_PREEMPTION_MODEL".to_string(),
            config
                .config_options
                .get("_PREEMPTION_MODEL")
                .cloned()
                .unwrap_or_else(|| "CONFIG_PREEMPT_VOLUNTARY=y".to_string()),
        );
        build_env_vars.insert(
            "GOATD_HZ_VALUE".to_string(),
            config
                .config_options
                .get("_HZ_VALUE")
                .cloned()
                .unwrap_or_else(|| "CONFIG_HZ=300".to_string()),
        );
        build_env_vars.insert(
            "GOATD_SECURE_BOOT".to_string(),
            if config.secure_boot { "1" } else { "0" }.to_string(),
        );
        build_env_vars.insert(
            "GOATD_ENABLE_SECURE_BOOT".to_string(),
            if config.secure_boot { "1" } else { "0" }.to_string(),
        );

        // Hardening flag: only set if profile specifically requests "Hardened" status
        let hardening_enabled = config.hardening == crate::models::HardeningLevel::Hardened;
        build_env_vars.insert(
            "GOATD_ENABLE_HARDENING".to_string(),
            if hardening_enabled { "1" } else { "0" }.to_string(),
        );

        build_env_vars.insert(
            "GOATD_ENABLE_SELINUX_APPARMOR".to_string(),
            if hardening_enabled { "1" } else { "0" }.to_string(),
        );

        // Native optimizations configuration (-march=native)
        build_env_vars.insert(
            "GOATD_NATIVE_OPTIMIZATIONS".to_string(),
            if config.native_optimizations {
                "1"
            } else {
                "0"
            }
            .to_string(),
        );
        if config.native_optimizations {
            eprintln!("[Build] [ORCHESTRATOR] Native optimizations enabled (-march=native)");
        } else {
            eprintln!("[Build] [ORCHESTRATOR] Native optimizations disabled");
        }

        // Polly optimization configuration (first-class field)
        build_env_vars.insert(
            "GOATD_USE_POLLY".to_string(),
            if config.use_polly { "1" } else { "0" }.to_string(),
        );

        // CRITICAL FIX: Export Polly optimization flags for PHASE G1 injection
        // These flags are used in the prebuild LTO enforcer template to append Polly
        // optimizations to CFLAGS/CXXFLAGS/LDFLAGS during the kernel build
        let polly_flags = if config.use_polly {
            // Standard LLVM Polly loop optimization flags for kernel builds
            // -mllvm -polly: Enable the Polly loop optimizer
            // -mllvm -polly-vectorizer=stripmine: Use stripmine vectorizer (better for kernels)
            // -mllvm -polly-opt-fusion=max: Maximize loop fusion opportunities
            "-mllvm -polly -mllvm -polly-vectorizer=stripmine -mllvm -polly-opt-fusion=max"
                .to_string()
        } else {
            String::new()
        };
        build_env_vars.insert("GOATD_POLLY_FLAGS".to_string(), polly_flags.clone());

        if config.use_polly {
            eprintln!("[Build] [ORCHESTRATOR] Polly optimization enabled");
            eprintln!("[Build] [ORCHESTRATOR] Polly flags: {}", polly_flags);
        } else {
            eprintln!("[Build] [ORCHESTRATOR] Polly optimization disabled");
        }

        // MGLRU configuration
        if config.use_mglru {
            build_env_vars.insert("GOATD_USE_MGLRU".to_string(), "1".to_string());
            build_env_vars.insert(
                "GOATD_MGLRU_CONFIG_LRU_GEN".to_string(),
                "CONFIG_LRU_GEN=y".to_string(),
            );
            build_env_vars.insert(
                "GOATD_MGLRU_CONFIG_LRU_GEN_ENABLED".to_string(),
                "CONFIG_LRU_GEN_ENABLED=y".to_string(),
            );
            build_env_vars.insert(
                "GOATD_MGLRU_CONFIG_LRU_GEN_STATS".to_string(),
                "CONFIG_LRU_GEN_STATS=y".to_string(),
            );

            // CRITICAL FIX: Construct concatenated GOATD_MGLRU_CONFIGS string
            // Concatenate all LRU_GEN* config options into a single string for patcher
            // CRITICAL: Use newlines instead of spaces to properly separate entries
            let mut mglru_configs = String::new();
            mglru_configs.push_str("CONFIG_LRU_GEN=y");
            mglru_configs.push('\n');
            mglru_configs.push_str("CONFIG_LRU_GEN_ENABLED=y");
            mglru_configs.push('\n');
            mglru_configs.push_str("CONFIG_LRU_GEN_STATS=y");

            build_env_vars.insert("GOATD_MGLRU_CONFIGS".to_string(), mglru_configs.clone());
            eprintln!(
                "[Build] [ORCHESTRATOR] Constructed GOATD_MGLRU_CONFIGS: {}",
                mglru_configs
            );
        } else {
            build_env_vars.insert("GOATD_USE_MGLRU".to_string(), "0".to_string());
        }

        // Build config_options for KernelPatcher
        let config_options = config.config_options.clone();

        // Instantiate KernelPatcher
        use crate::kernel::patcher::KernelPatcher;
        let patcher = KernelPatcher::new(self.kernel_path.clone());

        // =========================================================================
        // NOTE: GPU LTO shielding logic has been MOVED TO THE FINALIZER
        // =========================================================================
        // The Finalizer (config::finalizer) is now the authoritative source for
        // determining which GPU modules need LTO shielding. The finalized config
        // contains the resolved lto_shield_modules field.
        let shield_modules = config.lto_shield_modules.clone();
        eprintln!(
            "[Build] [ORCHESTRATOR] Using {} LTO-shielded modules from Finalizer: {:?}",
            shield_modules.len(),
            shield_modules
        );

        // Execute the unified patcher with finalized shield modules
        match patcher.execute_full_patch_with_env(shield_modules, config_options, build_env_vars) {
            Ok(()) => {
                eprintln!(
                    "[Build] [SUCCESS] Unified patcher completed all PKGBUILD and .config patches"
                );
                self.record_patch_result(true).await;
            }
            Err(e) => {
                eprintln!(
                    "[Build] [WARNING] Patcher error: {:?}, continuing with build anyway",
                    e
                );
                // Don't fail the entire build - the .config and PKGBUILD may still be usable
                self.record_patch_result(false).await;
            }
        }

        // Update progress: Patching phase is 8-10%
        let progress = 10;
        self.set_progress(progress).await;
        eprintln!(
            "[Build] [PROGRESS] Unified patching complete: {}%",
            progress
        );

        // Transition to Building phase
        self.transition_phase(BuildPhaseState::Building).await
    }

    /// Executes kernel build, maps progress to orchestration state.
    pub async fn build(&self) -> Result<()> {
        // Send status update at phase start
        self.send_status("Building: Starting kernel compilation...".to_string())
            .await;

        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Building {
            return Err("Not in Building phase".into());
        }

        let config = state.config.clone();
        drop(state);

        eprintln!("[Build] [ORCHESTRATOR] Starting build phase with logging enabled");

        // Clone self for use in callback and pass the actual cancellation receiver
        let orch = self.clone();
        let cancel_rx = self.cancel_rx.clone();

        // CRITICAL FIX: Create callback that properly forwards logs to channel
        // The callback is called synchronously from executor's event loop
        // CRITICAL: The callback is the FINAL log pipe before UI - must be exempt from INITIALIZING gate
        let callback_fn = move |output: String, progress: Option<u32>| {
            // CRITICAL: Send to UI channel with non-blocking semantics
            // Only major status updates go to UI; raw logs go to LogCollector
            if let Some(ref tx) = orch.build_tx {
                // Detect if line is a high-level status message (compilation status)
                let is_status = output.starts_with("Compiling:")
                    || output.starts_with("Linking:")
                    || output.starts_with("Building:")
                    || output.starts_with("Linking vmlinux")
                    || output.starts_with("Compiling");

                // Only emit StatusUpdate for high-level status lines
                // Raw logs go to LogCollector (via main.rs bridge)
                if is_status {
                    // try_send fails only if receiver dropped or buffer full
                    match tx.try_send(crate::ui::controller::BuildEvent::StatusUpdate(output)) {
                        Ok(()) => {
                            // Event successfully queued for UI delivery
                        }
                        Err(_) => {
                            // Receiver was dropped or buffer full - log to stderr as final fallback
                            eprintln!("[Build] [LOG-PIPE] WARNING: Receiver dropped or buffer full, status update lost to disk fallback");
                        }
                    }
                }
            }

            // Update progress if available
            // Map build progress 0-100% to orchestration progress 10-90%
            if let Some(build_progress) = progress {
                let orchestration_progress = 10 + (build_progress * 80 / 100);
                eprintln!(
                    "[Build] [PROGRESS] Building: {}% (orchestration: {}%)",
                    build_progress, orchestration_progress
                );

                // CRITICAL: Use try_write for immediate progress updates
                // This avoids spawning async tasks that might be lost
                if let Ok(mut state) = orch.state.try_write() {
                    state.set_progress(orchestration_progress);
                } else {
                    // If lock is held, spawn a task to update eventually
                    let state_clone = orch.state.clone();
                    tokio::spawn(async move {
                        state_clone
                            .write()
                            .await
                            .set_progress(orchestration_progress);
                    });
                }

                // CRITICAL: Emit Progress event to UI channel for real-time UI updates
                if let Some(ref tx) = orch.build_tx {
                    let progress_f32 = (orchestration_progress as f32) / 100.0;
                    let _ = tx.try_send(crate::ui::controller::BuildEvent::Progress(progress_f32));
                }
            }
        };

        // CRITICAL: Call the real build executor with logging callback and timeout
        eprintln!("[Build] [EXECUTOR] Launching kernel build process");
        executor::run_kernel_build(
            &self.kernel_path,
            &config,
            callback_fn,
            cancel_rx,
            self.log_collector.clone(),
            self.test_timeout,
        )
        .await?;
        eprintln!("[Build] [EXECUTOR] Kernel build process completed");

        // Transition to Validation phase
        self.transition_phase(BuildPhaseState::Validation).await
    }

    /// Kernel build with output streaming callbacks.
    pub async fn build_with_output<F, G>(
        &self,
        mut output_handler: F,
        timer_handler: G,
    ) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
        G: FnMut(u64) + Send + 'static,
    {
        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Building {
            return Err("Not in Building phase".into());
        }

        let config = state.config.clone();
        drop(state);

        eprintln!("[Build] [ORCHESTRATOR] Starting build_with_output phase with logging enabled");

        // Use the actual cancellation receiver from UI
        let cancel_rx = self.cancel_rx.clone();

        // CRITICAL FIX: Capture self.state for progress updates in the callback
        let state_arc = self.state.clone();

        // Capture build_tx for emitting Progress events
        let build_tx_clone = self.build_tx.clone();

        // Spawn a timer task that emits elapsed seconds every second
        let timer_handle = {
            let timer_handler = std::sync::Arc::new(std::sync::Mutex::new(timer_handler));
            tokio::spawn(async move {
                let mut elapsed_seconds = 0u64;
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    elapsed_seconds += 1;
                    if let Ok(mut handler) = timer_handler.lock() {
                        handler(elapsed_seconds);
                    }
                }
            })
        };

        // Call the real build executor with output streaming and timeout
        executor::run_kernel_build(
            &self.kernel_path,
            &config,
            move |output, progress| {
                // CRITICAL FIX: Build/Log callbacks are EXEMPT from INITIALIZING gate
                // Pass output to provided handler WITHOUT diagnostic prefixes that confuse the log pipe
                output_handler(output);

                // CRITICAL FIX: Actually update progress if provided
                if let Some(build_progress) = progress {
                    eprintln!(
                        "[Build] [PROGRESS] Building: {}% (orchestration: {}%)",
                        build_progress,
                        10 + (build_progress * 80 / 100)
                    );
                    // Map build progress 0-100% to orchestration progress 10-90%
                    let orchestration_progress = 10 + (build_progress * 80 / 100);

                    // Use try_write for immediate updates, fallback to async task
                    if let Ok(mut state) = state_arc.try_write() {
                        state.set_progress(orchestration_progress);
                    } else {
                        // If lock is held, spawn async task to update eventually
                        let state_clone = state_arc.clone();
                        tokio::spawn(async move {
                            state_clone
                                .write()
                                .await
                                .set_progress(orchestration_progress);
                        });
                    }

                    // Emit Progress event to UI channel for real-time UI updates
                    if let Some(ref tx) = build_tx_clone {
                        let progress_f32 = (orchestration_progress as f32) / 100.0;
                        let _ =
                            tx.try_send(crate::ui::controller::BuildEvent::Progress(progress_f32));
                    }
                }
            },
            cancel_rx,
            self.log_collector.clone(),
            self.test_timeout,
        )
        .await?;

        // Cancel the timer task when build completes
        timer_handle.abort();

        eprintln!("[Build] [ORCHESTRATOR] build_with_output phase completed");
        // Transition to Validation phase
        self.transition_phase(BuildPhaseState::Validation).await
    }

    /// Verifies kernel artifacts exist and transitions to Completed.
    pub async fn validate(&self) -> Result<()> {
        // Validate phase
        let state = self.state.read().await;
        if state.phase != BuildPhaseState::Validation {
            return Err("Not in Validation phase".into());
        }

        let config = state.config.clone();
        drop(state);

        // Call the executor's validation function to verify config
        executor::validate_kernel_build(&config)?;

        // =========================================================================
        // DELEGATE ARTIFACT DISCOVERY TO PATCHER (Patcher Exclusivity)
        // =========================================================================
        // Only the Patcher should search for kernel artifacts.
        // This ensures the Patcher has exclusive authority over filesystem access.
        use crate::kernel::patcher::KernelPatcher;
        let patcher = KernelPatcher::new(self.kernel_path.clone());
        let artifacts = patcher
            .find_build_artifacts()
            .map_err(|e| format!("Failed to discover build artifacts: {:?}", e))?;

        if artifacts.is_empty() {
            return Err("Kernel image not found after build: expected *.pkg.tar.zst or arch/x86/boot/bzImage".into());
        }

        eprintln!(
            "[Build] [VALIDATION] Found {} build artifacts",
            artifacts.len()
        );

        // Verify LTO was applied if requested
        if config.lto_type != crate::models::LtoType::None {
            // In a real implementation, would check ELF sections or symbols
            // For now, log that LTO verification would happen here
            // This could check for specific symbols like __lto_ref_* or section info
        }

        // Update progress: Validation phase is 95%
        let progress = 95;
        self.set_progress(progress).await;
        eprintln!("[Build] [PROGRESS] Validation complete: {}%", progress);

        // CRITICAL FIX: Transition directly to Completed after Validation
        // Installation is now handled by the Kernel Manager, NOT the build pipeline
        eprintln!("[Build] [INFO] Build process complete. Packages ready for installation via Kernel Manager.");
        self.transition_phase(BuildPhaseState::Completed).await
    }

    /// Install the built kernel packages during the Installation phase.
    ///
    /// DEPRECATED: This method is superseded by AppController::install_kernel_async()
    /// which uses the unified batch_privileged_commands() flow for Polkit caching.
    ///
    /// DO NOT USE THIS METHOD. Instead, the AppController handles all installation
    /// through a unified privilege escalation pipeline to eliminate double prompts.
    ///
    /// The installation flow is now:
    /// 1. Kernel + Headers bundled into single pacman -U call (batch_privileged_commands)
    /// 2. DKMS autoinstall (separate command, same batch session for Polkit caching)
    /// 3. Symlink fallback (if needed, also via batch_privileged_commands)
    ///
    /// This ensures Polkit can cache authorization across all steps, preventing
    /// multiple sudo/pkexec prompts.
    #[deprecated(
        since = "3.0.0",
        note = "Use AppController::install_kernel_async() instead"
    )]
    pub async fn install(&self) -> Result<()> {
        eprintln!(
            "[Build] [INSTALL] ⚠ DEPRECATED: Direct AsyncOrchestrator::install() is no longer used"
        );
        eprintln!("[Build] [INSTALL] ℹ Installation is now handled by AppController::install_kernel_async()");
        eprintln!(
            "[Build] [INSTALL] ℹ This ensures unified privilege escalation with Polkit caching"
        );

        // Transition directly to Completed without doing anything
        // The actual installation will be triggered by the UI Controller
        self.transition_phase(BuildPhaseState::Completed).await
    }

    /// Generates bash script for MGLRU runtime tuning.
    pub async fn generate_mglru_runtime_tuning(&self) -> String {
        let state = self.state.read().await;
        let config = &state.config;

        if !config.use_mglru {
            return String::new();
        }

        let mut tuning = String::new();
        tuning.push_str("#!/bin/bash\n");
        tuning.push_str("# MGLRU (Multi-Gen LRU) Runtime Tuning Script\n");
        tuning.push_str("# Generated by GOATd Kernel Build System\n\n");

        // Use finalized MGLRU parameters from config
        let enabled_mask = config.mglru_enabled_mask;
        let min_ttl_ms = config.mglru_min_ttl_ms;

        if enabled_mask > 0 {
            tuning.push_str("# Enable MGLRU subsystem\n");
            tuning.push_str(&format!(
                "echo {} > /sys/module/lru_gen/parameters/lru_gen_enabled\n",
                enabled_mask
            ));
            tuning.push_str(&format!(
                "echo {} > /sys/module/lru_gen/parameters/lru_gen_min_ttl_ms\n",
                min_ttl_ms
            ));
            tuning.push_str("\n# Verify MGLRU is enabled\n");
            tuning.push_str("cat /sys/module/lru_gen/parameters/lru_gen_enabled\n");

            eprintln!("[Build] [MGLRU] Generated runtime tuning for {} profile: enabled=0x{:04x}, min_ttl_ms={}",
                config.profile, enabled_mask, min_ttl_ms);
        } else {
            tuning.push_str("# MGLRU disabled for this build\n");
            tuning.push_str("echo 0 > /sys/module/lru_gen/parameters/lru_gen_enabled\n");
            eprintln!(
                "[Build] [MGLRU] MGLRU disabled for {} profile",
                config.profile
            );
        }

        tuning
    }

    /// Executes all 5 phases sequentially.
    pub async fn run(&self) -> Result<()> {
        self.prepare().await?;
        self.configure().await?;
        self.patch().await?;
        self.build().await?;
        self.validate().await?;
        // CRITICAL FIX: Installation removed - deferred to Kernel Manager
        Ok(())
    }

    /// Get a snapshot of the current orchestration state for inspection/serialization.
    pub async fn state_snapshot(&self) -> OrchestrationState {
        self.state.read().await.clone()
    }

    /// Enable or disable recovery from checkpoints.
    pub fn set_recovery_enabled(&mut self, enabled: bool) {
        self.recovery_enabled = enabled;
    }

    /// Check if recovery from checkpoints is enabled.
    pub fn is_recovery_enabled(&self) -> bool {
        self.recovery_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_orchestrator_creation() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 16,
            disk_free_gb: 100,
            gpu_vendor: crate::models::GpuVendor::Nvidia,
            gpu_model: "Test GPU".to_string(),
            storage_type: crate::models::StorageType::Nvme,
            storage_model: "Test Storage".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            lto_type: crate::models::LtoType::Thin,
            use_modprobed: true,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
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
        };

        let (_, cancel_rx) = tokio::sync::watch::channel(false);
        let orch = AsyncOrchestrator::new(
            hw,
            config,
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/kernel-source"),
            None,
            cancel_rx,
            None,
            None,
            None,
        )
        .await;
        assert!(orch.is_ok());

        let orch = orch.unwrap();
        assert_eq!(orch.current_phase().await, BuildPhaseState::Preparation);
        assert_eq!(orch.current_progress().await, 0);
    }

    #[tokio::test]
    async fn test_progress_tracking() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 16,
            disk_free_gb: 100,
            gpu_vendor: crate::models::GpuVendor::Nvidia,
            gpu_model: "Test GPU".to_string(),
            storage_type: crate::models::StorageType::Nvme,
            storage_model: "Test Storage".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            lto_type: crate::models::LtoType::Thin,
            use_modprobed: true,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
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
        };

        let (_, cancel_rx) = tokio::sync::watch::channel(false);
        let orch = AsyncOrchestrator::new(
            hw,
            config,
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/kernel-source"),
            None,
            cancel_rx,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        orch.set_progress(50).await;
        assert_eq!(orch.current_progress().await, 50);

        orch.set_progress(150).await;
        assert_eq!(orch.current_progress().await, 100); // Clamped to 100
    }

    #[tokio::test]
    async fn test_phase_transitions() {
        let hw = crate::models::HardwareInfo {
            cpu_model: "Test CPU".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 16,
            disk_free_gb: 100,
            gpu_vendor: crate::models::GpuVendor::Nvidia,
            gpu_model: "Test GPU".to_string(),
            storage_type: crate::models::StorageType::Nvme,
            storage_model: "Test Storage".to_string(),
            boot_type: crate::models::BootType::Efi,
            boot_manager: crate::models::BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: crate::models::InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        };
        let config = crate::models::KernelConfig {
            version: "6.6.0".to_string(),
            lto_type: crate::models::LtoType::Thin,
            use_modprobed: true,
            use_whitelist: false,
            driver_exclusions: vec![],
            config_options: std::collections::HashMap::new(),
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
        };

        let (_, cancel_rx) = tokio::sync::watch::channel(false);
        let orch = AsyncOrchestrator::new(
            hw,
            config,
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/kernel-source"),
            None,
            cancel_rx,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert!(orch
            .transition_phase(BuildPhaseState::Configuration)
            .await
            .is_ok());
        assert_eq!(orch.current_phase().await, BuildPhaseState::Configuration);

        // Invalid transition should fail
        assert!(orch
            .transition_phase(BuildPhaseState::Validation)
            .await
            .is_err());
    }
}

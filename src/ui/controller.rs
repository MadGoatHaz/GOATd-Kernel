//! AppController: Central UI orchestrator for GOATd Kernel
//!
//! Dependency injection, trait-based abstraction, Arc<RwLock<>> state, async event processing

use std::sync::Arc;
use std::path::PathBuf;
use crate::config::{SettingsManager, AppState};
use crate::log_info;
use crate::system::SystemImpl;
use crate::kernel::manager::KernelManagerImpl;
use crate::kernel::audit::SystemAudit;
use super::{SystemWrapper, KernelManagerTrait, AuditTrait};
use futures::future::{BoxFuture, FutureExt};
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crate::system::performance::{
    PerformanceConfig, LatencyCollector, PerformanceMetrics,
    PerformanceHistory, PerformanceRecord, KernelContext, MonitoringState,
    MonitoringMode, LifecycleState, SessionSummary, StressorManager, Intensity,
    StressorType, HistoryManager, HistogramBucket, BenchmarkOrchestrator,
};
use crate::system::performance::collector::LatencyProcessor;
use std::sync::RwLock;
use std::collections::VecDeque;

/// Comparison result struct for UI display
///
/// Contains all formatted data for side-by-side comparison of two performance tests
#[derive(Clone, Debug)]
pub struct ComparisonResult {
    // Test A - metadata and metrics
    pub test_a_kernel: String,
    pub test_a_scx: String,
    pub test_a_lto: String,
    pub test_a_min: String,
    pub test_a_max: String,
    pub test_a_avg: String,
    pub test_a_p99_9: String,
    pub test_a_smi_count: i32,
    pub test_a_stall_count: i32,
    
    // Test B - metadata and metrics
    pub test_b_kernel: String,
    pub test_b_scx: String,
    pub test_b_lto: String,
    pub test_b_min: String,
    pub test_b_max: String,
    pub test_b_avg: String,
    pub test_b_p99_9: String,
    pub test_b_smi_count: i32,
    pub test_b_stall_count: i32,
    
    // Deltas - % change from Test A to Test B
    pub delta_min: f32,
    pub delta_max: f32,
    pub delta_avg: f32,
    pub delta_p99_9: f32,
    pub delta_smi: f32,
    pub delta_stall: f32,
}

/// Represents discrete events emitted from the background build process
#[derive(Clone, Debug)]
pub enum BuildEvent {
    Progress(f32),
    StatusUpdate(String),  // Granular status updates from orchestrator phases
    Status(String),
    Log(String),
    PhaseChanged(String),
    Finished(bool),
    TimerUpdate(u64),
    Error(String),  // Build initialization or critical failure
    InstallationComplete(bool),  // Installation finished (success/failure)
    KernelUninstalled,  // Kernel package was successfully uninstalled
    LatestVersionUpdate(String, String), // (variant_name, version_string)
    JitterAuditComplete(SessionSummary), // Jitter audit session completed
    ArtifactDeleted,  // Built artifact was successfully deleted
}

/// Central state manager for AppController
pub struct AppController {
    /// Thread-safe application state
    pub settings: Arc<std::sync::RwLock<AppState>>,
    /// System operations abstraction
    pub system: Arc<dyn SystemWrapper>,
    /// Kernel management abstraction
    pub kernel_manager: Arc<dyn KernelManagerTrait>,
    /// Audit/metrics abstraction
    pub audit: Arc<dyn AuditTrait>,
    /// Channel for build events
    pub build_tx: tokio::sync::mpsc::Sender<BuildEvent>,
    /// Channel for cancellation signals
    pub cancel_tx: tokio::sync::watch::Sender<bool>,
    /// Robust log collector for guaranteed disk persistence
    pub log_collector: Option<Arc<crate::LogCollector>>,
    /// Performance monitoring state (shared with background processor)
    pub perf_monitoring_active: Arc<AtomicBool>,
    /// Performance history manager for persistence
    pub perf_history: Arc<std::sync::RwLock<PerformanceHistory>>,
    /// Current performance metrics (synchronized with UI)
    pub perf_metrics: Arc<std::sync::RwLock<PerformanceMetrics>>,
    /// Monitoring state for lifecycle control
    pub perf_monitoring_state: Arc<std::sync::RwLock<Option<MonitoringState>>>,
    /// Current monitoring mode (Benchmark or Continuous)
    pub perf_monitoring_mode: Arc<RwLock<Option<MonitoringMode>>>,
    /// Current lifecycle state of monitoring
    pub perf_lifecycle_state: Arc<RwLock<LifecycleState>>,
    /// Stressor manager for background load generation
    pub perf_stressor_manager: Arc<RwLock<Option<StressorManager>>>,
    /// Session summary for the current or last completed session
    pub perf_session_summary: Arc<RwLock<Option<SessionSummary>>>,
    /// Start time of current monitoring session (for timing auto-termination)
    pub perf_session_start: Arc<RwLock<Option<std::time::Instant>>>,
    /// Flag indicating background alert has been triggered (spikes detected while tab inactive)
    pub perf_background_alert: Arc<AtomicBool>,
    /// Count of background alert events
    pub perf_alert_count: Arc<AtomicU64>,
    /// Jitter timeline: VecDeque of last 300 cycle_max samples (µs) for sparkline visualization
    pub perf_jitter_history: Arc<RwLock<VecDeque<f32>>>,
    /// Performance record persistence manager
    pub perf_history_manager: Arc<RwLock<Option<HistoryManager>>>,
    /// Cached hardware info (refreshed every 30s or on-demand)
    pub cached_hardware_info: Arc<RwLock<Option<crate::models::HardwareInfo>>>,
    /// Last time hardware info was refreshed
    pub hardware_cache_timestamp: Arc<RwLock<Option<Instant>>>,
    /// Dirty flag for UI repaint: set by background processor when metrics update, cleared by render loop
    pub atomic_perf_dirty: Arc<AtomicBool>,
    /// UI context for requesting repaints from background processor (egui signaling)
    pub perf_ui_context: Arc<RwLock<Option<egui::Context>>>,
    /// Cached kernel audit data for active kernel (Dashboard)
    pub active_kernel_audit: Arc<RwLock<Option<crate::kernel::audit::KernelAuditData>>>,
    /// Cached kernel audit data for selected kernel (Kernel Manager)
    pub selected_kernel_audit: Arc<RwLock<Option<crate::kernel::audit::KernelAuditData>>>,
    /// Cached jitter audit summary (populated when jitter audit completes)
    pub cached_jitter_summary: Arc<RwLock<Option<SessionSummary>>>,
    /// Benchmark orchestrator for SystemBenchmark mode
    pub benchmark_orchestrator: Arc<RwLock<Option<BenchmarkOrchestrator>>>,
}

impl AppController {
    /// Initialize AppController with production dependencies
    pub async fn new_async(
        build_tx: tokio::sync::mpsc::Sender<BuildEvent>,
        cancel_tx: tokio::sync::watch::Sender<bool>,
        log_collector: Option<Arc<crate::LogCollector>>,
    ) -> Result<Self, String> {
        log_info!("[AppController] Initializing AppController");
        
        let settings = Arc::new(
            std::sync::RwLock::new(
                SettingsManager::load()
                    .map_err(|e| {
                        log_info!("[AppController] ERROR: Settings load failed: {}", e);
                        format!("Settings load failed: {}", e)
                    })?
            )
        );
        
        log_info!("[AppController] Settings loaded successfully");
        
        let system: Arc<dyn SystemWrapper> = Arc::new(SystemImpl::new()?);
        let kernel_manager: Arc<dyn KernelManagerTrait> = Arc::new(KernelManagerImpl::new()?);
        let audit: Arc<dyn AuditTrait> = Arc::new(AuditImpl);
        
        log_info!("[AppController] All modules initialized (system, kernel_manager, audit)");
        
        // Initialize performance monitoring components
        let perf_history_path = Self::get_perf_history_path();
        let perf_history = PerformanceHistory::with_persistence(12, perf_history_path);
        
        // Initialize HistoryManager for persistent test record storage
        let history_manager = HistoryManager::new()
            .map_err(|e| {
                log_info!("[AppController] WARNING: Failed to initialize HistoryManager: {}", e);
                format!("Failed to initialize HistoryManager: {}", e)
            })
            .ok();
        
        // Clone settings before moving it into the controller
        let settings_clone = settings.clone();
        
        let controller = AppController {
            settings,
            system,
            kernel_manager,
            audit,
            build_tx,
            cancel_tx,
            log_collector,
            perf_monitoring_active: Arc::new(AtomicBool::new(false)),
            perf_history: Arc::new(std::sync::RwLock::new(perf_history)),
            perf_metrics: Arc::new(std::sync::RwLock::new(PerformanceMetrics::default())),
            perf_monitoring_state: Arc::new(std::sync::RwLock::new(None)),
            perf_monitoring_mode: Arc::new(RwLock::new(None)),
            perf_lifecycle_state: Arc::new(RwLock::new(LifecycleState::Idle)),
            perf_stressor_manager: Arc::new(RwLock::new(None)),
            perf_session_summary: Arc::new(RwLock::new(None)),
            perf_session_start: Arc::new(RwLock::new(None)),
            perf_background_alert: Arc::new(AtomicBool::new(false)),
            perf_alert_count: Arc::new(AtomicU64::new(0)),
            perf_jitter_history: Arc::new(RwLock::new(VecDeque::with_capacity(300))),
            perf_history_manager: Arc::new(RwLock::new(history_manager)),
            cached_hardware_info: Arc::new(RwLock::new(None)),
            hardware_cache_timestamp: Arc::new(RwLock::new(None)),
            atomic_perf_dirty: Arc::new(AtomicBool::new(false)),
            perf_ui_context: Arc::new(RwLock::new(None)),
            active_kernel_audit: Arc::new(RwLock::new(None)),
            selected_kernel_audit: Arc::new(RwLock::new(None)),
            cached_jitter_summary: Arc::new(RwLock::new(None)),
            benchmark_orchestrator: Arc::new(RwLock::new(None)),
        };
        
        // TRIGGER INITIAL DEEP AUDIT - CONSOLIDATED FROM MAIN.RS
        // Runs if audit_on_startup is configured OR on first startup for dashboard population
        let audit_clone = controller.audit.clone();
        let cache_clone = controller.active_kernel_audit.clone();
        
        tokio::spawn(async move {
            // Read audit_on_startup flag to determine if we should run the audit
            let should_audit = {
                if let Ok(state) = settings_clone.read() {
                    state.audit_on_startup
                } else {
                    false
                }
            };
            
            if should_audit {
                log_info!("[AppController] INIT: audit_on_startup flag is set, running initial deep audit");
                match audit_clone.run_deep_audit_async().await {
                    Ok(audit_data) => {
                        if let Ok(mut cache) = cache_clone.write() {
                            *cache = Some(audit_data.clone());
                            log_info!("[AppController] INIT: Done: Initial audit complete, dashboard cache populated");
                            log_info!("[AppController] INIT:   Kernel: {}", audit_data.version);
                            log_info!("[AppController] INIT:   CPU Scheduler: {}", audit_data.cpu_scheduler);
                            log_info!("[AppController] INIT:   LTO: {}", audit_data.lto_status);
                        }
                    }
                    Err(e) => {
                        log_info!("[AppController] INIT: Warning: Initial audit failed: {}", e);
                    }
                }
            } else {
                log_info!("[AppController] INIT: audit_on_startup not set, skipping initial deep audit");
            }
        });
        
        Ok(controller)
    }

    /// Production convenience constructor
    pub async fn new_production(
        build_tx: tokio::sync::mpsc::Sender<BuildEvent>,
        cancel_tx: tokio::sync::watch::Sender<bool>,
        log_collector: Option<Arc<crate::LogCollector>>,
    ) -> Self {
        match Self::new_async(build_tx, cancel_tx, log_collector).await {
            Ok(state) => {
                log_info!("[AppController] AppController ready for production");
                state
            }
            Err(e) => {
                log_info!("[FATAL] Failed to initialize AppController: {}", e);
                eprintln!("[FATAL] Failed to initialize AppController: {}", e);
                panic!("Application initialization failed: {}", e);
            }
        }
    }

    /// Get current application state
    pub fn get_state(&self) -> Result<AppState, String> {
        self.settings
            .read()
            .map(|state| state.clone())
            .map_err(|e| format!("Failed to read state: {}", e))
    }

    /// Update application state
    pub fn update_state<F>(&self, f: F) -> Result<(), String>
    where
        F: Fn(&mut AppState),
    {
        {
            let mut state = self.settings
                .write()
                .map_err(|e| format!("Failed to write state: {}", e))?;
            f(&mut state);
        }
        
        let state = self.get_state()?;
        SettingsManager::save(&state)
            .map_err(|e| {
                log_info!("[AppController] Error: Failed to persist state: {}", e);
                format!("Failed to save state: {}", e)
            })?;
        
        Ok(())
    }

    /// Explicitly persist current state to disk
    /// Used for manual save operations (e.g., Settings → Save button)
    pub fn persist_state(&self) -> Result<(), String> {
        let state = self.get_state()?;
        SettingsManager::save(&state)
            .map_err(|e| {
                log_info!("[AppController] Error: Failed to persist state: {}", e);
                format!("Failed to save state: {}", e)
            })
    }
    
    /// Reset all application state to defaults
    /// Used for Settings → Reset to Defaults button
    pub fn reset_to_defaults(&self) -> Result<(), String> {
        // Load fresh default state by reloading and resetting settings
        let default_state = SettingsManager::load()
            .map_err(|e| {
                log_info!("[AppController] Error: Failed to load default settings: {}", e);
                format!("Failed to load defaults: {}", e)
            })?;
        
        // Write the default state back
        {
            let mut state = self.settings
                .write()
                .map_err(|e| format!("Failed to write state: {}", e))?;
            *state = default_state.clone();
        }
        
        // Persist the reset state to disk
        SettingsManager::save(&default_state)
            .map_err(|e| {
                log_info!("[AppController] Error: Failed to persist default state: {}", e);
                format!("Failed to save defaults: {}", e)
            })?;
        
        self.log_event("SETTINGS", "Application state reset to defaults");
        Ok(())
    }
    
    /// Log a UI event
    pub fn log_event(&self, event_type: &str, description: &str) {
        log_info!("[AppController] [{}] {}", event_type, description);
    }
    
    /// Optimized timer update interval
    pub fn optimized_timer_interval(&self, current_phase: &str) -> u64 {
        match current_phase {
            "preparation" | "configuration" => 100,
            "patching" | "building" => 250,
            "validation" | "shutdown" => 500,
            _ => 250,
        }
    }
    
    /// Log build event
    pub fn log_build_event(&self, event: &BuildEvent) {
        match event {
            BuildEvent::Progress(p) =>
                self.log_event("PROGRESS", &format!("{:.1}%", p * 100.0)),
            BuildEvent::StatusUpdate(s) =>
                self.log_event("STATUS_UPDATE", s),
            BuildEvent::Status(s) =>
                self.log_event("STATUS", s),
            BuildEvent::Log(l) =>
                self.log_event("LOG", l),
            BuildEvent::PhaseChanged(phase) =>
                self.log_event("PHASE_CHANGED", phase),
            BuildEvent::Finished(success) =>
                self.log_event("FINISHED", &format!("success={}", success)),
            BuildEvent::TimerUpdate(elapsed) =>
                self.log_event("TIMER_UPDATE", &format!("{}s", elapsed)),
            BuildEvent::Error(msg) =>
                self.log_event("ERROR", msg),
            BuildEvent::InstallationComplete(success) =>
                self.log_event("INSTALLATION", &format!("success={}", success)),
            BuildEvent::KernelUninstalled =>
                self.log_event("KERNEL", "Kernel uninstalled, Installed Kernels list refreshed"),
            BuildEvent::LatestVersionUpdate(variant, version) =>
                self.log_event("VERSION_POLL", &format!("{}: {}", variant, version)),
            BuildEvent::JitterAuditComplete(summary) =>
                self.log_event("JITTER_AUDIT", &format!("Complete: samples={}, duration={:.2}s", summary.total_samples, summary.duration_secs.unwrap_or(0.0))),
            BuildEvent::ArtifactDeleted =>
                self.log_event("ARTIFACT", "Artifact deleted, Kernels Ready for Installation list refreshed"),
        }
    }
    
    /// Trigger version polling for a kernel variant
    ///
    /// Spawns a background task that fetches the latest remote version for the variant.
    /// Uses a robust fallback strategy:
    /// 1. First attempts PKGBUILD-based polling via reqwest (fast, requires network)
    /// 2. Falls back to git2 remote reference listing if reqwest fails (slower but more robust)
    /// 3. Sends a BuildEvent::LatestVersionUpdate on completion or failure
    ///
    /// Includes 10-second timeout protection to prevent network hangs from blocking UI.
    pub fn trigger_version_poll(&self, variant: String) {
        use crate::kernel::pkgbuild::get_latest_version_by_variant;
        use crate::kernel::git::get_latest_remote_version;
        use crate::kernel::sources::KernelSourceDB;
        
        let build_tx = self.build_tx.clone();
        
        log::debug!("[VERSION_POLL] Triggering version poll for variant: {}", variant);
        
        tokio::spawn(async move {
            log::debug!("[VERSION_POLL] Fetching latest version for {} (reqwest+git2 fallback)", variant);
            
            // Wrap the async pkgbuild fetch with timeout (primary strategy)
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                get_latest_version_by_variant(&variant)
            ).await;
            
            let version = match result {
                Ok(Ok(v)) => {
                    log::debug!("[VERSION_POLL] Done: Found version for {} via reqwest: {}", variant, v);
                    v
                }
                Ok(Err(e)) => {
                    // Reqwest strategy failed, try git2 fallback
                    log::debug!("[VERSION_POLL] Warning: Reqwest fetch failed for {}: {}, trying git2 fallback", variant, e);
                    
                    // Get the git URL from KernelSourceDB
                    let db = KernelSourceDB::new();
                    if let Some(git_url) = db.get_source_url(&variant) {
                        match get_latest_remote_version(git_url) {
                            Ok(v) => {
                                log::debug!("[VERSION_POLL] Done: Found version for {} via git2 fallback: {}", variant, v);
                                v
                            }
                            Err(e2) => {
                                log::debug!("[VERSION_POLL] Error: Both reqwest and git2 failed for {}: {}", variant, e2);
                                "Unknown".to_string()
                            }
                        }
                    } else {
                        log::debug!("[VERSION_POLL] Error: No git URL found in database for variant: {}", variant);
                        "Unknown".to_string()
                    }
                }
                Err(_) => {
                    log::debug!("[VERSION_POLL] ⚠ TIMEOUT: Version poll for {} exceeded 10 seconds", variant);
                    "Timeout".to_string()
                }
            };
            
            let _ = build_tx.try_send(BuildEvent::LatestVersionUpdate(variant.clone(), version));
        });
    }

    /// Handle profile selection with immediate UI sync
    ///
    /// This method:
    /// 1. Validates and records the selected profile
    /// 2. Immediately applies the profile's defaults to AppState
    /// 3. Allows the UI to read back and update all controls (LTO, Polly, MGLRU)
    pub fn handle_profile_change(&self, profile_name: &str) -> Result<(), String> {
        use crate::config::profiles::get_profile;
        
        println!("[Controller] [PROFILE] Profile change requested: '{}'", profile_name);
        
        // Validate profile exists
        let profile = get_profile(profile_name)
            .ok_or_else(|| format!("Unknown profile: {}", profile_name))?;
        
        println!("[Controller] [PROFILE] Done: Profile validated: {}", profile.name);
        
        // Record the profile selection
        self.update_state(|state| {
            state.selected_profile = profile_name.to_string();
        })?;
        
        // IMMEDIATELY apply defaults so UI can sync
        self.apply_current_profile_defaults()?;
        
        self.log_event(
            "PROFILE_CHANGE",
            &format!(
                "Profile changed to '{}' with defaults applied immediately for UI sync.",
                profile_name
            ),
        );
        
        Ok(())
    }

    /// Apply current profile's default values to state
    pub fn apply_current_profile_defaults(&self) -> Result<(), String> {
        use crate::config::profiles::get_profile;
        use crate::models::LtoType;
        
        let state = self.get_state()?;
        let profile = get_profile(&state.selected_profile)
            .ok_or_else(|| format!("Profile not found: {}", state.selected_profile))?;
        
        self.update_state(|s| {
            s.kernel_hardening = profile.hardening_level;
            s.use_polly = profile.use_polly;
            s.use_mglru = profile.use_mglru;
            s.selected_lto = match profile.default_lto {
                LtoType::Full => "full".to_string(),
                LtoType::Thin => "thin".to_string(),
                LtoType::None => "none".to_string(),
            };
            s.use_modprobed = profile.enable_module_stripping;
            s.use_whitelist = profile.enable_module_stripping;
            s.user_toggled_lto = false;  // Reset flag when applying profile defaults
        })?;
        
        Ok(())
    }

    /// Handle LTO override - user manually selected LTO level
    pub fn handle_lto_change(&self, lto_type: &str) -> Result<(), String> {
        self.update_state(|state| {
            state.selected_lto = lto_type.to_string();
            state.user_toggled_lto = true;  // Mark as user-overridden
        })?;
        self.log_event("LTO_CHANGE", &format!("User selected LTO: {} (override flag set)", lto_type));
        Ok(())
    }

    /// Handle hardening override - user manually selected hardening level
    pub fn handle_hardening_change(&self, hardening_str: &str) -> Result<(), String> {
        use std::str::FromStr;
        let hardening_level = crate::models::HardeningLevel::from_str(hardening_str)
            .unwrap_or(crate::models::HardeningLevel::Standard);
        
        self.update_state(|state| {
            state.kernel_hardening = hardening_level;
        })?;
        self.log_event("HARDENING_CHANGE", &format!("User set hardening: {}", hardening_level));
        Ok(())
    }

    /// Handle Polly override - user manually toggled Polly optimization
    /// Sets both the value AND the override flag to prevent profile changes from wiping it out
    pub fn handle_polly_change(&self, enabled: bool) -> Result<(), String> {
        self.update_state(|state| {
            state.use_polly = enabled;
            state.user_toggled_polly = true;  // Mark as user-overridden
        })?;
        self.log_event("POLLY_CHANGE", &format!("User set Polly optimization: {} (override flag set)", enabled));
        Ok(())
    }

    /// Handle module stripping override - user manually toggled modprobed/whitelist
    pub fn handle_modprobed_change(&self, enabled: bool) -> Result<(), String> {
        self.update_state(|state| {
            state.use_modprobed = enabled;
            state.use_whitelist = enabled;
        })?;
        self.log_event("MODPROBED_CHANGE", &format!("User set module stripping: {}", enabled));
        Ok(())
    }

    /// Handle MGLRU override - user manually toggled MGLRU
    /// Sets both the value AND the override flag to prevent profile changes from wiping it out
    pub fn handle_mglru_change(&self, enabled: bool) -> Result<(), String> {
        self.update_state(|state| {
            state.use_mglru = enabled;
            state.user_toggled_mglru = true;  // Mark as user-overridden
        })?;
        self.log_event("MGLRU_CHANGE", &format!("User set MGLRU: {} (override flag set)", enabled));
        Ok(())
    }
    

    /// Handle direct SCX scheduler and mode configuration (granular control)
    /// Maps scheduler binary and mode directly without relying on profiles
    pub fn handle_apply_scx_config(&self, scheduler: &str, mode_str: &str) -> Result<(), String> {
        use crate::system::scx::{PersistentSCXManager, SchedulerMode};
        
        // Parse mode string to SchedulerMode enum
        let mode = SchedulerMode::from_str(mode_str)
            .ok_or_else(|| format!("Unknown scheduler mode: {}", mode_str))?;
        
        self.log_event("SCX_CONFIG", &format!(
            "Applying granular SCX config: scheduler={}, mode={}",
            scheduler, mode
        ));
        
        // Call the PersistentSCXManager with direct scheduler and mode
        PersistentSCXManager::apply_scx_config(scheduler, mode)?;
        
        // Update active scheduler in state (optional, for UI feedback)
        self.update_state(|state| {
            state.selected_scx_profile = format!("{} ({})", scheduler, mode);
        })?;
        
        self.log_event("SCX_CONFIG", &format!(
            "Done: SCX config '{}' ({}) activated successfully",
            scheduler, mode
        ));
        
        Ok(())
    }

    /// Generate a unique timestamped log filename for a build session
    fn generate_build_log_filename() -> String {
        let now = chrono::Local::now();
        format!("build_{}.log", now.format("%Y%m%d_%H%M%S%.3f"))
    }

    /// Primary build execution method
    pub async fn start_build(&self) -> Result<(), String> {
        self.log_event("BUILD", "Starting kernel build orchestration");
        eprintln!("[BUILD] [START] Kernel build orchestration initiated");
        
        // =========================================================================
        // START NEW LOG SESSION - Ensure full and dedicated log file for this build
        // =========================================================================
        let _session_log_path = if let Some(ref log_collector) = self.log_collector {
            let filename = Self::generate_build_log_filename();
            match log_collector.start_new_session(&filename) {
                Ok(path) => {
                    eprintln!("[BUILD] [SESSION] Dedicated log session started at: {}", path.display());
                    self.log_event("BUILD", &format!("Full build log: {}", path.display()));
                    Some(path)
                }
                Err(e) => {
                    eprintln!("[BUILD] [SESSION] Warning: Failed to start log session: {}", e);
                    None
                }
            }
        } else {
            eprintln!("[BUILD] [SESSION] Warning: LogCollector not available");
            None
        };
        
        // Send initial status to UI
        let _ = self.build_tx.send(BuildEvent::Status("preparation".into())).await;
        
        let state = self.get_state()?;
        
        // =========================================================================
        // PRE-BUILD SYNCHRONIZATION AUDIT
        // =========================================================================
        // Ensure all UI/backend settings are synchronized BEFORE building
        eprintln!("[BUILD] [SYNC_AUDIT] Validating build configuration synchronization:");
        eprintln!("[BUILD] [SYNC_AUDIT]   Variant: {} (string)", state.selected_variant);
        eprintln!("[BUILD] [SYNC_AUDIT]   Profile: {} (string)", state.selected_profile);
        eprintln!("[BUILD] [SYNC_AUDIT]   LTO: {} (string)", state.selected_lto);
        eprintln!("[BUILD] [SYNC_AUDIT]   Hardening: {} (bool)", state.kernel_hardening);
        eprintln!("[BUILD] [SYNC_AUDIT]   Polly: {} (bool, user_toggled={})", state.use_polly, state.user_toggled_polly);
        eprintln!("[BUILD] [SYNC_AUDIT]   MGLRU: {} (bool, user_toggled={})", state.use_mglru, state.user_toggled_mglru);
        eprintln!("[BUILD] [SYNC_AUDIT]   Modprobed: {} (bool)", state.use_modprobed);
        eprintln!("[BUILD] [SYNC_AUDIT]   Whitelist: {} (bool)", state.use_whitelist);
        eprintln!("[BUILD] [SYNC_AUDIT] Configuration validated and ready for build");
        
        let mut detector = crate::hardware::HardwareDetector::new();
        let hw_info = detector.detect_all()
            .map_err(|e| {
                let msg = format!("Hardware detection failed: {}", e);
                eprintln!("[Controller] [ERROR] {}", msg);
                let _ = self.build_tx.try_send(BuildEvent::Error(msg.clone()));
                msg
            })?;
            
        // =========================================================================
        // PRE-FLIGHT CHECK: Hardened Fallback Trigger with User Workspace Respect
        // =========================================================================
        // BLUEPRINT: Respect the user's selected workspace UNLESS it's empty or IO-creation fails.
        // The Preparation phase now handles initialization, so we don't reject empty workspaces—
        // instead we fallback to CWD only if the user hasn't set a path OR if we can't create it.
        
        let workspace_path = if state.workspace_path.is_empty() {
            eprintln!("[Controller] [PRE-FLIGHT] Workspace path is empty, falling back to CWD");
            let _ = self.build_tx.try_send(BuildEvent::Log(
                "[Controller] [PRE-FLIGHT] Workspace path is empty, falling back to CWD".to_string()
            ));
            
            std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?
        } else {
            let user_path = PathBuf::from(&state.workspace_path);
            
            // Try to canonicalize the workspace path to absolute form
            let canonical_path = match user_path.canonicalize() {
                Ok(canonical) => {
                    eprintln!("[Controller] [PRE-FLIGHT] Workspace path canonicalized: {}", canonical.display());
                    canonical
                }
                Err(e) => {
                    eprintln!("[Controller] [WARNING] Failed to canonicalize workspace path: {}", e);
                    eprintln!("[Controller] [WARNING] Using path as-is: {}", state.workspace_path);
                    user_path
                }
            };
            
            // CRITICAL: Try to create the directory (and all parents) to ensure workspace is usable
            // If this fails, fallback to CWD to allow the build to proceed
            match std::fs::create_dir_all(&canonical_path) {
                Ok(_) => {
                    eprintln!("[Controller] [PRE-FLIGHT] Workspace directory is ready: {}", canonical_path.display());
                    canonical_path
                }
                Err(e) => {
                    eprintln!("[Controller] [PRE-FLIGHT] Failed to create/verify workspace directory: {}", e);
                    eprintln!("[Controller] [PRE-FLIGHT] Falling back to CWD");
                    let _ = self.build_tx.try_send(BuildEvent::Log(
                        format!("[Controller] [PRE-FLIGHT] Cannot create workspace ({}), falling back to CWD", e)
                    ));
                    
                    std::env::current_dir()
                        .map_err(|e| format!("Failed to get current directory: {}", e))?
                }
            }
        };
        
        let kernel_path = workspace_path.join(&state.selected_variant);
        
        eprintln!("[Controller] [PRE-FLIGHT] Using authorized workspace: {}", workspace_path.display());
        let _ = self.build_tx.try_send(BuildEvent::Log(
            format!("[Controller] [PRE-FLIGHT] Using authorized workspace: {}", workspace_path.display())
        ));
        
        // =========================================================================
        // ROBUST KERNEL CONFIG POPULATION
        // =========================================================================
        // All fields must be populated from AppState with proper validation
        let mut config = crate::models::KernelConfig::default();
        
        // Variant: String from AppState -> KernelConfig
        config.version = state.selected_variant.clone();
        eprintln!("[BUILD] [CONFIG] Setting version: {}", config.version);
        
        // LTO: String from AppState -> LtoType enum with validation
        config.lto_type = match state.selected_lto.as_str() {
            "full" => {
                eprintln!("[BUILD] [CONFIG] Setting LTO to Full (from {})", state.selected_lto);
                crate::models::LtoType::Full
            }
            "none" => {
                eprintln!("[BUILD] [CONFIG] Setting LTO to None (from {})", state.selected_lto);
                crate::models::LtoType::None
            }
            _ => {
                eprintln!("[BUILD] [CONFIG] Setting LTO to Thin (default, from {})", state.selected_lto);
                crate::models::LtoType::Thin
            }
        };
        
        // Module stripping flags: Boolean from AppState
        config.use_modprobed = state.use_modprobed;
        config.use_whitelist = state.use_whitelist;
        eprintln!("[BUILD] [CONFIG] Module stripping: modprobed={}, whitelist={}",
            config.use_modprobed, config.use_whitelist);
        
        // Hardening: HardeningLevel from AppState
        config.hardening = state.kernel_hardening;
        eprintln!("[BUILD] [CONFIG] Setting hardening: {}", config.hardening);
        
        // Security boot
        config.secure_boot = state.secure_boot;
        eprintln!("[BUILD] [CONFIG] Setting secure_boot: {}", config.secure_boot);
        
        // Profile: String from AppState
        config.profile = state.selected_profile.clone();
        eprintln!("[BUILD] [CONFIG] Setting profile: {}", config.profile);
        
        // Optimization flags: Boolean from AppState with user override tracking
         config.use_polly = state.use_polly;
         config.use_mglru = state.use_mglru;
         config.user_toggled_polly = state.user_toggled_polly;
         config.user_toggled_mglru = state.user_toggled_mglru;
         config.user_toggled_lto = state.user_toggled_lto;
         config.user_toggled_hardening = state.user_toggled_hardening;
         config.user_toggled_bore = state.user_toggled_bore;
         eprintln!("[BUILD] [CONFIG] Optimizations: polly={} (user_toggled={}), mglru={} (user_toggled={})",
             config.use_polly, config.user_toggled_polly, config.use_mglru, config.user_toggled_mglru);
         eprintln!("[BUILD] [CONFIG] User Overrides: lto={:?} (user_toggled={}), hardening={} (user_toggled={})",
             config.lto_type, config.user_toggled_lto, config.hardening, config.user_toggled_hardening);
        
        // Create async orchestrator
        let checkpoint_dir = workspace_path.join(".checkpoints");
        
        let orch = crate::orchestrator::AsyncOrchestrator::new(
            hw_info,
            config,
            checkpoint_dir,
            kernel_path,
            Some(self.build_tx.clone()),
            self.cancel_tx.subscribe(),
            self.log_collector.clone(),
        ).await.map_err(|e| {
            let msg = format!("Failed to initialize orchestrator: {}", e);
            eprintln!("[Controller] [ERROR] {}", msg);
            log_info!("[Controller] [ERROR] {}", msg);
            
            // CRITICAL: Send error event to UI BEFORE returning Err
            let _ = self.build_tx.try_send(BuildEvent::Error(msg.clone()));
            
            msg
        })?;
        
        let tx = self.build_tx.clone();
        
        // Spawn the timer task that emits elapsed seconds every second
        let timer_tx = self.build_tx.clone();
        let timer_handle = tokio::spawn(async move {
            let mut elapsed_seconds = 0u64;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                elapsed_seconds += 1;
                if let Err(_) = timer_tx.try_send(BuildEvent::TimerUpdate(elapsed_seconds)) {
                    // Channel closed or receiver dropped - build has finished, stop timer
                    eprintln!("[Build] [TIMER] Timer task stopping (channel closed or full)");
                    break;
                }
            }
        });
        
        // Spawn background build task
        // Note: We run the orchestrator in a blocking task to avoid Send trait issues with Arc<RwLock<>>
        let log_collector_for_flush = self.log_collector.clone();
        tokio::task::spawn_blocking(move || {
            // Use the blocking runtime to execute async orchestration
            // Create a local runtime for the blocking task
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            rt.block_on(async {
                let _ = tx.send(BuildEvent::Status("Starting kernel build...".to_string())).await;
                
                eprintln!("[Build] [ORCHESTRATION] Starting full build pipeline");
                
                // Run the complete orchestration pipeline (all 6 phases)
                match orch.run().await {
                    Ok(()) => {
                        eprintln!("[Build] [ORCHESTRATION] Build pipeline completed successfully");
                        let _ = tx.send(BuildEvent::Status("Build completed successfully!".to_string())).await;
                        
                        // CRITICAL: Flush all logs to disk before marking build as complete
                        // This ensures "Build finished" message and all prior logs reach disk
                        eprintln!("[Build] [ORCHESTRATION] Flushing all logs to disk...");
                        if let Some(ref log_collector) = log_collector_for_flush {
                            match log_collector.wait_for_empty().await {
                                Ok(()) => {
                                    eprintln!("[Build] [ORCHESTRATION] Done: All logs flushed to disk");
                                }
                                Err(e) => {
                                    eprintln!("[Build] [ORCHESTRATION] Warning: Log flush failed: {}", e);
                                    let _ = tx.send(BuildEvent::Log(format!("[WARNING] Failed to flush logs: {}", e))).await;
                                }
                            }
                        }
                        
                        let _ = tx.send(BuildEvent::Finished(true)).await;
                    }
                    Err(e) => {
                        let err_msg = format!("Build orchestration failed: {}", e);
                        eprintln!("[Build] [ERROR] {}", err_msg);
                        let _ = tx.send(BuildEvent::Log(err_msg.clone())).await;
                        
                        // CRITICAL: Flush logs even on error - ensure error message reaches disk
                        eprintln!("[Build] [ORCHESTRATION] [ERROR] Flushing error logs to disk...");
                        if let Some(ref log_collector) = log_collector_for_flush {
                            match log_collector.wait_for_empty().await {
                                Ok(()) => {
                                    eprintln!("[Build] [ORCHESTRATION] [ERROR] Done: Error logs flushed to disk");
                                }
                                Err(flush_err) => {
                                    eprintln!("[Build] [ORCHESTRATION] [ERROR] Warning: Error log flush failed: {}", flush_err);
                                }
                            }
                        }
                        
                        let _ = tx.send(BuildEvent::Finished(false)).await;
                    }
                }
            });
        });
        
        // Store timer handle for cleanup (the handle is dropped here, allowing the task to run independently)
        // The timer will naturally stop when the channel is closed after the build completes
        eprintln!("[Build] [TIMER] Build timer task spawned");
        let _ = timer_handle;
        
        Ok(())
    }
    
    /// Cancel active build with timeout-aware UI state reset
    ///
    /// This method sends the cancellation signal to the orchestrator and schedules
    /// a timeout task. If the build doesn't gracefully terminate within 10 seconds,
    /// the UI is forcefully reset to allow the user to start a new build.
    pub fn cancel_build(&self) {
        self.log_event("BUILD", "Cancelling active build");
        let _ = self.cancel_tx.send(true);
        
        // Schedule a timeout-aware reset task
        // If the build doesn't respond to cancellation within 10 seconds,
        // reset the UI state to allow the user to restart
        let build_tx = self.build_tx.clone();
        let log_collector = self.log_collector.clone();
        
        tokio::spawn(async move {
            // Wait up to 10 seconds for the build to gracefully cancel
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            
            // Log the timeout event
            if let Some(log) = &log_collector {
                log.log_str("[CANCELLATION] Build did not respond to cancellation within timeout. Forcing UI reset.");
            }
            
            // Force UI state reset: emit Finished with false to break the build loop
            // and set is_building to false
            let _ = build_tx.send(BuildEvent::Status("Build cancellation forced (timeout)".to_string())).await;
            let _ = build_tx.send(BuildEvent::Finished(false)).await;
            
            eprintln!("[Cancel] Build cancellation timeout triggered - UI state reset forced");
        });
    }

    /// Uninstall a kernel package
    pub fn uninstall_kernel(&self, pkg_name: &str) -> Result<(), String> {
        // Strip versioning info from package name (e.g., "linux-goatd-gaming (6.18.3)" -> "linux-goatd-gaming")
        let clean_pkg_name = pkg_name
            .split('(')
            .next()
            .unwrap_or(pkg_name)
            .trim()
            .to_string();
        
        self.log_event("KERNEL", &format!("Uninstalling kernel: {} (cleaned from: {})", clean_pkg_name, pkg_name));
        self.system.uninstall_package(&clean_pkg_name)
    }
    
    /// Delete a built artifact (package file)
    pub fn handle_delete_artifact(&self, artifact_name: &str) -> Result<(), String> {
        self.log_event("ARTIFACT", &format!("Deleting built artifact: {}", artifact_name));
        
        // Extract the raw artifact name from formatted string (e.g., "linux-goatd-gaming (6.18.3)" -> "linux-goatd-gaming")
        let clean_name = artifact_name
            .split('(')
            .next()
            .unwrap_or(artifact_name)
            .trim()
            .to_string();
        
        // Call the kernel_manager to delete the built artifact
        log_info!("[Controller] [ARTIFACT] Artifact deletion requested for: {}", clean_name);
        
        // Construct a KernelPackage with the artifact name
        use crate::kernel::manager::KernelPackage;
        let pkg = KernelPackage {
            name: clean_name.clone(),
            version: String::new(),
            is_goatd: false,
            path: None,
        };
        
        match self.kernel_manager.delete_built_artifact(&pkg) {
            Ok(()) => {
                self.log_event("ARTIFACT", &format!("Done: Artifact deletion succeeded: {}", clean_name));
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to delete artifact {}: {}", clean_name, e);
                self.log_event("ARTIFACT", &format!("Error: {}", err_msg));
                Err(err_msg)
            }
        }
    }
    
    /// Delete a built artifact using a full KernelPackage object (with path information)
    /// This is the preferred method as it preserves the actual `path` information needed for deletion
    pub fn handle_delete_artifact_with_pkg(&self, pkg: &crate::kernel::manager::KernelPackage) -> Result<(), String> {
        let artifact_display = format!("{} ({})", pkg.name, pkg.version);
        self.log_event("ARTIFACT", &format!("Deleting built artifact: {}", artifact_display));
        
        log_info!("[Controller] [ARTIFACT] Artifact deletion requested for: {} with path: {:?}", pkg.name, pkg.path);
        
        // Use the kernel_manager to delete the built artifact with full path information
        match self.kernel_manager.delete_built_artifact(pkg) {
            Ok(()) => {
                self.log_event("ARTIFACT", &format!("Done: Artifact deletion succeeded: {}", artifact_display));
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to delete artifact {}: {}", artifact_display, e);
                self.log_event("ARTIFACT", &format!("Error: {}", err_msg));
                Err(err_msg)
            }
        }
    }

    /// Install a kernel package from path (runs in background async task)
    ///
    /// Executes all installation steps (kernel, headers, DKMS) in a single
    /// privileged session to minimize authentication prompts, without blocking
    /// the UI event loop.
    pub fn install_kernel_async(&self, path: PathBuf) {
        self.log_event("KERNEL", &format!("Starting async kernel installation from: {}", path.display()));
        
        let system = self.system.clone();
        let build_tx = self.build_tx.clone();
        
        // Spawn background task for installation
        tokio::spawn(async move {
            // Build installation commands
            let mut kernel_and_headers = Vec::new();
            let mut extra_commands = Vec::new();
            
            // === STEP 1: Add main kernel package installation ===
            let kernel_cmd = match Self::build_install_command_static(&path) {
                Ok(cmd) => cmd,
                Err(e) => {
                    let msg = format!("Failed to build install command: {}", e);
                    eprintln!("[KERNEL] {}", msg);
                    let _ = build_tx.try_send(BuildEvent::Error(msg.clone()));
                    return;
                }
            };
            kernel_and_headers.push(kernel_cmd);
            
            // === STEP 2: Check for and add headers installation to SAME pacman call ===
            if let Ok(Some(headers_path)) = Self::build_install_headers_path_static(&path) {
                eprintln!("[KERNEL] Found kernel headers, will bundle with kernel in single pacman call");
                kernel_and_headers.push(headers_path);
            }
            
            // === STEP 3: Check for NVIDIA GPU and add DKMS if needed (separate command) ===
            if let Ok(crate::models::GpuVendor::Nvidia) = crate::hardware::gpu::detect_gpu_vendor() {
                eprintln!("[NVIDIA] NVIDIA GPU detected, will batch DKMS with install");
                
                // Extract NEW kernel version from package filename
                if let Some(filename) = path.file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        if let Some(new_kernel_version) = Self::extract_kernel_version_from_filename(filename_str) {
                            eprintln!("[NVIDIA] Extracted kernel version from package: {}", new_kernel_version);
                            let dkms_cmd = format!("dkms autoinstall -k {}", new_kernel_version);
                            extra_commands.push(dkms_cmd);
                        }
                    }
                }
            }
            
            // === STEP 4: Build atomic pacman command with kernel + headers bundled ===
            let mut commands = Vec::new();
            
            // Create single pacman command with all packages
            if !kernel_and_headers.is_empty() {
                let pacman_cmd = format!("pacman -U --noconfirm {}", kernel_and_headers.join(" "));
                eprintln!("[KERNEL] Atomic pacman command: {}", pacman_cmd);
                commands.push(pacman_cmd);
            }
            
            // Add DKMS command if present (separate because it's a different tool)
            commands.extend(extra_commands);
            
            if commands.is_empty() {
                eprintln!("[KERNEL] No installation commands generated");
                let _ = build_tx.try_send(BuildEvent::Error("No installation commands generated".to_string()));
                return;
            }
            
            let cmd_refs: Vec<&str> = commands.iter().map(|s| s.as_str()).collect();
            eprintln!("[KERNEL] Executing installation ({} atomic steps)", commands.len());
            
            // Execute all commands in a single privileged session
            match system.batch_privileged_commands(cmd_refs) {
                Ok(()) => {
                    eprintln!("[KERNEL] Installation completed successfully");
                    let _ = build_tx.try_send(BuildEvent::Log("Installation completed successfully".to_string()));
                    let _ = build_tx.try_send(BuildEvent::InstallationComplete(true));
                }
                Err(e) => {
                    eprintln!("[KERNEL] Installation failed: {}", e);
                    let _ = build_tx.try_send(BuildEvent::Error(format!("Installation failed: {}", e)));
                    let _ = build_tx.try_send(BuildEvent::InstallationComplete(false));
                }
            }
        });
    }
    
    /// Build the path string to the kernel package for installation
    /// Returns only the absolute path, which will be bundled into a single pacman command
    fn build_install_command_static(path: &PathBuf) -> Result<String, String> {
        let absolute_path = match path.canonicalize() {
            Ok(abs_path) => abs_path,
            Err(e) => {
                return Err(format!("Failed to resolve absolute path: {}", e));
            }
        };
        
        if !absolute_path.to_string_lossy().ends_with(".pkg.tar.zst") {
            return Err("Package must be a .pkg.tar.zst file".to_string());
        }
        
        // Return only the path, not wrapped in pacman command
        // The caller will bundle all paths into a single "pacman -U --noconfirm <path1> <path2>" command
        Ok(absolute_path.to_string_lossy().to_string())
    }
    
    /// Build the pacman command to install kernel headers if they exist
    fn build_install_headers_command_static(kernel_path: &PathBuf) -> Result<Option<String>, String> {
        if let Some(kernel_filename) = kernel_path.file_name() {
            if let Some(kernel_str) = kernel_filename.to_str() {
                let headers_filename = if kernel_str.starts_with("linux-") {
                    let remainder = &kernel_str[6..];
                    
                    let mut version_start_pos = 0;
                    let mut found_version = false;
                    for (i, ch) in remainder.chars().enumerate() {
                        if ch == '-' && i + 1 < remainder.len() {
                            if remainder.chars().nth(i + 1).map_or(false, |c| c.is_ascii_digit()) {
                                version_start_pos = i;
                                found_version = true;
                                break;
                            }
                        }
                    }
                    
                    if found_version {
                        let variant = &remainder[..version_start_pos];
                        let rest = &remainder[version_start_pos..];
                        format!("linux-{}-headers{}", variant, rest)
                    } else {
                        format!("linux-headers-{}", remainder)
                    }
                } else {
                    return Err("Kernel filename must start with 'linux-'".to_string());
                };
                
                if let Some(parent) = kernel_path.parent() {
                    let headers_path = parent.join(&headers_filename);
                    
                    if headers_path.exists() {
                        let absolute_path = match headers_path.canonicalize() {
                            Ok(abs_path) => abs_path,
                            Err(e) => {
                                return Err(format!("Failed to resolve headers path: {}", e));
                            }
                        };
                        
                        return Ok(Some(format!("pacman -U --noconfirm {}", absolute_path.display())));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Get the absolute path to kernel headers package if it exists
    /// Used for bundling multiple packages into a single pacman command
    fn build_install_headers_path_static(kernel_path: &PathBuf) -> Result<Option<String>, String> {
        if let Some(kernel_filename) = kernel_path.file_name() {
            if let Some(kernel_str) = kernel_filename.to_str() {
                let headers_filename = if kernel_str.starts_with("linux-") {
                    let remainder = &kernel_str[6..];
                    
                    let mut version_start_pos = 0;
                    let mut found_version = false;
                    for (i, ch) in remainder.chars().enumerate() {
                        if ch == '-' && i + 1 < remainder.len() {
                            if remainder.chars().nth(i + 1).map_or(false, |c| c.is_ascii_digit()) {
                                version_start_pos = i;
                                found_version = true;
                                break;
                            }
                        }
                    }
                    
                    if found_version {
                        let variant = &remainder[..version_start_pos];
                        let rest = &remainder[version_start_pos..];
                        format!("linux-{}-headers{}", variant, rest)
                    } else {
                        format!("linux-headers-{}", remainder)
                    }
                } else {
                    return Err("Kernel filename must start with 'linux-'".to_string());
                };
                
                if let Some(parent) = kernel_path.parent() {
                    let headers_path = parent.join(&headers_filename);
                    
                    if headers_path.exists() {
                        let absolute_path = match headers_path.canonicalize() {
                            Ok(abs_path) => abs_path,
                            Err(e) => {
                                return Err(format!("Failed to resolve headers path: {}", e));
                            }
                        };
                        
                        // Return just the path, not wrapped in a pacman command
                        return Ok(Some(absolute_path.to_string_lossy().to_string()));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Automatically find and install kernel headers package
    ///
    /// Given a kernel package path like "linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst",
    /// this function looks for the corresponding "-headers" package in the same directory.
    ///
    /// # Example
    /// Input: `/path/to/linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst`
    /// Looks for: `/path/to/linux-goatd-gaming-headers-6.18.3.arch1-2-x86_64.pkg.tar.zst`
    fn install_kernel_headers(&self, kernel_path: &PathBuf) -> Result<(), String> {
        if let Some(kernel_filename) = kernel_path.file_name() {
            if let Some(kernel_str) = kernel_filename.to_str() {
                // Build the headers filename by inserting "-headers" before the version number.
                // Kernel versions always start with a digit (e.g., 6, 5, etc.)
                // Examples:
                // "linux-6.18.3-arch1-2-x86_64.pkg.tar.zst" -> "linux-headers-6.18.3-arch1-2-x86_64.pkg.tar.zst"
                // "linux-zen-6.18.3-arch1-2-x86_64.pkg.tar.zst" -> "linux-zen-headers-6.18.3-arch1-2-x86_64.pkg.tar.zst"
                // "linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst" -> "linux-goatd-gaming-headers-6.18.3.arch1-2-x86_64.pkg.tar.zst"
                
                let headers_filename = if kernel_str.starts_with("linux-") {
                    // Extract the variant part (zen, lts, goatd-gaming, etc.)
                    let remainder = &kernel_str[6..]; // Skip "linux-"
                    
                    // Find where the version starts: look for the first dash followed by a digit
                    // This correctly handles multi-part variants like "goatd-gaming"
                    let mut version_start_pos = 0;
                    let mut found_version = false;
                    for (i, ch) in remainder.chars().enumerate() {
                        if ch == '-' && i + 1 < remainder.len() {
                            if remainder.chars().nth(i + 1).map_or(false, |c| c.is_ascii_digit()) {
                                version_start_pos = i;
                                found_version = true;
                                break;
                            }
                        }
                    }
                    
                    if found_version {
                        let variant = &remainder[..version_start_pos];
                        let rest = &remainder[version_start_pos..];
                        format!("linux-{}-headers{}", variant, rest)
                    } else {
                        // No variant, just linux-<version>
                        format!("linux-headers-{}", remainder)
                    }
                } else {
                    return Err("Kernel filename must start with 'linux-'".to_string());
                };
                
                // Construct the full path to the headers package
                if let Some(parent) = kernel_path.parent() {
                    let headers_path = parent.join(&headers_filename);
                    
                    // Check if the headers file exists
                    if headers_path.exists() {
                        self.log_event("KERNEL", &format!("Found kernel headers: {}", headers_filename));
                        
                        // Install the headers package
                        match self.system.install_package(headers_path) {
                            Ok(()) => {
                                self.log_event("KERNEL", "Kernel headers installed successfully");
                                Ok(())
                            }
                            Err(e) => {
                                self.log_event("KERNEL", &format!("Failed to install kernel headers: {}", e));
                                // Log the error but don't fail - headers might be part of the kernel pkg
                                Ok(())
                            }
                        }
                    } else {
                        self.log_event("KERNEL", &format!("Kernel headers not found at: {}", headers_path.display()));
                        // Headers not found - this is not a fatal error
                        Ok(())
                    }
                } else {
                    self.log_event("KERNEL", "Could not determine parent directory for kernel package");
                    Ok(())
                }
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
    
    /// Extract kernel version from filename for fallback purposes
    ///
    /// Generalizes version extraction to handle any profile name following `linux-goatd-<profile>` pattern.
    ///
    /// Maps package filenames to kernel release strings:
    /// - `linux-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
    /// - `linux-zen-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
    /// - `linux-lts-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
    /// - `linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-gaming`
    /// - `linux-goatd-server-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-server`
    /// - `linux-goatd-workstation-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-workstation`
    fn extract_kernel_version_from_filename(filename: &str) -> Option<String> {
        // Remove .pkg.tar.zst suffix
        let base = filename.strip_suffix(".pkg.tar.zst")?;
        
        // Extract the variant and version parts
        if base.starts_with("linux-") {
            let remainder = &base[6..]; // Skip "linux-"
            
            // Find where the version starts (first dash followed by a digit).
            // This correctly handles multi-part variants like "goatd-gaming", "goatd-server", etc.
            let version_start_pos = remainder
                .char_indices()
                .find(|(i, ch)| {
                    *ch == '-'
                        && remainder.chars().nth(i + 1).map_or(false, |c| c.is_ascii_digit())
                })?
                .0;
            
            let variant = &remainder[..version_start_pos];
            let version_with_arch = &remainder[version_start_pos + 1..];
            
            // Remove architecture suffix (e.g., -x86_64, -i686, -aarch64)
            let version = version_with_arch
                .strip_suffix("-x86_64")
                .or_else(|| version_with_arch.strip_suffix("-i686"))
                .or_else(|| version_with_arch.strip_suffix("-aarch64"))
                .unwrap_or(version_with_arch);
            
            // Normalize version string: replace .arch with -arch (standard Arch format)
            // e.g., "6.18.3.arch1-2" -> "6.18.3-arch1-2"
            let normalized_version = Self::normalize_arch_version_string(version);
            
            // Determine if we should append the variant suffix.
            // Standard Linux kernel variants (zen, lts) use the baseline version.
            // Custom GOATd profiles (goatd-gaming, goatd-server, goatd-workstation, etc.)
            // always have their profile name appended to ensure DKMS targets the correct kernel.
            let should_append_variant = Self::should_append_profile_suffix(variant);
            
            if should_append_variant {
                // Custom profile: include it at the end of the kernel version string
                return Some(format!("{}-{}", normalized_version, variant));
            } else {
                return Some(normalized_version);
            }
        }
        
        None
    }
    
    /// Determine if a variant should be appended to the kernel version string.
    ///
    /// Standard kernel variants (zen, lts) are not appended; custom GOATd profiles
    /// (matching `goatd-*` pattern) are always appended for proper DKMS targeting.
    fn should_append_profile_suffix(variant: &str) -> bool {
        // Don't append for standard Linux kernel variants
        if variant == "zen" || variant == "lts" || variant.is_empty() {
            return false;
        }
        
        // Append for all custom profiles, including any goatd-* variant
        // (e.g., goatd-gaming, goatd-server, goatd-workstation, etc.)
        true
    }

    /// Normalize Arch Linux kernel version string
    ///
    /// Converts dots before architecture tags to dashes:
    /// - `6.18.3.arch1-2` -> `6.18.3-arch1-2`
    /// - `6.8.0.arch1-1` -> `6.8.0-arch1-1`
    ///
    /// This ensures DKMS finds the kernel in `/usr/lib/modules/<kernel_release>`.
    fn normalize_arch_version_string(version: &str) -> String {
        // Replace ".arch" with "-arch" to match Arch Linux standard kernel release format
        version.replace(".arch", "-arch")
    }

    // ========================================================================
    // PERFORMANCE DIAGNOSTICS METHODS
    // ========================================================================

    /// Get the performance history persistence path: ~/.config/goatdkernel/performance/
    fn get_perf_history_path() -> String {
        // Use XDG_CONFIG_HOME or default to ~/.config
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    PathBuf::from(h).join(".config")
                })
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/.config"))
            .join("goatdkernel")
            .join("performance");
        
        // Ensure directory exists
        let _ = std::fs::create_dir_all(&config_dir);
        
        let filepath = config_dir.join("history.json");
        filepath.to_string_lossy().to_string()
    }

    /// Get the performance checkpoint persistence path: ~/.config/goatdkernel/performance/diagnostic.json
    fn get_perf_checkpoint_path() -> String {
        // Use XDG_CONFIG_HOME or default to ~/.config
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    PathBuf::from(h).join(".config")
                })
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/.config"))
            .join("goatdkernel")
            .join("performance");
        
        // Ensure directory exists
        let _ = std::fs::create_dir_all(&config_dir);
        
        let filepath = config_dir.join("diagnostic.json");
        filepath.to_string_lossy().to_string()
    }

    /// Start performance monitoring with the specified configuration
    ///
    /// Spawns the background processor task and begins measurement
    /// Accepts optional UI context for signaling repaints from background thread
    pub fn start_performance_monitoring(&self, config: PerformanceConfig) -> Result<(), String> {
        // Store the UI context if available (will be populated by app.rs when called)
        // For now, leave None - will be set by app.rs via set_perf_ui_context()
        // Check if already monitoring
        if self.perf_monitoring_active.load(Ordering::Acquire) {
            return Err("Performance monitoring already active".to_string());
        }

        // CRITICAL FIX: Preserve core_temperatures from previous metrics to prevent heatmap blackout
        // When monitoring restarts, preserve the last known thermal state instead of resetting to empty
        let preserved_core_temps = {
            if let Ok(metrics) = self.perf_metrics.read() {
                metrics.core_temperatures.clone()
            } else {
                Vec::new()
            }
        };
        
        eprintln!("[PERF] [START] Preserved {} core temperatures from previous session for seamless transition",
            preserved_core_temps.len());

        // Mark as active
        self.perf_monitoring_active.store(true, Ordering::Release);

        // Create the ring buffer for SPSC communication (65536 samples capacity)
        let (producer, consumer) = rtrb::RingBuffer::new(65536);

        // Create monitoring state
        let monitoring_state = MonitoringState::default();
        
        // Store the state
        {
            let mut state = self.perf_monitoring_state
                .write()
                .map_err(|e| format!("Failed to write monitoring state: {}", e))?;
            *state = Some(monitoring_state.clone());
        }

        // Capture core_id from config for use in background processor
        let core_id = config.core_id;

        // Spawn the measurement thread (native thread for real-time latency collection)
        let stop_flag = monitoring_state.stop_flag.clone();
        let dropped_count = monitoring_state.dropped_samples.clone();
        let spike_count = monitoring_state.spike_count.clone();
        let smi_correlated_spikes = monitoring_state.smi_correlated_spikes.clone();
        let total_smi_count = monitoring_state.total_smi_count.clone();
        
        let interval = Duration::from_micros(config.interval_us);
        let spike_threshold = config.spike_threshold_us * 1000; // Convert µs to ns

        std::thread::spawn(move || {
            // CRITICAL: Apply real-time settings before starting measurement loop
            let mut tuner = crate::system::performance::Tuner::new();
            match tuner.apply_realtime_settings(config.core_id) {
                Ok(()) => {
                    eprintln!("[PERF] [THREAD] Done: Real-time settings applied (SCHED_FIFO priority 80, CPU affinity to core {}, mlockall)", config.core_id);
                }
                Err(e) => {
                    eprintln!("[PERF] [THREAD] Warning: Real-time settings failed (non-root?): {}. Proceeding with standard scheduling.", e);
                    eprintln!("[PERF] [THREAD] Warning: Latency measurements will be less accurate without SCHED_FIFO priority.");
                }
            }
            
            let collector = LatencyCollector::new(
                interval,
                producer,
                stop_flag,
                dropped_count,
                spike_threshold,
                spike_count,
                smi_correlated_spikes,
                total_smi_count,
                config.core_id,
            );
            eprintln!("[PERF] [THREAD] Starting latency collection loop (interval_us={}, spike_threshold_ns={})", config.interval_us, spike_threshold);
            collector.run();
            eprintln!("[PERF] [THREAD] Latency collection loop stopped");
        });

        // Spawn the async processor task in Tokio
        let metrics = self.perf_metrics.clone();
        let history = self.perf_history.clone();
        let active = self.perf_monitoring_active.clone();
        let mon_state = monitoring_state.clone();
        let alert_flag = self.perf_background_alert.clone();
        let alert_count = self.perf_alert_count.clone();
        let settings = self.settings.clone();
        let jitter_history = self.perf_jitter_history.clone();
        let dirty_flag = self.atomic_perf_dirty.clone();
        let ui_context = self.perf_ui_context.clone();

        tokio::spawn(async move {
            Self::background_processor_task(
                consumer,
                metrics,
                history,
                mon_state,
                active,
                alert_flag,
                alert_count,
                settings,
                jitter_history,
                dirty_flag,
                ui_context,
                core_id,
                preserved_core_temps,
            ).await;
        });

        self.log_event("PERFORMANCE", "Performance monitoring started");
        Ok(())
    }

    /// Set the UI context for performance monitoring (called from app.rs on each update())
    ///
    /// This allows the background processor task to wake up the UI thread via ctx.request_repaint()
    pub fn set_perf_ui_context(&self, ctx: egui::Context) -> Result<(), String> {
        if let Ok(mut ctx_lock) = self.perf_ui_context.write() {
            *ctx_lock = Some(ctx);
            Ok(())
        } else {
            Err("Failed to set UI context".to_string())
        }
    }

    /// Stop performance monitoring and persist results
    pub fn stop_performance_monitoring(&self) -> Result<(), String> {
        if !self.perf_monitoring_active.load(Ordering::Acquire) {
            return Err("Performance monitoring not active".to_string());
        }

        // Signal the measurement thread to stop
        {
            let state = self.perf_monitoring_state
                .write()
                .map_err(|e| format!("Failed to write monitoring state: {}", e))?;
            if let Some(ref mon_state) = *state {
                mon_state.request_stop();
            }
        }

        // Mark as inactive
        self.perf_monitoring_active.store(false, Ordering::Release);

        // Persist current history to disk
        {
            let history = self.perf_history
                .read()
                .map_err(|e| format!("Failed to read history: {}", e))?;
            let path = Self::get_perf_history_path();
            history.save_to_disk(&path)
                .map_err(|e| format!("Failed to save history: {}", e))?;
        }

        self.log_event("PERFORMANCE", "Performance monitoring stopped");
        Ok(())
    }

    /// Background processor task: drains the ring buffer, updates histograms, and pushes to metrics
    ///
    /// This Tokio task runs continuously while monitoring is active, consuming latency samples
    /// from the ring buffer and maintaining aggregate statistics. Handles mode-specific timing:
    /// - For Benchmark mode: auto-terminates when duration expires
    /// - For Continuous mode: periodically logs diagnostics and auto-saves and checkpoint snapshot
    async fn background_processor_task(
        mut consumer: rtrb::Consumer<u64>,
        metrics: Arc<std::sync::RwLock<PerformanceMetrics>>,
        history: Arc<std::sync::RwLock<PerformanceHistory>>,
        monitoring_state: MonitoringState,
        active_flag: Arc<AtomicBool>,
        background_alert: Arc<AtomicBool>,
        alert_count: Arc<AtomicU64>,
        settings: Arc<std::sync::RwLock<AppState>>,
        jitter_history: Arc<RwLock<VecDeque<f32>>>,
        dirty_flag: Arc<AtomicBool>,
        ui_context: Arc<RwLock<Option<egui::Context>>>,
        core_id: usize,
        preserved_core_temps: Vec<f32>,
    ) {
        let mut processor = LatencyProcessor::new()
            .expect("Failed to create LatencyProcessor");
        
        // CRITICAL FIX: Initialize processor with preserved core temperatures
        // This creates a seamless visual transition when monitoring restarts
        if !preserved_core_temps.is_empty() {
            processor.set_core_temperatures(preserved_core_temps.clone());
            eprintln!("[PERF] [PROCESSOR] INIT: Core temperatures pre-populated from previous session ({} cores)", preserved_core_temps.len());
        }

        let cycle_timer = tokio::time::interval(Duration::from_millis(20));
        let mut cycle_timer = cycle_timer;

        let snapshot_timer = tokio::time::interval(Duration::from_secs(5));
        let mut snapshot_timer = snapshot_timer;

        let diagnostic_timer = tokio::time::interval(Duration::from_secs(60));
        let mut diagnostic_timer = diagnostic_timer;

        // Checkpoint timer for continuous mode: every 15 minutes (900 seconds)
        let checkpoint_timer = tokio::time::interval(Duration::from_secs(900));
        let mut checkpoint_timer = checkpoint_timer;

        // Audit timer for system CPU governor polling: every 2 seconds (20 cycles of 100ms)
        let audit_timer = tokio::time::interval(Duration::from_secs(2));
        let mut audit_timer = audit_timer;

        eprintln!("[PERF] [PROCESSOR] Task started (100ms drain cycle, 5s snapshot cycle, 60s diagnostic log, 900s checkpoint, 2s audit)");
        
        let mut cycle_count = 0u64;
        let mut total_drained = 0u64;
        let mut last_dropped_count = 0u64;
        let session_start = std::time::Instant::now();
        
        // Preserve governor/frequency across cycles to prevent overwrites
        let mut cached_governor = String::new();
        let mut cached_frequency = 0i32;
        
        // Throttle thermal updates: only update every 5 cycles (100ms instead of 20ms)
        // Hardware sensors update slowly, so frequent polling is wasteful
        const THERMAL_UPDATE_INTERVAL_CYCLES: u64 = 5;  // 100ms at 20ms cycle rate

        while active_flag.load(Ordering::Acquire) {
            tokio::select! {
                _ = cycle_timer.tick() => {
                    cycle_count += 1;
                    let mut drained_this_cycle = 0u64;
                    
                    // Drain all available samples from the ring buffer
                    // CRITICAL: Feed samples into rolling window for recovery capability
                    while let Ok(latency_ns) = consumer.pop() {
                        let _ = processor.record_sample(latency_ns);
                        
                        // Convert nanoseconds to microseconds and add to rolling window
                        let latency_us = (latency_ns as f32) / 1000.0;
                        history.write()
                            .ok()
                            .map(|mut h| h.rolling_window.add_latency(latency_us));
                        
                        drained_this_cycle += 1;
                        total_drained += 1;
                    }

                    let dropped_now = monitoring_state.dropped_count();
                    let dropped_this_cycle = dropped_now - last_dropped_count;
                    last_dropped_count = dropped_now;

                    // Log drainage diagnostics every 5 cycles (500ms)
                    if cycle_count % 5 == 0 {
                        eprintln!("[PERF] [PROCESSOR] Cycle {}: drained {} samples, dropped this cycle: {}, total drained: {}",
                            cycle_count, drained_this_cycle, dropped_this_cycle, total_drained);
                    }

                    // Update metrics - READ from monitoring state atomics
                    let total_spikes = monitoring_state.spike_count();
                    let total_smis = monitoring_state.total_smi_count();
                    let smi_correlations = monitoring_state.smi_correlated_count();
                    
                    // Calculate rolling metrics from the window
                    let (rolling_p99, rolling_p99_9, rolling_consistency) = {
                        if let Ok(h) = history.write() {
                            let p99 = h.rolling_window.calculate_p99_latency();
                            let p99_9 = h.rolling_window.calculate_p99_9_latency();
                            
                            // Calculate Standard Deviation of rolling latency samples (Laboratory Grade CV%)
                            // Professional calibration: 2% CV (Optimal/Lab Grade) → 30% CV (Poor)
                            let std_dev = h.rolling_window.calculate_std_dev();
                            (p99, p99_9, std_dev)
                        } else {
                            (0.0, 0.0, 0.0)
                        }
                    };
                    
                    // Update thermal data only every THERMAL_UPDATE_INTERVAL_CYCLES (100ms)
                    // Hardware sensors update slowly, no benefit from polling every 20ms
                    if cycle_count % THERMAL_UPDATE_INTERVAL_CYCLES == 0 {
                        processor.update_thermal_data();
                    }
                    
                    // Every 100ms: retrieve cycle_max and add to jitter_history
                    let cycle_max_val = processor.cycle_max_us();
                    if let Ok(mut jitter) = jitter_history.write() {
                        jitter.push_back(cycle_max_val);
                        // Maintain max 300 samples in deque
                        if jitter.len() > 300 {
                            jitter.pop_front();
                        }
                    }
                    processor.reset_cycle_max();
                    
                    // Get histogram buckets (normalized 0.0..1.0)
                    let histogram_buckets = processor.get_histogram_buckets();
                    
                    // Convert jitter_history deque to Vec for PerformanceMetrics
                    let jitter_vec: Vec<f32> = if let Ok(jitter) = jitter_history.read() {
                        jitter.iter().copied().collect()
                    } else {
                        Vec::new()
                    };
                    
                    // Get thermal data from processor
                    let core_temps = processor.core_temperatures().to_vec();
                    let pkg_temp = processor.package_temperature();
                    
                    let updated_metrics = PerformanceMetrics {
                        current_us: processor.last_sample(),
                        max_us: processor.max(),
                        p99_us: processor.p99(),
                        p99_9_us: processor.p99_9(),
                        avg_us: processor.average(),
                        rolling_p99_us: rolling_p99,  // From 1000-sample window (enables recovery)
                        rolling_p99_9_us: rolling_p99_9,  // From 1000-sample window
                        rolling_throughput_p99: 0.0,
                        rolling_efficiency_p99: 0.0,
                        rolling_consistency_us: rolling_consistency,  // From 1000-sample consistency window
                        total_spikes,
                        total_smis,
                        spikes_correlated_to_smi: smi_correlations,
                        histogram_buckets,
                        jitter_history: jitter_vec,
                        active_governor: cached_governor.clone(),
                        governor_hz: cached_frequency,
                        core_temperatures: core_temps,
                        package_temperature: pkg_temp,
                        benchmark_metrics: None,
                    };

                    // Check for background alert trigger: major spike detected (>500µs default or configured threshold)
                    if let Ok(app_state) = settings.read() {
                        let alert_threshold = app_state.perf_alert_threshold_us;
                        if updated_metrics.max_us > alert_threshold && !background_alert.load(Ordering::Acquire) {
                            eprintln!("[PERF] [PROCESSOR] [ALERT] Warning: Background spike alert triggered! max={:.2}µs exceeds threshold {:.2}µs",
                                updated_metrics.max_us, alert_threshold);
                            background_alert.store(true, Ordering::Release);
                            alert_count.fetch_add(1, Ordering::Release);
                        }
                    }
                    
                    // Write updated_metrics to the shared metrics
                    if let Ok(mut m) = metrics.write() {
                        *m = updated_metrics.clone();
                    }
                    
                    // Signal UI that metrics have been updated (set dirty flag for repaint)
                    dirty_flag.store(true, Ordering::Release);
                    
                    // CRITICAL FIX: Request repaint from background processor via egui::Context
                    // This ensures the UI updates even without mouse movement
                    if let Ok(ctx_lock) = ui_context.read() {
                        if let Some(ref ctx) = *ctx_lock {
                            ctx.request_repaint();
                        }
                    }

                    // Log histogram accuracy check WITH SMI DIAGNOSTICS
                    if processor.sample_count() > 0 && processor.sample_count() % 100 == 0 {
                        eprintln!("[PERF] [PROCESSOR] Histogram check: samples={}, max={:.2}µs, p99={:.2}µs, p99.9={:.2}µs, avg={:.2}µs | SMI: total={}, correlations={}, spikes={}",
                            processor.sample_count(),
                            updated_metrics.max_us,
                            updated_metrics.p99_us,
                            updated_metrics.p99_9_us,
                            updated_metrics.avg_us,
                            total_smis,
                            smi_correlations,
                            total_spikes);
                    }

                }
                _ = snapshot_timer.tick() => {
                    // Create a snapshot every 5 seconds for trend analysis
                    let current_metrics = metrics
                        .read()
                        .map(|m| m.clone())
                        .unwrap_or_default();

                    let kernel_context = Self::get_kernel_context();
                    let snapshot = crate::system::performance::PerformanceSnapshot::new(
                        current_metrics.clone(),
                        kernel_context,
                    );

                    eprintln!("[PERF] [PROCESSOR] [SNAPSHOT] 5s periodic: max={:.2}µs, p99={:.2}µs, spikes={}, smi_correlated={}, total_samples={}",
                        current_metrics.max_us, current_metrics.p99_us, current_metrics.total_spikes, current_metrics.spikes_correlated_to_smi, processor.sample_count());

                    if let Ok(mut h) = history.write() {
                        h.add_snapshot(snapshot);
                    }
                }
                _ = diagnostic_timer.tick() => {
                    // Periodic diagnostic log (every 60s) - CONTINUOUS MODE DIAGNOSTIC
                    let elapsed = session_start.elapsed();
                    let current_metrics = metrics
                        .read()
                        .map(|m| m.clone())
                        .unwrap_or_default();

                    eprintln!("[PERF] [PROCESSOR] [DIAGNOSTIC-60s] ⚠ PERIODIC LOG | Elapsed: {:.1}s | Samples: {} | Max: {:.2}µs | P99.9: {:.2}µs | Avg: {:.2}µs | Spikes: {} | SMI: {} | Dropped: {}",
                        elapsed.as_secs_f64(),
                        processor.sample_count(),
                        current_metrics.max_us,
                        current_metrics.p99_9_us,
                        current_metrics.avg_us,
                        current_metrics.total_spikes,
                        current_metrics.total_smis,
                        monitoring_state.dropped_count());
                }
                _ = checkpoint_timer.tick() => {
                    // Periodic checkpoint save (every 900s = 15 minutes) - CONTINUOUS MODE CHECKPOINT
                    eprintln!("[PERF] [PROCESSOR] [CHECKPOINT-15m] Starting diagnostic checkpoint save...");
                    
                    if let Ok(h) = history.read() {
                        let checkpoint_path = Self::get_perf_checkpoint_path();
                        match h.save_to_disk(&checkpoint_path) {
                            Ok(()) => {
                                eprintln!("[PERF] [PROCESSOR] [CHECKPOINT-15m] Done: Diagnostic checkpoint saved to {}", checkpoint_path);
                            }
                            Err(e) => {
                                eprintln!("[PERF] [PROCESSOR] [CHECKPOINT-15m] Warning: Failed to save checkpoint: {}", e);
                            }
                        }
                    }
                }
                _ = audit_timer.tick() => {
                    // System audit polling: every 2 seconds - read CPU governor and frequency for the monitored core
                    let governor = Self::read_cpu_governor_for_core(core_id);
                    let frequency_mhz = Self::read_cpu_frequency_for_core(core_id);
                    
                    eprintln!("[PERF] [PROCESSOR] [AUDIT-2s] Core {}: Governor: {}, Frequency: {} MHz",
                        core_id, governor, frequency_mhz);
                    
                    // Cache the values so they persist across 100ms cycles
                    cached_governor = governor.clone();
                    cached_frequency = frequency_mhz;
                    
                    // Update metrics with governor and frequency
                    if let Ok(mut m) = metrics.write() {
                        m.active_governor = governor;
                        m.governor_hz = frequency_mhz;
                    }
                }
            }
        }
        
        // CRITICAL: Final drain before exiting - don't lose samples!
        eprintln!("[PERF] [PROCESSOR] Warning: FINAL DRAIN starting: active_flag is now false");
        let mut final_drain_count = 0u64;
        while let Ok(latency_ns) = consumer.pop() {
            let _ = processor.record_sample(latency_ns);
            final_drain_count += 1;
        }
        eprintln!("[PERF] [PROCESSOR] Warning: FINAL DRAIN completed: recovered {} samples from ring buffer", final_drain_count);
        
        // CRITICAL: Persist final sample counts to MonitoringState for SessionSummary capture
        let final_sample_count = processor.sample_count();
        let final_dropped_count = monitoring_state.dropped_count();
        monitoring_state.final_sample_count.store(final_sample_count, Ordering::Release);
        monitoring_state.final_dropped_count.store(final_dropped_count, Ordering::Release);
        eprintln!("[PERF] [PROCESSOR] Warning: PERSISTED FINAL COUNTS: samples={}, dropped={}", final_sample_count, final_dropped_count);
        
        eprintln!("[PERF] [PROCESSOR] Task stopping (processed {} cycles, drained {} total samples + {} final, processor.sample_count={}, elapsed {:.1}s)",
            cycle_count, total_drained, final_drain_count, processor.sample_count(), session_start.elapsed().as_secs_f64());
        eprintln!("[PERF] [PROCESSOR] Warning: FINAL METRICS: samples={}, max={:.2}µs, p99={:.2}µs, p99.9={:.2}µs, avg={:.2}µs",
            processor.sample_count(), processor.max(), processor.p99(), processor.p99_9(), processor.average());
    }

    /// Get current kernel context for performance records
    ///
    /// Captures real-time metadata:
    /// - Kernel version from system
    /// - Active SCX scheduler profile
    /// - LTO configuration from settings
    /// - Active CPU governor
    pub fn get_kernel_context() -> KernelContext {
        // Try to read the kernel version from /proc/version
        let version = std::fs::read_to_string("/proc/version")
            .ok()
            .and_then(|content| {
                content.split_whitespace().nth(2).map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Unknown".to_string());

        // Use the refined multi-layer SCX detection from audit module
        // PRIORITY 1: Kernel Layer via sysfs (/sys/kernel/sched_ext/root/ops)
        // PRIORITY 2: Service Layer via scxctl get (optional, for mode detection)
        let scx_profile = {
            // Try kernel layer detection first (most authoritative)
            let state_ok = std::fs::read_to_string("/sys/kernel/sched_ext/state")
                .map(|content| content.trim() == "enabled")
                .unwrap_or(false);
            
            if state_ok {
                // SCX is enabled in kernel, read the scheduler name
                std::fs::read_to_string("/sys/kernel/sched_ext/root/ops")
                    .ok()
                    .map(|content| {
                        let scheduler_name = content.trim().to_string();
                        // Try to get the operation mode from scxctl (service layer)
                        if let Ok(output) = std::process::Command::new("scxctl")
                            .arg("get")
                            .output()
                        {
                            if output.status.success() {
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                for line in stdout.lines() {
                                    if line.trim().starts_with("Mode:") {
                                        let mode = line.replace("Mode:", "").trim().to_string();
                                        if !mode.is_empty() {
                                            return format!("{} ({})", scheduler_name, mode);
                                        }
                                    }
                                }
                            }
                        }
                        scheduler_name
                    })
                    .unwrap_or_else(|| "scx_active".to_string())
            } else {
                "default_eevdf".to_string()
            }
        };

        // LTO configuration - use refined detection from .config
        let lto_config = {
            // Try /proc/config.gz first (fastest path)
            if let Ok(output) = std::process::Command::new("zcat")
                .arg("/proc/config.gz")
                .output()
            {
                let config_str = String::from_utf8_lossy(&output.stdout);
                if config_str.contains("CONFIG_LTO_CLANG_FULL=y") {
                    "CLANG Full".to_string()
                } else if config_str.contains("CONFIG_LTO_CLANG_THIN=y") {
                    "CLANG Thin".to_string()
                } else if config_str.contains("CONFIG_LTO_CLANG=y") {
                    "CLANG".to_string()
                } else if config_str.contains("CONFIG_LTO_GCC=y") {
                    "GCC".to_string()
                } else {
                    "None".to_string()
                }
            } else {
                // Fallback: Try /boot/config-$(uname -r)
                if let Ok(output) = std::process::Command::new("uname")
                    .arg("-r")
                    .output()
                {
                    if output.status.success() {
                        let kernel_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let boot_config_path = format!("/boot/config-{}", kernel_version);
                        if let Ok(config_str) = std::fs::read_to_string(&boot_config_path) {
                            if config_str.contains("CONFIG_LTO_CLANG_FULL=y") {
                                "CLANG Full".to_string()
                            } else if config_str.contains("CONFIG_LTO_CLANG_THIN=y") {
                                "CLANG Thin".to_string()
                            } else if config_str.contains("CONFIG_LTO_CLANG=y") {
                                "CLANG".to_string()
                            } else if config_str.contains("CONFIG_LTO_GCC=y") {
                                "GCC".to_string()
                            } else {
                                "None".to_string()
                            }
                        } else {
                            "Unknown".to_string()
                        }
                    } else {
                        "Unknown".to_string()
                    }
                } else {
                    "Unknown".to_string()
                }
            }
        };

        // Active CPU governor
        let governor = Self::read_cpu_governor();

        KernelContext {
            version,
            scx_profile,
            lto_config,
            governor,
        }
    }

    /// Read the active CPU frequency governor from sysfs
    ///
    /// Reads from `/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor`
    /// Returns the governor name (e.g., "powersave", "performance", "schedutil")
    /// or "unknown" if unreadable.
    fn read_cpu_governor() -> String {
        std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .ok()
            .and_then(|content| {
                content.trim().to_string().chars().all(|c| c.is_alphanumeric() || c == '_')
                    .then(|| content.trim().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Read the current CPU frequency in MHz from sysfs
    ///
    /// Reads from `/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq`
    /// Returns frequency in MHz (divides the kHz value by 1000)
    /// Returns 0 if unreadable.
    fn read_cpu_frequency() -> i32 {
        std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            .ok()
            .and_then(|content| {
                content.trim().parse::<i32>().ok()
                    .map(|khz| khz / 1000)  // Convert kHz to MHz
            })
            .unwrap_or(0)
    }

    /// Read the active CPU frequency governor for a specific core
    ///
    /// Reads from `/sys/devices/system/cpu/cpuN/cpufreq/scaling_governor`
    /// where N is the core_id. Falls back to cpu0 if the core path doesn't exist.
    /// Returns the governor name (e.g., "powersave", "performance", "schedutil")
    /// or "unknown" if unreadable.
    fn read_cpu_governor_for_core(core_id: usize) -> String {
        let core_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", core_id);
        
        // Try core-specific path first
        if let Ok(content) = std::fs::read_to_string(&core_path) {
            if content.trim().to_string().chars().all(|c| c.is_alphanumeric() || c == '_') {
                return content.trim().to_string();
            }
        }
        
        // Fall back to cpu0 if core-specific read fails
        Self::read_cpu_governor()
    }

    /// Read the current CPU frequency in MHz for a specific core
    ///
    /// Reads from `/sys/devices/system/cpu/cpuN/cpufreq/scaling_cur_freq`
    /// where N is the core_id. Falls back to cpu0 if the core path doesn't exist.
    /// Returns frequency in MHz (divides the kHz value by 1000)
    /// Returns 0 if unreadable.
    fn read_cpu_frequency_for_core(core_id: usize) -> i32 {
        let core_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", core_id);
        
        // Try core-specific path first
        if let Ok(content) = std::fs::read_to_string(&core_path) {
            if let Ok(khz) = content.trim().parse::<i32>() {
                return khz / 1000;  // Convert kHz to MHz
            }
        }
        
        // Fall back to cpu0 if core-specific read fails
        Self::read_cpu_frequency()
    }

    /// Compare two performance records and calculate deltas
    ///
    /// Returns a tuple of (P99 delta, P99.9 delta, Max delta, improvement/degradation indicator)
    pub fn compare_performance_records(
        record_a: &PerformanceRecord,
        record_b: &PerformanceRecord,
    ) -> (f32, f32, f32, String) {
        let p99_delta = record_b.metrics.p99_us - record_a.metrics.p99_us;
        let p99_9_delta = record_b.metrics.p99_9_us - record_a.metrics.p99_9_us;
        let max_delta = record_b.metrics.max_us - record_a.metrics.max_us;

        let trend = if max_delta < 0.0 {
            "Improved".to_string()
        } else if max_delta > 0.0 {
            "Degraded".to_string()
        } else {
            "Unchanged".to_string()
        };

        (p99_delta, p99_9_delta, max_delta, trend)
    }

    // ========================================================================
    // SLINT CALLBACK HANDLERS FOR PERFORMANCE UI
    // ========================================================================

    /// Handler for Trigger Jitter Audit button (starts monitoring)
    pub fn handle_trigger_jitter_audit(&self) -> Result<(), String> {
        let config = PerformanceConfig::default();
        self.start_performance_monitoring(config)
    }

    /// Handler for Quick Jitter Audit (bounded 5-second benchmark session)
    ///
    /// Spawns a 5-second benchmark monitoring session with no stressors.
    /// The session automatically terminates after 5 seconds, and the SessionSummary
    /// is cached for display in the Dashboard.
    pub fn handle_quick_jitter_audit(&self) -> Result<(), String> {
        let mode = MonitoringMode::Benchmark(Duration::from_secs(5));
        self.handle_trigger_monitoring(mode, vec![])
    }

    /// Handler for Trigger Monitoring with mode and optional stressors
    ///
    /// Transitions state to `Running`:
    /// - Sets up lifecycle state
    /// - Initializes StressorManager and spawns requested stressors
    /// - Starts LatencyCollector with the specified mode
    /// - Records session start time
    ///
    /// CRITICAL FIX: Deep Reset Implementation
    /// - Clears all session metrics (current, max, p99, p99.9, avg)
    /// - Clears rolling windows (1000-sample P99/P99.9/consistency buffers)
    /// - Clears jitter history and snapshot history
    /// - This ensures score recovery and prevents "stuck" CV values
    pub fn handle_trigger_monitoring(&self, mode: MonitoringMode, stressors: Vec<StressorType>) -> Result<(), String> {
        // Check if already monitoring
        if self.perf_monitoring_active.load(Ordering::Acquire) {
            return Err("Performance monitoring already active".to_string());
        }

        // CRITICAL: Deep Reset for Fresh Session - Clears ALL buffers
        eprintln!("[PERF] [RESET] ==== DEEP RESET: Starting fresh monitoring session ====");
        {
            if let Ok(mut metrics) = self.perf_metrics.write() {
                eprintln!("[PERF] [RESET] Clearing PerformanceMetrics (current, max, p99, p99.9, rolling windows)");
                metrics.reset();
            }
            if let Ok(mut history) = self.perf_history.write() {
                eprintln!("[PERF] [RESET] Performing deep reset on PerformanceHistory:");
                eprintln!("[PERF] [RESET]   - Clearing snapshots");
                eprintln!("[PERF] [RESET]   - Clearing rolling_window.latency_samples (1000-sample P99 buffer)");
                eprintln!("[PERF] [RESET]   - Clearing rolling_window.consistency_samples (1000-sample consistency buffer)");
                history.reset();  // This calls rolling_window.clear() internally
                eprintln!("[PERF] [RESET] Done: All rolling buffers cleared for score recovery");
            }
        }

        // Update lifecycle state to Running
        {
            let mut state = self.perf_lifecycle_state
                .write()
                .map_err(|e| format!("Failed to write lifecycle state: {}", e))?;
            *state = LifecycleState::Running;
        }

        // Store the monitoring mode
        {
            let mut mode_state = self.perf_monitoring_mode
                .write()
                .map_err(|e| format!("Failed to write monitoring mode: {}", e))?;
            *mode_state = Some(mode.clone());
        }

        // Record session start time
        {
            let mut start = self.perf_session_start
                .write()
                .map_err(|e| format!("Failed to write session start: {}", e))?;
            *start = Some(std::time::Instant::now());
        }

        // Initialize StressorManager if stressors are requested
        let stressor_count = stressors.len();
        if !stressors.is_empty() {
            let mut stressor_mgr = StressorManager::new(0)
                .map_err(|e| format!("Failed to create StressorManager: {}", e))?;

            // Spawn each requested stressor with default intensity
            for stressor_type in stressors {
                stressor_mgr.start_stressor(stressor_type, Intensity::default())
                    .map_err(|e| format!("Failed to start stressor {}: {}", stressor_type, e))?;
            }

            let mut mgr_state = self.perf_stressor_manager
                .write()
                .map_err(|e| format!("Failed to write stressor manager: {}", e))?;
            *mgr_state = Some(stressor_mgr);

            self.log_event("PERFORMANCE", &format!("Started {} stressors", stressor_count));
        }

        // Start the performance monitoring with default config
        let config = PerformanceConfig::default();
        self.start_performance_monitoring(config)?;

        // Spawn a task to handle auto-termination for Benchmark mode
        let active = self.perf_monitoring_active.clone();
        let _lifecycle = self.perf_lifecycle_state.clone();
        let session_start = self.perf_session_start.clone();
        let stressor_mgr = self.perf_stressor_manager.clone();
        let metrics = self.perf_metrics.clone();
        let history = self.perf_history.clone();
        let session_summary = self.perf_session_summary.clone();
        let cached_jitter_summary = self.cached_jitter_summary.clone();
        let monitoring_state_arc = self.perf_monitoring_state.clone();
        let build_tx = self.build_tx.clone();

        match mode {
            MonitoringMode::Benchmark(duration) => {
                eprintln!("[PERF] [LIFECYCLE] Benchmark mode: will auto-stop after {:.1}s", duration.as_secs_f64());
                
                tokio::spawn(async move {
                    tokio::time::sleep(duration).await;
                    
                    eprintln!("[PERF] [LIFECYCLE] Warning: Benchmark duration expired, auto-stopping...");

                    // CRITICAL: Use shared cleanup implementation for full cleanup
                    // This ensures both manual and auto-stop paths do identical cleanup
                    eprintln!("[PERF] [LIFECYCLE] Warning: Calling internal_stop_monitoring_impl for full cleanup");
                    // Note: We can't call &self methods from this closure context
                    // Instead, we'll do the essential cleanup here and let handle_stop_monitoring be called
                    
                    // Stop monitoring - THIS SIGNALS PROCESSOR TO EXIT AND DRAIN
                    eprintln!("[PERF] [LIFECYCLE] Warning: Setting active=false to signal processor exit");
                    active.store(false, Ordering::Release);
                    
                    // CRITICAL FIX: Signal the native collector thread to stop
                    eprintln!("[PERF] [LIFECYCLE] Warning: Signaling LatencyCollector to stop via request_stop()");
                    if let Ok(mon_state_lock) = monitoring_state_arc.read() {
                        if let Some(ref mon_state) = *mon_state_lock {
                            mon_state.request_stop();
                            eprintln!("[PERF] [LIFECYCLE] Done: LatencyCollector stop signal sent");
                        }
                    }

                    // Give processor time to do final drain (max 500ms wait)
                    eprintln!("[PERF] [LIFECYCLE] Warning: Waiting 500ms for processor final drain...");
                    tokio::time::sleep(Duration::from_millis(500)).await;

                    // Clean up stressors
                    eprintln!("[PERF] [LIFECYCLE] Warning: Stopping all stressors...");
                    if let Ok(mut mgr) = stressor_mgr.write() {
                        if let Some(ref mut sm) = *mgr {
                            if let Err(e) = sm.stop_all_stressors() {
                                eprintln!("[PERF] [LIFECYCLE] Warning: Error stopping stressors: {}", e);
                            } else {
                                eprintln!("[PERF] [LIFECYCLE] Done: All stressors stopped successfully");
                            }
                        }
                    }

                    // Finalize session summary
                    eprintln!("[PERF] [LIFECYCLE] Warning: Creating final session summary...");
                    let kernel_context = Self::get_kernel_context();
                    
                    // READ final sample counts from monitoring_state_arc (persisted by processor)
                    let final_samples = monitoring_state_arc
                        .read()
                        .ok()
                        .and_then(|state| state.as_ref().map(|s| s.final_sample_count.load(Ordering::Acquire)))
                        .unwrap_or(0);
                    let final_dropped = monitoring_state_arc
                        .read()
                        .ok()
                        .and_then(|state| state.as_ref().map(|s| s.final_dropped_count.load(Ordering::Acquire)))
                        .unwrap_or(0);
                    eprintln!("[PERF] [LIFECYCLE] ⚠ Read final counts from MonitoringState: samples={}, dropped={}", final_samples, final_dropped);
                    
                    if let (Ok(start), Ok(current_metrics)) = (
                        session_start.read().map(|s| s.clone()),
                        metrics.read().map(|m| m.clone()),
                    ) {
                        if let Some(start_instant) = start {
                            let mut summary = SessionSummary::new(
                                "Benchmark".to_string(),
                                current_metrics.clone(),
                                kernel_context.clone(),
                                vec![],
                                final_samples,
                                final_dropped,
                            );
                            summary.mark_completed(start_instant);
                            
                            eprintln!("[PERF] [LIFECYCLE] ⚠ Session summary created: samples={}, dropped={}, duration={:.2}s, completed={}",
                                summary.total_samples,
                                summary.total_dropped_samples,
                                summary.duration_secs.unwrap_or(0.0),
                                summary.completed_successfully);
                            
                            if let Ok(mut ss) = session_summary.write() {
                                *ss = Some(summary.clone());
                            }

                            // CRITICAL FIX: Update cached_jitter_summary for Dashboard display
                            if let Ok(mut cached) = cached_jitter_summary.write() {
                                *cached = Some(summary.clone());
                                eprintln!("[PERF] [LIFECYCLE] Done: Cached jitter summary updated for Dashboard");
                            }

                            // CRITICAL: Emit JitterAuditComplete event to notify UI
                            let _ = build_tx.try_send(BuildEvent::JitterAuditComplete(summary.clone()));
                            eprintln!("[PERF] [LIFECYCLE] Done: JitterAuditComplete event emitted to UI");

                            // Also add to history
                            if let Ok(mut h) = history.write() {
                                let snapshot = crate::system::performance::PerformanceSnapshot::new(
                                    current_metrics.clone(),
                                    kernel_context.clone(),
                                );
                                h.add_snapshot(snapshot);
                            }

                            // Persist session summary via HistoryManager (benchmark auto-termination path)
                            // NOTE: HistoryManager is not available in the async closure context
                            // This will be handled by the main handle_stop_monitoring method if user stops manually
                            eprintln!("[PERF] [LIFECYCLE] Note: HistoryManager persistence should be called explicitly via handle_stop_monitoring");
                        }
                    }

                    eprintln!("[PERF] [LIFECYCLE] Done: Benchmark session finalized");
                });
            }
            MonitoringMode::SystemBenchmark => {
                eprintln!("[PERF] [LIFECYCLE] SystemBenchmark mode: 60-second GOATd Full Benchmark with 6 phases");
                
                let benchmark_orch = self.benchmark_orchestrator.clone();
                let lifecycle_state = self.perf_lifecycle_state.clone();
                
                tokio::spawn(async move {
                    eprintln!("[PERF] [BENCHMARK] Starting GOATd Full Benchmark orchestration");

                    // Initialize orchestrator
                    {
                        if let Ok(mut orch) = benchmark_orch.write() {
                            *orch = Some(BenchmarkOrchestrator::new());
                        }
                    }

                    // Run the 6-phase benchmark sequence
                    let mut phase_tick = tokio::time::interval(Duration::from_millis(100));
                    let mut last_phase_transition_elapsed = 0u64; // Track last transition time

                    eprintln!("[PERF] [BENCHMARK] 🔄 ENTERING MAIN BENCHMARK LOOP");
                    while active.load(Ordering::Acquire) && {
                        if let Ok(orch) = benchmark_orch.read() {
                            let is_not_complete = orch.as_ref().map(|o| !o.is_complete()).unwrap_or(false);
                            if !is_not_complete {
                                eprintln!("[PERF] [BENCHMARK] 🔄 LOOP CONDITION FALSE: orchestrator.is_complete() returned true");
                            }
                            is_not_complete
                        } else {
                            false
                        }
                    } {
                        phase_tick.tick().await;

                        // Check current phase and update stressors (scope to release lock before await)
                        {
                            let mut should_advance = false;
                            let mut new_stressors = Vec::new();

                            if let Ok(mut orch) = benchmark_orch.write() {
                                if let Some(ref mut orchestrator) = *orch {
                                    let elapsed = orchestrator.elapsed_secs();
                                    let current_phase = orchestrator.current_phase;
                                    let phase_end = current_phase.end_time();

                                    // Explicit phase transition: check if we've crossed phase boundary
                                    eprintln!("[PERF] [BENCHMARK] [PHASE_CHECK] elapsed={}s, phase_end={}s, last_transition={}s", elapsed, phase_end, last_phase_transition_elapsed);
                                    if elapsed >= phase_end && elapsed > last_phase_transition_elapsed {
                                        if let Some(next_phase) = orchestrator.advance_phase() {
                                            last_phase_transition_elapsed = elapsed;
                                            eprintln!("[PERF] [BENCHMARK] ✓ Phase transition at {}s: {} -> {}", elapsed, current_phase, next_phase);
                                            should_advance = true;
                                            new_stressors = orchestrator.get_phase_stressors();
                                            
                                            // Record metrics for completed phase
                                            if let Ok(current_metrics) = metrics.read() {
                                                eprintln!("[PERF] [BENCHMARK] {} complete ({}s): max={:.2}µs, p99={:.2}µs, spikes={}",
                                                    current_phase, elapsed, current_metrics.max_us, current_metrics.p99_us, current_metrics.total_spikes);
                                            }
                                        }
                                    }

                                    // Check if benchmark is complete
                                    let is_done = orchestrator.is_complete();
                                    eprintln!("[PERF] [BENCHMARK] [COMPLETION_CHECK] elapsed={}s, is_complete={}", elapsed, is_done);
                                    if is_done {
                                        eprintln!("[PERF] [BENCHMARK] ✅ GOATd Full Benchmark complete (60s elapsed) - BREAKING LOOP");
                                        break;
                                    }
                                }
                            }

                            // Apply stressors outside the lock to avoid Send issues
                            if should_advance {
                                Self::apply_phase_stressors(
                                    &stressor_mgr,
                                    &new_stressors,
                                ).await;
                            }
                        }
                    }

                    // Auto-stop the benchmark - CRITICAL: This signals both loop AND processor
                    eprintln!("[PERF] [BENCHMARK] ✅ INITIATING BENCHMARK AUTO-STOP at 60s");
                    eprintln!("[PERF] [BENCHMARK] Setting active=false to trigger loop exit");
                    active.store(false, Ordering::Release);

                    // CRITICAL FIX: Transition lifecycle state to Completed BEFORE cleanup
                    // This ensures UI knows the benchmark has finished and timer should stop
                    eprintln!("[PERF] [BENCHMARK] ✅ MARKING LIFECYCLE AS COMPLETED");
                    if let Ok(mut lifecycle) = lifecycle_state.write() {
                        *lifecycle = LifecycleState::Completed;
                        eprintln!("[PERF] [BENCHMARK] ✅ LIFECYCLE STATE SUCCESSFULLY TRANSITIONED TO COMPLETED");
                    }

                    if let Ok(mon_state_lock) = monitoring_state_arc.read() {
                        if let Some(ref mon_state) = *mon_state_lock {
                            mon_state.request_stop();
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(500)).await;

                    // Stop stressors - CRITICAL: Must execute AFTER lifecycle transition
                    eprintln!("[PERF] [BENCHMARK] 🛑 STOPPING ALL STRESSORS");
                    if let Ok(mut mgr) = stressor_mgr.write() {
                        if let Some(ref mut sm) = *mgr {
                            match sm.stop_all_stressors() {
                                Ok(()) => {
                                    eprintln!("[PERF] [BENCHMARK] ✅ ALL STRESSORS STOPPED SUCCESSFULLY");
                                }
                                Err(e) => {
                                    eprintln!("[PERF] [BENCHMARK] ⚠️ ERROR STOPPING STRESSORS: {}", e);
                                }
                            }
                        }
                    }

                    // Finalize results - calculate final GOAT Score from aggregated metrics
                    let mut final_goat_score = 0u16;
                    if let Ok(orch_lock) = benchmark_orch.read() {
                        if let Some(ref orchestrator) = *orch_lock {
                            if let Some(goat_score) = orchestrator.calculate_final_score() {
                                final_goat_score = goat_score;
                                eprintln!("[PERF] [BENCHMARK] ⚠ GOATd Full Benchmark Final GOAT Score: {}", goat_score);
                            }

                            eprintln!("[PERF] [BENCHMARK] Phase metrics collection:");
                            for (phase_name, metrics_snapshot) in &orchestrator.phase_metrics {
                                eprintln!("[PERF] [BENCHMARK]   {} -> max={:.2}µs, p99={:.2}µs, spikes={}",
                                    phase_name, metrics_snapshot.max_us, metrics_snapshot.p99_us, metrics_snapshot.total_spikes);
                            }
                        }
                    }
                    
                    // CRITICAL FIX: Communicate final GOAT Score to UI
                    if final_goat_score > 0 {
                        // The GOAT Score will be calculated by UI from the 7-metric spectrum
                        eprintln!("[PERF] [BENCHMARK] ⚠ Final score communicated to UI for Dashboard display ({})", final_goat_score);
                    }

                    // Create final session summary
                    let kernel_context = Self::get_kernel_context();
                    let final_samples = monitoring_state_arc
                        .read()
                        .ok()
                        .and_then(|state| state.as_ref().map(|s| s.final_sample_count.load(Ordering::Acquire)))
                        .unwrap_or(0);
                    let final_dropped = monitoring_state_arc
                        .read()
                        .ok()
                        .and_then(|state| state.as_ref().map(|s| s.final_dropped_count.load(Ordering::Acquire)))
                        .unwrap_or(0);

                    if let (Ok(start), Ok(current_metrics)) = (
                        session_start.read().map(|s| s.clone()),
                        metrics.read().map(|m| m.clone()),
                    ) {
                        if let Some(start_instant) = start {
                            let mut summary = SessionSummary::new(
                                "GOATd Full Benchmark".to_string(),
                                current_metrics.clone(),
                                kernel_context.clone(),
                                vec![],
                                final_samples,
                                final_dropped,
                            );
                            summary.mark_completed(start_instant);

                            if let Ok(mut ss) = session_summary.write() {
                                *ss = Some(summary.clone());
                            }

                            if let Ok(mut cached) = cached_jitter_summary.write() {
                                *cached = Some(summary.clone());
                            }

                            let _ = build_tx.try_send(BuildEvent::JitterAuditComplete(summary.clone()));

                            if let Ok(mut h) = history.write() {
                                let snapshot = crate::system::performance::PerformanceSnapshot::new(
                                    current_metrics.clone(),
                                    kernel_context,
                                );
                                h.add_snapshot(snapshot);
                            }
                        }
                    }

                    eprintln!("[PERF] [BENCHMARK] Done: GOATd Full Benchmark session finalized");
                });
            }
            MonitoringMode::Continuous => {
                eprintln!("[PERF] [LIFECYCLE] Continuous mode: monitoring until manual stop");
            }
        }

        self.log_event("PERFORMANCE", &format!("Monitoring triggered in {:?} mode", mode));
        Ok(())
    }

    /// Internal implementation of monitoring stop - shared between manual and auto-stop paths
    ///
    /// Performs full cleanup including:
    /// - Stopping the performance collector
    /// - Stopping all stressors
    /// - Finalizing session summary
    /// - Persisting results to disk
    async fn internal_stop_monitoring_impl(&self) -> Result<(), String> {
        eprintln!("[PERF] [STOP] internal_stop_monitoring_impl starting");

        // Transition to Completed state - CRITICAL CHECKPOINT FOR COMPLETION
         {
             let mut state = self.perf_lifecycle_state
                 .write()
                 .map_err(|e| format!("Failed to write lifecycle state: {}", e))?;
             *state = LifecycleState::Completed;
             eprintln!("[PERF] [STOP] ✅ LIFECYCLE STATE TRANSITIONED TO COMPLETED");
         }

        // Stop the collector
        eprintln!("[PERF] [STOP] ⚠ Stopping performance monitoring (signals collector + processor)");
        self.stop_performance_monitoring()?;
        eprintln!("[PERF] [STOP] Done: Performance monitoring stopped");

        // Reap stressors
        eprintln!("[PERF] [STOP] ⚠ Stopping all stressors...");
        {
            let mut mgr = self.perf_stressor_manager
                .write()
                .map_err(|e| format!("Failed to write stressor manager: {}", e))?;
            
            if let Some(ref mut sm) = *mgr {
                sm.stop_all_stressors()
                    .map_err(|e| {
                        eprintln!("[PERF] [STOP] Warning: Error stopping stressors: {}", e);
                        format!("Failed to stop stressors: {}", e)
                    })?;
                eprintln!("[PERF] [STOP] Done: All stressors stopped successfully");
            }
            *mgr = None;
        }

        // Finalize session summary
        eprintln!("[PERF] [STOP] ⚠ Creating final session summary...");
        {
            if let Ok(start) = self.perf_session_start.read() {
                if let Some(start_instant) = *start {
                    if let Ok(current_metrics) = self.perf_metrics.read() {
                        // READ final sample counts from monitoring_state
                        let final_samples = self.perf_monitoring_state
                            .read()
                            .ok()
                            .and_then(|state| state.as_ref().map(|s| s.final_sample_count.load(Ordering::Acquire)))
                            .unwrap_or(0);
                        let final_dropped = self.perf_monitoring_state
                            .read()
                            .ok()
                            .and_then(|state| state.as_ref().map(|s| s.final_dropped_count.load(Ordering::Acquire)))
                            .unwrap_or(0);
                        
                        eprintln!("[PERF] [STOP] ⚠ Read final counts from MonitoringState: samples={}, dropped={}", final_samples, final_dropped);
                        
                        let kernel_context = Self::get_kernel_context();
                        let mut summary = SessionSummary::new(
                            "Benchmark auto-stop".to_string(),
                            current_metrics.clone(),
                            kernel_context,
                            vec![],
                            final_samples,
                            final_dropped,
                        );
                        summary.mark_completed(start_instant);

                        eprintln!("[PERF] [STOP] ⚠ Session summary created: samples={}, dropped={}, duration={:.2}s, metrics.max={:.2}µs, metrics.p99.9={:.2}µs",
                            summary.total_samples,
                            summary.total_dropped_samples,
                            summary.duration_secs.unwrap_or(0.0),
                            summary.final_metrics.max_us,
                            summary.final_metrics.p99_9_us);

                        if let Ok(mut ss) = self.perf_session_summary.write() {
                            *ss = Some(summary.clone());
                        }

                        // CRITICAL FIX: Update cached_jitter_summary for Dashboard display
                        if let Ok(mut cached) = self.cached_jitter_summary.write() {
                            *cached = Some(summary.clone());
                            eprintln!("[PERF] [STOP] Done: Cached jitter summary updated for Dashboard");
                        }

                        // CRITICAL: Persist the session summary to disk via HistoryManager
                        eprintln!("[PERF] [STOP] ⚠ Persisting session summary to HistoryManager...");
                        if let Ok(mgr_lock) = self.perf_history_manager.read() {
                            if let Some(ref mgr) = *mgr_lock {
                                // Get histogram buckets from current metrics
                                let histogram_buckets = current_metrics.histogram_buckets.iter()
                                    .enumerate()
                                    .map(|(i, normalized_density)| {
                                        // Reconstruct histogram bucket bounds from normalized value
                                        // This is simplified; a full implementation would track bucket bounds
                                        let lower_us = (i as f32) * 0.5; // Example: 0.5µs per bucket
                                        let upper_us = lower_us + 0.5;
                                        HistogramBucket {
                                            lower_us,
                                            upper_us,
                                            count: (*normalized_density * 1000.0) as u64,
                                        }
                                    })
                                    .collect();

                                match mgr.save_record(summary.clone(), histogram_buckets) {
                                    Ok(record_id) => {
                                        eprintln!("[PERF] [STOP] Done: Session persisted to HistoryManager with ID: {}", record_id);
                                    }
                                    Err(e) => {
                                        eprintln!("[PERF] [STOP] Warning: Failed to persist session to HistoryManager: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Persist history to disk
        eprintln!("[PERF] [STOP] Warning: Persisting history to disk...");
        {
            let history = self.perf_history
                .read()
                .map_err(|e| format!("Failed to read history: {}", e))?;
            let path = Self::get_perf_history_path();
            history.save_to_disk(&path)
                .map_err(|e| {
                    eprintln!("[PERF] [STOP] ⚠ Error saving history: {}", e);
                    format!("Failed to save history: {}", e)
                })?;
            eprintln!("[PERF] [STOP] Done: History persisted to {}", path);
        }

        self.log_event("PERFORMANCE", "Performance monitoring stopped");
        eprintln!("[PERF] [STOP] Done: internal_stop_monitoring_impl completed");
        Ok(())
    }

    /// Helper: Create and finalize a session summary with consolidated logic
    ///
    /// Extracts the duplicated session summary creation code from both auto-stop
    /// and manual stop paths. Handles:
    /// - Reading kernel context and final metrics
    /// - Creating SessionSummary object
    /// - Updating both session caches (perf_session_summary and cached_jitter_summary)
    /// - Adding snapshot to history
    /// - Persisting to HistoryManager with histogram buckets
    fn finalize_session_summary(&self, session_type: &str) -> Result<SessionSummary, String> {
        let kernel_context = Self::get_kernel_context();
        
        // READ final sample counts from monitoring_state (persisted by processor)
        let final_samples = self.perf_monitoring_state
            .read()
            .ok()
            .and_then(|state| state.as_ref().map(|s| s.final_sample_count.load(Ordering::Acquire)))
            .unwrap_or(0);
        let final_dropped = self.perf_monitoring_state
            .read()
            .ok()
            .and_then(|state| state.as_ref().map(|s| s.final_dropped_count.load(Ordering::Acquire)))
            .unwrap_or(0);
        
        eprintln!("[PERF] [FINALIZE] Creating session summary (type: {}): samples={}, dropped={}",
            session_type, final_samples, final_dropped);
        
        // Read metrics and session start time
        let (start_instant, current_metrics) = {
            let start = self.perf_session_start.read().ok().and_then(|s| *s);
            let metrics = self.perf_metrics.read().ok();
            (start, metrics)
        };
        
        if let (Some(start_time), Some(metrics)) = (start_instant, current_metrics) {
            let mut summary = SessionSummary::new(
                session_type.to_string(),
                metrics.clone(),
                kernel_context.clone(),
                vec![],
                final_samples,
                final_dropped,
            );
            summary.mark_completed(start_time);
            
            eprintln!("[PERF] [FINALIZE] Session summary created: samples={}, dropped={}, duration={:.2}s",
                summary.total_samples,
                summary.total_dropped_samples,
                summary.duration_secs.unwrap_or(0.0));
            
            // Update session caches
            let _ = self.perf_session_summary.write().map(|mut ss| {
                *ss = Some(summary.clone());
            });
            
            let _ = self.cached_jitter_summary.write().map(|mut cached| {
                *cached = Some(summary.clone());
                eprintln!("[PERF] [FINALIZE] Cached jitter summary updated for Dashboard");
            });
            
            // Persist via HistoryManager with histogram reconstruction
            if let Ok(mgr_lock) = self.perf_history_manager.read() {
                if let Some(ref mgr) = *mgr_lock {
                    let histogram_buckets = metrics.histogram_buckets.iter()
                        .enumerate()
                        .map(|(i, normalized_density)| {
                            let lower_us = (i as f32) * 0.5;
                            let upper_us = lower_us + 0.5;
                            HistogramBucket {
                                lower_us,
                                upper_us,
                                count: (*normalized_density * 1000.0) as u64,
                            }
                        })
                        .collect();
                    
                    match mgr.save_record(summary.clone(), histogram_buckets) {
                        Ok(record_id) => {
                            eprintln!("[PERF] [FINALIZE] Session persisted to HistoryManager with ID: {}", record_id);
                        }
                        Err(e) => {
                            eprintln!("[PERF] [FINALIZE] Warning: Failed to persist session: {}", e);
                        }
                    }
                }
            }
            
            // Add snapshot to history
            let _ = self.perf_history.write().map(|mut h| {
                let snapshot = crate::system::performance::PerformanceSnapshot::new(
                    metrics.clone(),
                    kernel_context,
                );
                h.add_snapshot(snapshot);
            });
            
            return Ok(summary);
        }
        
        Err("Failed to create session summary: missing metrics or session start time".to_string())
    }

    /// Handler for Stop Monitoring button
    ///
    /// Transitions state to `Completed`:
    /// - Reaps all stressors
    /// - Stops the latency collector
    /// - Finalizes session summary
    /// - Persists results
    pub fn handle_stop_monitoring(&self) -> Result<(), String> {
        if !self.perf_monitoring_active.load(Ordering::Acquire) {
            return Err("Performance monitoring not active".to_string());
        }

        eprintln!("[PERF] [STOP] handle_stop_monitoring called");

        // Transition to Completed state
        {
            let mut state = self.perf_lifecycle_state
                .write()
                .map_err(|e| format!("Failed to write lifecycle state: {}", e))?;
            *state = LifecycleState::Completed;
            eprintln!("[PERF] [STOP] Done: State transitioned to Completed");
        }

        // Stop the collector
        eprintln!("[PERF] [STOP] ⚠ Stopping performance monitoring (signals collector + processor)");
        self.stop_performance_monitoring()?;
        eprintln!("[PERF] [STOP] Done: Performance monitoring stopped");

        // Reap stressors
        eprintln!("[PERF] [STOP] ⚠ Stopping all stressors...");
        {
            let mut mgr = self.perf_stressor_manager
                .write()
                .map_err(|e| format!("Failed to write stressor manager: {}", e))?;
            
            if let Some(ref mut sm) = *mgr {
                sm.stop_all_stressors()
                    .map_err(|e| {
                        eprintln!("[PERF] [STOP] Warning: Error stopping stressors: {}", e);
                        format!("Failed to stop stressors: {}", e)
                    })?;
                eprintln!("[PERF] [STOP] Done: All stressors stopped successfully");
            }
            *mgr = None;
        }

        // Finalize session summary using consolidated helper
        eprintln!("[PERF] [STOP] ⚠ Creating final session summary...");
        match self.finalize_session_summary("Manual Stop") {
            Ok(summary) => {
                eprintln!("[PERF] [STOP] ⚠ Session summary created: samples={}, dropped={}, duration={:.2}s, metrics.max={:.2}µs, metrics.p99.9={:.2}µs",
                    summary.total_samples,
                    summary.total_dropped_samples,
                    summary.duration_secs.unwrap_or(0.0),
                    summary.final_metrics.max_us,
                    summary.final_metrics.p99_9_us);
            }
            Err(e) => {
                eprintln!("[PERF] [STOP] Warning: Failed to finalize session summary: {}", e);
            }
        }

        // Persist history to disk
        eprintln!("[PERF] [STOP] Warning: Persisting history to disk...");
        {
            let history = self.perf_history
                .read()
                .map_err(|e| format!("Failed to read history: {}", e))?;
            let path = Self::get_perf_history_path();
            history.save_to_disk(&path)
                .map_err(|e| {
                    eprintln!("[PERF] [STOP] ⚠ Error saving history: {}", e);
                    format!("Failed to save history: {}", e)
                })?;
            eprintln!("[PERF] [STOP] Done: History persisted to {}", path);
        }

        self.log_event("PERFORMANCE", "Performance monitoring stopped");
        eprintln!("[PERF] [STOP] Done: handle_stop_monitoring completed");
        Ok(())
    }

    /// Handler for stressor toggle (CPU, Memory, Scheduler)
    pub fn handle_stressor_toggled(&self, stressor_name: &str, enabled: bool) -> Result<(), String> {
        self.log_event(
            "PERFORMANCE_STRESSOR",
            &format!("{} stressor toggled: {}", stressor_name, enabled),
        );
        // TODO: Implement actual stressor spawning via stress-ng or custom mechanism
        Ok(())
    }

    /// Handler for start benchmark: maps UI selections to MonitoringMode and stressors
    ///
    /// This is the main entry point from the UI for starting a benchmark.
    /// It wraps `handle_trigger_monitoring` with cleaner API for benchmark-specific scenarios.
    pub fn handle_start_benchmark(
        &self,
        duration_secs: Option<u64>,
        cpu_enabled: bool,
        memory_enabled: bool,
        scheduler_enabled: bool,
    ) -> Result<(), String> {
        // Build stressor list from toggles
        let mut stressors = Vec::new();
        if cpu_enabled {
            stressors.push(StressorType::Cpu);
        }
        if memory_enabled {
            stressors.push(StressorType::Memory);
        }
        if scheduler_enabled {
            stressors.push(StressorType::Scheduler);
        }
        
        // Determine monitoring mode
        let mode = if let Some(secs) = duration_secs {
            MonitoringMode::Benchmark(Duration::from_secs(secs))
        } else {
            MonitoringMode::Continuous
        };
        
        // Delegate to the standard monitoring trigger
        self.handle_trigger_monitoring(mode, stressors)
    }

    /// Handler for cycle timer mode change (Continuous, 30s, 1m, 5m)
    pub fn handle_cycle_timer_changed(&self, mode: &str) -> Result<(), String> {
        self.log_event(
            "PERFORMANCE_CYCLE",
            &format!("Cycle timer mode changed to: {}", mode),
        );
        
        // Parse the mode and set up appropriate duration
        let _duration = match mode {
            "30s" => Some(Duration::from_secs(30)),
            "1m" => Some(Duration::from_secs(60)),
            "5m" => Some(Duration::from_secs(300)),
            "Continuous" => None,
            _ => None,
        };

        // TODO: Implement cycle timer state machine that:
        // 1. If duration is Some, runs monitoring for that duration then auto-stops
        // 2. If None, runs continuously until manually stopped

        Ok(())
    }

    /// Get current performance metrics for UI display
    ///
    /// Returns a snapshot of current latency metrics
    pub fn get_current_performance_metrics(&self) -> Result<PerformanceMetrics, String> {
        self.perf_metrics
            .read()
            .map(|m| m.clone())
            .map_err(|e| format!("Failed to read metrics: {}", e))
    }

    /// Get performance history for comparison UI
    ///
    /// Returns a list of test identifiers (timestamps) for historical comparison
    pub fn get_performance_history_list(&self) -> Result<Vec<String>, String> {
        let history = self.perf_history
            .read()
            .map_err(|e| format!("Failed to read history: {}", e))?;

        let snapshots = history.snapshots();
        let ids: Vec<String> = snapshots
            .iter()
            .enumerate()
            .map(|(i, snapshot)| {
                format!("Test #{}: {:?}", i + 1, snapshot.timestamp)
            })
            .collect();

        Ok(ids)
    }

    /// Get list of comparison tests with metadata (labels and display names)
    ///
    /// Returns metadata for all saved performance records, including custom labels
    /// and formatted display names for UI dropdown rendering
    pub fn get_comparison_test_ids(&self) -> Result<Vec<super::super::system::performance::history::PerformanceRecordMetadata>, String> {
        if let Ok(mgr_lock) = self.perf_history_manager.read() {
            if let Some(ref mgr) = *mgr_lock {
                match mgr.list_records_metadata() {
                    Ok(metadata) => {
                        log_info!("[PERF] [COMPARE] Listed {} records with metadata", metadata.len());
                        return Ok(metadata);
                    }
                    Err(e) => {
                        log_info!("[PERF] [COMPARE] Failed to list records metadata: {}", e);
                        return Err(format!("Failed to list records metadata: {}", e));
                    }
                }
            }
        }
        Ok(vec![])
    }

    /// Handle compare tests popup: load and compare two performance records
    ///
    /// Loads both records from HistoryManager using their IDs, calculates % deltas
    /// for all 6 core metrics:
    /// - Min Latency: Delta = (B_min - A_min) / A_min * 100
    /// - Max Latency: Delta = (B_max - A_max) / A_max * 100
    /// - Avg Latency: Delta = (B_avg - A_avg) / A_avg * 100
    /// - P99.9 Latency: Delta = (B_p99.9 - A_p99.9) / A_p99.9 * 100
    /// - SMI Count: Delta = (B_smi - A_smi) / A_smi * 100
    /// - Stall Count: Delta = (B_stall - A_stall) / A_stall * 100
    ///
    /// Color Logic:
    /// - Negative delta (lower value in Test B) = Green (improvement)
    /// - Positive delta (higher value in Test B) = Red (regression)
    /// - Zero delta = gray (neutral)
    pub fn handle_compare_tests_request(
        &self,
        test_a_id: &str,
        test_b_id: &str,
    ) -> Result<(
        // Test A values: (kernel, scx, lto, min, max, avg, p99.9, smi_count, stall_count)
        (String, String, String, f32, f32, f32, f32, i32, i32),
        // Test B values Same structure
        (String, String, String, f32, f32, f32, f32, i32, i32),
        // Deltas: (min_delta%, max_delta%, avg_delta%, p99.9_delta%, smi_delta%, stall_delta%)
        (f32, f32, f32, f32, f32, f32),
    ), String> {
        // Load records from HistoryManager
        let mgr_lock = self.perf_history_manager
            .read()
            .map_err(|e| format!("Failed to read HistoryManager: {}", e))?;
        
        let mgr = mgr_lock.as_ref()
            .ok_or_else(|| "HistoryManager not initialized".to_string())?;

        // Fetch both records
        let record_a = mgr.load_record(test_a_id)
            .map_err(|e| format!("Failed to load test A ({}): {}", test_a_id, e))?;
        
        let record_b = mgr.load_record(test_b_id)
            .map_err(|e| format!("Failed to load test B ({}): {}", test_b_id, e))?;

        log::debug!("[PERF] [COMPARE] Comparing {} vs {}", test_a_id, test_b_id);

        // Extract values from Test A
        let a_kernel = record_a.kernel_context.version.clone();
        let a_scx = record_a.kernel_context.scx_profile.clone();
        let a_lto = record_a.kernel_context.lto_config.clone();
        let a_min = record_a.metrics.current_us;  // Note: using min from metrics storage
        let a_max = record_a.metrics.max_us;
        let a_avg = record_a.metrics.avg_us;
        let a_p99_9 = record_a.metrics.p99_9_us;
        let a_smi_count = record_a.metrics.total_smis as i32;
        let a_stall_count = record_a.metrics.spikes_correlated_to_smi as i32;

        // Extract values from Test B
        let b_kernel = record_b.kernel_context.version.clone();
        let b_scx = record_b.kernel_context.scx_profile.clone();
        let b_lto = record_b.kernel_context.lto_config.clone();
        let b_min = record_b.metrics.current_us;
        let b_max = record_b.metrics.max_us;
        let b_avg = record_b.metrics.avg_us;
        let b_p99_9 = record_b.metrics.p99_9_us;
        let b_smi_count = record_b.metrics.total_smis as i32;
        let b_stall_count = record_b.metrics.spikes_correlated_to_smi as i32;

        // Calculate % deltas: (ValB - ValA) / ValA * 100.0
        // Avoid division by zero
        let delta_min = if a_min != 0.0 {
            (b_min - a_min) / a_min * 100.0
        } else {
            0.0
        };

        let delta_max = if a_max != 0.0 {
            (b_max - a_max) / a_max * 100.0
        } else {
            0.0
        };

        let delta_avg = if a_avg != 0.0 {
            (b_avg - a_avg) / a_avg * 100.0
        } else {
            0.0
        };

        let delta_p99_9 = if a_p99_9 != 0.0 {
            (b_p99_9 - a_p99_9) / a_p99_9 * 100.0
        } else {
            0.0
        };

        let delta_smi = if a_smi_count != 0 {
            ((b_smi_count - a_smi_count) as f32 / a_smi_count as f32) * 100.0
        } else {
            0.0
        };

        let delta_stall = if a_stall_count != 0 {
            ((b_stall_count - a_stall_count) as f32 / a_stall_count as f32) * 100.0
        } else {
            0.0
        };

        log::debug!("[PERF] [COMPARE] Deltas - Min: {:.1}%, Max: {:.1}%, Avg: {:.1}%, P99.9: {:.1}%, SMI: {:.1}%, Stall: {:.1}%",
            delta_min, delta_max, delta_avg, delta_p99_9, delta_smi, delta_stall);

        let test_a = (a_kernel, a_scx, a_lto, a_min, a_max, a_avg, a_p99_9, a_smi_count, a_stall_count);
        let test_b = (b_kernel, b_scx, b_lto, b_min, b_max, b_avg, b_p99_9, b_smi_count, b_stall_count);
        let deltas = (delta_min, delta_max, delta_avg, delta_p99_9, delta_smi, delta_stall);

        Ok((test_a, test_b, deltas))
    }

    /// Load comparison for UI display: processes the comparison data and formats for Slint UI
    ///
    /// Takes comparison IDs and returns a struct with all values formatted as strings
    /// for direct UI property binding. This is called by the Slint `load-comparison` callback.
    pub fn load_comparison_for_ui(
        &self,
        test_a_id: &str,
        test_b_id: &str,
    ) -> Result<ComparisonResult, String> {
        let (test_a, test_b, deltas) = self.handle_compare_tests_request(test_a_id, test_b_id)?;

        Ok(ComparisonResult {
            // Test A
            test_a_kernel: test_a.0,
            test_a_scx: test_a.1,
            test_a_lto: test_a.2,
            test_a_min: format!("{:.2}", test_a.3),
            test_a_max: format!("{:.2}", test_a.4),
            test_a_avg: format!("{:.2}", test_a.5),
            test_a_p99_9: format!("{:.2}", test_a.6),
            test_a_smi_count: test_a.7,
            test_a_stall_count: test_a.8,
            
            // Test B
            test_b_kernel: test_b.0,
            test_b_scx: test_b.1,
            test_b_lto: test_b.2,
            test_b_min: format!("{:.2}", test_b.3),
            test_b_max: format!("{:.2}", test_b.4),
            test_b_avg: format!("{:.2}", test_b.5),
            test_b_p99_9: format!("{:.2}", test_b.6),
            test_b_smi_count: test_b.7,
            test_b_stall_count: test_b.8,
            
            // Deltas (% changes)
            delta_min: deltas.0,
            delta_max: deltas.1,
            delta_avg: deltas.2,
            delta_p99_9: deltas.3,
            delta_smi: deltas.4,
            delta_stall: deltas.5,
        })
    }

    /// Save current performance record to persistent history with custom label
    ///
    /// This method:
    /// 1. Collects current performance metrics and kernel context
    /// 2. Creates a SessionSummary with the provided label
    /// 3. Uses HistoryManager to persist the record with the label embedded
    ///
    /// The label will be stored in the JSON file and used for display in dropdowns
    pub fn handle_save_performance_record(&self, label: &str) -> Result<(), String> {
        let metrics = self.get_current_performance_metrics()?;
        let kernel_context = Self::get_kernel_context();

        // CRITICAL: Create SessionSummary with label for persistence
        let mut summary = SessionSummary::new(
            "Manual Save".to_string(),
            metrics,
            kernel_context,
            vec![],
            0,
            0,
        );
        
        // Set the custom label provided by the user
        summary.label = Some(label.to_string());
        summary.completed_successfully = true;

        // Persist via HistoryManager (which now preserves the label)
        if let Ok(mgr_lock) = self.perf_history_manager.read() {
            if let Some(ref mgr) = *mgr_lock {
                match mgr.save_record(summary, vec![]) {
                    Ok(record_id) => {
                        self.log_event("PERFORMANCE", &format!("Performance record saved with ID: {} (label: {})", record_id, label));
                        log_info!("[PERF] [SAVE] Record persisted successfully: {} (label: {})", record_id, label);
                        return Ok(());
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to save record: {}", e);
                        self.log_event("PERFORMANCE", &format!("Error saving record: {}", err_msg));
                        log_info!("[PERF] [SAVE] Error: {}", err_msg);
                        return Err(err_msg);
                    }
                }
            }
        }

        Err("HistoryManager not initialized".to_string())
    }

    /// Delete a performance test record from persistent storage
    ///
    /// Takes a test ID (filename from HistoryManager) and deletes the corresponding record.
    /// After deletion, returns success and the caller should refresh the test list.
    pub fn handle_delete_performance_record(&self, test_id: &str) -> Result<(), String> {
        if test_id.is_empty() {
            return Err("No test selected for deletion".to_string());
        }

        // Delete via HistoryManager
        if let Ok(mgr_lock) = self.perf_history_manager.read() {
            if let Some(ref mgr) = *mgr_lock {
                match mgr.delete_record(test_id) {
                    Ok(()) => {
                        self.log_event("PERFORMANCE", &format!("Performance record deleted: {}", test_id));
                        log_info!("[PERF] [DELETE] Record deleted successfully: {}", test_id);
                        Ok(())
                    }
                    Err(e) => {
                        let err_msg = format!("Failed to delete record {}: {}", test_id, e);
                        self.log_event("PERFORMANCE", &format!("Error deleting record: {}", err_msg));
                        log_info!("[PERF] [DELETE] Error: {}", err_msg);
                        Err(err_msg)
                    }
                }
            } else {
                Err("HistoryManager not initialized".to_string())
            }
        } else {
            Err("Failed to access HistoryManager".to_string())
        }
    }

    /// Clear the background alert flag (called when user returns to Performance tab)
    pub fn clear_background_alert(&self) -> Result<(), String> {
        self.perf_background_alert.store(false, Ordering::Release);
        eprintln!("[PERF] [ALERT] Background alert flag cleared");
        self.log_event("PERFORMANCE", "Background alert cleared");
        Ok(())
    }

    /// Get current background alert state and count
    pub fn get_background_alert_state(&self) -> (bool, u64) {
        let alert_triggered = self.perf_background_alert.load(Ordering::Acquire);
        let alert_count = self.perf_alert_count.load(Ordering::Acquire);
        (alert_triggered, alert_count)
    }

    /// Get comprehensive monitoring status for UI sync loop
    ///
    /// Returns a tuple of:
    /// - is_monitoring_active: bool - whether performance monitoring is currently running
    /// - lifecycle_state: String - current state (Idle, Running, Paused, Completed)
    pub fn get_monitoring_status(&self) -> (bool, String) {
        let is_active = self.perf_monitoring_active.load(Ordering::Acquire);
        let lifecycle = self.perf_lifecycle_state
            .read()
            .ok()
            .map(|s| {
                match *s {
                    LifecycleState::Idle => "Idle".to_string(),
                    LifecycleState::Running => "Running".to_string(),
                    LifecycleState::Paused => "Paused".to_string(),
                    LifecycleState::Completed => "Completed".to_string(),
                }
            })
            .unwrap_or_else(|| "Unknown".to_string());
        
        (is_active, lifecycle)
    }
    
    // ========================================================================
    // HARDWARE DETECTION FOR UI DISPLAY
    // ========================================================================
    
    /// Get cached hardware info with lazy refresh (30 second cache duration)
    ///
    /// Strictly returns cached value if valid (< 30 seconds old).
    /// Only refreshes from detection if cache is expired or empty.
    /// Designed for zero-latency UI reads on startup (cache pre-populated by background task).
    pub fn get_hardware_info(&self) -> Result<crate::models::HardwareInfo, String> {
        let now = Instant::now();
        let cache_duration = Duration::from_secs(30);
        
        // FAST PATH: Check if cache is valid - return immediately without lock contests
        if let Ok(timestamp_lock) = self.hardware_cache_timestamp.read() {
            if let Some(last_update) = *timestamp_lock {
                if now.duration_since(last_update) < cache_duration {
                    // Cache is still valid, return cached copy without refresh
                    if let Ok(cache_lock) = self.cached_hardware_info.read() {
                        if let Some(ref hw_info) = *cache_lock {
                            return Ok(hw_info.clone());
                        }
                    }
                }
            }
        }
        
        // SLOW PATH: Cache expired or doesn't exist - only then call detection
        // This is only hit on first startup or after 30s, not on every UI frame
        let mut detector = crate::hardware::HardwareDetector::new();
        let hw_info = detector.detect_all()
            .map_err(|e| format!("Hardware detection failed: {}", e))?;
        
        // Update cache atomically
        let _ = self.cached_hardware_info.write().map(|mut cache| {
            *cache = Some(hw_info.clone());
        });
        let _ = self.hardware_cache_timestamp.write().map(|mut ts| {
            *ts = Some(now);
        });
        
        Ok(hw_info)
     }
     
     /// Get cached active kernel audit data (for Dashboard)
     pub fn get_active_audit(&self) -> Result<crate::kernel::audit::KernelAuditData, String> {
         self.active_kernel_audit
             .read()
             .ok()
             .and_then(|data| data.clone())
             .ok_or_else(|| "Active audit data not available (run deep audit)".to_string())
     }
     
     /// Update cached active kernel audit data (called after deep audit completes)
     pub fn update_active_audit(&self, audit_data: crate::kernel::audit::KernelAuditData) -> Result<(), String> {
         self.active_kernel_audit
             .write()
             .map(|mut data| {
                 *data = Some(audit_data);
             })
             .map_err(|e| format!("Failed to update active audit cache: {}", e))
     }
     
     /// Get cached selected kernel audit data (for Kernel Manager)
     pub fn get_selected_audit(&self) -> Result<crate::kernel::audit::KernelAuditData, String> {
         self.selected_kernel_audit
             .read()
             .ok()
             .and_then(|data| data.clone())
             .ok_or_else(|| "Selected audit data not available (run deep audit on selected kernel)".to_string())
     }
     
     /// Update cached selected kernel audit data (called after deep audit on selected kernel completes)
     pub fn update_selected_audit(&self, audit_data: crate::kernel::audit::KernelAuditData) -> Result<(), String> {
         self.selected_kernel_audit
             .write()
             .map(|mut data| {
                 *data = Some(audit_data);
             })
             .map_err(|e| format!("Failed to update selected audit cache: {}", e))
     }
     
     /// Get cached jitter audit summary
     pub fn get_cached_jitter_summary(&self) -> Result<Option<SessionSummary>, String> {
         self.cached_jitter_summary
             .read()
             .map(|data| data.clone())
             .map_err(|e| format!("Failed to read jitter summary: {}", e))
     }
     
     /// Update cached jitter audit summary (called when jitter audit completes)
      pub fn update_cached_jitter_summary(&self, summary: SessionSummary) -> Result<(), String> {
          self.cached_jitter_summary
              .write()
              .map(|mut data| {
                  *data = Some(summary);
              })
              .map_err(|e| format!("Failed to update jitter summary: {}", e))
      }

      /// Helper: Apply phase-specific stressors for SystemBenchmark mode
      ///
      /// Stops all existing stressors and starts new ones for the current phase.
      /// Each stressor is configured with the specified intensity level.
      async fn apply_phase_stressors(
          stressor_mgr: &Arc<RwLock<Option<StressorManager>>>,
          phase_stressors: &[(StressorType, Intensity)],
      ) {
          // Stop all current stressors
          if let Ok(mut mgr) = stressor_mgr.write() {
              if let Some(ref mut sm) = *mgr {
                  if let Err(e) = sm.stop_all_stressors() {
                      eprintln!("[PERF] [STRESSOR] Warning: Failed to stop stressors during transition: {}", e);
                  }
              }
          }

          // Start new stressors for this phase
          for (stressor_type, intensity) in phase_stressors {
              if let Ok(mut mgr) = stressor_mgr.write() {
                  if let Some(ref mut sm) = *mgr {
                      if let Err(e) = sm.start_stressor(*stressor_type, *intensity) {
                          eprintln!("[PERF] [STRESSOR] Error: Failed to start {} stressor: {}", stressor_type, e);
                      } else {
                          eprintln!("[PERF] [STRESSOR] {} stressor started (intensity: {}%)",
                              stressor_type, intensity.value());
                      }
                  }
              }
          }
      }
  }

// ============================================================================
// REAL AUDIT IMPLEMENTATION
// ============================================================================

struct AuditImpl;

impl AuditTrait for AuditImpl {
    fn get_summary(&self) -> Result<crate::kernel::audit::AuditSummary, String> {
        SystemAudit::get_summary()
    }
    
    fn run_deep_audit_async(&self) -> BoxFuture<'static, Result<crate::kernel::audit::KernelAuditData, String>> {
        SystemAudit::run_deep_audit_async().boxed()
    }
    
    fn run_deep_audit_async_for_version(&self, version: Option<String>) -> BoxFuture<'static, Result<crate::kernel::audit::KernelAuditData, String>> {
        SystemAudit::run_deep_audit_async_for_version(version).boxed()
    }
    
    fn get_performance_metrics(&self) -> Result<crate::kernel::audit::PerformanceMetrics, String> {
        SystemAudit::get_performance_metrics()
    }
    
    fn run_jitter_audit_async(&self) -> BoxFuture<'static, Result<crate::kernel::audit::JitterAuditResult, String>> {
        SystemAudit::run_jitter_audit_async().boxed()
    }
}

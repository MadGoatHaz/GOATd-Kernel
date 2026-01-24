/// Main App Orchestrator and UI State Management
/// 
/// This module provides the central application state and the eframe::App implementation
/// for the egui-based frontend. It manages tab routing, async event processing, and
/// delegates to specialized view modules for rendering.

use eframe::egui;
use std::sync::Arc;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use crate::kernel::manager::KernelPackage;

/// Tab identifiers for navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Build,
    Performance,
    KernelManager,
    Settings,
}

/// Transient UI state - state that doesn't persist across sessions
/// Note: Default impl is manual (see below) to initialize cached_scx_readiness
#[derive(Clone)]
pub struct UIState {
    /// Dirty flag: set when data changes, cleared after render
    /// Used for adaptive repainting instead of fixed-frequency repaints
    pub needs_repaint: bool,
    
    /// Last repaint time (for idle-based fallback repaints)
    pub last_repaint_time: Instant,
    
    /// Currently active tab
    pub active_tab: Tab,
    
    /// Build log buffer (fixed-size VecDeque of last N lines for O(1) appends)
    pub build_log: VecDeque<String>,
    
    /// Current build progress (0-100)
    pub build_progress: i32,
    
    /// Build status message
    pub build_status: String,
    
    /// Current build phase (e.g., "preparation", "building", "validation")
    pub current_build_phase: String,
    
    /// Whether a build is currently in progress
    pub is_building: bool,
    
    /// Build elapsed time in seconds
    pub build_elapsed_seconds: u64,
    
    /// Build error messages (separate from general error_message for isolated display)
    pub build_errors: Vec<String>,
    
    /// Path to the current build's full log file on disk
    pub build_log_file_path: Option<String>,
    
    /// Whether to show the comparison modal
    pub show_compare_popup: bool,
    
    /// Selected kernel for audit details
    pub selected_kernel_name: String,
    
    /// Error message to display (if any)
    pub error_message: Option<String>,
    
    /// Success message to display (if any)
    pub success_message: Option<String>,
    
    /// Build configuration: selected variant
    pub selected_variant: usize,
    
    /// Build configuration: selected profile
    pub selected_profile: usize,
    
    /// Build configuration: selected LTO level
    pub selected_lto: usize,
    
    /// Build configuration: selected hardening level
    pub selected_hardening: usize,
    
    /// Use modprobed-db for driver filtering
    pub use_modprobed: bool,
    
    /// Use whitelist safety net
    pub use_whitelist: bool,
    
    /// Use Polly LLVM vectorization
    pub use_polly: bool,
    
    /// Use MGLRU memory management
    pub use_mglru: bool,
    
    /// Use native optimizations (-march=native)
    pub native_optimizations: bool,
    
    /// Selected SCX profile index
    pub selected_scx_profile: Option<usize>,
    
    /// Current SCX status (Enabled/Disabled)
    pub scx_enabled: bool,
    
    /// Active SCX scheduler binary (e.g., "scx_bpfland")
    pub active_scx_binary: String,
    
    /// SCX activation in progress
    pub scx_activating: bool,
    
    /// Index of the selected scheduler in the available list (granular SCX control)
    pub selected_scx_type_idx: Option<usize>,
    
    /// Index of the selected mode (granular SCX control)
    pub selected_scx_mode_idx: Option<usize>,
    
    /// List of detected SCX scheduler binaries (e.g., ["scx_bpfland", "scx_lavd"])
    pub available_scx_schedulers: Vec<String>,
    
    /// Flag to prevent redundant SCX detection polls when no schedulers are found
    pub scx_detection_attempted: bool,
    
    /// Informational message (for Polkit auth feedback)
    pub info_message: Option<String>,
    
    /// Settings UI state: persisted across renders
    pub settings_ui_state: super::settings::SettingsUIState,
    
    /// Selected kernel index for Kernel Manager (for row selection and Audit Details)
    pub selected_kernel_index: Option<usize>,
    
    /// Selected artifact index for Kernel Manager (for artifact selection and deletion)
    pub selected_artifact_index: Option<usize>,
    
    /// Installed kernels list (live from controller)
    pub installed_kernels: Vec<String>,
    
    /// Built artifacts list (live from controller) - stores actual KernelPackage objects with path info
    pub built_artifacts: Vec<KernelPackage>,
    
    /// Flag to track if kernel list has been initialized (prevents re-scanning on every frame)
    pub ui_state_initialized: bool,
    
    /// Map of kernel variant name to its latest fetched version string
    pub latest_versions: HashMap<String, String>,
    
    /// Set of variant names currently polling for latest version
    pub version_poll_active: HashSet<String>,
    
    /// Map of variant name to the last time it was successfully polled or attempted
    pub last_version_poll: HashMap<String, Instant>,
    
    /// Deep Audit Results - holds the last audit data
    pub deep_audit_results: Option<crate::kernel::audit::KernelAuditData>,
    
    /// Jitter Audit Summary - holds the last jitter audit data
    pub jitter_audit_summary: Option<crate::system::performance::SessionSummary>,
    
    /// Flag indicating Deep Audit is currently running
    pub is_auditing_deep: bool,
    
    /// Flag indicating Jitter Audit is currently running
    pub is_auditing_jitter: bool,
    
    /// SCX task completion flag (Arc<Mutex> for thread-safe signaling)
    pub scx_task_completion_flag: Option<Arc<std::sync::Mutex<bool>>>,
    
    /// SCX task start time (to track when async task was initiated)
    pub scx_task_start_time: Option<std::time::Instant>,
    
    /// Cached SCX readiness state (computed in update(), used in render)
    /// Prevents repeated expensive /proc/config.gz reads during frame rendering
    pub cached_scx_readiness: crate::system::scx::SCXReadiness,
    
    /// Last time SCX readiness was checked (for frame-throttling)
    /// Ensures get_scx_readiness() is called at most once per second
    pub last_scx_readiness_check: Option<Instant>,
    
    /// Active SCX metadata for current scheduler/mode selection
    /// Contains description, best-for use cases, CLI flags, and recommendation level
    pub active_scx_metadata: Option<crate::system::scx::ScxMetadata>,
    
    /// Flag to track if initial SCX scheduler synchronization has been performed
    /// Triggers one-time mirroring of active scheduler to UI selections on startup
    pub scx_initial_sync_done: bool,
    
    /// Cached theme index to avoid recalculating visuals
    pub cached_theme_idx: Option<usize>,
    
    /// Cached egui Visuals object to avoid re-creation on every frame
    pub cached_visuals: Option<egui::Visuals>,
    
    /// Cached list of missing AUR packages from the last health check
    /// Used to determine if UI features should be blocked due to missing dependencies
    pub missing_aur_packages: Vec<String>,
    
    /// Cached list of missing optional tools from the last health check
    /// Used to determine if UI features should be blocked due to missing dependencies (e.g., scx-tools)
    pub missing_optional_tools: Vec<String>,
    
    /// Whether to show the fix system health modal
    pub show_fix_modal: bool,
    
    /// Pending privileged command to execute after user confirmation
    pub pending_fix_command: String,
    
    /// Copy-to-clipboard feedback state: (message, timestamp)
    /// Used to show temporary "✓ Copied!" feedback for 2 seconds
    pub copy_to_clipboard_feedback: Option<(String, std::time::Instant)>,
}

impl Default for Tab {
    fn default() -> Self {
        Tab::Dashboard
    }
}

impl Default for UIState {
    fn default() -> Self {
        use crate::system::scx::SCXReadiness;
        Self {
            needs_repaint: true,
            last_repaint_time: Instant::now(),
            active_tab: Tab::default(),
            build_log: VecDeque::with_capacity(5000),
            build_progress: 0,
            build_status: String::new(),
            current_build_phase: String::new(),
            is_building: false,
            build_elapsed_seconds: 0,
            build_errors: Vec::new(),
            build_log_file_path: None,
            show_compare_popup: false,
            selected_kernel_name: String::new(),
            error_message: None,
            success_message: None,
            selected_variant: 0,
            selected_profile: 0,
            selected_lto: 0,
            selected_hardening: 0,
            use_modprobed: false,
            use_whitelist: false,
            use_polly: false,
            use_mglru: false,
            native_optimizations: true,
            selected_scx_profile: None,
            scx_enabled: false,
            active_scx_binary: String::new(),
            scx_activating: false,
            selected_scx_type_idx: None,
            selected_scx_mode_idx: None,
            available_scx_schedulers: Vec::new(),
            scx_detection_attempted: false,
            info_message: None,
            settings_ui_state: super::settings::SettingsUIState::default(),
            selected_kernel_index: None,
            selected_artifact_index: None,
            installed_kernels: Vec::new(),
            built_artifacts: Vec::new(),
            ui_state_initialized: false,
            latest_versions: HashMap::new(),
            version_poll_active: HashSet::new(),
            last_version_poll: HashMap::new(),
            deep_audit_results: None,
            jitter_audit_summary: None,
            is_auditing_deep: false,
            is_auditing_jitter: false,
            scx_task_completion_flag: None,
            scx_task_start_time: None,
            cached_scx_readiness: SCXReadiness::Ready,
            last_scx_readiness_check: None,
            active_scx_metadata: None,
            scx_initial_sync_done: false,
            cached_theme_idx: None,
            cached_visuals: None,
            missing_aur_packages: Vec::new(),
            missing_optional_tools: Vec::new(),
            show_fix_modal: false,
            pending_fix_command: String::new(),
            copy_to_clipboard_feedback: None,
        }
    }
}

/// Main Application UI Structure
pub struct AppUI {
    /// Persistent application state (profiles, settings, hardware info)
    pub controller: Arc<RwLock<AppController>>,
    
    /// Transient UI state
    pub ui_state: UIState,
    
    /// Channel receiver for build events
    pub build_rx: Option<tokio::sync::mpsc::Receiver<crate::ui::controller::BuildEvent>>,
    
    /// Context handle for requesting repaints from background threads
    pub ctx_handle: Option<egui::Context>,
}

impl AppUI {
    /// Create a new AppUI instance
    pub fn new(
        controller: Arc<RwLock<AppController>>,
        build_rx: Option<tokio::sync::mpsc::Receiver<crate::ui::controller::BuildEvent>>,
    ) -> Self {
        Self {
            controller,
            ui_state: UIState::default(),
            build_rx,
            ctx_handle: None,
        }
    }
    
    /// Process all pending build events from the channel
    /// Sets dirty flag when data changes (adaptive repaint trigger)
    fn process_build_events(&mut self) {
        if let Some(ref mut rx) = self.build_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    crate::ui::controller::BuildEvent::Progress(progress) => {
                        self.ui_state.build_progress = (progress * 100.0) as i32;
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::StatusUpdate(status) => {
                        self.ui_state.build_status = status.clone();
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::Status(status) => {
                        self.ui_state.build_status = status.clone();
                        
                        // CRITICAL: Capture current session log path when build starts
                        if status.contains("Starting kernel build") ||
                           status.contains("preparation") {
                            if let Ok(controller) = self.controller.try_read() {
                                if let Some(ref log_collector) = controller.log_collector {
                                    if let Some(path) = log_collector.get_session_log_path() {
                                        self.ui_state.build_log_file_path =
                                            Some(path.to_string_lossy().to_string());
                                        log::debug!("[BUILD] [SESSION] Captured log path for UI: {}",
                                            self.ui_state.build_log_file_path.as_ref().unwrap());
                                    }
                                }
                            }
                        }
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::Log(msg) => {
                        self.ui_state.build_log.push_back(msg);
                        // Cap log to fixed 5000 lines max (O(1) on overflow)
                        // VecDeque automatically maintains capacity in amortized O(1)
                        const MAX_LOG_LINES: usize = 5000;
                        while self.ui_state.build_log.len() > MAX_LOG_LINES {
                            self.ui_state.build_log.pop_front();
                        }
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::PhaseChanged(phase) => {
                        // Track phase change in dedicated field AND log it
                        self.ui_state.current_build_phase = phase.clone();
                        self.ui_state.build_log.push_back(format!("[PHASE] {}", phase));
                        // Cap log to prevent overflow
                        const MAX_LOG_LINES: usize = 5000;
                        while self.ui_state.build_log.len() > MAX_LOG_LINES {
                            self.ui_state.build_log.pop_front();
                        }
                        log::debug!("[UI] Build phase changed: {}", phase);
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::Finished(success) => {
                        self.ui_state.is_building = false;
                        if success {
                            self.ui_state.success_message =
                                Some("Build completed successfully!".to_string());
                            self.ui_state.ui_state_initialized = false;
                        } else {
                            self.ui_state.error_message =
                                Some("Build failed. Check the log for details.".to_string());
                        }
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::TimerUpdate(seconds) => {
                        self.ui_state.build_elapsed_seconds = seconds;
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::Error(msg) => {
                        self.ui_state.is_building = false;
                        // Track errors in dedicated vec AND generate transient error message
                        self.ui_state.build_errors.push(msg.clone());
                        self.ui_state.error_message = Some(format!("Build error: {}", msg));
                        self.ui_state.build_log.push_back(format!("[ERROR] {}", msg));
                        // Cap log to prevent overflow
                        const MAX_LOG_LINES: usize = 5000;
                        while self.ui_state.build_log.len() > MAX_LOG_LINES {
                            self.ui_state.build_log.pop_front();
                        }
                        log::debug!("[UI] Build error: {}", msg);
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::InstallationComplete(success) => {
                        if success {
                            self.ui_state.success_message =
                                Some("Kernel installation completed successfully!".to_string());
                            self.ui_state.ui_state_initialized = false;
                        } else {
                            self.ui_state.error_message =
                                Some("Kernel installation failed. Check the log for details.".to_string());
                        }
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::KernelUninstalled => {
                        self.ui_state.ui_state_initialized = false;
                        self.ui_state.success_message =
                            Some("Kernel uninstalled successfully!".to_string());
                        log::debug!("[UI] Kernel uninstalled event received, refreshing Installed Kernels list");
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::LatestVersionUpdate(variant, version) => {
                        // Update the latest version for this variant and remove from polling set
                        self.ui_state.latest_versions.insert(variant.clone(), version);
                        self.ui_state.version_poll_active.remove(&variant);
                        log::debug!("[UI] Version update received for {}", variant);
                        // Version update affects UI display, so mark dirty
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::JitterAuditComplete(summary) => {
                        // Update UI state with the completed jitter audit summary
                        self.ui_state.jitter_audit_summary = Some(summary.clone());
                        self.ui_state.is_auditing_jitter = false;
                        log::debug!("[UI] Jitter audit completed: samples={}, duration={:.2}s",
                            summary.total_samples,
                            summary.duration_secs.unwrap_or(0.0));
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::ArtifactDeleted => {
                        // Artifact was successfully deleted, trigger refresh of Kernels Ready for Installation list
                        self.ui_state.ui_state_initialized = false;
                        log::debug!("[UI] Artifact deleted event received, refreshing Kernels Ready for Installation list");
                        self.ui_state.needs_repaint = true;
                    }
                    crate::ui::controller::BuildEvent::VersionResolved(version) => {
                        // Dynamic version was successfully resolved to a concrete version string
                        log::debug!("[UI] Version resolved: {}", version);
                        // Note: This is informational - the actual resolved version is used in the config
                        // The build log will also show this resolution, so we just trigger a repaint
                        self.ui_state.needs_repaint = true;
                    }
                }
            }
        }
   }
    
    /// Clear transient messages after displaying
    fn clear_transient_messages(&mut self, _ctx: &egui::Context) {
        // Keep messages for at least 1 second
        if self.ui_state.error_message.is_some()
            || self.ui_state.success_message.is_some() {
            // Messages will be cleared manually via UI or on timeout
            // For now, they persist in the UI layer
        }
    }
    
    /// Render the top navigation bar (tab selector)
    fn render_top_nav(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("GOATd Kernel");
                ui.separator();
                
                let tabs = vec![
                    (Tab::Dashboard, "Dashboard"),
                    (Tab::Build, "Build"),
                    (Tab::Performance, "Performance"),
                    (Tab::KernelManager, "Kernel Manager"),
                    (Tab::Settings, "Settings"),
                ];
                
                for (tab, label) in tabs {
                    let selected = self.ui_state.active_tab == tab;
                    if ui.selectable_label(selected, label).clicked() {
                        self.ui_state.active_tab = tab;
                    }
                }
            });
        });
    }
    
    /// Render transient messages (errors, success, info)
    fn render_messages(&mut self, ctx: &egui::Context) {
        if let Some(ref msg) = self.ui_state.error_message.clone() {
            egui::TopBottomPanel::top("error_panel").show(ctx, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 100, 100),
                    format!("Error: {}", msg)
                );
                if ui.button("Dismiss").clicked() {
                    self.ui_state.error_message = None;
                }
            });
        }
        
        if let Some(ref msg) = self.ui_state.success_message.clone() {
            egui::TopBottomPanel::top("success_panel").show(ctx, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 255, 100),
                    format!("{}", msg)
                );
                if ui.button("Dismiss").clicked() {
                    self.ui_state.success_message = None;
                }
            });
        }
        
        if let Some(ref msg) = self.ui_state.info_message.clone() {
            egui::TopBottomPanel::top("info_panel").show(ctx, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(100, 150, 255),
                    format!("{}", msg)
                );
                if ui.button("Dismiss").clicked() {
                    self.ui_state.info_message = None;
                }
            });
        }
    }
    
    /// Render the central content based on active tab
    fn render_content(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let controller = self.controller.clone();
            match self.ui_state.active_tab {
                Tab::Dashboard => {
                    super::dashboard::render_dashboard(ui, &controller, &mut self.ui_state);
                }
                Tab::Build => {
                    super::build::render_build(ui, self, &controller);
                }
                Tab::Performance => {
                    super::performance::render_performance(ui, &controller);
                }
                Tab::KernelManager => {
                    super::kernels::render_kernel_manager(ui, self, &controller);
                }
                Tab::Settings => {
                    super::settings::render_settings(ui, &mut self.ui_state.settings_ui_state, &controller);
                }
            }
        });
    }
    
    /// Render build control buttons in a fixed bottom panel (only on Build tab)
    fn render_build_controls_panel(&mut self, ctx: &egui::Context) {
        if self.ui_state.active_tab == Tab::Build {
            let controller = self.controller.clone();
            super::build::render_build_controls(ctx, self, &controller);
        }
    }
    
    /// Synchronize UIState selections (usize indices) with AppState backend (HardeningLevel enum)
    /// This ensures bidirectional consistency between the two representations
    ///
    /// This is a silent, defensive sync that runs every frame to catch any divergence between
    /// the UI state (usize indices) and backend state (HardeningLevel enum). When mismatches are detected,
    /// the UI state is updated to match the backend state, ensuring accurate representation.
    fn sync_build_settings(&mut self) {
        if self.ui_state.active_tab != Tab::Build {
            return;
        }
        
        if let Ok(guard) = self.controller.try_read() {
            if let Ok(state) = guard.get_state() {
                // Sync variant: AppState string -> UIState index
                let variant_options = vec!["linux", "linux-lts", "linux-hardened", "linux-mainline"];
                if let Some(variant_idx) = variant_options.iter().position(|&v| v == state.selected_variant) {
                    if self.ui_state.selected_variant != variant_idx {
                        log::debug!("[SYNC] Variant mismatch: UI index={}, Backend='{}'. Updating UI state.",
                            self.ui_state.selected_variant, state.selected_variant);
                        self.ui_state.selected_variant = variant_idx;
                    }
                }
                
                // Sync profile: AppState string -> UIState index
                let profile_options = vec!["Gaming", "Workstation", "Laptop", "Server"];
                if let Some(profile_idx) = profile_options.iter().position(|&p| p == state.selected_profile) {
                    if self.ui_state.selected_profile != profile_idx {
                        log::debug!("[SYNC] Profile mismatch: UI index={}, Backend='{}'. Updating UI state.",
                            self.ui_state.selected_profile, state.selected_profile);
                        self.ui_state.selected_profile = profile_idx;
                    }
                }
                
                // Sync LTO: AppState string -> UIState index (none=0, thin=1, full=2)
                let lto_options = vec!["none", "thin", "full"];
                if let Some(lto_idx) = lto_options.iter().position(|&l| l == state.selected_lto) {
                    if self.ui_state.selected_lto != lto_idx {
                        log::debug!("[SYNC] LTO mismatch: UI index={}, Backend='{}'. Updating UI state.",
                            self.ui_state.selected_lto, state.selected_lto);
                        self.ui_state.selected_lto = lto_idx;
                    }
                }
                
                // Sync hardening: AppState kernel_hardening HardeningLevel -> UIState index (0=Minimal, 1=Standard, 2=Hardened)
                let hardening_idx = state.kernel_hardening.to_index();
                if self.ui_state.selected_hardening != hardening_idx {
                    log::debug!("[SYNC] Hardening mismatch: UI index={}, Backend={:?}. Updating UI state.",
                        self.ui_state.selected_hardening, state.kernel_hardening);
                    self.ui_state.selected_hardening = hardening_idx;
                }
                
                // Sync Polly boolean: direct copy from AppState
                if self.ui_state.use_polly != state.use_polly {
                    log::debug!("[SYNC] Polly mismatch: UI={}, Backend={}. Updating UI state.",
                        self.ui_state.use_polly, state.use_polly);
                    self.ui_state.use_polly = state.use_polly;
                }
                
                // Sync MGLRU boolean: direct copy from AppState
                if self.ui_state.use_mglru != state.use_mglru {
                    log::debug!("[SYNC] MGLRU mismatch: UI={}, Backend={}. Updating UI state.",
                        self.ui_state.use_mglru, state.use_mglru);
                    self.ui_state.use_mglru = state.use_mglru;
                }
                
                // Sync modprobed boolean: direct copy from AppState
                if self.ui_state.use_modprobed != state.use_modprobed {
                    log::debug!("[SYNC] Modprobed mismatch: UI={}, Backend={}. Updating UI state.",
                        self.ui_state.use_modprobed, state.use_modprobed);
                    self.ui_state.use_modprobed = state.use_modprobed;
                }
                
                // Sync whitelist boolean: direct copy from AppState
                if self.ui_state.use_whitelist != state.use_whitelist {
                    log::debug!("[SYNC] Whitelist mismatch: UI={}, Backend={}. Updating UI state.",
                        self.ui_state.use_whitelist, state.use_whitelist);
                    self.ui_state.use_whitelist = state.use_whitelist;
                }
            }
        }
    }
}

impl eframe::App for AppUI {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Store context for background thread access
        self.ctx_handle = Some(ctx.clone());
        
        // CRITICAL FIX: Store the UI context in the controller for background processor signals
        // This allows the performance monitoring background thread to request repaints via ctx.request_repaint()
        if let Ok(controller) = self.controller.try_read() {
            let _ = controller.set_perf_ui_context(ctx.clone());
        }
        
        // CRITICAL: Apply theme globally at the start of every frame
        // This ensures theme changes are persistent and apply to all UI elements
        self.apply_theme_from_state(ctx);
        
        // Process all pending async events (sets needs_repaint flag on data changes)
        self.process_build_events();
        
        // ADAPTIVE REPAINTING: Only repaint if data has changed or on idle timeout
        // This replaces fixed-frequency repaints and significantly reduces CPU usage
        const IDLE_REPAINT_INTERVAL_MS: u64 = 500;  // Fallback repaint every 500ms if no data changes
        let elapsed_since_last_repaint = self.ui_state.last_repaint_time.elapsed();
        
        // Check if performance data has been updated (set by background processor)
        let perf_data_dirty = if let Ok(controller) = self.controller.try_read() {
            controller.atomic_perf_dirty.load(std::sync::atomic::Ordering::Acquire)
        } else {
            false
        };
        
        if self.ui_state.needs_repaint || perf_data_dirty {
            // Data changed: request immediate repaint
            ctx.request_repaint();
            self.ui_state.needs_repaint = false;
            self.ui_state.last_repaint_time = Instant::now();
        } else if elapsed_since_last_repaint.as_millis() > IDLE_REPAINT_INTERVAL_MS as u128 {
            // Idle timeout: request fallback repaint to catch slow updates
            ctx.request_repaint_after(std::time::Duration::from_millis(IDLE_REPAINT_INTERVAL_MS));
            self.ui_state.last_repaint_time = Instant::now();
        }
        
        // Tab-specific polling repaints (for periodic non-event-driven updates)
        // Dashboard: poll audit status every 200ms
        if self.ui_state.active_tab == Tab::Dashboard {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        
        // Performance: poll monitoring status every 100ms (background processor signals via dirty flag)
        if self.ui_state.active_tab == Tab::Performance {
            if let Ok(controller) = self.controller.try_read() {
                let (is_monitoring, _) = controller.get_monitoring_status();
                if is_monitoring {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        
        // Kernel Manager: poll kernel list every 200ms
        if self.ui_state.active_tab == Tab::KernelManager {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        
        // HEAVY LOGIC OUT OF RENDER LOOP (FIX 4) - TAB-GATED AND FRAME-THROTTLED
        // Compute SCX readiness only when on Dashboard or KernelManager tabs
        // Throttle to once per second to avoid redundant /proc/config.gz reads + internal cache hits
        // SCXManager::get_scx_readiness() internally caches for 60 seconds, but we throttle UI calls
        use crate::system::scx::SCXManager;
        
        let is_scx_check_active_tab = matches!(self.ui_state.active_tab, Tab::Dashboard | Tab::KernelManager);
        let should_update_scx_readiness = if is_scx_check_active_tab {
            // Only when on relevant tabs: check if >= 1 second has elapsed since last check
            if let Some(last_check) = self.ui_state.last_scx_readiness_check {
                last_check.elapsed().as_secs_f64() >= 1.0
            } else {
                // First time checking on this tab session
                true
            }
        } else {
            // Not on Dashboard or KernelManager, skip check
            false
        };
        
        if should_update_scx_readiness {
            self.ui_state.cached_scx_readiness = SCXManager::get_scx_readiness();
            self.ui_state.last_scx_readiness_check = Some(Instant::now());
        }
        
        // CACHE MISSING AUR PACKAGES for UI blocking (from cached health report)
        // This allows UI elements tied to missing dependencies to be disabled/greyed out
        // Uses 60-second cached health check instead of blocking on I/O every frame
        if let Ok(controller) = self.controller.try_read() {
            let health_report = controller.get_cached_health_report();
            self.ui_state.missing_aur_packages = health_report.missing_aur_packages.clone();
            self.ui_state.missing_optional_tools = health_report.missing_optional_tools.clone();
        }
        
        // SYNC SCX STATUS: Update UIState with current kernel SCX status
        // This ensures the Kernel Manager tab always shows accurate SCX state
        if let Ok(controller) = self.controller.try_read() {
            let kernel_context = controller.get_kernel_context();
            self.ui_state.scx_enabled = kernel_context.scx_profile != "default_eevdf";
            self.ui_state.active_scx_binary = kernel_context.scx_profile.clone();
        }
        
        // INITIAL SCX SYNC: One-time synchronization of active scheduler to UI selections
        // This mirrors the current system scheduler state into the UI dropdown selections
        // Triggered only once when !scx_initial_sync_done && !available_scx_schedulers.is_empty()
        if !self.ui_state.scx_initial_sync_done && !self.ui_state.available_scx_schedulers.is_empty() {
            // Determine target scheduler: use active_scx_binary if set, otherwise use EEVDF (Stock)
            let target_scheduler = if self.ui_state.active_scx_binary.is_empty() {
                "EEVDF (Stock)".to_string()
            } else {
                self.ui_state.active_scx_binary.clone()
            };
            
            // Find the target scheduler in available list and set selected_scx_type_idx
            if let Some(idx) = self.ui_state.available_scx_schedulers.iter().position(|s| s == &target_scheduler) {
                self.ui_state.selected_scx_type_idx = Some(idx);
                log::debug!("[SCX SYNC] Initial sync: matched target scheduler '{}' at index {}", target_scheduler, idx);
            } else {
                // Fallback: if target not found, default to first available scheduler
                self.ui_state.selected_scx_type_idx = Some(0);
                log::debug!("[SCX SYNC] Initial sync: target scheduler '{}' not found, defaulting to index 0", target_scheduler);
            }
            
            // Set selected_scx_mode_idx to Auto (0) as default
            self.ui_state.selected_scx_mode_idx = Some(0);
            
            // Mark synchronization as complete
            self.ui_state.scx_initial_sync_done = true;
            log::debug!("[SCX SYNC] Initial SCX synchronization completed");
        }
        
        // SYNC SCX METADATA: Detect changes to scheduler/mode selections and refresh metadata
        // This ensures the UI always shows current metadata for the selected configuration
        if let (Some(sched_idx), Some(mode_idx)) = (
            self.ui_state.selected_scx_type_idx,
            self.ui_state.selected_scx_mode_idx,
        ) {
            // Get current scheduler name and mode string
            if let Some(scheduler) = self.ui_state.available_scx_schedulers.get(sched_idx) {
                let modes = vec!["Auto", "Gaming", "LowLatency", "PowerSave", "Server"];
                if let Some(&mode_str) = modes.get(mode_idx) {
                    // Fetch fresh metadata for this scheduler/mode pair
                    use crate::system::scx::get_scx_metadata;
                    let metadata = get_scx_metadata(scheduler, mode_str);
                    self.ui_state.active_scx_metadata = Some(metadata);
                }
            }
        } else {
            // No selection yet, clear metadata
            self.ui_state.active_scx_metadata = None;
        }
        
        // SYNC BUILD SETTINGS: Ensure UIState (usize indices) and AppState (Strings) stay synchronized
        // This is called every frame to maintain consistency between UI selections and backend config
        self.sync_build_settings();
        
        // Trigger version polling when Build tab is focused with debouncing (30-second throttle per variant)
        // Polls all canonical kernel variants: linux, linux-lts, linux-hardened, linux-mainline, linux-zen, linux-tkg
        let current_tab = self.ui_state.active_tab;
        if current_tab == Tab::Build {
            let variants = vec!["linux", "linux-lts", "linux-hardened", "linux-mainline", "linux-zen", "linux-tkg"];
            let now = Instant::now();
            let poll_interval = std::time::Duration::from_secs(1800);
            
            for variant in variants {
                // Only poll if:
                // 1. Not currently polling for this variant
                // 2. Either missing from latest_versions OR last poll was >= 30 seconds ago
                if !self.ui_state.version_poll_active.contains(variant) {
                    let should_poll = if !self.ui_state.latest_versions.contains_key(variant) {
                        // Version not fetched yet
                        true
                    } else if let Some(&last_poll_time) = self.ui_state.last_version_poll.get(variant) {
                        // Version exists but check if refresh is needed (30s throttle)
                        now.duration_since(last_poll_time) >= poll_interval
                    } else {
                        // Version exists but no last_poll record (shouldn't happen, but be safe)
                        true
                    };
                    
                    if should_poll {
                        let variant_str = variant.to_string();
                        let controller = self.controller.clone();
                        tokio::spawn(async move {
                            let ctrl = controller.read().await;
                            ctrl.trigger_version_poll(variant_str);
                        });
                        // Mark as polling
                        self.ui_state.version_poll_active.insert(variant.to_string());
                        // Record poll attempt time
                        self.ui_state.last_version_poll.insert(variant.to_string(), now);
                    }
                }
            }
        }
        
        // Render UI layers
        self.render_top_nav(ctx);
        self.render_messages(ctx);
        
        // Render build control buttons in fixed bottom panel (if on Build tab)
        // This must be called BEFORE render_content to ensure proper z-order and prevent clipping
        self.render_build_controls_panel(ctx);
        
        self.render_content(ctx);
        
        // Clear transient messages
        self.clear_transient_messages(ctx);
    }
}

impl AppUI {
    /// Apply theme from controller state to egui context
    /// This is called every frame to ensure theme changes are persistent
    ///
    /// Optimization: Caches the calculated egui::Visuals object. Only re-calculates
    /// and re-applies the theme if the theme index has actually changed.
    fn apply_theme_from_state(&mut self, ctx: &egui::Context) {
        if let Ok(guard) = self.controller.try_read() {
            if let Ok(state) = guard.get_state() {
                // Check if theme index has changed
                let theme_changed = self.ui_state.cached_theme_idx != Some(state.theme_idx);
                
                if theme_changed {
                    // Map theme index to egui visuals and cache it
                    let visuals = match state.theme_idx {
                        0 => Self::create_4rchcybrpnk_visuals(),  // 4rchCybrPnk
                        1 => egui::Visuals::dark(),  // Dark
                        2 => egui::Visuals::light(), // Light
                        _ => Self::create_4rchcybrpnk_visuals(),  // Default to 4rchCybrPnk
                    };
                    
                    // Cache the visuals and theme index
                    self.ui_state.cached_visuals = Some(visuals.clone());
                    self.ui_state.cached_theme_idx = Some(state.theme_idx);
                    
                    // Apply the visuals to the context
                    ctx.set_visuals(visuals);
                }
                
                // Apply font size globally if configured (based on selected point size: 12.0, 14.0, 16.0, etc.)
                if state.ui_font_size > 0.0 {
                    // Point sizes map directly to pixels_per_point scale
                    // e.g., 12pt → 1.15, 14pt → 1.35, 16pt → 1.54 (baseline: 10.4pt)
                    let pixels_per_point = (state.ui_font_size as f32) / 10.4;
                    ctx.set_pixels_per_point(pixels_per_point);
                }
            }
        }
    }
    
    /// Create the 4rchCybrPnk theme visuals with cyberpunk aesthetic
    ///
    /// Color palette:
    /// - Azure/Cyan: #51afef (Primary/Highlights)
    /// - Dark Purple/Indigo: #4b0082 (Primary accent for selections and hovers)
    /// - Dark Background: #1c1f24 (Main UI background)
    /// - Orange/Yellow: #da8548 (Data values/Warnings)
    fn create_4rchcybrpnk_visuals() -> egui::Visuals {
        let mut visuals = egui::Visuals::dark();
        
        // Color definitions (Cyberpunk aesthetic)
        let cyan = egui::Color32::from_rgb(0x51, 0xaf, 0xef);        // #51afef - Azure/Cyan (Primary)
        let dark_purple = egui::Color32::from_rgb(0x4b, 0x00, 0x82); // #4b0082 - Dark Purple/Indigo (Accent)
        let dark_bg = egui::Color32::from_rgb(0x1c, 0x1f, 0x24);     // #1c1f24 - Dark background
        let orange = egui::Color32::from_rgb(0xda, 0x85, 0x48);      // #da8548 - Orange (Warnings)
        let indigo = egui::Color32::from_rgb(0xc6, 0x78, 0xdd);      // #c678dd - Indigo (Button text)
        let light_green = egui::Color32::from_rgb(0x98, 0xbe, 0x65); // #98be65 - Light Green (Borders/Separators)
        
        // Panel and window styling
        visuals.panel_fill = dark_bg;
        visuals.window_fill = dark_bg;
        visuals.window_stroke.color = light_green;
        visuals.window_stroke.width = 1.0;
        
        // Selection styling
        visuals.selection.bg_fill = dark_purple;
        visuals.selection.stroke.color = light_green;
        visuals.selection.stroke.width = 1.0;
        
        // Widget styling - noninteractive state
        visuals.widgets.noninteractive.bg_fill = dark_bg;
        visuals.widgets.noninteractive.bg_stroke.color = light_green;
        visuals.widgets.noninteractive.bg_stroke.width = 0.5;
        
        // Widget styling - inactive state
        visuals.widgets.inactive.bg_fill = dark_bg;
        visuals.widgets.inactive.bg_stroke.color = indigo;
        visuals.widgets.inactive.fg_stroke.color = indigo;
        
        // Widget styling - hovered state (use dark purple highlight)
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x2c, 0x2f, 0x34);
        visuals.widgets.hovered.bg_stroke.color = light_green;
        visuals.widgets.hovered.fg_stroke.color = indigo;
        
        // Widget styling - active state (use dark purple)
        visuals.widgets.active.bg_fill = dark_purple;
        visuals.widgets.active.bg_stroke.color = orange;
        visuals.widgets.active.fg_stroke.color = indigo;
        
        // Override text color for primary text with cyan
        visuals.override_text_color = Some(cyan);
        
        // Set hyperlink color to dark purple
        visuals.hyperlink_color = dark_purple;
        
        visuals
    }
    
}

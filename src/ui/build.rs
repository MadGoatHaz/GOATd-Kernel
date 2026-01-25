use super::app::AppUI;
use crate::log_info;
use crate::ui::controller::{AppController, BuildEvent};
/// Build Engine View
///
/// Manages kernel build orchestration with real-time log streaming, progress tracking,
/// and build parameter selection (variant, profile, LTO, hardening, Polly, MGLRU).
use eframe::egui;
use egui_extras::StripBuilder;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Persistent state for the Build view
#[derive(Default, Clone)]
pub struct BuildUIState {
    /// Selected variant (linux, linux-lts, etc.)
    pub selected_variant: usize,

    /// Selected profile (Gaming, Workstation, Server, Laptop)
    pub selected_profile: usize,

    /// Selected LTO level (None, Thin, Full)
    pub selected_lto: usize,

    /// Selected hardening level (Minimal, Standard, Hardened)
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

    /// Resolved kernel version (populated when dynamic version is resolved)
    pub resolved_kernel_version: Option<String>,
}

/// Render the Build view
pub fn render_build(ui: &mut egui::Ui, app: &mut AppUI, controller: &Arc<RwLock<AppController>>) {
    ui.heading("Kernel Build Engine");
    ui.separator();

    // Hybrid 2-cell layout: Scrollable config/status section + Log viewer filling remainder
    StripBuilder::new(ui)
        .size(egui_extras::Size::initial(350.0).at_least(350.0)) // Scrollable top section (config, progress, errors)
        .size(egui_extras::Size::remainder())     // Log viewer (fills remaining space)
        .vertical(|mut strip| {
            // Cell 1: Scrollable section containing Parameters, Progress, and Errors
            strip.cell(|ui| {
                egui::ScrollArea::vertical()
                    .id_source("build_config_scroll")
                    .min_scrolled_height(200.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.group(|ui| {
                            render_build_parameters(ui, app, controller);
                        });
                        
                        if app.ui_state.is_building {
                            ui.add_space(8.0);
                            ui.group(|ui| {
                                render_build_progress(ui, app);
                            });
                        }
                        
                        if !app.ui_state.build_errors.is_empty() {
                            ui.add_space(8.0);
                            ui.group(|ui| {
                                render_build_errors(ui, app);
                            });
                        }
                    });
            });
            
            // Cell 2: Log Viewer (fills remaining space)
            strip.cell(|ui| {
                render_build_log(ui, app);
            });
        });
}

/// Render configuration parameters with responsive wrapping
fn render_build_parameters(ui: &mut egui::Ui, app: &mut AppUI, controller: &Arc<RwLock<AppController>>) {
    ui.label("Build Configuration");

    ui.horizontal(|ui| {
        ui.label("Variant:");
        let variants = vec!["linux", "linux-lts", "linux-hardened", "linux-mainline"];
        let selected_variant_name = variants.get(app.ui_state.selected_variant).copied().unwrap_or("linux");
        egui::ComboBox::from_id_source("build_variant_combo")
            .selected_text(selected_variant_name)
            .show_ui(ui, |ui| {
                for (i, variant) in variants.iter().enumerate() {
                    if ui.selectable_value(&mut app.ui_state.selected_variant, i, *variant).changed() {
                        let variant_str = variant.to_string();
                        let controller_clone = Arc::clone(controller);

                        eprintln!("[BUILD] [VARIANT_CHANGE] ⭐ START VARIANT SELECTION TRACE");
                        eprintln!("[BUILD] [VARIANT_CHANGE] User selected variant: '{}'", variant_str);
                        eprintln!("[BUILD] [VARIANT_CHANGE] UI Index: {} -> String: '{}'", i, variant_str);

                        // CRITICAL FIX: Update state SYNCHRONOUSLY before async polling task
                        // This ensures start_build() will read the correct selected_variant
                        // The race condition was: user selects variant → async update → user clicks BUILD
                        // Now: user selects variant → SYNC update → async polling → user clicks BUILD (uses correct variant)
                        if let Ok(controller_guard) = controller.try_read() {
                            let result = controller_guard.update_state(|state| {
                                let old_variant = state.selected_variant.clone();
                                state.selected_variant = variant_str.clone();
                                eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] State updated synchronously: '{}' -> '{}'", old_variant, variant_str);
                                eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] After update, state.selected_variant = '{}'", state.selected_variant);
                            });
                            match result {
                                Ok(()) => {
                                    eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] ✓ State persistence SUCCESS");
                                    // DIAGNOSTIC: Verify the state was actually saved
                                    if let Ok(controller_guard2) = controller.try_read() {
                                        if let Ok(saved_state) = controller_guard2.get_state() {
                                            eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] VERIFICATION: saved state.selected_variant = '{}'", saved_state.selected_variant);
                                            if saved_state.selected_variant != variant_str {
                                                eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] ❌ MISMATCH! Expected '{}', got '{}'", variant_str, saved_state.selected_variant);
                                            }
                                        }
                                    }
                                }
                                Err(e) => eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] ✗ State persistence FAILED: {}", e),
                            }
                        } else {
                            eprintln!("[BUILD] [VARIANT_CHANGE_SYNC] ✗ Could not acquire controller read lock");
                        }
                        eprintln!("[BUILD] [VARIANT_CHANGE] ⭐ END VARIANT SELECTION TRACE");

                        // Version polling can happen async without blocking the UI
                        tokio::spawn(async move {
                            eprintln!("[BUILD] [VARIANT_CHANGE_ASYNC] Variant selection change async task started for '{}'", variant_str);
                            let controller = controller_clone.read().await;
                            eprintln!("[BUILD] [VARIANT_CHANGE_ASYNC] Controller lock acquired for variant '{}'", variant_str);
                            // Trigger version polling (non-blocking, UI-independent)
                            controller.trigger_version_poll(variant_str.clone());
                            eprintln!("[BUILD] [VARIANT_CHANGE_ASYNC] ✓ Version polling triggered for '{}'", variant_str);
                        });
                    }
                }
            });

        // Display latest version or polling indicator
        let selected_variant_name = variants.get(app.ui_state.selected_variant).copied().unwrap_or("linux");
        if app.ui_state.version_poll_active.contains(selected_variant_name) {
            ui.add(egui::Spinner::new().size(12.0));
            ui.label("Checking version...");
        } else if let Some(version) = app.ui_state.latest_versions.get(selected_variant_name) {
            ui.label(format!("(Latest: {})", version));
        } else {
            // Version not found in cache and polling is not active
            eprintln!("[BUILD] [VERSION_DISPLAY] Variant '{}' version not in cache, showing fallback", selected_variant_name);
            ui.label("(Latest: Unknown)");
        }
    });

    ui.horizontal(|ui| {
        ui.label("Profile:");
        let profiles = vec!["Gaming", "Workstation", "Laptop", "Server"];
        let profiles_lower = vec!["gaming", "workstation", "laptop", "server"];
        let selected_profile_name = profiles.get(app.ui_state.selected_profile).copied().unwrap_or("Gaming");
        egui::ComboBox::from_id_source("build_profile_combo")
            .selected_text(selected_profile_name)
            .show_ui(ui, |ui| {
                for (i, profile) in profiles.iter().enumerate() {
                    if ui.selectable_value(&mut app.ui_state.selected_profile, i, *profile).changed() {
                        let profile_str = profile.to_string();
                        let profile_str_clone = profile_str.clone();  // Clone before async move
                        let controller_clone = Arc::clone(controller);

                        // Spawn background task to update controller (fire-and-forget for consistency)
                        // The UI will sync on next frame if needed via reactive pattern
                        tokio::spawn(async move {
                            let controller = controller_clone.read().await;
                            let _ = controller.handle_profile_change(&profile_str_clone);
                        });

                        // For immediate UI feedback: try to sync controls from current controller state
                        // This is a best-effort sync; the actual build will use controller's state
                        if let Ok(controller_guard) = controller.try_read() {
                            if let Ok(state) = controller_guard.get_state() {
                                // Sync LTO dropdown to match profile default
                                let lto_options = vec!["none", "thin", "full"];
                                if let Some(pos) = lto_options.iter().position(|&x| x == state.selected_lto.as_str()) {
                                    app.ui_state.selected_lto = pos;
                                }

                                // Sync Polly checkbox
                                app.ui_state.use_polly = state.use_polly;

                                // Sync MGLRU checkbox
                                app.ui_state.use_mglru = state.use_mglru;

                                eprintln!("[BUILD] [PROFILE_SYNC] ✓ UI synced: profile={}, LTO={}, Polly={}, MGLRU={}",
                                    profile_str, state.selected_lto, state.use_polly, state.use_mglru);
                            }
                        }
                    }
                }
            });

        // Display profile explanation to the right of the ComboBox
        if let Some(profile_key) = profiles_lower.get(app.ui_state.selected_profile) {
            if let Some(profile_def) = crate::config::profiles::get_profile(profile_key) {
                ui.label(profile_def.explanation);
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label("LTO Level:");
        let lto_options = vec!["none", "thin", "full"];
        egui::ComboBox::from_id_source("build_lto_combo")
            .selected_text(lto_options.get(app.ui_state.selected_lto).copied().unwrap_or("thin"))
            .show_ui(ui, |ui| {
                for (i, lto) in lto_options.iter().enumerate() {
                    if ui.selectable_value(&mut app.ui_state.selected_lto, i, *lto).changed() {
                        let lto_str = lto.to_string();
                        let controller_clone = Arc::clone(controller);
                        tokio::spawn(async move {
                            let controller = controller_clone.read().await;
                            let _ = controller.handle_lto_change(&lto_str);
                        });
                    }
                }
            });

        // Display LTO level explanation to the right of the ComboBox
        let selected_lto_explanation = match lto_options.get(app.ui_state.selected_lto).copied().unwrap_or("thin") {
            "none" => "Disables Link Time Optimization. Fastest build time, but largest and slowest binary.",
            "thin" => "Enables parallel Link Time Optimization. Balance between build speed and binary performance.",
            "full" => "Enables maximum Link Time Optimization. Slowest build time, but smallest and fastest binary.",
            _ => "Unknown LTO level",
        };
        ui.label(selected_lto_explanation).on_hover_text(selected_lto_explanation);
    });

    ui.horizontal(|ui| {
        ui.label("Hardening:");
        let hardening_options = vec!["Minimal", "Standard", "Hardened"];
        let selected_hardening_name = hardening_options.get(app.ui_state.selected_hardening).copied().unwrap_or("Standard");
        egui::ComboBox::from_id_source("build_hardening_combo")
            .selected_text(selected_hardening_name)
            .show_ui(ui, |ui| {
                for (i, h) in hardening_options.iter().enumerate() {
                    if ui.selectable_value(&mut app.ui_state.selected_hardening, i, *h).changed() {
                        let hardening_str = h.to_string();
                        let controller_clone = Arc::clone(controller);
                        tokio::spawn(async move {
                            let controller = controller_clone.read().await;
                            let _ = controller.handle_hardening_change(&hardening_str);
                        });
                    }
                }
            });

        // Display hardening level explanation to the right of the ComboBox
        let selected_hardening_explanation = match hardening_options.get(app.ui_state.selected_hardening).copied().unwrap_or("Standard") {
            "Minimal" => "Basic security protections. Best for maximum performance with minimum overhead.",
            "Standard" => "Recommended security features. Balance between protection and performance.",
            "Hardened" => "Extreme security measures. Best for high-security environments, may have slight performance impact.",
            _ => "Unknown hardening level",
        };
        ui.label(selected_hardening_explanation).on_hover_text(selected_hardening_explanation);
    });

    ui.separator();

    ui.heading("Optimization Flags");

    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.ui_state.use_polly, "🔵 Polly LLVM Vectorization").changed() {
            let controller_clone = Arc::clone(controller);
            let polly_enabled = app.ui_state.use_polly;
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                let _ = controller.handle_polly_change(polly_enabled);
            });
        }
        ui.label("↳ Enable Polly LLVM loop optimization and vector transforms");
    });

    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.ui_state.use_mglru, "📊 MGLRU Memory Management").changed() {
            let controller_clone = Arc::clone(controller);
            let mglru_enabled = app.ui_state.use_mglru;
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                let _ = controller.handle_mglru_change(mglru_enabled);
            });
        }
        ui.label("↳ Enable Multi-Gen LRU for improved memory efficiency");
    });

    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.ui_state.native_optimizations, "🚀 Native Optimizations (-march=native)").changed() {
            let controller_clone = Arc::clone(controller);
            let native_opts_enabled = app.ui_state.native_optimizations;
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                let _ = controller.update_state(|state| {
                    state.native_optimizations = native_opts_enabled;
                });
            });
        }
        ui.label("↳ Enable CPU-specific optimizations for best performance on this system");
    });

    ui.separator();
    ui.heading("Module & Safety Flags");

    ui.horizontal(|ui| {
        // Check if modprobed-db is missing
        let modprobed_missing = app.ui_state.missing_aur_packages.contains(&"modprobed-db".to_string());

        // Disable checkbox if modprobed-db is not installed
        if modprobed_missing {
            // Show disabled checkbox with tooltip
            let response = ui.add_enabled(false, egui::Checkbox::new(&mut app.ui_state.use_modprobed, "Use modprobed-db (Driver Auto-Discovery)"));
            response.on_hover_text("modprobed-db is not installed. Install with: yay -S modprobed-db");
        } else {
            if ui.checkbox(&mut app.ui_state.use_modprobed, "Use modprobed-db (Driver Auto-Discovery)").changed() {
                let controller_clone = Arc::clone(controller);
                let modprobed_enabled = app.ui_state.use_modprobed;
                tokio::spawn(async move {
                    let controller = controller_clone.read().await;
                    let _ = controller.handle_modprobed_change(modprobed_enabled);
                });
            }
        }
    });

    // CRITICAL CONSTRAINT: Whitelist safety net REQUIRES modprobed-db to be enabled
    // If modprobed is disabled, automatically disable whitelist for safety
    if !app.ui_state.use_modprobed && app.ui_state.use_whitelist {
        app.ui_state.use_whitelist = false;
    }

    ui.horizontal(|ui| {
        // Disable whitelist checkbox if modprobed-db is disabled (safety constraint)
        let whitelist_enabled_for_ui = app.ui_state.use_modprobed;

        let checkbox_response = ui.add_enabled(
            whitelist_enabled_for_ui,
            egui::Checkbox::new(&mut app.ui_state.use_whitelist, "Use whitelist safety net")
        );

        if checkbox_response.changed() && whitelist_enabled_for_ui {
            // Only handle change if modprobed is enabled
            let controller_clone = Arc::clone(controller);
            let whitelist_enabled = app.ui_state.use_whitelist;
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                let _ = controller.update_state(|state| {
                    state.use_whitelist = whitelist_enabled;
                });
            });
        }

        // Show helptext when disabled
        if !whitelist_enabled_for_ui {
            ui.label("(requires modprobed-db to be enabled)");
        }
    });
}

/// Render build progress section
fn render_build_progress(ui: &mut egui::Ui, app: &AppUI) {
    // Display current phase with visual emphasis
    ui.horizontal(|ui| {
        ui.label("Phase:");
        ui.colored_label(
            egui::Color32::from_rgb(70, 180, 255),
            if app.ui_state.current_build_phase.is_empty() {
                "Initializing...".to_string()
            } else {
                format!("[{}]", app.ui_state.current_build_phase.to_uppercase())
            },
        );
    });

    ui.label(format!("Status: {}", app.ui_state.build_status));
    ui.label(format!(
        "Progress: {}% | Elapsed: {}s",
        app.ui_state.build_progress, app.ui_state.build_elapsed_seconds
    ));
    let progress = (app.ui_state.build_progress as f32 / 100.0).clamp(0.0, 1.0);
    ui.add(egui::ProgressBar::new(progress));
}

/// Render build errors section
fn render_build_errors(ui: &mut egui::Ui, app: &mut AppUI) {
    ui.colored_label(egui::Color32::from_rgb(255, 100, 100), "⚠️ Build Errors");
    let mut errors_to_remove = Vec::new();
    for (idx, error) in app.ui_state.build_errors.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("[{}]", idx + 1));
            ui.label(error);
            if ui.button("✕").clicked() {
                errors_to_remove.push(idx);
            }
        });
    }
    // Remove marked errors in reverse order to preserve indices
    for idx in errors_to_remove.iter().rev() {
        app.ui_state.build_errors.remove(*idx);
    }
}

/// Render build log viewer with dynamic height
fn render_build_log(ui: &mut egui::Ui, app: &AppUI) {
    ui.group(|ui| {
        ui.label("Build Log");

        // CRITICAL: Display full log path and truncation notice
        ui.horizontal(|ui| {
            ui.label("📋 Log:");
            if let Some(ref log_path) = app.ui_state.build_log_file_path {
                ui.label(format!("{}", log_path));
                ui.colored_label(
                    egui::Color32::from_rgb(200, 150, 0),
                    "(Full logs available at this path)",
                );
            } else {
                ui.label("(Full logs path will appear here during build)");
            }
        });

        // Display truncation notice if log is capped
        const MAX_LOG_LINES: usize = 5000;
        if app.ui_state.build_log.len() >= MAX_LOG_LINES {
            ui.colored_label(
                egui::Color32::from_rgb(255, 150, 0),
                "⚠️ Displaying last ~5000 lines. Full build log is preserved on disk.",
            );
        }

        ui.separator();

        // The log viewer dynamically fills remaining space via StripBuilder
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true) // Auto-scroll to bottom on new content
            .show(ui, |ui| {
                // Render VecDeque as newline-separated text (O(N) but only in render, not on every log event)
                let log_text = if app.ui_state.build_log.is_empty() {
                    "Awaiting build output...".to_string()
                } else {
                    app.ui_state
                        .build_log
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                ui.monospace(log_text);
                ui.add_space(8.0); // Add bottom padding to prevent clipping
            });
    });
}

/// Render the fixed bottom panel with build control buttons
/// This ensures the buttons are always visible regardless of scroll position
pub fn render_build_controls(
    ctx: &egui::Context,
    app: &mut AppUI,
    controller: &Arc<RwLock<AppController>>,
) {
    egui::TopBottomPanel::bottom("build_controls_panel")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let button_width = 180.0;
                let button_height = 45.0;

                if !app.ui_state.is_building {
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new("START BUILD")
                                .color(egui::Color32::from_rgb(0x4b, 0x00, 0x82))
                        )
                            .min_size([button_width, button_height].into())
                            .fill(egui::Color32::from_rgb(76, 175, 80))
                    ).clicked() {
                        // Clean state reset before execution
                        app.ui_state.is_building = true;
                        app.ui_state.build_log.clear();
                        app.ui_state.build_progress = 0;
                        app.ui_state.build_status = "Initializing...".to_string();
                        app.ui_state.current_build_phase = String::new();
                        app.ui_state.build_elapsed_seconds = 0;
                        app.ui_state.build_errors.clear();

                        // Spawn async build task via tokio with context for repaints
                        let controller_clone = Arc::clone(controller);
                        let ctx_clone = ctx.clone();
                        tokio::spawn(async move {
                            let controller = controller_clone.read().await;
                            match start_build(&*controller, Some(ctx_clone)).await {
                                Ok(()) => {
                                    eprintln!("[UI] Build started successfully");
                                }
                                Err(e) => {
                                    eprintln!("[UI] Build failed to start: {}", e);
                                }
                            }
                        });
                    }
                } else {
                    if ui.add(
                        egui::Button::new("⏹️ CANCEL BUILD")
                            .min_size([button_width, button_height].into())
                            .fill(egui::Color32::from_rgb(244, 67, 54))
                    ).clicked() {
                        // Send cancellation signal to orchestrator (non-blocking via async)
                        let controller_clone = Arc::clone(controller);
                        tokio::spawn(async move {
                            match controller_clone.try_read() {
                                Ok(controller_handle) => {
                                    cancel_build(&*controller_handle);
                                }
                                Err(_) => {
                                    eprintln!("[UI] Could not acquire controller lock for cancel - retrying async");
                                    // Lock was held - try again in background
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    if let Ok(controller_handle) = controller_clone.try_read() {
                                        cancel_build(&*controller_handle);
                                    }
                                }
                            }
                        });
                        app.ui_state.is_building = false;
                    }
                }

                ui.separator();

                if ui.button("🔄 Clear Log").clicked() {
                    app.ui_state.build_log.clear();  // VecDeque::clear() is O(n) but user-initiated, not per-frame
                }

                // Add some spacing at the end
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(if app.ui_state.is_building {
                        "Build in progress..."
                    } else {
                        "Ready to build"
                    });
                });
            });
        });
}

// ============================================================================
// BUILD ORCHESTRATION - ATOMIC CHUNK: Build Logic
// ============================================================================

/// Generate a unique timestamped log filename for a build session
fn generate_build_log_filename() -> String {
    let now = chrono::Local::now();
    format!("build_{}.log", now.format("%Y%m%d_%H%M%S%.3f"))
}

/// Execute kernel build with orchestration pipeline
///
/// This function:
/// 1. Validates configuration from AppState
/// 2. Detects hardware capabilities
/// 3. Resolves workspace path with fallback to CWD
/// 4. Initializes AsyncOrchestrator with KernelConfig
/// 5. Spawns background build task with timer
/// 6. Returns immediately (build runs async)
///
/// # Arguments
/// * `controller` - AppController reference for state access and event emission
/// * `ctx_handle` - Optional egui context for requesting repaints from background threads
///
/// # Returns
/// Result indicating whether build initialization succeeded
pub async fn start_build(controller: &AppController, ctx_handle: Option<egui::Context>) -> Result<(), String> {
    eprintln!("[BUILD] [START_BUILD] ⭐ START_BUILD CALLED");
    controller.log_event("BUILD", "Starting kernel build orchestration");

    // =========================================================================
    // START NEW LOG SESSION - Ensure full and dedicated log file for this build
    // =========================================================================
    let _session_log_path = if let Some(ref log_collector) = controller.log_collector {
        let filename = generate_build_log_filename();
        match log_collector.start_new_session(&filename).await {
            Ok(path) => {
                controller.log_event("BUILD", &format!("Full build log: {}", path.display()));
                Some(path)
            }
            Err(e) => {
                log_info!("[BUILD] Warning: Failed to start log session: {}", e);
                None
            }
        }
    } else {
        log_info!("[BUILD] Warning: LogCollector not available");
        None
    };

    // Send initial status to UI
    let _ = controller
        .build_tx
        .send(BuildEvent::Status("preparation".into()))
        .await;

    eprintln!("[BUILD] [START_BUILD] ⚠ About to call get_state() to read AppState");
    let state = controller.get_state()?;
    eprintln!("[BUILD] [START_BUILD] ✓ get_state() succeeded");

    // =========================================================================
    // PRE-BUILD SYNCHRONIZATION AUDIT
    // =========================================================================
    eprintln!("[BUILD] [START_BUILD] ⭐ PRE-BUILD AUDIT - READING FROM APPSTATE");
    eprintln!(
        "[BUILD] [START_BUILD]   selected_variant (from AppState): '{}'",
        state.selected_variant
    );
    eprintln!(
        "[BUILD] [START_BUILD]   selected_profile: '{}'",
        state.selected_profile
    );
    eprintln!(
        "[BUILD] [START_BUILD]   selected_lto: '{}'",
        state.selected_lto
    );
    eprintln!(
        "[BUILD] [START_BUILD]   kernel_hardening: {:?}",
        state.kernel_hardening
    );

    // CRITICAL: Validate that selected_variant is not empty
    if state.selected_variant.is_empty() {
        eprintln!("[BUILD] [START_BUILD] ❌ CRITICAL ERROR: selected_variant is EMPTY!");
        eprintln!(
            "[BUILD] [START_BUILD] This WILL cause the orchestrator to default to 'linux' variant"
        );
    } else {
        eprintln!(
            "[BUILD] [START_BUILD] ✓ selected_variant is non-empty: '{}'",
            state.selected_variant
        );
    }

    log_info!(
        "[BUILD] Configuration synchronized: variant={}, profile={}, lto={}, hardening={}",
        state.selected_variant,
        state.selected_profile,
        state.selected_lto,
        state.kernel_hardening
    );

    let mut detector = crate::hardware::HardwareDetector::new();
    let hw_info = detector.detect_all().map_err(|e| {
        let msg = format!("Hardware detection failed: {}", e);
        eprintln!("[Controller] [ERROR] {}", msg);
        let _ = controller.build_tx.try_send(BuildEvent::Error(msg.clone()));
        msg
    })?;

    // =========================================================================
    // PRE-FLIGHT CHECK: Hardened Fallback Trigger with User Workspace Respect
    // =========================================================================
    let workspace_path = if state.workspace_path.is_empty() {
        let _ = controller.build_tx.try_send(BuildEvent::Log(
            "[Controller] [PRE-FLIGHT] Workspace path is empty, falling back to CWD".to_string(),
        ));

        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))?
    } else {
        let user_path = PathBuf::from(&state.workspace_path);

        // Try to canonicalize the workspace path to absolute form
        let canonical_path = match user_path.canonicalize() {
            Ok(canonical) => {
                log_info!(
                    "[BUILD] Workspace path canonicalized: {}",
                    canonical.display()
                );
                canonical
            }
            Err(e) => {
                log_info!("[BUILD] Failed to canonicalize workspace path: {}", e);
                user_path
            }
        };

        // CRITICAL: Try to create the directory (and all parents) to ensure workspace is usable
        match std::fs::create_dir_all(&canonical_path) {
            Ok(_) => {
                log_info!(
                    "[BUILD] Workspace directory ready: {}",
                    canonical_path.display()
                );
                canonical_path
            }
            Err(e) => {
                log_info!(
                    "[BUILD] Failed to create workspace directory: {}, falling back to CWD",
                    e
                );
                let _ = controller.build_tx.try_send(BuildEvent::Log(format!(
                    "[Controller] [PRE-FLIGHT] Cannot create workspace ({}), falling back to CWD",
                    e
                )));

                std::env::current_dir()
                    .map_err(|e| format!("Failed to get current directory: {}", e))?
            }
        }
    };

    // =========================================================================
    // PRE-FLIGHT VALIDATION: Check if workspace path is valid for Kbuild
    // =========================================================================
    use crate::kernel::validator::validate_kbuild_path;

    if let Err(validation_err) = validate_kbuild_path(&workspace_path) {
        let error_msg = validation_err.user_message();
        log_info!("[BUILD] Path validation failed: {}", error_msg);
        let _ = controller
            .build_tx
            .try_send(BuildEvent::Error(error_msg.clone()));
        return Err(error_msg);
    }

    let kernel_path = workspace_path.join(&state.selected_variant);
    let _ = controller.build_tx.try_send(BuildEvent::Log(format!(
        "[Controller] [PRE-FLIGHT] Using authorized workspace: {}",
        workspace_path.display()
    )));

    // =========================================================================
    // ROBUST KERNEL CONFIG POPULATION
    // =========================================================================
    eprintln!("[BUILD] [CONFIG_POPULATION] ⭐ STARTING CONFIG POPULATION");
    let mut config = crate::models::KernelConfig::default();

    // CRITICAL: Keep version as "latest" for dynamic orchestrator resolution
    config.version = "latest".to_string();
    config.kernel_variant = state.selected_variant.clone();

    // DIAGNOSTIC: Validate variant assignment
    eprintln!("[BUILD] [CONFIG_POPULATION] ⭐ kernel_variant assignment:");
    eprintln!(
        "[BUILD] [CONFIG_POPULATION]   Source (state.selected_variant): '{}'",
        state.selected_variant
    );
    eprintln!(
        "[BUILD] [CONFIG_POPULATION]   Target (config.kernel_variant): '{}'",
        config.kernel_variant
    );
    eprintln!(
        "[BUILD] [CONFIG_POPULATION]   Match: {}",
        state.selected_variant == config.kernel_variant
    );

    if config.kernel_variant.is_empty() {
        eprintln!("[BUILD] [CONFIG_POPULATION] ❌ CRITICAL ERROR: kernel_variant is EMPTY!");
        return Err("kernel_variant is empty - cannot proceed with build".to_string());
    } else {
        eprintln!(
            "[BUILD] [CONFIG_POPULATION] ✓ kernel_variant is non-empty: '{}'",
            config.kernel_variant
        );
    }

    // LTO: String from AppState -> LtoType enum with validation
    config.lto_type = match state.selected_lto.as_str() {
        "full" => crate::models::LtoType::Full,
        "none" => crate::models::LtoType::None,
        _ => crate::models::LtoType::Thin,
    };

    // Module stripping flags: Boolean from AppState
    config.use_modprobed = state.use_modprobed;
    config.use_whitelist = state.use_whitelist;

    // Hardening: HardeningLevel from AppState
    config.hardening = state.kernel_hardening;

    // Security boot
    config.secure_boot = state.secure_boot;

    // Profile: String from AppState
    config.profile = state.selected_profile.clone();

    // Optimization flags: Boolean from AppState with user override tracking
    config.use_polly = state.use_polly;
    config.use_mglru = state.use_mglru;
    config.user_toggled_polly = state.user_toggled_polly;
    config.user_toggled_mglru = state.user_toggled_mglru;
    config.user_toggled_lto = state.user_toggled_lto;
    config.user_toggled_hardening = state.user_toggled_hardening;
    config.user_toggled_bore = state.user_toggled_bore;

    // DIAGNOSTIC: Validate config population
    log_info!("[BUILD] [CONFIG_VALIDATION] version field: '{}' (should be 'latest' for dynamic resolution)", config.version);
    log_info!("[BUILD] [CONFIG_VALIDATION] kernel_variant field: '{}' (identifies which variant to fetch)", config.kernel_variant);
    log_info!(
        "[BUILD] Config: variant={}, lto={:?}, profile={}",
        config.version,
        config.lto_type,
        config.profile
    );

    // Create async orchestrator
    let checkpoint_dir = workspace_path.join(".checkpoints");

    let orch = crate::orchestrator::AsyncOrchestrator::new(
        hw_info,
        config,
        checkpoint_dir,
        kernel_path,
        Some(controller.build_tx.clone()),
        controller.cancel_tx.subscribe(),
        controller.log_collector.clone(),
        None,
        ctx_handle,
    )
    .await
    .map_err(|e| {
        let msg = format!("Failed to initialize orchestrator: {}", e);
        eprintln!("[Controller] [ERROR] {}", msg);
        log_info!("[Controller] [ERROR] {}", msg);

        // CRITICAL: Send error event to UI BEFORE returning Err
        let _ = controller.build_tx.try_send(BuildEvent::Error(msg.clone()));

        msg
    })?;

    let tx = controller.build_tx.clone();

    // Spawn the timer task that emits elapsed seconds every second
    let timer_tx = controller.build_tx.clone();
    let timer_handle = tokio::spawn(async move {
        let mut elapsed_seconds = 0u64;
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            elapsed_seconds += 1;
            if let Err(_) = timer_tx.try_send(BuildEvent::TimerUpdate(elapsed_seconds)) {
                eprintln!("[Build] [TIMER] Timer task stopping (channel closed or full)");
                break;
            }
        }
    });

    // Spawn background build task
    let log_collector_for_flush = controller.log_collector.clone();
    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let _ = tx
                .send(BuildEvent::Status("Starting kernel build...".to_string()))
                .await;

            log::debug!("[Build] Starting full build pipeline");

            // Run the complete orchestration pipeline (all 6 phases)
            match orch.run().await {
                Ok(()) => {
                    let _ = tx
                        .send(BuildEvent::Status(
                            "Build completed successfully!".to_string(),
                        ))
                        .await;

                    // CRITICAL: Flush all logs to disk before marking build as complete
                    if let Some(ref log_collector) = log_collector_for_flush {
                        match log_collector.wait_for_empty().await {
                            Ok(()) => {
                                log::debug!("[Build] All logs flushed to disk");
                            }
                            Err(e) => {
                                log::warn!("[Build] Log flush failed: {}", e);
                                let _ = tx
                                    .send(BuildEvent::Log(format!(
                                        "[WARNING] Failed to flush logs: {}",
                                        e
                                    )))
                                    .await;
                            }
                        }
                    }

                    let _ = tx.send(BuildEvent::Finished(true)).await;
                }
                Err(e) => {
                    let err_msg = format!("Build orchestration failed: {}", e);
                    log::error!("[Build] {}", err_msg);
                    let _ = tx.send(BuildEvent::Log(err_msg.clone())).await;

                    // CRITICAL: Flush logs even on error
                    if let Some(ref log_collector) = log_collector_for_flush {
                        match log_collector.wait_for_empty().await {
                            Ok(()) => {
                                log::debug!("[Build] Error logs flushed to disk");
                            }
                            Err(flush_err) => {
                                log::warn!("[Build] Error log flush failed: {}", flush_err);
                            }
                        }
                    }

                    let _ = tx.send(BuildEvent::Finished(false)).await;
                }
            }
        });
    });

    log::debug!("[Build] Build timer task spawned");
    let _ = timer_handle;

    Ok(())
}

/// Cancel active build with timeout-aware UI state reset
///
/// This method sends the cancellation signal to the orchestrator and schedules
/// a timeout task. If the build doesn't gracefully terminate within 10 seconds,
/// the UI is forcefully reset to allow the user to start a new build.
///
/// # Arguments
/// * `controller` - AppController reference for cancellation signal and event emission
pub fn cancel_build(controller: &AppController) {
    controller.log_event("BUILD", "Cancelling active build");
    let _ = controller.cancel_tx.send(true);

    // Schedule a timeout-aware reset task
    let build_tx = controller.build_tx.clone();
    let log_collector = controller.log_collector.clone();

    tokio::spawn(async move {
        // Wait up to 10 seconds for the build to gracefully cancel
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // Log the timeout event
        if let Some(log) = &log_collector {
            log.log_str("[CANCELLATION] Build did not respond to cancellation within timeout. Forcing UI reset.");
        }

        // Force UI state reset: emit Finished with false to break the build loop
        let _ = build_tx
            .send(BuildEvent::Status(
                "Build cancellation forced (timeout)".to_string(),
            ))
            .await;
        let _ = build_tx.send(BuildEvent::Finished(false)).await;

        log::debug!("[Cancel] Build cancellation timeout triggered - UI state reset forced");
    });
}

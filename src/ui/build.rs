/// Build Engine View
///
/// Manages kernel build orchestration with real-time log streaming, progress tracking,
/// and build parameter selection (variant, profile, LTO, hardening, Polly, MGLRU).

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use super::app::AppUI;

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
}

/// Render the Build view
pub fn render_build(
    ui: &mut egui::Ui,
    app: &mut AppUI,
    controller: &Arc<RwLock<AppController>>,
) {
    ui.heading("Kernel Build Engine");
    ui.separator();
    
    // Build Parameters Section
    ui.group(|ui| {
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
                            
                            // Update state (version polling is now handled by the update() loop with debouncing)
                            tokio::spawn(async move {
                                let controller = controller_clone.read().await;
                                let _ = controller.update_state(|state| {
                                    state.selected_variant = variant_str.clone();
                                });
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
    });
    
    ui.separator();
    
    // Progress Section
    if app.ui_state.is_building {
        ui.group(|ui| {
            // Display current phase with visual emphasis
            ui.horizontal(|ui| {
                ui.label("Phase:");
                ui.colored_label(
                    egui::Color32::from_rgb(70, 180, 255),
                    if app.ui_state.current_build_phase.is_empty() {
                        "Initializing...".to_string()
                    } else {
                        format!("[{}]", app.ui_state.current_build_phase.to_uppercase())
                    }
                );
            });
            
            ui.label(format!("Status: {}", app.ui_state.build_status));
            ui.label(format!(
                "Progress: {}% | Elapsed: {}s",
                app.ui_state.build_progress, app.ui_state.build_elapsed_seconds
            ));
            let progress = (app.ui_state.build_progress as f32 / 100.0).clamp(0.0, 1.0);
            ui.add(egui::ProgressBar::new(progress));
        });
    }
    
    ui.separator();
    
    // Build Errors Section - display all accumulated errors as alerts
    if !app.ui_state.build_errors.is_empty() {
        ui.group(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(255, 100, 100),
                "⚠️ Build Errors"
            );
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
        });
        ui.separator();
    }
    
    // Log Viewer Section
    // NOTE: Auto-scroll enabled via stick_to_bottom(true)
    // Line-capping: Logs are capped at 10000 lines, keeping last 5000 to prevent memory bloat
    ui.group(|ui| {
        ui.label("Build Log");
        
        // CRITICAL: Display full log path and truncation notice
        ui.horizontal(|ui| {
            ui.label("📋 Log:");
            if let Some(ref log_path) = app.ui_state.build_log_file_path {
                ui.label(format!("{}", log_path));
                ui.colored_label(
                    egui::Color32::from_rgb(200, 150, 0),
                    "(Full logs available at this path)"
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
                "⚠️ Displaying last ~5000 lines. Full build log is preserved on disk."
            );
        }
        
        ui.separator();
        
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)  // Auto-scroll to bottom on new content
            .show(ui, |ui| {
                // Render VecDeque as newline-separated text (O(N) but only in render, not on every log event)
                let log_text = if app.ui_state.build_log.is_empty() {
                    "Awaiting build output...".to_string()
                } else {
                    app.ui_state.build_log.iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                ui.monospace(log_text);
                ui.add_space(8.0);  // Add bottom padding to prevent clipping
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
                        
                        // Spawn async build task via tokio
                        let controller_clone = Arc::clone(controller);
                        tokio::spawn(async move {
                            let controller = controller_clone.read().await;
                            match controller.start_build().await {
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
                                    controller_handle.cancel_build();
                                }
                                Err(_) => {
                                    eprintln!("[UI] Could not acquire controller lock for cancel - retrying async");
                                    // Lock was held - try again in background
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    if let Ok(controller_handle) = controller_clone.try_read() {
                                        controller_handle.cancel_build();
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

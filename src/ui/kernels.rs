/// Kernel Manager View
///
/// Manages installed kernels, built artifacts, installations, uninstalls,
/// and Sched_ext (SCX) configuration.

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use crate::system::scx::SCXManager;
use super::app::AppUI;

/// Extract kernel version from formatted kernel string
///
/// Parses strings like "linux-goatd-gaming (6.18.3)" to extract "6.18.3".
/// Handles malformed inputs gracefully by returning empty string.
///
/// # Arguments
/// * `kernel_str` - Formatted kernel string with version in parentheses
///
/// # Returns
/// Extracted version string, or empty string if parsing fails
fn extract_kernel_version(kernel_str: &str) -> String {
    kernel_str
        .split('(')
        .nth(1)
        .and_then(|s| s.split(')').next())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Render the Kernel Manager tab
pub fn render_kernel_manager(
    ui: &mut egui::Ui,
    app: &mut AppUI,
    controller: &Arc<RwLock<AppController>>,
) {
    ui.heading("Kernel Manager");
    ui.separator();
    
    // Load kernel lists only once on first render (not every frame)
    // Track if we've already loaded data to avoid repeated scanning
    let should_load_data = !app.ui_state.ui_state_initialized;
    
    if should_load_data {
        if let Ok(guard) = controller.try_read() {
            // Get installed kernels via list_installed()
            let installed = guard.kernel_manager.list_installed();
            app.ui_state.installed_kernels = installed.iter()
                .map(|k| format!("{} ({})", k.name, k.version))
                .collect();
            eprintln!("[UI] [KERNELS] Loaded {} installed kernels", app.ui_state.installed_kernels.len());
            
            // Get built artifacts via scan_workspace() with workspace path from settings
            if let Ok(state) = guard.get_state() {
                let artifacts = guard.kernel_manager.scan_workspace(&state.workspace_path);
                app.ui_state.built_artifacts = artifacts;  // Store raw KernelPackage objects, not formatted strings
                eprintln!("[UI] [KERNELS] Loaded/refreshed {} built artifacts from {}", app.ui_state.built_artifacts.len(), state.workspace_path);
            }
            // Mark as initialized only if we successfully scanned
            app.ui_state.ui_state_initialized = true;
        }
    }
    
    // PERSISTENT CACHE SYNC: Pull audit data from controller cache into UIState cache
    // This ensures the persistent cache is always up-to-date without clearing it
    if app.ui_state.selected_kernel_index.is_some() {
        if let Ok(guard) = controller.try_read() {
            if let Ok(audit_data) = guard.get_selected_audit() {
                // Only update if the controller has fresh data
                // This allows async audit tasks to populate the cache gradually
                app.ui_state.deep_audit_results = Some(audit_data);
            }
        }
    }
    
    // Left panel: Kernel list + Right panel: Audit details and SCX config
    // Use columns to ensure both panels remain visible with proper width constraints
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.columns(2, |columns| {
        columns[0].vertical(|ui| {
            ui.group(|ui| {
                ui.label("Installed Kernels");
                ui.separator();
                
                // Render installed kernels as selectable list
                for (idx, kernel) in app.ui_state.installed_kernels.iter().enumerate() {
                    let is_selected = app.ui_state.selected_kernel_index == Some(idx);
                    if ui.selectable_label(is_selected, kernel).clicked() {
                         app.ui_state.selected_kernel_index = Some(idx);
                         app.ui_state.selected_kernel_name = kernel.clone();
                         eprintln!("[UI] [KERNELS] Selected kernel: {} (index {})", kernel, idx);
                         
                         // Request repaint to preserve layout when focus is lost
                         ui.ctx().request_repaint();
                         
                         // Trigger versioned deep audit for selected kernel
                         // Store to persistent UIState cache to avoid flicker during repaints
                         let controller_clone = Arc::clone(controller);
                         let version_str = kernel.clone();
                         tokio::spawn(async move {
                             // Extract version from kernel string (e.g., "linux-goatd-gaming (6.18.3)" -> "6.18.3")
                             let version = extract_kernel_version(&version_str);
                             
                             if !version.is_empty() {
                                let controller = controller_clone.read().await;
                                match controller.audit.run_deep_audit_async_for_version(Some(version.clone())).await {
                                    Ok(audit_data) => {
                                        eprintln!("[UI] [KERNELS] ✓ Versioned audit completed for: {}", version);
                                        // Update controller cache - UIState will sync from this in render loop
                                        let _ = controller.update_selected_audit(audit_data);
                                    }
                                    Err(e) => {
                                        eprintln!("[UI] [KERNELS] ✗ Versioned audit failed for {}: {}", version, e);
                                    }
                                }
                            }
                       });
                }
                }
                
                // If no kernels available
                if app.ui_state.installed_kernels.is_empty() {
                    ui.monospace("[No installed kernels]");
                }
                
                ui.separator();
                
                // Kernel actions
                ui.horizontal(|ui| {
                    // Uninstall button enabled only if kernel is selected
                    let uninstall_enabled = app.ui_state.selected_kernel_index.is_some();
                    if ui.add_enabled(uninstall_enabled, egui::Button::new("Uninstall")).clicked() {
                        let controller_clone = Arc::clone(controller);
                        let kernel_name = app.ui_state.selected_kernel_name.clone();
                        let kernel_name_copy = kernel_name.clone();
                        
                        tokio::spawn(async move {
                            match controller_clone.try_read() {
                                Ok(controller) => {
                                    match controller.uninstall_kernel(&kernel_name) {
                                        Ok(()) => {
                                            eprintln!("[UI] [KERNELS] ✓ Kernel uninstall succeeded: {}", kernel_name);
                                            
                                            // Send KernelUninstalled event to trigger UI refresh
                                            let _ = controller.build_tx.try_send(crate::ui::controller::BuildEvent::KernelUninstalled);
                                        }
                                        Err(e) => eprintln!("[UI] [KERNELS] ✗ Kernel uninstall failed: {}", e),
                                    }
                                }
                                Err(_) => {
                                    eprintln!("[UI] [KERNELS] ✗ Could not acquire controller lock for uninstall");
                                }
                            }
                        });
                        
                        app.ui_state.success_message = Some(format!("Uninstalling kernel: {}", kernel_name_copy));
                        app.ui_state.selected_kernel_index = None;
                    }
                });
            });
            
            ui.separator();
            
            ui.group(|ui| {
                ui.label("Kernels Ready for Installation");
                ui.separator();
                
                // Render built artifacts as selectable list
                if app.ui_state.built_artifacts.is_empty() {
                    ui.monospace("[No built kernels available]");
                    ui.monospace("(Build a kernel to see artifacts here)");
                } else {
                    for (idx, artifact) in app.ui_state.built_artifacts.iter().enumerate() {
                        let is_selected = app.ui_state.selected_artifact_index == Some(idx);
                        let display_label = format!("{} ({})", artifact.name, artifact.version);
                        if ui.selectable_label(is_selected, &display_label).clicked() {
                            app.ui_state.selected_artifact_index = Some(idx);
                            eprintln!("[UI] [KERNELS] Selected artifact: {} (index {})", display_label, idx);
                        }
                    }
                }
                
                ui.separator();
                
                // Artifact actions (Install Selected and Delete Artifact in horizontal layout)
                ui.horizontal(|ui| {
                    // Install Selected button enabled only if artifact is selected
                    let install_enabled = app.ui_state.selected_artifact_index.is_some();
                    if ui.add_enabled(install_enabled, egui::Button::new("Install Selected")).clicked() {
                        let controller_clone = Arc::clone(controller);
                        let artifact_pkg = if let Some(idx) = app.ui_state.selected_artifact_index {
                            app.ui_state.built_artifacts.get(idx).cloned()
                        } else {
                            None
                        };
                        
                        if let Some(artifact_pkg) = artifact_pkg {
                            let artifact_display = format!("{} ({})", artifact_pkg.name, artifact_pkg.version);
                            let artifact_display_for_async = artifact_display.clone();
                            
                            tokio::spawn(async move {
                                let controller = controller_clone.read().await;
                                if let Some(artifact_path) = artifact_pkg.path {
                                    controller.install_kernel_async(artifact_path);
                                    eprintln!("[UI] [KERNELS] Installation task spawned for artifact: {}", artifact_display_for_async);
                                } else {
                                    eprintln!("[UI] [KERNELS] ✗ Artifact path is missing for: {}", artifact_display_for_async);
                                }
                            });
                            
                            app.ui_state.success_message = Some(format!("Installing kernel from artifact: {}", artifact_display));
                        }
                    }
                    
                    // Delete Artifact button enabled only if artifact is selected
                    let delete_enabled = app.ui_state.selected_artifact_index.is_some();
                    if ui.add_enabled(delete_enabled, egui::Button::new("Delete Artifact")).clicked() {
                        let controller_clone = Arc::clone(controller);
                        let artifact_pkg = if let Some(idx) = app.ui_state.selected_artifact_index {
                            app.ui_state.built_artifacts.get(idx).cloned()
                        } else {
                            None
                        };
                        
                        if let Some(artifact_pkg) = artifact_pkg {
                            let artifact_display = format!("{} ({})", artifact_pkg.name, artifact_pkg.version);
                            let artifact_display_for_msg = artifact_display.clone();  // Clone before moving into closure
                            let artifact_pkg_clone = artifact_pkg.clone();
                            tokio::spawn(async move {
                                match controller_clone.try_read() {
                                    Ok(controller) => {
                                        match controller.handle_delete_artifact_with_pkg(&artifact_pkg_clone) {
                                            Ok(()) => {
                                                eprintln!("[UI] [KERNELS] ✓ Artifact deletion succeeded: {}", artifact_display);
                                                
                                                // Send ArtifactDeleted event to trigger UI refresh
                                                let _ = controller.build_tx.try_send(crate::ui::controller::BuildEvent::ArtifactDeleted);
                                            }
                                            Err(e) => eprintln!("[UI] [KERNELS] ✗ Artifact deletion failed: {}", e),
                                        }
                                    }
                                    Err(_) => {
                                        eprintln!("[UI] [KERNELS] ✗ Could not acquire controller lock for artifact deletion");
                                    }
                                }
                            });
                            
                            app.ui_state.success_message = Some(format!("Deleting artifact: {}", artifact_display_for_msg));
                            app.ui_state.selected_artifact_index = None;
                        }
                    }
                });
            });
        });
        
        columns[1].vertical(|ui| {
            // SCX READINESS CHECK AND DIAGNOSTIC
            // Use cached readiness state from UIState (computed in update() loop, not render)
            // This avoids redundant /proc/config.gz reads during frame rendering
            use crate::system::scx::SCXReadiness;
            let scx_readiness = app.ui_state.cached_scx_readiness;
            
            // Show diagnostic or ready indicators
            if scx_readiness != SCXReadiness::Ready {
                ui.group(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 165, 0),
                        "SCX Environment Status"
                    );
                    ui.separator();
                    
                    let diagnostic_msg = match scx_readiness {
                        SCXReadiness::KernelMissingSupport => {
                            "❌ Kernel Missing Support: Your kernel does not have CONFIG_SCHED_CLASS_EXT=y enabled.\nYou need a kernel built with sched-ext support to use SCX schedulers."
                        }
                        SCXReadiness::PackagesMissing => {
                            "❌ SCX Packages Missing: Required scheduler packages (scx-scheds) are not installed.\nYou can provision the SCX environment automatically from here."
                        }
                        SCXReadiness::ServiceMissing => {
                            "❌ SCX Service Missing: The scx_loader systemd service is not installed.\nYou can provision the SCX environment automatically from here."
                        }
                        SCXReadiness::Ready => unreachable!(),
                    };
                    
                    ui.label(diagnostic_msg);
                    ui.separator();
                    
                    // Provision button (if not kernel issue)
                    if scx_readiness != SCXReadiness::KernelMissingSupport {
                        if ui.button("Provision SCX Environment").clicked() {
                            tokio::spawn(async move {
                                match SCXManager::provision_scx_environment() {
                                    Ok(()) => {
                                        eprintln!("[UI] [SCX] ✓ SCX environment provisioned successfully");
                                    }
                                    Err(e) => {
                                        eprintln!("[UI] [SCX] ✗ Failed to provision SCX environment: {}", e);
                                    }
                                }
                            });
                        }
                    }
                });
                
                ui.separator();
            }
            
            // Poll for SCX task completion and reset state if done
            if app.ui_state.scx_activating {
                let mut task_completed = false;
                
                if let Some(ref completion_flag) = app.ui_state.scx_task_completion_flag {
                    if let Ok(flag) = completion_flag.lock() {
                        if *flag {
                            task_completed = true;
                        }
                    }
                }
                
                if task_completed {
                    // Task completed - reset activation state and clear message
                    app.ui_state.scx_activating = false;
                    app.ui_state.info_message = None;
                    app.ui_state.scx_task_completion_flag = None;
                    app.ui_state.scx_task_start_time = None;
                    eprintln!("[UI] [SCX] Activation state reset after task completion");
                } else if let Some(start_time) = app.ui_state.scx_task_start_time {
                    // Safety timeout: if task is still running after 30 seconds, force reset
                    if start_time.elapsed().as_secs() > 30 {
                        eprintln!("[UI] [SCX] Activation timeout (30s) - forcing state reset");
                        app.ui_state.scx_activating = false;
                        app.ui_state.info_message = None;
                        app.ui_state.scx_task_completion_flag = None;
                        app.ui_state.scx_task_start_time = None;
                    }
                }
            }
            
            // Sched_ext Section
            ui.group(|ui| {
                ui.heading("⚙️ Sched_ext (SCX) Configuration");
                ui.separator();
                
                // Initialize available SCX schedulers on first render
                // Use detection_attempted flag to prevent redundant polling when no schedulers are found
                if !app.ui_state.scx_detection_attempted {
                    use crate::system::scx::SCXManager;
                    let mut detected = SCXManager::is_scx_installed();
                    
                    // Prepend EEVDF (Stock) at index 0
                    detected.insert(0, "EEVDF (Stock)".to_string());
                    
                    app.ui_state.available_scx_schedulers = detected.clone();
                    app.ui_state.scx_detection_attempted = true;
                    
                    // Initialize default metadata for EEVDF (Stock) + Auto mode
                    app.ui_state.active_scx_metadata = Some(
                        crate::system::scx::get_scx_metadata("EEVDF (Stock)", "Auto")
                    );
                    
                    eprintln!("[UI] [SCX] Scheduler list initialized with EEVDF (Stock) prepended: {:?}", app.ui_state.available_scx_schedulers);
                }
                
                // SCX Status - dynamic based on UI state with visual emphasis
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Current Status:").strong());
                        let (status_color, status_icon) = if app.ui_state.scx_enabled {
                            (egui::Color32::from_rgb(100, 200, 100), "✓ Enabled")
                        } else {
                            (egui::Color32::from_rgb(180, 100, 100), "✗ Disabled")
                        };
                        
                        ui.colored_label(status_color, status_icon);
                        
                        // Show if activation is in progress with animation-like indicator
                        if app.ui_state.scx_activating {
                            ui.colored_label(
                                egui::Color32::from_rgb(255, 200, 0),
                                "⟳ Activating..."
                            );
                        }
                    });
                    
                    // SCX Binary with better formatting
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Active Binary:").strong());
                        if app.ui_state.active_scx_binary.is_empty() {
                            ui.monospace(egui::RichText::new("(EEVDF kernel scheduler - built-in)").color(egui::Color32::from_rgb(120, 120, 120)));
                        } else {
                            ui.monospace(format!("{} (via scx_loader.service)", app.ui_state.active_scx_binary));
                        }
                    });
                });
                
                ui.separator();
                
                // ========== GRANULAR SCX CONTROL ==========
                ui.group(|ui| {
                    ui.label(egui::RichText::new("🎛️ Permanent Scheduler Configuration").strong());
                    ui.separator();
                    
                    // Scheduler Type Dropdown with description
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Scheduler Type").strong());
                        ui.label(egui::RichText::new("Choose the SCX scheduler that best fits your workload").small().italics());
                        
                        if app.ui_state.available_scx_schedulers.is_empty() {
                            ui.monospace("[No SCX schedulers available]");
                        } else {
                            let mut selected_sched_idx = app.ui_state.selected_scx_type_idx.unwrap_or(0);
                            let sched_text = app.ui_state.available_scx_schedulers
                                .get(selected_sched_idx)
                                .cloned()
                                .unwrap_or_else(|| "(Select scheduler)".to_string());
                            
                            egui::ComboBox::from_id_source("scx_scheduler_type_combo")
                                .selected_text(&sched_text)
                                .width(340.0)
                                .show_ui(ui, |ui| {
                                    for (i, sched) in app.ui_state.available_scx_schedulers.iter().enumerate() {
                                        if ui.selectable_value(&mut selected_sched_idx, i, sched).changed() {
                                            app.ui_state.selected_scx_type_idx = Some(selected_sched_idx);
                                            eprintln!("[UI] [SCX] Selected scheduler: {} (index {})", sched, selected_sched_idx);
                                            // Trigger metadata update when scheduler changes
                                            if let Some(mode_idx) = app.ui_state.selected_scx_mode_idx {
                                                let mode_str = vec!["Auto", "Gaming", "LowLatency", "PowerSave", "Server"]
                                                    .get(mode_idx)
                                                    .copied()
                                                    .unwrap_or("Auto");
                                                app.ui_state.active_scx_metadata = Some(
                                                    crate::system::scx::get_scx_metadata(&sched, mode_str)
                                                );
                                            }
                                        }
                                    }
                                });
                        }
                    });
                    
                    ui.separator();
                    
                    // Mode Dropdown with description
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Scheduler Mode").strong());
                        ui.label(egui::RichText::new("Configure optimization profile for your specific workload type").small().italics());
                        
                        let modes = vec!["Auto", "Gaming", "LowLatency", "PowerSave", "Server"];
                        let mode_descriptions = vec![
                            "Automatic adaptive scheduling for general use",
                            "Optimized for frame delivery and interactive responsiveness",
                            "Minimized latency for precision timing and jitter sensitivity",
                            "Power-efficient scheduling for battery-powered devices",
                            "Throughput-optimized for server and batch workloads",
                        ];
                        let mut selected_mode_idx = app.ui_state.selected_scx_mode_idx.unwrap_or(0);
                        let mode_text = modes.get(selected_mode_idx).copied().unwrap_or("Auto");
                        
                        egui::ComboBox::from_id_source("scx_mode_combo")
                            .selected_text(mode_text)
                            .width(340.0)
                            .show_ui(ui, |ui| {
                                for (i, mode) in modes.iter().enumerate() {
                                    let desc = mode_descriptions.get(i).copied().unwrap_or("");
                                    let display_text = format!("{} - {}", mode, desc);
                                    if ui.selectable_value(&mut selected_mode_idx, i, display_text).changed() {
                                        app.ui_state.selected_scx_mode_idx = Some(selected_mode_idx);
                                        eprintln!("[UI] [SCX] Selected mode: {} (index {})", mode, selected_mode_idx);
                                        // Trigger metadata update when mode changes
                                        if let Some(sched_idx) = app.ui_state.selected_scx_type_idx {
                                            if let Some(sched) = app.ui_state.available_scx_schedulers.get(sched_idx) {
                                                app.ui_state.active_scx_metadata = Some(
                                                    crate::system::scx::get_scx_metadata(sched, *mode)
                                                );
                                            }
                                        }
                                    }
                                }
                            });
                    });
                });
                
                ui.separator();
                
                // Activate Permanent Change Button with enhanced UI
                ui.group(|ui| {
                    ui.label(egui::RichText::new("🚀 Apply Configuration").strong());
                    ui.label(egui::RichText::new("Activate the selected scheduler and mode permanently via scx_loader").small().italics());
                    ui.separator();
                    
                    let can_activate = !app.ui_state.available_scx_schedulers.is_empty()
                        && app.ui_state.selected_scx_type_idx.is_some()
                        && app.ui_state.selected_scx_mode_idx.is_some()
                        && !app.ui_state.scx_activating;
                    
                    let button_text = if app.ui_state.scx_activating {
                        "⟳ Activating... (Authorization Required)"
                    } else {
                        "✓ Activate Permanent Change"
                    };
                    
                    if ui.add_enabled(can_activate, egui::Button::new(button_text).min_size([340.0, 40.0].into())).clicked() {
                        if let (Some(sched_idx), Some(mode_idx)) = (app.ui_state.selected_scx_type_idx, app.ui_state.selected_scx_mode_idx) {
                            if let (Some(scheduler), Some(mode_text)) = (
                                app.ui_state.available_scx_schedulers.get(sched_idx),
                                vec!["Auto", "Gaming", "LowLatency", "PowerSave", "Server"].get(mode_idx),
                            ) {
                                let scheduler_clone = scheduler.clone();
                                let mode_clone = mode_text.to_string();
                                
                                eprintln!("[UI] [SCX] Activate permanent change clicked: scheduler={}, mode={}", scheduler_clone, mode_clone);
                                app.ui_state.info_message = Some(format!(
                                    "⏳ Applying permanent SCX configuration: {} ({})\n(Polkit authorization required - check system dialog)",
                                    scheduler_clone, mode_clone
                                ));
                                app.ui_state.scx_activating = true;
                                
                                let controller_clone = Arc::clone(controller);
                                
                                // Use a shared flag to signal completion (Arc<Mutex> for thread-safe modification)
                                let completion_flag = Arc::new(std::sync::Mutex::new(false));
                                let completion_flagged = Arc::clone(&completion_flag);
                                
                                // Store reference so we can check it during render
                                app.ui_state.scx_task_completion_flag = Some(completion_flag);
                                app.ui_state.scx_task_start_time = Some(std::time::Instant::now());
                                
                                tokio::spawn(async move {
                                    let controller = controller_clone.read().await;
                                    match controller.handle_apply_scx_config(&scheduler_clone, &mode_clone) {
                                        Ok(()) => {
                                            eprintln!("[UI] [SCX] ✓ Permanent SCX config activated: {} ({})", scheduler_clone, mode_clone);
                                        }
                                        Err(e) => {
                                            eprintln!("[UI] [SCX] ✗ Failed to apply permanent SCX config: {}", e);
                                        }
                                    }
                                    
                                    // Signal that the task has completed
                                    if let Ok(mut flag) = completion_flagged.lock() {
                                        *flag = true;
                                        eprintln!("[UI] [SCX] Task completion signaled");
                                    }
                                });
                            }
                        }
                    } else if app.ui_state.scx_activating {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 200, 0),
                            "⟳ Activation in progress... Check system authorization dialog"
                        );
                    }
                });
                
                ui.separator();
                
                // ========== RICH SCX METADATA PANEL ==========
                // Display dynamic metadata for selected scheduler/mode pairing
                // Panel is always visible; shows loading state if metadata is not yet loaded
                ui.group(|ui| {
                    ui.heading("📋 Scheduler Configuration Details");
                    ui.separator();
                    
                    if let Some(ref metadata) = app.ui_state.active_scx_metadata {
                        // Recommendation level with color coding and icon
                        let (rec_color, rec_icon) = match metadata.recommendation {
                            crate::system::scx::RecommendationLevel::Recommended => {
                                (egui::Color32::from_rgb(100, 200, 100), "✓")
                            }
                            crate::system::scx::RecommendationLevel::NotRecommended => {
                                (egui::Color32::from_rgb(255, 165, 0), "⚠")
                            }
                        };
                        
                        let rec_label = format!("{} {}", rec_icon, metadata.recommendation.as_str());
                        ui.colored_label(rec_color, rec_label);
                        ui.separator();
                        
                        // Description with visual emphasis
                        ui.label(egui::RichText::new("📖 Description:").strong());
                        ui.label(egui::RichText::new(&metadata.description).italics().color(egui::Color32::from_rgb(180, 180, 180)));
                        
                        ui.separator();
                        
                        // Best Utilized For
                        ui.label(egui::RichText::new("🎯 Best Utilized For:").strong());
                        let best_for_lines: Vec<&str> = metadata.best_for.split(',').collect();
                        for line in best_for_lines {
                            ui.label(format!("  • {}", line.trim()));
                        }
                        
                        ui.separator();
                        
                        // CLI Flags with copy-friendly formatting
                        ui.label(egui::RichText::new("⚙️ Command Flags:").strong());
                        if metadata.cli_flags.is_empty() {
                            ui.monospace(egui::RichText::new("[Default flags - no additions]").color(egui::Color32::from_rgb(120, 120, 120)));
                        } else {
                            // Show flags in a selectable monospace format for easy copying
                            let flags_text = format!("-k {}", metadata.cli_flags);
                            ui.monospace(egui::RichText::new(&flags_text).color(egui::Color32::from_rgb(150, 255, 150)));
                            
                            // Helper text
                            ui.label(egui::RichText::new("(Flags shown for reference; automatically applied during activation)").small().italics());
                        }
                        
                        ui.separator();
                        
                        // Performance Tier Indicator
                        let perf_tier = if metadata.best_for.contains("latency") || metadata.best_for.contains("Real-time") {
                            "⚡ Ultra-Low Latency"
                        } else if metadata.best_for.contains("power") || metadata.best_for.contains("battery") {
                            "🔋 Power Optimized"
                        } else if metadata.best_for.contains("Gaming") || metadata.best_for.contains("audio") {
                            "🎮 Responsiveness Optimized"
                        } else if metadata.best_for.contains("Server") || metadata.best_for.contains("throughput") {
                            "⚙️ Throughput Optimized"
                        } else {
                            "⚖️ Balanced"
                        };
                        
                        ui.label(egui::RichText::new(format!("Optimization Profile: {}", perf_tier)).small());
                        
                    } else {
                        // Placeholder when metadata is not available
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 100, 100),
                            "ℹ️ Select a scheduler and mode to view detailed configuration"
                        );
                    }
                });
                
            });
        });
        });
    });
}

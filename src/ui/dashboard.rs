/// Dashboard View
///
/// Displays hardware information, system overview, and kernel status at a glance.

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use crate::system::health::{HealthManager, HealthStatus};

/// Render the Dashboard tab content
pub fn render_dashboard(ui: &mut egui::Ui, controller: &Arc<RwLock<AppController>>, ui_state: &mut super::app::UIState) {
    ui.heading("System Overview");
    
    ui.separator();
    
    // Fetch hardware information non-blocking (via try_read)
    // If the lock is held, display cached/placeholder data instead of blocking
    let hardware_info = {
        match controller.try_read() {
            Ok(controller_guard) => {
                controller_guard.get_hardware_info()
            }
            Err(_) => {
                // Lock is held - display placeholder instead of blocking
                Err("Hardware info unavailable (updating)".to_string())
            }
        }
    };
    
    // Create a 2-column grid for hardware information
    egui::Grid::new("hardware_grid")
        .spacing([40.0, 10.0])
        .striped(true)
        .show(ui, |ui| {
            // CPU Information
            ui.label("CPU:");
            if let Ok(ref hw) = hardware_info {
                ui.label(&hw.cpu_model);
            } else {
                ui.label("Detection failed");
            }
            ui.end_row();
            
            // Cores and Threads
            ui.label("Cores/Threads:");
            if let Ok(ref hw) = hardware_info {
                ui.label(format!("{} / {}", hw.cpu_cores, hw.cpu_threads));
            } else {
                ui.label("N/A");
            }
            ui.end_row();
            
            // RAM Information
            ui.label("RAM:");
            if let Ok(ref hw) = hardware_info {
                ui.label(format!("{} GB", hw.ram_gb));
            } else {
                ui.label("N/A");
            }
            ui.end_row();
            
            // GPU Information
            ui.label("GPU:");
            if let Ok(ref hw) = hardware_info {
                let gpu_label = format!("{:?} - {}", hw.gpu_vendor, hw.gpu_model);
                ui.label(gpu_label);
            } else {
                ui.label("N/A");
            }
            ui.end_row();
            
            // Storage Information
            ui.label("Storage:");
            if let Ok(ref hw) = hardware_info {
                let storage_label = format!("{:?} - {}", hw.storage_type, hw.storage_model);
                ui.label(storage_label);
            } else {
                ui.label("N/A");
            }
            ui.end_row();
            
            // Boot Mode
            ui.label("Boot Mode:");
            if let Ok(ref hw) = hardware_info {
                let boot_label = format!("{:?}", hw.boot_type);
                ui.label(boot_label);
            } else {
                ui.label("N/A");
            }
            ui.end_row();
            
            // Init System
            ui.label("Init System:");
            if let Ok(ref hw) = hardware_info {
                ui.label(&hw.init_system.name);
            } else {
                ui.label("N/A");
            }
            ui.end_row();
        });
    
    ui.separator();
    
    // System Health Status Section
    ui.heading("System Health");
    
    let health_report = HealthManager::check_system_health();
    
    // Display health status with color coding
    let status_color = match health_report.status {
        HealthStatus::Excellent => egui::Color32::from_rgb(100, 200, 100),  // Green
        HealthStatus::Good => egui::Color32::from_rgb(150, 150, 100),      // Yellow-ish
        HealthStatus::Incomplete => egui::Color32::from_rgb(200, 150, 100), // Orange
        HealthStatus::Poor => egui::Color32::from_rgb(200, 100, 100),      // Red
    };
    
    ui.horizontal(|ui| {
        ui.label("Status:");
        ui.colored_label(status_color, health_report.status.as_str());
    });
    
    ui.label(&health_report.message);
    
    // Show missing OFFICIAL packages if any
    if !health_report.missing_official_packages.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(200, 100, 100),
            format!("Missing official packages: {}", health_report.missing_official_packages.join(", "))
        );
    }
    
    // Show missing GPG keys if any
    if !health_report.missing_gpg_keys.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(200, 150, 100),
            format!("Missing GPG keys: {}", health_report.missing_gpg_keys.join(", "))
        );
    }
    
    // Show missing AUR packages if any
    if !health_report.missing_aur_packages.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(200, 150, 100),
            format!("🔧 AUR Packages Missing: {}", health_report.missing_aur_packages.join(", "))
        );
        ui.colored_label(
            egui::Color32::from_rgb(200, 150, 100),
            "  → Install manually: yay -S ".to_string() + &health_report.missing_aur_packages.join(" ")
        );
    }
    
    // Show optional tools if missing
    if !health_report.missing_optional_tools.is_empty() {
        ui.colored_label(
            egui::Color32::from_rgb(150, 150, 100),
            format!("Optional: {}", health_report.missing_optional_tools.join(", "))
        );
    }
    
    // Fix System Environment button - only if there are official packages, optional tools, or GPG keys to fix
    if !health_report.missing_official_packages.is_empty() || !health_report.missing_gpg_keys.is_empty() || !health_report.missing_optional_tools.is_empty() {
        ui.separator();
        if ui.button("🔧 Fix System Environment (pkexec)").clicked() {
            let fix_cmd = crate::system::health::HealthManager::generate_fix_command(&health_report);
            ui_state.show_fix_modal = true;
            ui_state.pending_fix_command = fix_cmd;
        }
        ui.label("ℹ You will see a system authentication prompt (pkexec).");
        ui.label("ℹ This fixes official packages, optional tools, and GPG keys. AUR packages must be installed manually.");
    }
    
    ui.separator();
    
    // Quick Actions
    ui.heading("Quick Actions");
    ui.horizontal(|ui| {
        if ui.button("Run Deep Audit").clicked() {
            let controller_clone = Arc::clone(controller);
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                eprintln!("[DASHBOARD] [AUDIT] Starting deep audit...");
                match controller.audit.run_deep_audit_async().await {
                    Ok(audit_data) => {
                        eprintln!("[DASHBOARD] [AUDIT] ✓ Deep audit completed successfully");
                        // Update the cached audit data in the controller
                        drop(controller); // Release read lock before acquiring write lock
                        let controller = controller_clone.write().await;
                        if let Err(e) = controller.update_active_audit(audit_data) {
                            eprintln!("[DASHBOARD] [AUDIT] ⚠ Failed to cache active audit data: {}", e);
                        } else {
                            eprintln!("[DASHBOARD] [AUDIT] ✓ Active audit data cached successfully");
                        }
                    }
                    Err(e) => {
                        eprintln!("[DASHBOARD] [AUDIT] ✗ Deep audit failed: {}", e);
                    }
                }
            });
        }
        if ui.button("Run Jitter Audit").clicked() {
            let controller_clone = Arc::clone(controller);
            tokio::spawn(async move {
                let controller = controller_clone.read().await;
                eprintln!("[DASHBOARD] [AUDIT] Starting quick jitter audit...");
                match controller.handle_quick_jitter_audit() {
                    Ok(()) => {
                        eprintln!("[DASHBOARD] [AUDIT] ✓ Quick jitter audit initiated (async task spawned)");
                    }
                    Err(e) => {
                        eprintln!("[DASHBOARD] [AUDIT] ✗ Quick jitter audit failed to start: {}", e);
                    }
                }
            });
        }
    });
    
    ui.separator();
    
    // Recent Audit Results Section - 2-Column Layout
    ui.heading("Recent Audit Results");
    
    ui.columns(2, |cols| {
        // COLUMN 0: Deep Audit Results (Left Column)
        cols[0].group(|ui| {
            ui.label("Deep Audit");
            
            match controller.try_read() {
                Ok(controller_guard) => {
                    match controller_guard.get_active_audit() {
                        Ok(audit_data) => {
                            egui::Grid::new("deep_audit_grid")
                                .spacing([20.0, 8.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label("Kernel Version:");
                                    ui.monospace(&audit_data.version);
                                    ui.end_row();
                                    
                                    ui.label("Compiler:");
                                    ui.monospace(&audit_data.compiler);
                                    ui.end_row();
                                    
                                    ui.label("LTO Status:");
                                    ui.monospace(&audit_data.lto_status);
                                    ui.end_row();
                                    
                                    ui.label("Hardening Status:");
                                    ui.monospace(&audit_data.hardening_status);
                                    ui.end_row();
                                    
                                    ui.label("CPU Scheduler:");
                                    ui.monospace(&audit_data.cpu_scheduler);
                                    ui.end_row();
                                    
                                    ui.label("I/O Scheduler:");
                                    ui.monospace(&audit_data.io_scheduler);
                                    ui.end_row();
                                    
                                    ui.label("Timer Frequency:");
                                    ui.monospace(&audit_data.timer_frequency);
                                    ui.end_row();
                                    
                                    ui.label("MGLRU Status:");
                                    ui.monospace(&audit_data.mglru);
                                    ui.end_row();
                                    
                                    ui.label("Modules Count:");
                                    ui.monospace(audit_data.module_count.to_string());
                                    ui.end_row();
                                });
                        }
                        Err(_) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(150, 150, 150),
                                "(No deep audit results yet)"
                            );
                        }
                    }
                }
                Err(_) => {
                    ui.label("(Unable to fetch audit data)");
                }
            }
        });
        
        // COLUMN 1: Jitter Audit Results (Right Column)
        cols[1].group(|ui| {
            ui.label("Jitter Audit");
            
            match controller.try_read() {
                Ok(controller_guard) => {
                    match controller_guard.get_cached_jitter_summary() {
                        Ok(Some(summary)) => {
                            egui::Grid::new("jitter_audit_grid")
                                .spacing([20.0, 8.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label("Min Latency (µs):");
                                    ui.monospace(format!("{:.2}", summary.final_metrics.current_us));
                                    ui.end_row();
                                    
                                    ui.label("Max Latency (µs):");
                                    ui.monospace(format!("{:.2}", summary.final_metrics.max_us));
                                    ui.end_row();
                                    
                                    ui.label("Avg Latency (µs):");
                                    ui.monospace(format!("{:.2}", summary.final_metrics.avg_us));
                                    ui.end_row();
                                    
                                    ui.label("P99.9 Latency (µs):");
                                    ui.monospace(format!("{:.2}", summary.final_metrics.p99_9_us));
                                    ui.end_row();
                                    
                                    ui.label("Total Samples:");
                                    ui.monospace(summary.total_samples.to_string());
                                    ui.end_row();
                                    
                                    ui.label("Duration:");
                                    ui.monospace(format!("{:.2}s", summary.duration_secs.unwrap_or(0.0)));
                                    ui.end_row();
                                    
                                    ui.label("Completed:");
                                    let status_color = if summary.completed_successfully {
                                        egui::Color32::from_rgb(100, 200, 100)
                                    } else {
                                        egui::Color32::from_rgb(200, 100, 100)
                                    };
                                    ui.colored_label(
                                        status_color,
                                        if summary.completed_successfully { "✓ Yes" } else { "✗ No" }
                                    );
                                    ui.end_row();
                                });
                        }
                        Ok(None) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(150, 150, 150),
                                "(No jitter audit results yet)"
                            );
                        }
                        Err(_) => {
                            ui.colored_label(
                                egui::Color32::from_rgb(150, 150, 150),
                                "(Unable to fetch jitter audit data)"
                            );
                        }
                    }
                }
                Err(_) => {
                    ui.label("(Unable to fetch audit data)");
                }
            }
        });
    });
    
    // Modal for confirming system fix
    if ui_state.show_fix_modal {
        let mut window_open = true;
        let pending_cmd = ui_state.pending_fix_command.clone();
        
        egui::Window::new("Confirm System Fix")
            .collapsible(false)
            .resizable(true)
            .default_width(500.0)
            .open(&mut window_open)
            .show(ui.ctx(), |ui| {
                ui.heading("System Environment Fix");
                
                ui.separator();
                
                ui.label("📋 Action Summary");
                ui.group(|ui| {
                    ui.label("The following system fix will be applied with elevated privileges (pkexec):");
                    ui.separator();
                    ui.monospace(&pending_cmd);
                    ui.separator();
                    ui.label("This will:");
                    ui.label("  • Install missing official packages");
                    ui.label("  • Import required GPG keys");
                    ui.label("  • Install optional system tools");
                });
                
                ui.separator();
                
                ui.label("🔐 You will be prompted for your system password via pkexec");
                ui.label("The command will execute with elevated privileges to modify system packages.");
                
                ui.separator();
                
                ui.horizontal(|ui| {
                    if ui.button("✓ Confirm & Execute").clicked() {
                        let controller_clone = Arc::clone(controller);
                        let health_report_snapshot = health_report.clone();
                        let ctx_for_repaint = ui.ctx().clone();
                        
                        tokio::spawn(async move {
                            eprintln!("[DASHBOARD] [HEALTH] ========== STARTING 4-STEP SYSTEM FIX ==========");
                            let mut all_success = true;
                            
                            // ================================================================
                            // STEP 1: Install packages via pkexec
                            // ================================================================
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 1] Installing packages via pkexec...");
                            if !health_report_snapshot.missing_official_packages.is_empty() || !health_report_snapshot.missing_optional_tools.is_empty() {
                                let mut packages_to_install = health_report_snapshot.missing_official_packages.clone();
                                packages_to_install.extend(health_report_snapshot.missing_optional_tools.clone());
                                let pkg_list = packages_to_install.join(" ");
                                let install_cmd = format!("pacman -S --needed --noconfirm {}", pkg_list);
                                
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 1] Command: {}", install_cmd);
                                let controller_guard = controller_clone.read().await;
                                match controller_guard.system.batch_privileged_commands(vec![&install_cmd]) {
                                    Ok(()) => {
                                        eprintln!("[DASHBOARD] [HEALTH] [STEP 1] ✓ Packages installed successfully");
                                    }
                                    Err(e) => {
                                        eprintln!("[DASHBOARD] [HEALTH] [STEP 1] ✗ Package installation failed: {}", e);
                                        all_success = false;
                                    }
                                }
                            } else {
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 1] ✓ No official packages to install (skipped)");
                            }
                            
                            // ================================================================
                            // STEP 2: Import GPG keys as user (NO elevation)
                            // ================================================================
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 2] Importing GPG keys as user...");
                            if !health_report_snapshot.missing_gpg_keys.is_empty() {
                                let gpg_commands = HealthManager::generate_gpg_setup_commands(&health_report_snapshot);
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 2] Generated {} GPG key import commands", gpg_commands.len());
                                
                                let controller_guard = controller_clone.read().await;
                                let gpg_cmd_refs: Vec<&str> = gpg_commands.iter().map(|s| s.as_str()).collect();
                                match controller_guard.system.batch_user_commands(gpg_cmd_refs) {
                                    Ok(()) => {
                                        eprintln!("[DASHBOARD] [HEALTH] [STEP 2] ✓ GPG keys imported successfully (user-level, NO elevation)");
                                    }
                                    Err(e) => {
                                        eprintln!("[DASHBOARD] [HEALTH] [STEP 2] ✗ GPG key import failed: {}", e);
                                        all_success = false;
                                    }
                                }
                            } else {
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 2] ✓ No GPG keys to import (skipped)");
                            }
                            
                            // ================================================================
                            // STEP 3: Re-check system health
                            // ================================================================
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 3] Re-checking system health...");
                            let updated_health = HealthManager::check_system_health();
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 3] Health status after fixes: {} | Missing Official: {} | Missing GPG: {} | Missing Optional: {}",
                                updated_health.status.as_str(),
                                updated_health.missing_official_packages.len(),
                                updated_health.missing_gpg_keys.len(),
                                updated_health.missing_optional_tools.len());
                            
                            if updated_health.status as i32 == crate::system::health::HealthStatus::Excellent as i32 ||
                               updated_health.missing_official_packages.is_empty() && updated_health.missing_gpg_keys.is_empty() {
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 3] ✓ System health check complete - improvements detected");
                            } else {
                                eprintln!("[DASHBOARD] [HEALTH] [STEP 3] ⚠ Some issues remain after fixes");
                            }
                            
                            // ================================================================
                            // STEP 4: Request UI repaint
                            // ================================================================
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 4] Requesting UI repaint to refresh dashboard...");
                            ctx_for_repaint.request_repaint();
                            eprintln!("[DASHBOARD] [HEALTH] [STEP 4] ✓ UI repaint requested");
                            
                            // ================================================================
                            // Final result
                            // ================================================================
                            if all_success {
                                eprintln!("[DASHBOARD] [HEALTH] ========== ✓ ALL STEPS COMPLETED SUCCESSFULLY ==========");
                            } else {
                                eprintln!("[DASHBOARD] [HEALTH] ========== ✗ SOME STEPS FAILED - CHECK LOGS ==========");
                            }
                        });
                    }
                    
                    if ui.button("✗ Cancel").clicked() {
                        // Signal modal to close (will be handled below)
                        ui_state.show_fix_modal = false;
                    }
                });
            });
        
        // Update modal state based on window close button
        if !window_open {
            ui_state.show_fix_modal = false;
            ui_state.pending_fix_command.clear();
        }
    }
}

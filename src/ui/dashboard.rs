/// Dashboard View
///
/// Displays hardware information, system overview, and kernel status at a glance.

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use crate::system::health::{HealthManager, HealthStatus};
use crate::system::scx::{SCXManager, SCXReadiness};

/// Render the Dashboard tab content
pub fn render_dashboard(ui: &mut egui::Ui, controller: &Arc<RwLock<AppController>>) {
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
            let controller_clone = Arc::clone(controller);
            let fix_cmd = crate::system::health::HealthManager::generate_fix_command(&health_report);
            
            tokio::spawn(async move {
                eprintln!("[DASHBOARD] [HEALTH] Starting system fix with command: {}", fix_cmd);
                let controller_guard = controller_clone.read().await;
                match controller_guard.system.batch_privileged_commands(vec![&fix_cmd]) {
                    Ok(()) => {
                        eprintln!("[DASHBOARD] [HEALTH] ✓ System environment fixed successfully");
                    }
                    Err(e) => {
                        eprintln!("[DASHBOARD] [HEALTH] ✗ Failed to fix system environment: {}", e);
                    }
                }
            });
        }
        ui.label("ℹ You will see a system authentication prompt (pkexec).");
        ui.label("ℹ This fixes official packages, optional tools, and GPG keys. AUR packages must be installed manually.");
    }
    
    ui.separator();
    
    // SCX Readiness Status Section
    ui.heading("SCX Scheduler Status");
    
    let scx_readiness = SCXManager::get_scx_readiness();
    let scx_installed = !SCXManager::is_scx_installed().is_empty();
    
    let (scx_color, scx_status_text) = match scx_readiness {
        SCXReadiness::Ready => (
            egui::Color32::from_rgb(100, 200, 100),
            "✓ Ready"
        ),
        SCXReadiness::KernelMissingSupport => (
            egui::Color32::from_rgb(200, 100, 100),
            "✗ Kernel Missing CONFIG_SCHED_CLASS_EXT"
        ),
        SCXReadiness::PackagesMissing => (
            egui::Color32::from_rgb(200, 150, 100),
            "⚠ Packages Not Installed"
        ),
        SCXReadiness::ServiceMissing => (
            egui::Color32::from_rgb(200, 150, 100),
            "⚠ Service Not Registered"
        ),
    };
    
    ui.horizontal(|ui| {
        ui.label("Status:");
        ui.colored_label(scx_color, scx_status_text);
    });
    
    // Show specific guidance based on readiness state
    match scx_readiness {
        SCXReadiness::Ready => {
            ui.label("✓ SCX environment is fully configured and ready to use.");
        }
        SCXReadiness::KernelMissingSupport => {
            ui.colored_label(
                egui::Color32::from_rgb(200, 100, 100),
                "Your kernel does not support CONFIG_SCHED_CLASS_EXT. \
                 A custom kernel build with SCX support is required."
            );
        }
        SCXReadiness::PackagesMissing => {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 100),
                "SCX scheduler packages are not installed. \
                 Install: sudo pacman -S scx-tools"
            );
        }
        SCXReadiness::ServiceMissing => {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 100),
                "SCX package detected but Service not registered. \
                 This can happen after fresh package installation due to systemd registry lag."
            );
            
            // Only show the reload button if package is installed but service is missing
            if scx_installed {
                ui.separator();
                if ui.button("🔄 Reload System Units (systemctl daemon-reload)").clicked() {
                    let controller_clone = Arc::clone(controller);
                    tokio::spawn(async move {
                        eprintln!("[DASHBOARD] [SCX] Starting systemd daemon-reload...");
                        let controller_guard = controller_clone.read().await;
                        
                        // Execute pkexec systemctl daemon-reload
                        match controller_guard.system.batch_privileged_commands(vec!["systemctl daemon-reload"]) {
                            Ok(()) => {
                                eprintln!("[DASHBOARD] [SCX] ✓ Systemd units reloaded successfully");
                                eprintln!("[DASHBOARD] [SCX] SCX service should now be visible. Try provisioning again.");
                            }
                            Err(e) => {
                                eprintln!("[DASHBOARD] [SCX] ✗ Failed to reload systemd units: {}", e);
                            }
                        }
                    });
                }
                ui.label("ℹ Reloads systemd unit files and refreshes service registry.");
                ui.label("ℹ After reloading, you may need to retry provisioning SCX environment.");
            }
        }
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
}

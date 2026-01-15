/// Dashboard View
///
/// Displays hardware information, system overview, and kernel status at a glance.

use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::ui::controller::AppController;

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

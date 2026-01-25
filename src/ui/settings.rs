use crate::ui::controller::AppController;
/// Settings View
///
/// Manages application configuration: workspace path, security settings,
/// UI customization, and startup behaviors.
use eframe::egui;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Persistent settings state for rendering
#[derive(Clone, Default)]
pub struct SettingsUIState {
    pub workspace_path: String,
    pub secure_boot_enabled: bool,
    pub verify_signatures: bool,
    pub prep_timeout_mins: String,
    pub config_timeout_mins: String,
    pub patch_timeout_mins: String,
    pub build_timeout_mins: String,
    pub theme: usize,
    pub font_size: f32,
    pub auto_scroll_logs: bool,
    pub audit_on_startup: bool,
    pub minimize_to_tray: bool,
    pub save_window_state: bool,
    pub debug_logging: bool,
    pub tokio_tracing: bool,
}

/// Render the Settings tab
pub fn render_settings(
    ui: &mut egui::Ui,
    app_ui_state: &mut SettingsUIState,
    controller: &Arc<RwLock<AppController>>,
) {
    ui.heading("Settings");
    ui.separator();

    // Load current settings from controller on first frame only
    if app_ui_state.workspace_path.is_empty()
        && app_ui_state.theme == 0
        && app_ui_state.font_size == 0.0
    {
        if let Ok(guard) = controller.try_read() {
            if let Ok(state) = guard.get_state() {
                app_ui_state.workspace_path = state.workspace_path.clone();
                app_ui_state.secure_boot_enabled = state.secure_boot;
                app_ui_state.verify_signatures = state.verify_signatures;
                app_ui_state.prep_timeout_mins = "5".to_string();
                app_ui_state.config_timeout_mins = "5".to_string();
                app_ui_state.patch_timeout_mins = "5".to_string();
                app_ui_state.build_timeout_mins = "120".to_string();
                app_ui_state.theme = state.theme_idx;
                app_ui_state.font_size = state.ui_font_size;
                app_ui_state.auto_scroll_logs = state.auto_scroll_logs;
                app_ui_state.audit_on_startup = state.audit_on_startup;
                app_ui_state.minimize_to_tray = state.minimize_to_tray;
                app_ui_state.save_window_state = state.save_window_state;
                app_ui_state.debug_logging = state.debug_logging;
                app_ui_state.tokio_tracing = state.tokio_tracing;
            }
        }
    }

    // Workspace Management Section
    ui.group(|ui| {
        ui.label("Workspace Management");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Workspace Path:");
            // React to changes: update happens immediately after text_edit
            if ui
                .text_edit_singleline(&mut app_ui_state.workspace_path)
                .changed()
            {
                // Wire workspace path change to controller immediately
                let controller_clone = Arc::clone(controller);
                let path = app_ui_state.workspace_path.clone();
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        match controller_handle.update_state(|state| {
                            state.workspace_path = path.clone();
                        }) {
                            Ok(()) => {
                                eprintln!(
                                    "[UI] [SETTINGS] Workspace path updated and persisted: {}",
                                    path
                                );
                                // Trigger WorkspaceChanged event to force UI refresh
                                let _ = controller_handle
                                    .build_tx
                                    .try_send(crate::ui::controller::BuildEvent::WorkspaceChanged);
                                eprintln!("[UI] [SETTINGS] WorkspaceChanged event sent");
                            }
                            Err(e) => {
                                eprintln!("[UI] [SETTINGS] Failed to persist workspace path: {}", e)
                            }
                        }
                    }
                });
            }

            if ui.button("Browse...").clicked() {
                // Open file dialog via rfd
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    app_ui_state.workspace_path = path.to_string_lossy().to_string();

                    // Wire selected path to controller immediately
                    let controller_clone = Arc::clone(controller);
                    let selected_path = app_ui_state.workspace_path.clone();
                    tokio::spawn(async move {
                        if let Ok(controller_handle) = controller_clone.try_read() {
                            match controller_handle.update_state(|state| {
                                state.workspace_path = selected_path.clone();
                            }) {
                                Ok(()) => {
                                    eprintln!(
                                        "[UI] [SETTINGS] Workspace path selected and persisted: {}",
                                        selected_path
                                    );
                                    // Trigger WorkspaceChanged event to force UI refresh
                                    let _ = controller_handle
                                        .build_tx
                                        .try_send(crate::ui::controller::BuildEvent::WorkspaceChanged);
                                    eprintln!("[UI] [SETTINGS] WorkspaceChanged event sent");
                                }
                                Err(e) => eprintln!(
                                    "[UI] [SETTINGS] Failed to persist selected path: {}",
                                    e
                                ),
                            }
                        }
                    });
                }
            }
        });

        // =========================================================================
        // VISUAL WARNING: Path Validation for Kbuild
        // =========================================================================
        // Display a red warning if the workspace path contains spaces or colons,
        // which are forbidden by Kbuild and will cause build failures
        let path_is_valid = {
            use crate::kernel::validator::validate_kbuild_path;
            use std::path::Path;
            validate_kbuild_path(Path::new(&app_ui_state.workspace_path)).is_ok()
        };

        if !path_is_valid && !app_ui_state.workspace_path.is_empty() {
            ui.colored_label(
                egui::Color32::from_rgb(255, 100, 100),
                "⚠ WARNING: Path contains spaces or colons - Kbuild will fail!",
            );
        }

        ui.horizontal(|ui| {
            ui.label("Kernel Source:");
            ui.monospace(format!("{}/src", app_ui_state.workspace_path));
        });

        ui.horizontal(|ui| {
            ui.label("Build Output:");
            ui.monospace(format!("{}/pkg", app_ui_state.workspace_path));
        });
    });

    ui.separator();

    // Security & Build Settings
    ui.group(|ui| {
        ui.label("Security & Build Settings");
        ui.separator();

        // Secure Boot checkbox with persistence
        if ui
            .checkbox(
                &mut app_ui_state.secure_boot_enabled,
                "Enable Secure Boot (requires UEFI key management)",
            )
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.secure_boot_enabled;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.secure_boot = enabled;
                    });
                }
            });
        }

        // Verify signatures checkbox with persistence
        if ui
            .checkbox(
                &mut app_ui_state.verify_signatures,
                "Verify kernel signatures",
            )
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.verify_signatures;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.verify_signatures = enabled;
                    });
                }
            });
        }

        ui.separator();
    });

    ui.separator();

    // UI Customization
    ui.group(|ui| {
        ui.label("UI Customization");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Theme:");
            let themes = vec!["4rchCybrPnk", "Dark", "Light"];
            let selected_theme = themes
                .get(app_ui_state.theme)
                .copied()
                .unwrap_or("4rchCybrPnk");
            let old_theme = app_ui_state.theme;
            egui::ComboBox::from_id_source("settings_theme_combo")
                .selected_text(selected_theme)
                .show_ui(ui, |ui| {
                    for (i, theme) in themes.iter().enumerate() {
                        if ui
                            .selectable_value(&mut app_ui_state.theme, i, *theme)
                            .changed()
                        {
                            // Apply theme immediately to egui
                            let new_visuals = match i {
                                0 => egui::Visuals::dark(), // 4rchCybrPnk (custom colors applied via controller)
                                1 => egui::Visuals::dark(), // Dark
                                2 => egui::Visuals::light(), // Light
                                _ => egui::Visuals::dark(), // Default to dark
                            };
                            ui.ctx().set_visuals(new_visuals);
                        }
                    }
                });

            // If theme changed, persist it
            if app_ui_state.theme != old_theme {
                let controller_clone = Arc::clone(controller);
                let theme_idx = app_ui_state.theme;
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        let _ = controller_handle.update_state(|state| {
                            state.theme_idx = theme_idx;
                        });
                    }
                });
            }
        });

        ui.horizontal(|ui| {
            ui.label("Font Size:");
            if ui
                .add(egui::Slider::new(&mut app_ui_state.font_size, 8.0..=20.0).text("pixels"))
                .changed()
            {
                let controller_clone = Arc::clone(controller);
                let font_size = app_ui_state.font_size;
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        let _ = controller_handle.update_state(|state| {
                            state.ui_font_size = font_size;
                        });
                    }
                });
            }
        });

        if ui
            .checkbox(&mut app_ui_state.auto_scroll_logs, "Auto-scroll build logs")
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.auto_scroll_logs;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.auto_scroll_logs = enabled;
                    });
                }
            });
        }
    });

    ui.separator();

    // Startup & Behaviors
    ui.group(|ui| {
        ui.label("Startup & Behaviors");
        ui.separator();

        if ui
            .checkbox(
                &mut app_ui_state.audit_on_startup,
                "Run hardware audit on startup",
            )
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.audit_on_startup;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.audit_on_startup = enabled;
                    });
                }
            });
        }

        if ui
            .checkbox(
                &mut app_ui_state.minimize_to_tray,
                "Minimize to system tray on close",
            )
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.minimize_to_tray;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.minimize_to_tray = enabled;
                    });
                }
            });
        }

        if ui
            .checkbox(
                &mut app_ui_state.save_window_state,
                "Save window position and size",
            )
            .changed()
        {
            let controller_clone = Arc::clone(controller);
            let enabled = app_ui_state.save_window_state;
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    let _ = controller_handle.update_state(|state| {
                        state.save_window_state = enabled;
                    });
                }
            });
        }
    });

    ui.separator();

    // Advanced Settings
    ui.group(|ui| {
        ui.label("Advanced Settings");
        ui.separator();

        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut app_ui_state.debug_logging, "Enable debug logging")
                .changed()
            {
                let controller_clone = Arc::clone(controller);
                let enabled = app_ui_state.debug_logging;
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        match controller_handle.update_logging_level(enabled) {
                            Ok(()) => {
                                eprintln!("[UI] [SETTINGS] Debug logging updated: {}", enabled);
                            }
                            Err(e) => {
                                eprintln!("[UI] [SETTINGS] Failed to update logging level: {}", e);
                            }
                        }
                    }
                });
            }
            if ui.button("View Logs").clicked() {
                let controller_clone = Arc::clone(controller);
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        match controller_handle.open_logs_dir() {
                            Ok(()) => {
                                eprintln!("[UI] [SETTINGS] Logs directory opened successfully");
                            }
                            Err(e) => {
                                eprintln!("[UI] [SETTINGS] Failed to open logs directory: {}", e);
                            }
                        }
                    }
                });
            }
        });

        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut app_ui_state.tokio_tracing, "Enable Tokio tracing")
                .changed()
            {
                let controller_clone = Arc::clone(controller);
                let enabled = app_ui_state.tokio_tracing;
                tokio::spawn(async move {
                    if let Ok(controller_handle) = controller_clone.try_read() {
                        let _ = controller_handle.update_state(|state| {
                            state.tokio_tracing = enabled;
                        });
                    }
                });
            }
            if ui.button("Export Traces").clicked() {
                let controller_clone = Arc::clone(controller);
                tokio::spawn(async move {
                    // Open file save dialog
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name("traces_export.json")
                        .save_file()
                    {
                        if let Ok(controller_handle) = controller_clone.try_read() {
                            match controller_handle.export_traces(path.clone()) {
                                Ok(()) => {
                                    eprintln!("[UI] [SETTINGS] Traces exported successfully to {}", path.display());
                                }
                                Err(e) => {
                                    eprintln!("[UI] [SETTINGS] Failed to export traces: {}", e);
                                }
                            }
                        }
                    }
                });
            }
        });
    });

    ui.separator();

    // Action Buttons
    ui.horizontal(|ui| {
        if ui.button("Save Settings").clicked() {
            // Wire persistence to controller AND ensure local state is saved
            let controller_clone = Arc::clone(controller);
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    match controller_handle.persist_state() {
                        Ok(()) => eprintln!("[UI] [SETTINGS] ✓ Settings persisted successfully"),
                        Err(e) => eprintln!("[UI] [SETTINGS] ✗ Failed to persist settings: {}", e),
                    }
                } else {
                    eprintln!("[UI] [SETTINGS] ✗ Could not acquire lock for persist_state");
                }
            });
        }

        if ui.button("Reset to Defaults").clicked() {
            eprintln!("[UI] [SETTINGS] Reset to Defaults clicked - resetting controller state");

            // Wire reset to defaults to controller
            let controller_clone = Arc::clone(controller);
            tokio::spawn(async move {
                if let Ok(controller_handle) = controller_clone.try_read() {
                    match controller_handle.reset_to_defaults() {
                        Ok(()) => {
                            eprintln!("[UI] [SETTINGS] ✓ Settings reset to defaults successfully");
                        }
                        Err(e) => {
                            eprintln!("[UI] [SETTINGS] ✗ Failed to reset settings to defaults: {}", e);
                        }
                    }
                } else {
                    eprintln!("[UI] [SETTINGS] ✗ Could not acquire lock for reset_to_defaults");
                }
            });

            // CRITICAL: Clear the local UI state so it reloads from controller on next render
            // This forces a refresh of the Settings tab
            *app_ui_state = SettingsUIState::default();
            eprintln!("[UI] [SETTINGS] Local UI state cleared - will reload from controller on next frame");
        }
    });
}

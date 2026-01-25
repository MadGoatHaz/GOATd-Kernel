use super::app::AppUI;
use crate::ui::controller::AppController;
/// Kernel Manager View & Management
///
/// Manages installed kernels, built artifacts, installations, uninstalls,
/// variant logic, and SCX Scheduler (sched-ext) configuration.
use eframe::egui;
use egui_extras::StripBuilder;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// KERNEL MANAGEMENT: Install/Uninstall/Variant Logic (Phase 3 Modularization)
// ============================================================================

/// Uninstall a kernel package
pub fn uninstall_kernel(
    system: &Arc<dyn crate::ui::SystemWrapper>,
    pkg_name: &str,
) -> Result<(), String> {
    // Strip versioning info from package name (e.g., "linux-goatd-gaming (6.18.3)" -> "linux-goatd-gaming")
    let clean_pkg_name = pkg_name
        .split('(')
        .next()
        .unwrap_or(pkg_name)
        .trim()
        .to_string();

    system.uninstall_package(&clean_pkg_name)
}

/// De-rebrand a rebranded kernel variant name to its base variant
///
/// Maps GOATd rebranded names back to their base kernel variants:
/// - `linux-goatd-zen-*` -> `linux-zen`
/// - `linux-goatd-lts-*` -> `linux-lts`
/// - `linux-goatd-hardened-*` -> `linux-hardened`
/// - `linux-goatd-mainline-*` -> `linux-mainline`
/// - `linux-goatd-tkg-*` -> `linux-tkg`
/// - `linux-goatd-*` -> `linux` (fallback for any other goatd variant)
/// - Otherwise returns the original string unchanged
pub fn get_base_variant(rebranded_name: &str) -> &str {
    // Check for specific known variant mappings
    if rebranded_name.starts_with("linux-goatd-zen-") {
        "linux-zen"
    } else if rebranded_name.starts_with("linux-goatd-lts-") {
        "linux-lts"
    } else if rebranded_name.starts_with("linux-goatd-hardened-") {
        "linux-hardened"
    } else if rebranded_name.starts_with("linux-goatd-mainline-") {
        "linux-mainline"
    } else if rebranded_name.starts_with("linux-goatd-tkg-") {
        "linux-tkg"
    } else if rebranded_name.starts_with("linux-goatd-") {
        // Fallback for any other linux-goatd-* variant
        "linux"
    } else {
        // Not a rebranded variant, return as-is
        rebranded_name
    }
}

/// Resolve kernel version from .kernelrelease file with fallback to pacman extraction
///
/// This implements Step 3 of the DKMS fix by providing a "Source of Truth" for kernel version.
///
/// Strategy:
/// 1. Try to read `.kernelrelease` file from the build workspace (PREFERRED)
///    First checks the base variant directory, then the rebranded directory
/// 2. If not found, fallback to extracting from package using pacman (ROBUST FALLBACK)
/// 3. If both fail, return empty string to signal error upstream
///
/// # Arguments
/// * `kernel_path` - Path to the kernel package file
/// * `workspace_path` - Path to the build workspace directory
///
/// # Returns
/// Resolved kernel version string, or empty string if both methods fail
pub fn resolve_kernel_version(kernel_path: &PathBuf, workspace_path: &PathBuf) -> String {
    log::info!("[KERNEL] [RESOLVE] Starting kernel version resolution");
    log::debug!(
        "[KERNEL] [RESOLVE] Input: kernel_path={}, workspace_path={}",
        kernel_path.display(),
        workspace_path.display()
    );

    // === PROXIMITY-FIRST SEARCH: Calculate pkg_dir from kernel_path ===
    let pkg_dir = match kernel_path.parent() {
        Some(parent) => parent.to_path_buf(),
        None => {
            log::warn!("[KERNEL] [RESOLVE] Cannot determine parent of kernel_path");
            return String::new();
        }
    };

    log::debug!(
        "[KERNEL] [RESOLVE] Package directory (proximity-first): {}",
        pkg_dir.display()
    );

    // === SEARCH HIERARCHY: Try locations in proximity order ===
    // a. pkg_dir/.kernelrelease (same directory as the package)
    log::debug!(
        "[KERNEL] [RESOLVE] Attempting proximity search (a): {}",
        pkg_dir.join(".kernelrelease").display()
    );
    let kernelrelease_path_pkg = pkg_dir.join(".kernelrelease");
    if kernelrelease_path_pkg.exists() {
        match std::fs::read_to_string(&kernelrelease_path_pkg) {
            Ok(contents) => {
                let version = contents.trim().to_string();
                if !version.is_empty() {
                    log::info!(
                        "[KERNEL] [RESOLVE] Found .kernelrelease in package directory: {}",
                        version
                    );
                    return version;
                }
            }
            Err(e) => {
                log::warn!(
                    "[KERNEL] [RESOLVE] Failed to read .kernelrelease from pkg_dir: {}",
                    e
                );
            }
        }
    }

    // b. pkg_dir.parent()/.kernelrelease (parent directory of package)
    if let Some(parent_dir) = pkg_dir.parent() {
        log::info!(
            "[KERNEL] [RESOLVE] Attempting proximity search (b): {}",
            parent_dir.join(".kernelrelease").display()
        );
        let kernelrelease_path_parent = parent_dir.join(".kernelrelease");
        if kernelrelease_path_parent.exists() {
            match std::fs::read_to_string(&kernelrelease_path_parent) {
                Ok(contents) => {
                    let version = contents.trim().to_string();
                    if !version.is_empty() {
                        eprintln!(
                            "[KERNEL] [RESOLVE] ✓ Found .kernelrelease in parent directory: {}",
                            version
                        );
                        log::info!("[KERNEL] [RESOLVE] ✓ SUCCESS (proximity b): {}", version);
                        return version;
                    }
                }
                Err(e) => {
                    eprintln!("[KERNEL] [RESOLVE] ⚠ .kernelrelease in parent_dir exists but failed to read: {}", e);
                    log::warn!(
                        "[KERNEL] [RESOLVE] Failed to read .kernelrelease from parent_dir: {}",
                        e
                    );
                }
            }
        } else {
            log::info!("[KERNEL] [RESOLVE] .kernelrelease not found in parent directory");
        }
    }

    // c. pkg_dir.parent()/base_variant/.kernelrelease (workspace variant directory)
    // Extract variant from kernel filename to find the kernel subfolder
    let rebranded_variant = if let Some(filename) = kernel_path.file_name() {
        if let Some(filename_str) = filename.to_str() {
            // Extract variant from filename like "linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst"
            // Find where version starts (first dash followed by digit) and use preceding part as variant
            if filename_str.starts_with("linux-") {
                let remainder = &filename_str[6..]; // Skip "linux-"
                let version_start_pos = remainder
                    .char_indices()
                    .find(|(i, ch)| {
                        *ch == '-'
                            && remainder
                                .chars()
                                .nth(i + 1)
                                .map_or(false, |c| c.is_ascii_digit())
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);

                if version_start_pos > 0 {
                    format!("linux-{}", &remainder[..version_start_pos])
                } else {
                    "linux".to_string()
                }
            } else {
                "kernel".to_string()
            }
        } else {
            "kernel".to_string()
        }
    } else {
        "kernel".to_string()
    };

    let base_variant = get_base_variant(&rebranded_variant);
    log::info!(
        "[KERNEL] [RESOLVE] Extracted variant from filename: rebranded='{}', base='{}'",
        rebranded_variant,
        base_variant
    );

    // c1. Try base variant in parent directory
    if let Some(parent_dir) = pkg_dir.parent() {
        log::info!(
            "[KERNEL] [RESOLVE] Attempting proximity search (c1): {}",
            parent_dir
                .join(base_variant)
                .join(".kernelrelease")
                .display()
        );
        let kernelrelease_path_variant = parent_dir.join(base_variant).join(".kernelrelease");
        if kernelrelease_path_variant.exists() {
            match std::fs::read_to_string(&kernelrelease_path_variant) {
                Ok(contents) => {
                    let version = contents.trim().to_string();
                    if !version.is_empty() {
                        eprintln!("[KERNEL] [RESOLVE] ✓ Found .kernelrelease in base variant subfolder '{}': {}", base_variant, version);
                        log::info!("[KERNEL] [RESOLVE] ✓ SUCCESS (proximity c1): {}", version);
                        return version;
                    }
                }
                Err(e) => {
                    eprintln!("[KERNEL] [RESOLVE] ⚠ .kernelrelease in variant subfolder exists but failed to read: {}", e);
                    log::warn!("[KERNEL] [RESOLVE] Failed to read .kernelrelease from variant subfolder: {}", e);
                }
            }
        } else {
            log::info!("[KERNEL] [RESOLVE] .kernelrelease not found in base variant subfolder");
        }
    }

    // d. Fall back to workspace_path variant search (old logic)
    log::info!(
        "[KERNEL] [RESOLVE] Attempting workspace search: {}",
        workspace_path
            .join(base_variant)
            .join(".kernelrelease")
            .display()
    );
    let kernelrelease_path_workspace = workspace_path.join(base_variant).join(".kernelrelease");
    if kernelrelease_path_workspace.exists() {
        match std::fs::read_to_string(&kernelrelease_path_workspace) {
            Ok(contents) => {
                let version = contents.trim().to_string();
                if !version.is_empty() {
                    eprintln!(
                        "[KERNEL] [RESOLVE] ✓ Found .kernelrelease in workspace base variant: {}",
                        version
                    );
                    log::info!(
                        "[KERNEL] [RESOLVE] ✓ SUCCESS (workspace fallback): {}",
                        version
                    );
                    return version;
                }
            }
            Err(e) => {
                eprintln!("[KERNEL] [RESOLVE] ⚠ .kernelrelease in workspace exists but failed to read: {}", e);
                log::warn!(
                    "[KERNEL] [RESOLVE] Failed to read .kernelrelease from workspace: {}",
                    e
                );
            }
        }
    }

    // === METHOD 2: Fallback to pacman extraction (Robust Fallback) ===
    eprintln!("[KERNEL] [RESOLVE] All proximity searches exhausted, attempting pacman fallback");
    log::info!("[KERNEL] [RESOLVE] All proximity searches exhausted, attempting pacman extraction");

    let package_path = match kernel_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "[KERNEL] [RESOLVE] ✗ Failed to canonicalize kernel path: {}",
                e
            );
            log::error!(
                "[KERNEL] [RESOLVE] Failed to canonicalize kernel path: {}",
                e
            );
            return String::new();
        }
    };

    let package_str = package_path.to_string_lossy();

    // Construct pacman command to extract versioned module directory (skip base directory)
    // Pattern: pacman -Qlp | grep module paths | find versioned subdirectory (not the base)
    let cmd_str = format!(
        "pacman -Qlp '{}' | grep '/usr/lib/modules/.*/' | grep -v '/usr/lib/modules/$' | head -1 | sed -n 's|.*/usr/lib/modules/\\([^/]*\\)/.*|\\1|p'",
        package_str
    );

    eprintln!("[KERNEL] [RESOLVE] Executing pacman query: {}", cmd_str);
    log::info!("[KERNEL] [RESOLVE] Executing pacman: {}", cmd_str);

    // Use shell to execute the pipeline
    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !version.is_empty() {
                    eprintln!(
                        "[KERNEL] [RESOLVE] ✓ pacman extraction succeeded: {}",
                        version
                    );
                    log::info!(
                        "[KERNEL] [RESOLVE] ✓ SUCCESS (pacman fallback): {}",
                        version
                    );
                    return version;
                } else {
                    eprintln!(
                        "[KERNEL] [RESOLVE] ⚠ pacman query succeeded but returned empty stdout"
                    );
                    log::warn!("[KERNEL] [RESOLVE] pacman succeeded but stdout is empty");
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                eprintln!(
                    "[KERNEL] [RESOLVE] ✗ pacman exited with code: {:?}",
                    output.status.code()
                );
                eprintln!("[KERNEL] [RESOLVE] stderr: {}", stderr);
                eprintln!("[KERNEL] [RESOLVE] stdout: {}", stdout);
                log::error!(
                    "[KERNEL] [RESOLVE] pacman failed with exit code: {:?}",
                    output.status.code()
                );
                log::error!("[KERNEL] [RESOLVE] stderr: {}", stderr);
                log::error!("[KERNEL] [RESOLVE] stdout: {}", stdout);
            }
        }
        Err(e) => {
            eprintln!(
                "[KERNEL] [RESOLVE] ✗ Failed to execute pacman command: {}",
                e
            );
            log::error!("[KERNEL] [RESOLVE] Failed to execute pacman: {}", e);
        }
    }

    eprintln!("[KERNEL] [RESOLVE] ✗ FAILED: All resolution methods exhausted");
    log::error!("[KERNEL] [RESOLVE] ✗ FAILED: All resolution methods exhausted");
    String::new()
}

/// Extract kernel version from filename for fallback purposes
///
/// PHASE 20: Supports dynamic `linux-{variant}-goatd-{profile}` scheme.
/// Generalizes version extraction to handle any profile name following `-goatd-` pattern.
///
/// Maps package filenames to kernel release strings:
/// - `linux-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
/// - `linux-zen-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
/// - `linux-lts-6.18.3-arch1-1-x86_64.pkg.tar.zst` -> `6.18.3-arch1-1`
/// - `linux-zen-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-zen-goatd-gaming`
/// - `linux-goatd-gaming-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-gaming`
/// - `linux-goatd-server-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-server`
/// - `linux-goatd-workstation-6.18.3.arch1-2-x86_64.pkg.tar.zst` -> `6.18.3-arch1-2-goatd-workstation`
///
/// Dynamic profile extraction: Any content after `-goatd-` is treated as the profile name,
/// allowing new profiles to be added without code changes (e.g., `-goatd-custom`, `-goatd-optimized`).
pub fn extract_kernel_version_from_filename(filename: &str) -> Option<String> {
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
                    && remainder
                        .chars()
                        .nth(i + 1)
                        .map_or(false, |c| c.is_ascii_digit())
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
        let normalized_version = normalize_arch_version_string(version);

        // Determine if we should append the variant suffix.
        // Standard Linux kernel variants (zen, lts) use the baseline version.
        // Custom GOATd profiles (goatd-gaming, goatd-server, goatd-workstation, etc.)
        // always have their profile name appended to ensure DKMS targets the correct kernel.
        let should_append_variant = should_append_profile_suffix(variant);

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
/// PHASE 20: Dynamic profile support via `-goatd-` pivot.
///
/// Standard kernel variants (zen, lts) are not appended; custom GOATd profiles
/// containing `-goatd-` are always appended for proper DKMS targeting.
///
/// Examples:
/// - "zen" -> false (standard variant, don't append)
/// - "lts" -> false (standard variant, don't append)
/// - "goatd-gaming" -> true (custom profile, append)
/// - "zen-goatd-gaming" -> true (base variant with profile, append)
/// - "hardened-goatd-workstation" -> true (base variant with profile, append)
/// - Any content after "-goatd-" is dynamically treated as a profile
fn should_append_profile_suffix(variant: &str) -> bool {
    // PHASE 20: Pivot on "-goatd-" to detect dynamic profiles
    // If the variant contains "-goatd-", it's a custom profile and should be appended
    if variant.contains("-goatd-") {
        return true;
    }

    // Don't append for standard Linux kernel variants
    if variant == "zen" || variant == "lts" || variant.is_empty() {
        return false;
    }

    // Append for any other custom profiles
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

// ============================================================================
// UI RENDERING
// ============================================================================

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

/// Render SCX Environment Status section
///
/// Displays diagnostic messages when SCX is not ready (missing kernel support, packages, or service).
/// Shows warning indicators when prerequisites are not met.
fn render_scx_environment(ui: &mut egui::Ui, app: &AppUI) {
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
                    "❌ SCX Packages Missing: Required scheduler packages (scx-tools) are not installed.\nYou can provision the SCX environment automatically from here."
                }
                SCXReadiness::ServiceMissing => {
                    "❌ SCX Service Missing: The scx_loader systemd service is not installed.\nYou can provision the SCX environment automatically from here."
                }
                SCXReadiness::Ready => unreachable!(),
            };

            ui.label(diagnostic_msg);
            ui.separator();
        });

        ui.separator();
    }
}

/// Poll for SCX task completion and reset state if done
///
/// Monitors the SCX activation task and resets state when complete or timeout occurs.
fn render_scx_task_completion_poll(_ui: &mut egui::Ui, app: &mut AppUI) {
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
}

/// Render SCX Scheduler Configuration section
///
/// Displays the complete SCX scheduler configuration UI including header, binary path,
/// scheduler type selector, mode selector, metadata panel, and activation button.
fn render_scx_scheduler_configuration(
    ui: &mut egui::Ui,
    app: &mut AppUI,
    controller: &Arc<RwLock<AppController>>,
) {
    // SCX Scheduler Section
    ui.group(|ui| {
        // ========== HEADER WITH STATUS BADGE (Horizontal) ==========
        // Combines title and status badge in a single row for better space efficiency
        ui.horizontal(|ui| {
            // Left: Title
            ui.heading("⚙️ SCX Scheduler Configuration");

            // Right: Status badge with dynamic color and styling
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Determine badge color, icon, and text based on SCX state
                let (badge_color, badge_icon, badge_text) = if app.ui_state.scx_activating {
                    (egui::Color32::from_rgb(255, 200, 0), "⟳", "Activating...")
                } else if app.ui_state.scx_enabled {
                    (egui::Color32::from_rgb(100, 200, 100), "✓", "Enabled")
                } else {
                    (egui::Color32::from_rgb(180, 100, 100), "✗", "Disabled")
                };

                let badge_text_colored = egui::RichText::new(format!("{} {}", badge_icon, badge_text))
                    .strong()
                    .color(egui::Color32::WHITE);

                // Render badge as a frame with background color
                egui::Frame::none()
                    .fill(badge_color)
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .rounding(egui::Rounding::same(4.0))
                    .show(ui, |ui| {
                        ui.label(badge_text_colored);
                    });
            });
        });

        ui.separator();

        // ========== BINARY PATH ROW (Horizontal) ==========
        // Shows active SCX binary with truncation, tooltip, and copy button
        ui.horizontal(|ui| {
            // Left: Label
            ui.label(egui::RichText::new("Active Binary:").small().color(egui::Color32::from_rgb(120, 120, 120)));

            // Middle: Path (truncated)
            if app.ui_state.active_scx_binary.is_empty() {
                ui.monospace(egui::RichText::new("(EEVDF kernel scheduler - built-in)")
                    .small()
                    .color(egui::Color32::from_rgb(120, 120, 120)));
            } else {
                // Truncate path to fit display while preserving extension and architecture
                let truncated = truncate_path(&app.ui_state.active_scx_binary, 60);
                let path_response = ui.monospace(&truncated);

                // Add full path tooltip on hover
                path_response.on_hover_text(&app.ui_state.active_scx_binary);

                // Right: Copy button with feedback
                // Check if we should show "Copied!" feedback (expires after 2 seconds)
                let mut show_copy_feedback = false;
                if let Some((_, time)) = &app.ui_state.copy_to_clipboard_feedback {
                    if time.elapsed().as_secs_f32() < 2.0 {
                        show_copy_feedback = true;
                    } else {
                        // Feedback expired, clear it
                        app.ui_state.copy_to_clipboard_feedback = None;
                    }
                }

                let copy_button_text = if show_copy_feedback { "✓ Copied!" } else { "📋" };
                if ui.button(copy_button_text).clicked() {
                    // Copy full path to clipboard using egui's clipboard mechanism
                    ui.output_mut(|o| o.copied_text = app.ui_state.active_scx_binary.clone());
                    // Set feedback state with current timestamp
                    app.ui_state.copy_to_clipboard_feedback = Some((
                        "Copied to clipboard!".to_string(),
                        std::time::Instant::now()
                    ));
                }
            }
        });

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

        // ========== GRANULAR SCX CONTROL ==========
        // Check if BOTH scx-tools and scx-scheds are available
        let scx_tools_missing = app.ui_state.missing_optional_tools.contains(&"scx-tools".to_string());
        let scx_scheds_missing = app.ui_state.missing_optional_tools.contains(&"scx-scheds".to_string());
        let scx_packages_missing = scx_tools_missing || scx_scheds_missing;

        // Title and description (removed outer group wrapper)
        ui.label(egui::RichText::new("🎛️ Permanent Scheduler Configuration").strong());
        ui.label(egui::RichText::new("Configured via scx_loader.service with /etc/scx_loader/config.toml (persists across reboots)").small().italics());
        ui.separator();

        // Show block message if scx-tools or scx-scheds is missing
        if scx_packages_missing {
            ui.colored_label(
                egui::Color32::from_rgb(200, 150, 100),
                "⚠ SCX Scheduler Feature Unavailable"
            );
            ui.label("Required SCX packages are not installed on your system:");
            ui.label(format!("  • scx-tools: {}", if scx_tools_missing { "missing" } else { "installed" }));
            ui.label(format!("  • scx-scheds: {}", if scx_scheds_missing { "missing" } else { "installed" }));
            ui.label("Advanced SCX scheduler selection is disabled.");
            ui.separator();
            ui.label(egui::RichText::new("To enable this feature:").strong());
            ui.label("Use the 'Fix System Environment' button on the Dashboard tab to auto-install both packages.");
            ui.label("After installation, restart the application.");
        } else {
            // 2-Column Horizontal Layout for Scheduler Type and Mode
            ui.columns(2, |columns| {
                // Column 0: Scheduler Type
                columns[0].vertical(|ui| {
                    // Label with info icon tooltip
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Scheduler Type").strong());
                        if ui.small_button("ⓘ")
                            .on_hover_text(
                                "Choose the SCX scheduler that best fits your workload. \
                                Each scheduler optimizes for different system characteristics."
                            )
                            .clicked()
                        {
                            // Optional: log info button click
                        }
                    });

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

                // Column 1: Scheduler Mode
                columns[1].vertical(|ui| {
                    // Label with info icon tooltip
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Scheduler Mode").strong());
                        if ui.small_button("ⓘ")
                            .on_hover_text(
                                "Configure optimization profile for your specific workload type:\n\
                                • Auto: Adaptive scheduling (recommended)\n\
                                • Gaming: Low latency & responsiveness\n\
                                • LowLatency: Ultra-low latency for real-time\n\
                                • PowerSave: Power-efficient scheduling\n\
                                • Server: Throughput optimization"
                            )
                            .clicked()
                        {
                            // Optional: log info button click
                        }
                    });

                    let modes = vec!["Auto", "Gaming", "LowLatency", "PowerSave", "Server"];
                    let mode_descriptions = vec![
                        "Automatic adaptive scheduling - scheduler detects workload & adjusts in real-time",
                        "Optimized for frame delivery and interactive responsiveness - gaming & responsive apps",
                        "Minimized latency for precision timing - audio, video, real-time apps",
                        "Power-efficient scheduling - laptops, battery-powered devices, low power systems",
                        "Throughput-optimized for server and batch workloads - microservices, batch jobs",
                    ];
                    let mut selected_mode_idx = app.ui_state.selected_scx_mode_idx.unwrap_or(0);
                    let mode_text = modes.get(selected_mode_idx).copied().unwrap_or("Auto");

                    egui::ComboBox::from_id_source("scx_mode_combo")
                        .selected_text(mode_text)
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
        }

        // Activate Permanent Change Button (Full Width, no group wrapper)
        if !scx_packages_missing {
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

            // Button expands to fill available width
            if ui.add_enabled(can_activate,
                egui::Button::new(button_text)
                    .min_size([ui.available_width(), 40.0].into())
            ).clicked() {
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
            }

            // Show status message inline if activating
            if app.ui_state.scx_activating {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 200, 0),
                    "⟳ Activation in progress... Check system authorization dialog"
                );
            }
        }

        ui.separator();

        // ========== RICH SCX METADATA PANEL ==========
        // Display dynamic metadata for selected scheduler/mode pairing
        // Panel is always visible; shows loading state if metadata is not yet loaded
        egui::Frame::none()
            .fill(egui::Color32::from_rgb(30, 30, 35))
            .inner_margin(egui::Margin::same(12.0))
            .rounding(egui::Rounding::same(6.0))
            .show(ui, |ui| {
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

                    // Best Utilized For - Consolidated to comma-separated inline text
                    ui.label(egui::RichText::new("🎯 Best Utilized For:").strong());
                    let best_for_items: Vec<&str> = metadata.best_for
                        .split(',')
                        .map(|s| s.trim())
                        .collect();
                    let comma_separated = best_for_items.join(", ");
                    ui.label(egui::RichText::new(comma_separated)
                        .color(egui::Color32::from_rgb(150, 200, 150))
                        .italics());

                    ui.separator();

                    // ========== TECHNICAL DETAILS FOOTER BAR ==========
                    egui::Frame::none()
                        .fill(egui::Color32::from_rgb(20, 20, 25))
                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                        .rounding(egui::Rounding::same(4.0))
                        .show(ui, |ui| {
                            ui.columns(2, |columns| {
                                // Column 0: Command Flags (left-aligned)
                                columns[0].vertical(|ui| {
                                    ui.label(egui::RichText::new("⚡ Command Flags")
                                        .small()
                                        .strong()
                                        .color(egui::Color32::from_rgb(200, 200, 200)));

                                    if metadata.cli_flags.is_empty() {
                                        ui.monospace(egui::RichText::new("[Default]")
                                            .small()
                                            .color(egui::Color32::from_rgb(100, 100, 100)));
                                    } else {
                                        let flags_text = format!("-k {}", metadata.cli_flags);
                                        ui.monospace(egui::RichText::new(&flags_text)
                                            .small()
                                            .color(egui::Color32::from_rgb(150, 255, 150)));
                                    }
                                });

                                // Column 1: Optimization Profile (right-aligned)
                                columns[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let perf_tier = determine_perf_tier(&metadata.best_for);

                                    ui.label(egui::RichText::new("📊 Profile")
                                        .small()
                                        .strong()
                                        .color(egui::Color32::from_rgb(200, 200, 200)));

                                    ui.label(egui::RichText::new(perf_tier)
                                        .small()
                                        .color(egui::Color32::from_rgb(255, 200, 100)));
                                });
                            });
                        });

                } else {
                    // Placeholder when metadata is not available
                    ui.colored_label(
                        egui::Color32::from_rgb(100, 100, 100),
                        "ℹ️ Select a scheduler and mode to view detailed configuration"
                    );
                }
            });
    });
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
            app.ui_state.installed_kernels = installed
                .iter()
                .map(|k| format!("{} ({})", k.name, k.version))
                .collect();
            eprintln!(
                "[UI] [KERNELS] Loaded {} installed kernels",
                app.ui_state.installed_kernels.len()
            );

            // Get built artifacts via scan_workspace() with workspace path from settings
            if let Ok(state) = guard.get_state() {
                let artifacts = guard.kernel_manager.scan_workspace(&state.workspace_path);
                app.ui_state.built_artifacts = artifacts; // Store raw KernelPackage objects, not formatted strings
                eprintln!(
                    "[UI] [KERNELS] Loaded/refreshed {} built artifacts from {}",
                    app.ui_state.built_artifacts.len(),
                    state.workspace_path
                );
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
    // Responsive layout: horizontal split (30/70) above 1100px, stacked below
    egui::ScrollArea::vertical().show(ui, |ui| {
        let available_width = ui.available_width();
        let use_horizontal = available_width >= 1100.0;

        if use_horizontal {
            // Horizontal layout with StripBuilder (30% sidebar / 70% content)
            StripBuilder::new(ui)
                .size(egui_extras::Size::relative(0.30))
                .size(egui_extras::Size::remainder())
                .horizontal(|mut strip| {
                    // Left panel (30%): Kernel list
                    strip.cell(|ui| {
                        ui.vertical(|ui| {
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
                                                      match uninstall_kernel(&controller.system, &kernel_name) {
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
                    });

                    // Right panel (70%): Audit details and SCX config
                    strip.cell(|ui| {
                        ui.vertical(|ui| {
                            // Render SCX environment status
                            render_scx_environment(ui, app);

                            // Poll for SCX task completion and reset state if done
                            render_scx_task_completion_poll(ui, app);

                            // Render SCX scheduler configuration
                            render_scx_scheduler_configuration(ui, app, controller);
                        });
                    });
                });
        } else {
            // Vertical stacked layout for screens < 1100px
            ui.vertical(|ui| {
                // Top section: Kernel list
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
                                          match uninstall_kernel(&controller.system, &kernel_name) {
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

                // Middle section: Built artifacts
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

                ui.separator();

                // Bottom section: Audit details and SCX config
                // Render SCX environment status
                render_scx_environment(ui, app);

                // Poll for SCX task completion and reset state if done
                render_scx_task_completion_poll(ui, app);

                // Render SCX scheduler configuration
                render_scx_scheduler_configuration(ui, app, controller);
            });
        }
    });
}

/// Truncate a file path to fit within a maximum length while preserving important parts
///
/// Preserves the directory prefix and file suffix (e.g., architecture info like -x86_64-linux_gnu)
/// Truncates the middle with "..." ellipsis.
///
/// # Arguments
/// * `full_path` - The complete file path to truncate
/// * `max_len` - Maximum display length in characters
///
/// # Returns
/// Truncated path string, or original path if it fits within max_len
pub fn truncate_path(full_path: &str, max_len: usize) -> String {
    // If already short enough, return as-is
    if full_path.len() <= max_len {
        return full_path.to_string();
    }

    // Split into directory and filename
    let (dir, filename) = if let Some((dir_part, file_part)) = full_path.rsplit_once('/') {
        (dir_part, file_part)
    } else {
        ("", full_path)
    };

    // Extract file suffix (e.g., "-x86_64-linux_gnu")
    let suffix_len = find_architecture_suffix_length(filename);
    let suffix = if suffix_len > 0 {
        &filename[filename.len().saturating_sub(suffix_len)..]
    } else {
        ""
    };

    // Calculate available space for middle content
    let prefix_display = if dir.is_empty() { "" } else { "/" };
    let ellipsis_len = 3; // "..."
    let available = max_len.saturating_sub(prefix_display.len() + suffix_len + ellipsis_len);

    if available <= 0 {
        // Extreme case: just show ellipsis and suffix
        return format!("...{}", suffix);
    }

    // Take start of filename
    let filename_without_suffix =
        &filename[0..available.min(filename.len().saturating_sub(suffix_len))];

    if suffix_len > 0 {
        format!(
            "{}{}{}...{}",
            dir, prefix_display, filename_without_suffix, suffix
        )
    } else {
        format!("{}{}{}", dir, prefix_display, filename_without_suffix)
    }
}

/// Find the length of architecture suffix in a filename
///
/// Looks for patterns like "-x86_64-linux_gnu" or "-aarch64-unknown-linux-gnu"
/// Returns the length from first dash that looks like architecture pattern.
///
/// # Arguments
/// * `filename` - The filename to analyze
///
/// # Returns
/// Length of the architecture suffix, or 0 if no pattern found
pub fn find_architecture_suffix_length(filename: &str) -> usize {
    for (idx, _) in filename.rmatch_indices('-') {
        let potential_suffix = &filename[idx..];
        if is_architecture_pattern(potential_suffix) {
            return filename.len() - idx;
        }
    }
    0
}

/// Check if a string matches a known architecture pattern
///
/// Detects common architecture-related keywords like x86, aarch, linux, gnu, etc.
///
/// # Arguments
/// * `s` - The string to check
///
/// # Returns
/// True if the string contains architecture-related keywords
pub fn is_architecture_pattern(s: &str) -> bool {
    let s_lower = s.to_lowercase();
    s_lower.contains("x86")
        || s_lower.contains("aarch")
        || s_lower.contains("linux")
        || s_lower.contains("gnu")
        || s_lower.contains("musl")
        || s_lower.contains("arm")
}

/// Determine the performance tier indicator based on scheduler best-for description
///
/// Analyzes the "best_for" metadata field to classify the scheduler into performance tiers.
/// Returns an emoji-prefixed tier name for visual identification.
///
/// # Arguments
/// * `best_for` - The "best_for" field from SCX scheduler metadata
///
/// # Returns
/// A string indicating the performance tier (e.g., "⚡ Ultra-Low Latency")
fn determine_perf_tier(best_for: &str) -> String {
    let best_for_lower = best_for.to_lowercase();

    if best_for_lower.contains("latency") || best_for_lower.contains("real-time") {
        "⚡ Ultra-Low Latency".to_string()
    } else if best_for_lower.contains("power") || best_for_lower.contains("battery") {
        "🔋 Power Optimized".to_string()
    } else if best_for_lower.contains("gaming") || best_for_lower.contains("audio") {
        "🎮 Responsiveness Optimized".to_string()
    } else if best_for_lower.contains("server") || best_for_lower.contains("throughput") {
        "⚙️ Throughput Optimized".to_string()
    } else {
        "⚖️ Balanced".to_string()
    }
}

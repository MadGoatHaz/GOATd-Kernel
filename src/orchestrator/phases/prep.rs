//! Phase 1: Preparation - Hardware validation and build environment setup.
//!
//! This module handles the initial preparation phase which validates:
//! - Hardware meets minimum requirements (4GB RAM, 20GB disk)
//! - Kernel source path is valid and accessible
//! - Build workspace is properly structured

use crate::error::BuildError;
use crate::models::HardwareInfo;
use std::path::Path;

/// Verifies hardware and source structure.
///
/// This function performs the core preparation phase validation:
/// 1. Hardware requirements (minimum 4GB RAM, 20GB free disk)
/// 2. Kernel source path existence and validity
/// 3. Presence of PKGBUILD or Makefile
/// 4. Kbuild path safety checks (no spaces or colons that could break builds)
/// 5. Creates .goatd_anchor file for cross-mount workspace resolution
///
/// # Arguments
/// * `hardware` - System hardware information to validate
/// * `kernel_path` - Path to kernel source directory
///
/// # Returns
/// * `Ok(())` if all preparation checks pass
/// * `Err(BuildError::PreparationFailed)` if any validation fails
pub fn prepare_build_environment(
    hardware: &HardwareInfo,
    kernel_path: &Path,
) -> Result<(), BuildError> {
    eprintln!(
        "[Build] [DEBUG] Preparing build environment at: {}",
        kernel_path.display()
    );

    validate_hardware(hardware)?;

    // =========================================================================
    // SAFEGUARD VALIDATION: Check if workspace path is valid for Kbuild
    // =========================================================================
    // Redundant check: even if controller validation is bypassed, this ensures
    // we catch invalid paths (with spaces or colons) before attempting the build
    use crate::kernel::validator::validate_kbuild_path;

    validate_kbuild_path(kernel_path).map_err(|app_err| {
        let error_msg = app_err.user_message();
        eprintln!("[Build] [SAFEGUARD] Path validation failed: {}", error_msg);
        BuildError::PreparationFailed(error_msg)
    })?;

    eprintln!("[Build] [SAFEGUARD] ✓ Workspace path validation passed");

    if !kernel_path.exists() {
        return Err(BuildError::PreparationFailed(format!(
            "Kernel source not found at: {}",
            kernel_path.display()
        )));
    } else {
        eprintln!(
            "[Build] [PREPARATION] Kernel source found at: {}",
            kernel_path.display()
        );
    }

    if !kernel_path.is_dir() {
        return Err(BuildError::PreparationFailed(format!(
            "Kernel source path exists but is not a directory: {}",
            kernel_path.display()
        )));
    }

    if !kernel_path.join("PKGBUILD").exists() && !kernel_path.join("Makefile").exists() {
        return Err(BuildError::PreparationFailed(format!(
            "Valid kernel source not found in {}. Missing PKGBUILD or Makefile.",
            kernel_path.display()
        )));
    }

    // =========================================================================
    // PHASE 14: CROSS-MOUNT PATH RESOLUTION - Create .goatd_anchor
    // =========================================================================
    // Create an empty .goatd_anchor file at the workspace root to provide
    // definitive absolute path resolution across mount points and fakeroot transitions.
    // The workspace root is the parent directory of kernel_path.
    // PHASE 18: Non-destructive anchor creation - check for existence before writing
    if let Some(workspace_root) = kernel_path.parent() {
        let anchor_path = workspace_root.join(".goatd_anchor");

        // PHASE 18: IDEMPOTENCY GUARD - Check if anchor already exists
        if anchor_path.exists() {
            eprintln!(
                "[Build] [ANCHOR] ✓ .goatd_anchor already exists at workspace root: {}",
                anchor_path.display()
            );
        } else {
            // Non-destructive write: only create if it doesn't exist
            match std::fs::write(&anchor_path, "") {
                Ok(_) => {
                    eprintln!(
                        "[Build] [ANCHOR] ✓ Created .goatd_anchor at workspace root: {}",
                        anchor_path.display()
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[Build] [ANCHOR] WARNING: Could not create .goatd_anchor at {}: {}",
                        anchor_path.display(),
                        e
                    );
                    // Non-fatal - the resolver will search for it with fallbacks
                }
            }
        }
    } else {
        eprintln!("[Build] [ANCHOR] WARNING: Could not determine workspace root (kernel_path has no parent)");
    }

    eprintln!("[Build] [PREPARATION] Environment ready");
    Ok(())
}

/// Validates memory and disk requirements.
///
/// # Arguments
/// * `hardware` - Hardware information to validate
///
/// # Returns
/// * `Ok(())` if hardware meets minimum requirements
/// * `Err(BuildError::PreparationFailed)` if insufficient resources
fn validate_hardware(hardware: &HardwareInfo) -> Result<(), BuildError> {
    if hardware.ram_gb < 4 {
        return Err(BuildError::PreparationFailed(
            "Insufficient RAM: minimum 4GB required".to_string(),
        ));
    }

    if hardware.disk_free_gb < 20 {
        return Err(BuildError::PreparationFailed(
            "Insufficient disk space: minimum 20GB free required".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BootManager, BootType, GpuVendor, InitSystem, StorageType};

    fn create_test_hardware() -> HardwareInfo {
        HardwareInfo {
            cpu_model: "Intel Core i7".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 32,
            disk_free_gb: 100,
            gpu_vendor: GpuVendor::Nvidia,
            gpu_model: "NVIDIA RTX 3080".to_string(),
            gpu_active_driver: true,
            storage_type: StorageType::Nvme,
            storage_model: "Samsung 970 EVO".to_string(),
            boot_type: BootType::Efi,
            boot_manager: BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        }
    }

    #[test]
    fn test_validate_hardware_sufficient() {
        let hw = create_test_hardware();
        assert!(validate_hardware(&hw).is_ok());
    }

    #[test]
    fn test_validate_hardware_insufficient_ram() {
        let mut hw = create_test_hardware();
        hw.ram_gb = 2;
        assert!(validate_hardware(&hw).is_err());
    }

    #[test]
    fn test_validate_hardware_insufficient_disk() {
        let mut hw = create_test_hardware();
        hw.disk_free_gb = 10;
        assert!(validate_hardware(&hw).is_err());
    }

    #[test]
    fn test_prepare_build_environment_success() {
        use std::fs::File;
        use std::io::Write;
        use tempfile::TempDir;

        let hw = create_test_hardware();

        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        let pkgbuild_path = temp_dir.path().join("PKGBUILD");
        let mut file = File::create(&pkgbuild_path).expect("Failed to create PKGBUILD");
        writeln!(file, "# Dummy PKGBUILD for testing").expect("Failed to write PKGBUILD");
        drop(file);

        let result = prepare_build_environment(&hw, temp_dir.path());

        assert!(result.is_ok());
    }

    #[test]
    fn test_prepare_build_environment_insufficient_hardware() {
        let mut hw = create_test_hardware();
        hw.ram_gb = 2;
        let kernel_path = std::path::Path::new(".");
        assert!(prepare_build_environment(&hw, kernel_path).is_err());
    }
}

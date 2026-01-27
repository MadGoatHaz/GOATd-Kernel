//! Essential driver whitelist protection module.
//!
//! Ensures critical drivers cannot be excluded from the kernel build when
//! whitelist protection is enabled. This module manages a hardcoded list of
//! essential drivers that must always be present for basic system functionality.
//!
//! # CANONICAL TRUTH: Essential Driver Whitelist
//!
//! When `use_modprobed=true`, the kernel build uses dynamic hardware detection
//! to include only drivers for detected hardware. However, to prevent boot
//! failures on systems with undetected hardware, a minimal whitelist of
//! essential drivers is always included:
//!
//! - **HID**: evdev, hid, hid-generic, usbhid
//! - **Storage**: nvme, ahci, libata, scsi
//! - **Filesystems**: ext4, btrfs, vfat, exfat, nls_cp437, nls_iso8859_1, nls_utf8
//! - **USB**: usb_core, usb_storage, usb-common, xhci_hcd, ehci_hcd, ohci_hcd
//!
//! # CRITICAL DEPENDENCY: Auto-Discovery Integration
//!
//! **The whitelist logic ONLY applies when `use_modprobed=true`.**
//! If modprobed-db is disabled (auto-discovery is off), the whitelist is NOT
//! used. The caller MUST ensure whitelist functions are only called when
//! `config.use_modprobed` is true.
//!
//! # Module Responsibilities
//!
//! - Maintaining a comprehensive list of essential drivers (CANONICAL TRUTH)
//! - Enforcing whitelist constraints on exclusions when modprobed is enabled
//! - Validating configuration compliance
//! - Reporting whitelist violations
//!
//! # Examples
//!
//! ```no_run
//! use goatd_kernel::config::whitelist::*;
//! use goatd_kernel::models::KernelConfig;
//! use std::collections::HashMap;
//!
//! let mut config = KernelConfig::default();
//! config.use_whitelist = true;
//! config.use_modprobed = true;  // CRITICAL: Whitelist only applies with modprobed
//! config.driver_exclusions = vec!["nouveau_fake".to_string(), "nvme".to_string()];
//!
//! // Apply whitelist protection (ONLY if use_modprobed is true)
//! if config.use_modprobed && config.use_whitelist {
//!     apply_whitelist(&mut config);
//! }
//!
//! // Validate no essential drivers are excluded
//! validate_whitelist(&config)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::ConfigError;
use crate::models::KernelConfig;

/// Essential drivers that must always be present in the kernel.
///
/// CRITICAL: This is a MINIMAL "safety net" to ensure basic desktop functionality
/// if modprobed-db hasn't detected specific hardware yet. We DO NOT include:
/// - GPU drivers (nouveau, amdgpu, i915, drm, etc.) - handled by modprobed-db + GPU patches
/// - Network drivers (e1000, e1000e, r8169, virtio_net) - handled by modprobed-db
///
/// These cover ONLY critical storage, input, and filesystem functionality.
/// Target size: ~100MB kernel, not 500MB.
const ESSENTIAL_DRIVERS: &[&str] = &[
    // ===================================================================
    // CANONICAL TRUTH: Essential Driver Whitelist for GOATd Kernel
    // ===================================================================
    // These drivers are the MINIMAL set required for basic desktop
    // functionality. They are ONLY applied when use_modprobed is true.
    // All other device drivers are discovered dynamically via modprobed-db.
    //
    // FORCE-BUILTIN TRANSITION:
    // - Filesystem drivers (ext4, btrfs, vfat, fat, exfat) are TRANSITIONED to =y
    // - NLS codepages (nls_ascii, nls_cp437, nls_utf8, nls_iso8859_1) are TRANSITIONED to =y
    // - This ensures these drivers are built into vmlinux and immune to modprobed-db filtering
    // - See FORCE_Y_WHITELIST_BLUEPRINT.md for architecture details
    //
    // SYNCHRONIZATION: When adding drivers to this list, also update
    // src/kernel/patcher/templates.rs WHITELIST_INJECTION constant with
    // corresponding CONFIG_* entries to ensure patcher templates match.
    // Also update src/kernel/patcher/pkgbuild.rs get_force_builtin_config_map() function.

    // Storage Controllers (CRITICAL for boot detection)
    "nvme",   // NVMe SSD support (modern standard)
    "ahci",   // SATA controller (fallback to SATA if NVMe fails)
    "libata", // LibATA framework (supports SATA)
    "scsi",   // SCSI subsystem (for block devices)
    // Filesystems (CRITICAL: boot and base USB/SD access - ALL FORCED TO =y)
    "ext4",          // Common Linux filesystem
    "btrfs",         // Btrfs filesystem (modern copy-on-write filesystem)
    "vfat",          // FAT filesystem (boot partitions, USB compatibility) [FORCE-Y]
    "exfat",         // ExFAT filesystem (modern FAT extension, USB compatibility) [FORCE-Y]
    "nls_cp437",     // DOS/Windows codepage for VFAT/ExFAT (US/English) [FORCE-Y]
    "nls_iso8859_1", // ISO-8859-1 codepage for VFAT/ExFAT (Western European) [FORCE-Y]
    "nls_utf8",      // UTF-8 codepage for ExFAT support (modern filesystems) [FORCE-Y]
    "nls_ascii",     // ASCII codepage for basic filesystem support [FORCE-Y]
    // Input Devices (CRITICAL: keyboard/mouse functionality)
    "evdev",       // Input event device handler (base for all input)
    "hid",         // Human Interface Device base (required before hid-generic)
    "hid-generic", // Generic HID driver (keyboards, mice, USB input)
    "usbhid",      // USB HID protocol (USB keyboard/mouse support)
    // USB Subsystem (CRITICAL: USB storage and devices)
    "usb_core",    // USB core framework (dependency for all USB)
    "usb_storage", // USB mass storage (USB drives/external drives)
    "usb-common",  // USB common code
    "xhci_hcd",    // USB 3.0 host controller (modern standard)
    "ehci_hcd",    // USB 2.0 host controller (fallback)
    "ohci_hcd",    // USB 1.1 host controller (legacy fallback)
];

/// Get the list of essential drivers that must not be excluded.
///
/// Returns a static slice of driver names. These drivers are hardcoded
/// and provide basic functionality for storage, input, and display.
///
/// # Returns
///
/// A vector of &'static str references to essential driver names.
/// No allocation beyond the Vec wrapper - data is static.
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::whitelist::get_essential_drivers;
///
/// let drivers = get_essential_drivers();
/// assert!(drivers.contains(&"ext4"));
/// assert!(drivers.contains(&"nvme"));
/// ```
pub fn get_essential_drivers() -> Vec<&'static str> {
    ESSENTIAL_DRIVERS.to_vec()
}

/// Check if a driver is in the essential driver whitelist.
///
/// Performs case-insensitive matching to handle variations in driver name
/// formatting. Converts both the input and whitelist entries to lowercase
/// for comparison.
///
/// # Arguments
///
/// * `driver_name` - Name of the driver to check
///
/// # Returns
///
/// true if the driver is essential, false otherwise.
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::whitelist::is_essential_driver;
///
/// assert!(is_essential_driver("ext4"));
/// assert!(is_essential_driver("EXT4")); // case-insensitive
/// assert!(is_essential_driver("nvme"));
/// assert!(!is_essential_driver("nouveau_fake"));
/// ```
pub fn is_essential_driver(driver_name: &str) -> bool {
    let driver_lower = driver_name.to_lowercase();
    ESSENTIAL_DRIVERS
        .iter()
        .any(|&essential| essential.to_lowercase() == driver_lower)
}

/// Apply whitelist protection to kernel configuration.
///
/// Removes any essential drivers from the driver exclusions list,
/// ensuring they cannot be excluded. This modifies the configuration
/// in-place.
///
/// # Arguments
///
/// * `config` - Mutable reference to KernelConfig to update
///
/// # Examples
///
/// ```no_run
/// # use goatd_kernel::config::whitelist::apply_whitelist;
/// # use goatd_kernel::models::KernelConfig;
///
/// let mut config = KernelConfig::default();
/// config.use_whitelist = true;
/// config.driver_exclusions = vec!["nvme".to_string()];
///
/// apply_whitelist(&mut config);
/// // nvme has been removed from driver_exclusions
/// assert!(config.driver_exclusions.is_empty());
/// ```
pub fn apply_whitelist(config: &mut KernelConfig) {
    // Remove any essential drivers from exclusions
    config
        .driver_exclusions
        .retain(|excluded| !is_essential_driver(excluded));
}

/// Get list of whitelist violations in the configuration.
///
/// Returns a vector of essential driver names that are currently
/// in the driver_exclusions list. Useful for error reporting and
/// debugging configuration issues.
///
/// # Arguments
///
/// * `config` - KernelConfig to check for violations
///
/// # Returns
///
/// Vector of driver names that violate the whitelist. Empty if compliant.
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::whitelist::get_whitelist_violations;
/// # use goatd_kernel::models::KernelConfig;
///
/// let mut config = KernelConfig::default();
/// config.use_whitelist = true;
/// config.driver_exclusions = vec!["nvme".to_string(), "ext4".to_string()];
///
/// let violations = get_whitelist_violations(&config);
/// assert_eq!(violations.len(), 2);
/// assert!(violations.contains(&"nvme".to_string()));
/// assert!(violations.contains(&"ext4".to_string()));
/// ```
pub fn get_whitelist_violations(config: &KernelConfig) -> Vec<String> {
    config
        .driver_exclusions
        .iter()
        .filter(|excluded| is_essential_driver(excluded))
        .cloned()
        .collect()
}

/// Validate that the configuration complies with whitelist constraints.
///
/// Checks that no essential drivers are in the driver_exclusions list.
/// Returns an error with detailed violation information if any are found.
///
/// # Arguments
///
/// * `config` - KernelConfig to validate
///
/// # Returns
///
/// Ok(()) if configuration is compliant.
/// Err(ConfigError::ValidationFailed) if essential drivers are excluded.
///
/// # Examples
///
/// ```
/// # use goatd_kernel::config::whitelist::validate_whitelist;
/// # use goatd_kernel::models::KernelConfig;
///
/// let mut config_bad = KernelConfig::default();
/// config_bad.driver_exclusions = vec!["nvme".to_string()];
///
/// let mut config_good = KernelConfig::default();
/// config_good.driver_exclusions = vec!["fake_driver".to_string()];
///
/// assert!(validate_whitelist(&config_bad).is_err());
/// assert!(validate_whitelist(&config_good).is_ok());
/// ```
pub fn validate_whitelist(config: &KernelConfig) -> Result<(), ConfigError> {
    let violations = get_whitelist_violations(config);

    if violations.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::ValidationFailed(format!(
            "Whitelist validation failed: essential drivers cannot be excluded: {}",
            violations.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_essential_drivers_not_empty() {
        let drivers = get_essential_drivers();
        assert!(!drivers.is_empty());
    }

    #[test]
    fn test_get_essential_drivers_contains_storage() {
        let drivers = get_essential_drivers();
        assert!(drivers.contains(&"ext4"));
        assert!(drivers.contains(&"nvme"));
        assert!(drivers.contains(&"btrfs"));
    }

    #[test]
    fn test_get_essential_drivers_contains_input() {
        let drivers = get_essential_drivers();
        assert!(drivers.contains(&"evdev"));
        assert!(drivers.contains(&"hid"));
        assert!(drivers.contains(&"usbhid"));
    }

    #[test]
    fn test_get_essential_drivers_contains_filesystem() {
        let drivers = get_essential_drivers();
        assert!(drivers.contains(&"ext4"));
        assert!(drivers.contains(&"vfat"));
    }

    #[test]
    fn test_is_essential_driver_exact_match() {
        assert!(is_essential_driver("ext4"));
        assert!(is_essential_driver("hid-generic"));
        assert!(is_essential_driver("usb_core"));
    }

    #[test]
    fn test_is_essential_driver_case_insensitive() {
        assert!(is_essential_driver("EXT4"));
        assert!(is_essential_driver("Ext4"));
        assert!(is_essential_driver("HID-GENERIC"));
        assert!(is_essential_driver("hid-generic"));
        assert!(is_essential_driver("USB_CORE"));
        assert!(is_essential_driver("Usb_Core"));
    }

    #[test]
    fn test_is_essential_driver_not_essential() {
        assert!(!is_essential_driver("fake_driver"));
        assert!(!is_essential_driver("nonexistent"));
        assert!(!is_essential_driver(""));
    }

    #[test]
    fn test_apply_whitelist_removes_essential() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["ext4".to_string(), "hid-generic".to_string()],
            ..KernelConfig::default()
        };

        apply_whitelist(&mut config);

        // Both are essential and should be removed
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_apply_whitelist_keeps_non_essential() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["fake_driver".to_string()],
            ..KernelConfig::default()
        };

        apply_whitelist(&mut config);

        // fake_driver is not essential, should remain
        assert_eq!(config.driver_exclusions.len(), 1);
        assert_eq!(config.driver_exclusions[0], "fake_driver");
    }

    #[test]
    fn test_apply_whitelist_mixed_exclusions() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec![
                "hid-generic".to_string(),
                "fake_driver".to_string(),
                "ext4".to_string(),
                "another_fake".to_string(),
            ],
            ..KernelConfig::default()
        };

        apply_whitelist(&mut config);

        // Only non-essential drivers should remain
        assert_eq!(config.driver_exclusions.len(), 2);
        assert!(config
            .driver_exclusions
            .contains(&"fake_driver".to_string()));
        assert!(config
            .driver_exclusions
            .contains(&"another_fake".to_string()));
    }

    #[test]
    fn test_apply_whitelist_case_insensitive() {
        let mut config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["HID-GENERIC".to_string(), "EXT4".to_string()],
            ..KernelConfig::default()
        };

        apply_whitelist(&mut config);

        // Case-insensitive matches should be removed
        assert!(config.driver_exclusions.is_empty());
    }

    #[test]
    fn test_get_whitelist_violations_none() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["fake_driver".to_string()],
            ..KernelConfig::default()
        };

        let violations = get_whitelist_violations(&config);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_get_whitelist_violations_single() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["ext4".to_string()],
            ..KernelConfig::default()
        };

        let violations = get_whitelist_violations(&config);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0], "ext4");
    }

    #[test]
    fn test_get_whitelist_violations_multiple() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec![
                "hid-generic".to_string(),
                "ext4".to_string(),
                "fake".to_string(),
                "usb_core".to_string(),
            ],
            ..KernelConfig::default()
        };

        let violations = get_whitelist_violations(&config);
        assert_eq!(violations.len(), 3);
        assert!(violations.contains(&"hid-generic".to_string()));
        assert!(violations.contains(&"ext4".to_string()));
        assert!(violations.contains(&"usb_core".to_string()));
        assert!(!violations.contains(&"fake".to_string()));
    }

    #[test]
    fn test_get_whitelist_violations_case_insensitive() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["HID-GENERIC".to_string(), "EXT4".to_string()],
            ..KernelConfig::default()
        };

        let violations = get_whitelist_violations(&config);
        assert_eq!(violations.len(), 2);
        assert!(violations.contains(&"HID-GENERIC".to_string()));
        assert!(violations.contains(&"EXT4".to_string()));
    }

    #[test]
    fn test_validate_whitelist_compliant() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["fake_driver".to_string()],
            ..KernelConfig::default()
        };

        let result = validate_whitelist(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_whitelist_non_compliant() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["ext4".to_string()],
            ..KernelConfig::default()
        };

        let result = validate_whitelist(&config);
        assert!(result.is_err());

        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("ext4"));
            assert!(msg.contains("Whitelist validation failed"));
        }
    }

    #[test]
    fn test_validate_whitelist_multiple_violations() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec!["hid-generic".to_string(), "ext4".to_string()],
            ..KernelConfig::default()
        };

        let result = validate_whitelist(&config);
        assert!(result.is_err());

        if let Err(e) = result {
            let msg = e.to_string();
            assert!(msg.contains("hid-generic"));
            assert!(msg.contains("ext4"));
        }
    }

    #[test]
    fn test_validate_whitelist_empty_exclusions() {
        let config = KernelConfig {
            version: "6.6.0".to_string(),
            driver_exclusions: vec![],
            ..KernelConfig::default()
        };

        let result = validate_whitelist(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_whitelist_coverage_storage() {
        // Verify minimal storage controllers are in whitelist
        // NVMe (modern standard)
        assert!(is_essential_driver("nvme"));
        // SATA controllers (fallback)
        assert!(is_essential_driver("ahci"));
        assert!(is_essential_driver("libata"));
        // SCSI subsystem (for block devices)
        assert!(is_essential_driver("scsi"));
    }

    #[test]
    fn test_whitelist_coverage_filesystems() {
        // Verify filesystem drivers are in whitelist (Canonical Truth: Ext4/BTRFS/VFAT/ExFAT)
        // Common Linux filesystem
        assert!(is_essential_driver("ext4"));
        // Btrfs filesystem (modern copy-on-write)
        assert!(is_essential_driver("btrfs"));
        // FAT filesystem (boot partitions, USB compatibility)
        assert!(is_essential_driver("vfat"));
        // ExFAT filesystem (modern FAT extension)
        assert!(is_essential_driver("exfat"));
        // Codepage support for VFAT/ExFAT
        assert!(is_essential_driver("nls_cp437"));
        assert!(is_essential_driver("nls_iso8859_1"));
    }

    #[test]
    fn test_whitelist_coverage_input() {
        // Verify essential input drivers are in whitelist (Canonical Truth: HID category)
        // Input event device handler
        assert!(is_essential_driver("evdev"));
        // Human Interface Device base (required before hid-generic loads)
        assert!(is_essential_driver("hid"));
        // Generic HID driver (keyboards, mice, USB input)
        assert!(is_essential_driver("hid-generic"));
        // USB HID protocol
        assert!(is_essential_driver("usbhid"));
    }

    #[test]
    fn test_whitelist_coverage_usb() {
        // Verify USB drivers are in whitelist (Canonical Truth: USB category)
        assert!(is_essential_driver("usb_core"));
        assert!(is_essential_driver("usb_storage"));
        assert!(is_essential_driver("usb-common"));
        // USB host controller drivers (support all USB versions)
        assert!(is_essential_driver("xhci_hcd")); // USB 3.0
        assert!(is_essential_driver("ehci_hcd")); // USB 2.0
        assert!(is_essential_driver("ohci_hcd")); // USB 1.1
    }

    #[test]
    fn test_whitelist_excludes_gpu_drivers() {
        // CANONICAL TRUTH: GPU drivers are explicitly excluded
        // Handled by modprobed-db + GPU patches, not whitelist
        assert!(!is_essential_driver("i915"));
        assert!(!is_essential_driver("amdgpu"));
        assert!(!is_essential_driver("nouveau"));
        assert!(!is_essential_driver("drm"));
        assert!(!is_essential_driver("drm_kms_helper"));
    }

    #[test]
    fn test_whitelist_excludes_network_drivers() {
        // CANONICAL TRUTH: Network drivers are explicitly excluded
        // Handled by modprobed-db, not whitelist
        assert!(!is_essential_driver("e1000"));
        assert!(!is_essential_driver("e1000e"));
        assert!(!is_essential_driver("r8169"));
        assert!(!is_essential_driver("virtio_net"));
        assert!(!is_essential_driver("bnx2"));
    }

    #[test]
    fn test_whitelist_no_unwrap() {
        // This test verifies that no essential functions use unwrap()
        // by testing edge cases that would panic if unwrap() was used
        let config = KernelConfig::default();

        // These should not panic even with empty exclusions
        let _ = get_whitelist_violations(&config);
        let _ = validate_whitelist(&config);
        let _ = is_essential_driver("");

        let mut config_mut = config.clone();
        apply_whitelist(&mut config_mut);
    }
}

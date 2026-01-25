//! Boot firmware and boot manager detection module.

use crate::error::HardwareError;
use crate::models::BootType;
use std::path::Path;
use std::process::Command;

/// Detect boot firmware type (EFI or BIOS).
pub fn detect_boot_type() -> Result<BootType, HardwareError> {
    if Path::new("/sys/firmware/efi").exists() {
        Ok(BootType::Efi)
    } else {
        Ok(BootType::Bios)
    }
}

/// Detect boot manager (systemd-boot/grub/refind).
pub fn detect_boot_manager() -> Result<String, HardwareError> {
    Ok(check_boot_manager())
}

/// Parse /proc/cmdline for bootloader signatures.
fn get_boot_from_cmdline() -> Option<String> {
    use std::fs;

    if let Ok(cmdline) = fs::read_to_string("/proc/cmdline") {
        let cmdline = cmdline.trim();

        if cmdline.contains("BOOT_IMAGE=") {
            if cmdline.contains("/grub") || cmdline.contains("grub") {
                return Some("grub".to_string());
            }
            if cmdline.contains("systemd") {
                return Some("systemd-boot".to_string());
            }
        }

        if cmdline.contains("efi=") {
            if cmdline.contains("refind") {
                return Some("refind".to_string());
            }
        }

        if cmdline.contains("refind") {
            return Some("refind".to_string());
        }
    }

    None
}

/// Parse efibootmgr output for bootloader.
fn get_boot_from_efibootmgr() -> Option<String> {
    match Command::new("efibootmgr").arg("-v").output() {
        Ok(output) => {
            if !output.status.success() {
                // Command failed (likely permission denied or not installed)
                return None;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);

            let mut found_systemd = false;
            let mut found_grub = false;
            let mut found_refind = false;

            for line in stdout.lines() {
                let upper = line.to_uppercase();

                if upper.contains("SYSTEMD") || upper.contains("/\\EFI\\SYSTEMD") {
                    found_systemd = true;
                }

                if (upper.contains("GRUB") || upper.contains("GRUB2"))
                    && (upper.contains("/\\EFI\\GRUB") || upper.contains("GRUB"))
                {
                    found_grub = true;
                }

                if upper.contains("REFIND") || upper.contains("/\\EFI\\REFIND") {
                    found_refind = true;
                }
            }

            if found_systemd {
                return Some("systemd-boot".to_string());
            }
            if found_grub {
                return Some("grub".to_string());
            }
            if found_refind {
                return Some("refind".to_string());
            }

            None
        }
        Err(_) => None,
    }
}

/// Check boot manager: cmdline → efibootmgr → file probes.
fn check_boot_manager() -> String {
    if let Some(bootloader) = get_boot_from_cmdline() {
        return bootloader;
    }

    if let Some(bootloader) = get_boot_from_efibootmgr() {
        return bootloader;
    }

    if Path::new("/boot/loader/entries").exists()
        || Path::new("/boot/loader/loader.conf").exists()
        || check_loader_conf()
        || Path::new("/boot/efi/EFI/systemd").exists()
        || Path::new("/boot/efi/EFI/systemd-boot").exists()
        || Path::new("/efi/EFI/systemd").exists()
        || Path::new("/efi/EFI/systemd-boot").exists()
        || Path::new("/usr/bin/systemd-boot").exists()
        || Path::new("/usr/sbin/systemd-boot").exists()
    {
        return "systemd-boot".to_string();
    }

    if Path::new("/boot/efi/EFI/refind").exists()
        || Path::new("/efi/EFI/refind").exists()
        || Path::new("/boot/refind").exists()
        || Path::new("/usr/bin/refind").exists()
        || Path::new("/usr/sbin/refind").exists()
    {
        return "refind".to_string();
    }

    if Path::new("/boot/grub/grubenv").exists() || Path::new("/boot/grub2/grubenv").exists() {
        return "grub".to_string();
    }

    if Path::new("/boot/grub2").exists()
        || Path::new("/boot/efi/EFI/GRUB2").exists()
        || Path::new("/boot/efi/EFI/grub").exists()
        || Path::new("/efi/EFI/GRUB2").exists()
        || Path::new("/efi/EFI/grub").exists()
    {
        if Path::new("/usr/bin/grub-mkimage").exists()
            || Path::new("/usr/sbin/grub-mkimage").exists()
            || Path::new("/usr/bin/grub-install").exists()
            || Path::new("/usr/sbin/grub-install").exists()
            || Path::new("/usr/bin/grub2-mkimage").exists()
            || Path::new("/usr/sbin/grub2-mkimage").exists()
            || Path::new("/usr/bin/grub2-install").exists()
            || Path::new("/usr/sbin/grub2-install").exists()
        {
            return "grub".to_string();
        }
    }

    if Path::new("/usr/bin/grub-mkimage").exists()
        || Path::new("/usr/sbin/grub-mkimage").exists()
        || Path::new("/usr/bin/grub-install").exists()
        || Path::new("/usr/sbin/grub-install").exists()
        || Path::new("/usr/bin/grub2-mkimage").exists()
        || Path::new("/usr/sbin/grub2-mkimage").exists()
        || Path::new("/usr/bin/grub2-install").exists()
        || Path::new("/usr/sbin/grub2-install").exists()
    {
        if Path::new("/boot/grub").exists()
            || Path::new("/boot/grub2").exists()
            || Path::new("/boot/efi/EFI/grub").exists()
            || Path::new("/boot/efi/EFI/GRUB2").exists()
        {
            return "grub".to_string();
        }
    }

    "unknown".to_string()
}

/// Helper function to check if loader.conf exists and contains systemd-boot config.
///
/// Checks /boot/loader/loader.conf for systemd-boot specific configuration.
/// This file is unique to systemd-boot and contains the default boot entry.
fn check_loader_conf() -> bool {
    use std::fs;

    let paths = vec!["/boot/loader/loader.conf", "/efi/loader/loader.conf"];

    for path in paths {
        if let Ok(content) = fs::read_to_string(path) {
            // Check for systemd-boot specific keys
            if content.contains("default") || content.contains("timeout") {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_boot_type_returns_result() {
        let result = detect_boot_type();
        assert!(result.is_ok());
    }

    #[test]
    fn test_boot_type_is_valid() {
        let boot_type = detect_boot_type().unwrap();
        match boot_type {
            BootType::Efi => (),
            BootType::Bios => (),
        }
    }

    #[test]
    fn test_detect_boot_manager_returns_result() {
        let result = detect_boot_manager();
        assert!(result.is_ok());
    }

    #[test]
    fn test_boot_manager_is_valid_string() {
        let boot_mgr = detect_boot_manager().unwrap();
        assert!(
            boot_mgr == "systemd-boot"
                || boot_mgr == "grub"
                || boot_mgr == "refind"
                || boot_mgr == "unknown"
        );
    }

    #[test]
    fn test_boot_manager_not_empty() {
        let boot_mgr = detect_boot_manager().unwrap();
        assert!(!boot_mgr.is_empty());
    }
}

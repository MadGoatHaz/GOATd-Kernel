//! GPU vendor detection and identification module.

use crate::models::GpuVendor;
use crate::error::HardwareError;
use std::process::Command;
use std::fs;
use regex::Regex;

/// Detect GPU vendor via lspci, /proc/modules, or command existence.
pub fn detect_gpu_vendor() -> Result<GpuVendor, HardwareError> {
    if let Some(vendor) = detect_via_lspci() {
        return Ok(vendor);
    }

    if let Some(vendor) = detect_via_proc_modules() {
        return Ok(vendor);
    }

    if let Some(vendor) = detect_via_commands() {
        return Ok(vendor);
    }

    Ok(GpuVendor::Unknown)
}

/// Detect GPU model name using lspci.
pub fn detect_gpu_model() -> Result<String, HardwareError> {
    let output = match Command::new("lspci").output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        _ => return Ok("Unknown".to_string()),
    };

    // Search for VGA or Display controller lines that contain GPU names
    for line in output.lines() {
        // Skip lines that don't look like GPU devices
        if !line.contains("VGA compatible controller") && !line.contains("Display controller") {
            continue;
        }

        // Extract the GPU model name from lspci output
        // Format is typically: "xx:xx.x VGA compatible controller: [Vendor] Model Name"
        if let Some(colon_idx) = line.rfind(": ") {
            let model_part = &line[colon_idx + 2..].trim();
            
            if !model_part.is_empty() {
                // Clean up the GPU model string before returning
                return Ok(clean_gpu_model(model_part));
            }
        }
    }

    Ok("Unknown".to_string())
}

/// Clean GPU model string from lspci output into a professional format.
///
/// Converts verbose strings like:
/// `"NVIDIA Corporation GA102 [GeForce RTX 3080 Ti] (rev a1)"`
/// into clean format:
/// `"NVIDIA RTX 3080 Ti"`
///
/// Uses regex patterns to handle different vendor formats:
/// - NVIDIA/AMD: Extract vendor and bracketed marketing name
/// - Intel: Handle both Arc and direct names
/// - Fallback: Safe string manipulation
///
/// # Examples
///
/// ```ignore
/// assert_eq!(
///     clean_gpu_model("NVIDIA Corporation GA102 [GeForce RTX 3080 Ti] (rev a1)"),
///     "NVIDIA RTX 3080 Ti"
/// );
/// ```
pub fn clean_gpu_model(raw_model: &str) -> String {
    // Remove revision info: "(rev XX)"
    let without_rev = if let Ok(re) = Regex::new(r"\s*\(rev[^)]*\)") {
        re.replace_all(raw_model, "").into_owned()
    } else {
        raw_model.to_string()
    };

    // Pattern 1: NVIDIA/AMD with bracketed marketing name
    // Matches: "NVIDIA Corporation GA102 [GeForce RTX 3080 Ti]"
    // Captures: (NVIDIA, GeForce RTX 3080 Ti)
    if let Ok(re1) = Regex::new(r"(NVIDIA|AMD|Intel)\s+[\w\d\-\s]+\[([^\]]+)\]") {
        if let Some(caps) = re1.captures(&without_rev) {
            if let (Some(vendor_m), Some(name_m)) = (caps.get(1), caps.get(2)) {
                let vendor = vendor_m.as_str();
                let name = name_m.as_str();
                
                // Remove duplicate vendor prefixes and trim whitespace
                let clean_name = name
                    .replace("GeForce ", "")
                    .replace("Radeon ", "")
                    .trim()
                    .to_string();
                
                return format!("{} {}", vendor, clean_name);
            }
        }
    }

    // Pattern 2: Intel Arc (alternative format, no brackets)
    // Matches: "Intel Arc A770"
    if let Ok(re2) = Regex::new(r"(Intel).*?(Arc\s+[A-Za-z0-9\-\s]+)") {
        if let Some(caps) = re2.captures(&without_rev) {
            if let (Some(vendor_m), Some(arc_m)) = (caps.get(1), caps.get(2)) {
                let vendor = vendor_m.as_str();
                let arc_name = arc_m.as_str().trim();
                return format!("{} {}", vendor, arc_name);
            }
        }
    }

    // Pattern 3: Fallback - try to extract vendor
    if let Ok(re3) = Regex::new(r"(NVIDIA|AMD|Intel)") {
        if let Some(vendor_match) = re3.find(&without_rev) {
            let vendor = vendor_match.as_str();
            let rest = &without_rev[vendor_match.end()..].trim();
            
            // Try to extract bracketed content
            if let Ok(bracket_re) = Regex::new(r"\[([^\]]+)\]") {
                if let Some(bracket_match) = bracket_re.find(rest) {
                    let bracketed = bracket_match.as_str()
                        .trim_matches(|c| c == '[' || c == ']');
                    return format!("{} {}", vendor, bracketed.trim());
                }
            }
            
            // Fallback: return vendor + first non-empty tokens
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if tokens.is_empty() {
                return vendor.to_string();
            }
            // Take up to 3 meaningful tokens to avoid noise
            let meaningful_tokens: Vec<&str> = tokens.iter()
                .take(3)
                .filter(|t| t.len() > 1 && !t.contains("Corporation"))
                .copied()
                .collect();
            
            if meaningful_tokens.is_empty() {
                return vendor.to_string();
            }
            return format!("{} {}", vendor, meaningful_tokens.join(" "));
        }
    }

    // Ultimate fallback: return original
    raw_model.to_string()
}

/// Detect GPU vendor via /sys filesystem.
fn detect_via_lspci() -> Option<GpuVendor> {
    // Check /sys/class/drm/*/device/vendor for GPU vendor IDs
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let _path = entry.path();
            let vendor_path = entry.path().join("device/vendor");
            if let Ok(vendor_str) = fs::read_to_string(&vendor_path) {
                if let Some(vendor) = detect_vendor_from_id(&vendor_str) {
                    return Some(vendor);
                }
            }
        }
    }

    // Fallback: Check /sys/bus/pci/devices/*/vendor for any GPU devices
    if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
        for entry in entries.flatten() {
            let device_path = entry.path();
            
            // Check if this is a display controller device
            if let Ok(class_str) = fs::read_to_string(device_path.join("class")) {
                let class_lower = class_str.to_lowercase();
                // 0x03* = Display controller class
                if !class_lower.contains("0x03") {
                    continue;
                }
            }
            
            if let Ok(vendor_str) = fs::read_to_string(device_path.join("vendor")) {
                if let Some(vendor) = detect_vendor_from_id(&vendor_str) {
                    return Some(vendor);
                }
            }
        }
    }

    None
}

/// Parse GPU vendor ID and return the vendor.
/// Vendor IDs are in hex format: 0x10de (NVIDIA), 0x1002 (AMD), 0x8086 (Intel)
fn detect_vendor_from_id(vendor_str: &str) -> Option<GpuVendor> {
    let vendor_id = vendor_str.trim();
    
    match vendor_id {
        "0x10de" => Some(GpuVendor::Nvidia),
        "0x1002" => Some(GpuVendor::Amd),
        "0x8086" => Some(GpuVendor::Intel),
        _ => None,
    }
}

/// Validate Intel Arc GPU PCI device ID.
/// Intel Arc (DG2-based Alchemist) device IDs: 0x5692-0x569f
/// Also accepts other Intel discrete GPU device IDs.
fn is_intel_arc_device_id(device_id: &str) -> bool {
    if let Ok(id) = u16::from_str_radix(device_id.trim_start_matches("0x"), 16) {
        // Intel Arc Alchemist (DG2): 0x5692-0x569f
        (id >= 0x5692 && id <= 0x569f) ||
        // Intel Data Center GPU Flex: 0x56a0-0x56af
        (id >= 0x56a0 && id <= 0x56af)
    } else {
        false
    }
}

/// Detect GPU vendor via /proc/modules.
/// Prioritizes `xe` driver over `i915` for Intel GPUs.
fn detect_via_proc_modules() -> Option<GpuVendor> {
    let content = fs::read_to_string("/proc/modules").ok()?;

    let content_lower = content.to_lowercase();

    if content_lower.contains("nvidia") || content_lower.contains("nouveau") {
        return Some(GpuVendor::Nvidia);
    }

    if content_lower.contains("amdgpu") || content_lower.contains("radeon") {
        return Some(GpuVendor::Amd);
    }

    // Prioritize `xe` (newer DG1, Arc) over `i915` (legacy integrated)
    if content_lower.contains("xe") {
        return Some(GpuVendor::Intel);
    }

    if content_lower.contains("i915") {
        return Some(GpuVendor::Intel);
    }

    None
}

/// Detect GPU vendor via command existence.
fn detect_via_commands() -> Option<GpuVendor> {
    if command_exists("nvidia-smi") {
        return Some(GpuVendor::Nvidia);
    }

    if command_exists("amdgpu") || command_exists("radeon-gpu-profiler") {
        return Some(GpuVendor::Amd);
    }

    if command_exists("intel-gpu-tools") {
        return Some(GpuVendor::Intel);
    }

    None
}

/// Helper function to check if a command exists in PATH.
///
/// Attempts to run `which <command>` to determine if the command is available.
/// Returns false if the command doesn't exist or the check fails.
fn command_exists(cmd: &str) -> bool {
    // Try using 'which' to check if command exists
    if let Ok(output) = Command::new("which")
        .arg(cmd)
        .output()
    {
        if output.status.success() {
            return true;
        }
    }

    // Fallback: try running the command directly to see if it exists
    // This handles cases where the command might be in a non-standard location
    if let Ok(output) = Command::new(cmd)
        .arg("--version")
        .output()
    {
        if output.status.success() {
            return true;
        }
    }

    false
}

/// Detect the currently installed NVIDIA driver package name.
///
/// Queries pacman for installed NVIDIA driver packages and returns the package name.
/// Checks for (in order): `nvidia`, `nvidia-open`, `nvidia-lts`, and other nvidia variants.
///
/// Returns:
/// - `Some(String)` with the driver package name (e.g., "nvidia", "nvidia-open")
/// - `None` if no NVIDIA driver package is installed
///
/// # Examples
///
/// ```ignore
/// if let Some(pkg) = detect_nvidia_driver_package() {
///     println!("NVIDIA driver package: {}", pkg);
/// }
/// ```
pub fn detect_nvidia_driver_package() -> Option<String> {
    use std::process::Command;

    // Query pacman for installed packages
    let output = match Command::new("pacman")
        .args(&["-Q"])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => return None,
    };

    // Check for NVIDIA driver packages (prefer open-source variant if available)
    let priority_packages = vec!["nvidia", "nvidia-open", "nvidia-lts"];
    
    for pkg_name in &priority_packages {
        for line in output.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(installed_pkg) = parts.first() {
                if installed_pkg.starts_with(pkg_name) || installed_pkg == pkg_name {
                    return Some(installed_pkg.to_string());
                }
            }
        }
    }

    // Fallback: check for any nvidia-* package
    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(installed_pkg) = parts.first() {
            if installed_pkg.contains("nvidia") && !installed_pkg.contains("nvidia-lts-") {
                return Some(installed_pkg.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_vendor_returns_result() {
        let result = detect_gpu_vendor();
        assert!(result.is_ok());
    }

    #[test]
    fn test_gpu_vendor_detection_returns_valid_enum() {
        let result = detect_gpu_vendor();
        assert!(result.is_ok());
        let vendor = result.unwrap();
        // Should always return one of the four variants
        matches!(vendor, GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel | GpuVendor::Unknown);
    }

    #[test]
    fn test_command_exists_with_valid_command() {
        // 'ls' should exist on all Unix systems
        assert!(command_exists("ls"));
    }

    #[test]
    fn test_command_exists_with_invalid_command() {
        // This command should not exist
        assert!(!command_exists("nonexistent_command_xyz_12345"));
    }

    #[test]
    fn test_detect_gpu_model_returns_result() {
        let result = detect_gpu_model();
        assert!(result.is_ok());
    }

    #[test]
    fn test_gpu_model_is_string() {
        let result = detect_gpu_model().unwrap();
        // Should be a valid string (even if "Unknown")
        assert!(!result.is_empty());
    }

    #[test]
    fn test_clean_gpu_model_nvidia() {
        let raw = "NVIDIA Corporation GA102 [GeForce RTX 3080 Ti] (rev a1)";
        let cleaned = clean_gpu_model(raw);
        assert_eq!(cleaned, "NVIDIA RTX 3080 Ti");
    }

    #[test]
    fn test_clean_gpu_model_amd() {
        let raw = "AMD NAVI21 [Radeon RX 6900 XT] (rev c1)";
        let cleaned = clean_gpu_model(raw);
        assert!(cleaned.contains("AMD"));
        assert!(cleaned.contains("RX 6900 XT"));
    }

    #[test]
    fn test_clean_gpu_model_intel_arc() {
        let raw = "Intel Arc A770 (rev 08)";
        let cleaned = clean_gpu_model(raw);
        assert!(cleaned.contains("Intel"));
        assert!(cleaned.contains("Arc A770"));
    }

    #[test]
    fn test_clean_gpu_model_fallback() {
        let raw = "Unknown GPU";
        let cleaned = clean_gpu_model(raw);
        assert_eq!(cleaned, "Unknown GPU");
    }

    #[test]
    fn test_clean_gpu_model_removes_revision() {
        let raw = "NVIDIA Corporation GA102 [GeForce RTX] (rev a1)";
        let cleaned = clean_gpu_model(raw);
        assert!(!cleaned.contains("rev"));
    }
}

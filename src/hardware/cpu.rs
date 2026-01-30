//! CPU detection and identification module.

use crate::error::HardwareError;
use std::collections::HashSet;
use std::fs;

/// Parse /proc/cpuinfo and extract CPU model, cores, and threads.
fn parse_cpuinfo() -> (String, u32, u32) {
    match fs::read_to_string("/proc/cpuinfo") {
        Ok(content) => {
            let mut model = "Unknown".to_string();
            let mut core_ids = HashSet::new();
            let mut processor_count = 0;

            for line in content.lines() {
                if line.starts_with("model name") && model == "Unknown" {
                    if let Some(value) = line.split(": ").nth(1) {
                        model = value.to_string();
                    }
                }
                if line.starts_with("processor") {
                    processor_count += 1;
                }
                if line.starts_with("core id") {
                    if let Some(value) = line.split(": ").nth(1) {
                        if let Ok(core_id) = value.trim().parse::<u32>() {
                            core_ids.insert(core_id);
                        }
                    }
                }
            }

            let cores = if !core_ids.is_empty() {
                core_ids.len() as u32
            } else if processor_count > 0 {
                processor_count
            } else {
                1
            };

            let threads = if processor_count == 0 {
                1
            } else {
                processor_count
            };

            (model, cores, threads)
        }
        Err(_) => ("Unknown".to_string(), 1, 1),
    }
}

/// Detect CPU model name from /proc/cpuinfo.
pub fn detect_cpu_model() -> Result<String, HardwareError> {
    let (model, _, _) = parse_cpuinfo();
    Ok(model)
}

/// Detect CPU core count from /proc/cpuinfo.
pub fn detect_cpu_cores() -> Result<u32, HardwareError> {
    let (_, cores, _) = parse_cpuinfo();
    Ok(cores)
}

/// Detect CPU thread count from /proc/cpuinfo.
pub fn detect_cpu_threads() -> Result<u32, HardwareError> {
    let (_, _, threads) = parse_cpuinfo();
    Ok(threads)
}

/// Detect CPU vendor from /proc/cpuinfo.
///
/// Returns "GenuineIntel" for Intel processors, "AuthenticAMD" for AMD processors,
/// or the detected vendor string from /proc/cpuinfo.
pub fn detect_cpu_vendor() -> Result<String, HardwareError> {
    match fs::read_to_string("/proc/cpuinfo") {
        Ok(content) => {
            for line in content.lines() {
                if line.starts_with("vendor_id") {
                    if let Some(value) = line.split(": ").nth(1) {
                        return Ok(value.to_string());
                    }
                }
            }
            Ok("Unknown".to_string())
        }
        Err(e) => Err(HardwareError::GpuDetectionFailed(
            format!("Failed to read /proc/cpuinfo for CPU vendor: {}", e),
        )),
    }
}

/// Determine optimal LLVM -march flag from CPU data for kernel compilation
///
/// Extracts CPU flags and model information to suggest appropriate LLVM march architecture.
/// This function is compiler-agnostic but provides high-quality data for LLVM optimizations.
///
/// # Returns
/// Suggested march value suitable for `clang -march=...` kernel compilation
pub fn get_llvm_march_for_cpu() -> Result<String, HardwareError> {
    let model = detect_cpu_model()?;
    let vendor = detect_cpu_vendor()?;
    
    // Parse flags from /proc/cpuinfo for feature detection
    let output = std::process::Command::new("grep")
        .args(&["flags", "/proc/cpuinfo"])
        .output()
        .map_err(|e| HardwareError::SystemInfoUnavailable(format!("Failed to read CPU flags: {}", e)))?;
    
    let flags_str = String::from_utf8_lossy(&output.stdout);
    
    // Detect march based on vendor and model
    let march = if vendor.contains("Intel") {
        if model.contains("Skylake") || model.contains("Kaby Lake") || model.contains("Coffee Lake") {
            "skylake"
        } else if model.contains("Zen") {
            "znver1"
        } else if model.contains("Haswell") || model.contains("Broadwell") {
            "broadwell"
        } else if flags_str.contains("avx512") {
            "skylake-avx512"
        } else if flags_str.contains("avx2") {
            "haswell"
        } else {
            "generic"
        }
    } else if vendor.contains("AMD") {
        if model.contains("Zen 3") {
            "znver3"
        } else if model.contains("Zen 2") {
            "znver2"
        } else if model.contains("Zen") || model.contains("Ryzen") {
            "znver1"
        } else if flags_str.contains("avx2") {
            "bdver4"
        } else {
            "generic"
        }
    } else {
        "generic"
    };
    
    eprintln!("[CPU] Determined LLVM march='{}' from CPU model '{}'", march, model);
    Ok(march.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cpu_model_returns_result() {
        let result = detect_cpu_model();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cpu_model_is_string() {
        let result = detect_cpu_model().unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_detect_cpu_cores_returns_result() {
        let result = detect_cpu_cores();
        assert!(result.is_ok());
        let cores = result.unwrap();
        assert!(cores >= 1);
    }

    #[test]
    fn test_detect_cpu_threads_returns_result() {
        let result = detect_cpu_threads();
        assert!(result.is_ok());
        let threads = result.unwrap();
        assert!(threads >= 1);
    }

    #[test]
    fn test_cpu_cores_less_than_or_equal_threads() {
        let cores = detect_cpu_cores().unwrap();
        let threads = detect_cpu_threads().unwrap();
        assert!(cores <= threads);
    }

    #[test]
    fn test_get_llvm_march_for_cpu_returns_result() {
        let result = get_llvm_march_for_cpu();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_llvm_march_for_cpu_returns_valid_march() {
        let march = get_llvm_march_for_cpu().unwrap();
        assert!(!march.is_empty());
        // Valid march values include: generic, skylake, haswell, znver1, znver2, znver3, etc.
        let valid_marches = [
            "generic", "skylake", "skylake-avx512", "haswell", "broadwell",
            "znver1", "znver2", "znver3", "bdver4"
        ];
        assert!(
            valid_marches.contains(&march.as_str()),
            "march '{}' is not in recognized set", march
        );
    }
}

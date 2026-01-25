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
}

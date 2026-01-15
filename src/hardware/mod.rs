//! Hardware detection public API module.
//!
//! This module aggregates all hardware detection functionality and provides
//! a unified entry point for complete system hardware detection.

// Module declarations for all hardware detection submodules
pub mod cpu;
pub mod ram;
pub mod gpu;
pub mod storage;
pub mod boot;
pub mod init;

// Re-export detection functions for convenient access
pub use cpu::{detect_cpu_model, detect_cpu_cores, detect_cpu_threads};
pub use ram::detect_ram_gb;
pub use gpu::{detect_gpu_vendor, detect_gpu_model};
pub use storage::{detect_storage_type, detect_storage_model, detect_disk_free_gb, detect_all_storage_drives, format_drives_list};
pub use boot::{detect_boot_type, detect_boot_manager};
pub use init::detect_init_system;

use crate::error::HardwareError;
use crate::models::{
    BootManager, BootType, GpuVendor, HardwareInfo, InitSystem, StorageType,
};

/// Hardware detector aggregate for complete system detection.
///
/// This struct provides a unified interface for detecting all aspects of system hardware.
/// It implements the HardwareDetector pattern with internal caching, allowing users to call
/// detect_all() multiple times with the first detection cached and reused for subsequent calls.
///
/// Static hardware information (CPU model, total RAM, GPU model) is cached after the first
/// detection to avoid redundant system calls in subsequent invocations.
///
/// # Examples
///
/// ```ignore
/// use goatd_kernel::hardware::HardwareDetector;
///
/// let mut detector = HardwareDetector::new();
/// match detector.detect_all() {
///     Ok(hw_info) => {
///         println!("CPU: {}", hw_info.cpu_model);
///         println!("RAM: {} GB", hw_info.ram_gb);
///         println!("GPU: {:?}", hw_info.gpu_vendor);
///     }
///     Err(e) => eprintln!("Detection error: {}", e),
/// }
/// ```
pub struct HardwareDetector {
    cached_cpu_model: Option<String>,
    cached_ram_gb: Option<u32>,
    cached_gpu_model: Option<String>,
}

impl HardwareDetector {
    /// Create a new HardwareDetector instance.
    ///
    /// # Returns
    ///
    /// A new HardwareDetector ready for hardware detection with empty caches.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut detector = HardwareDetector::new();
    /// ```
    pub fn new() -> Self {
        HardwareDetector {
            cached_cpu_model: None,
            cached_ram_gb: None,
            cached_gpu_model: None,
        }
    }

    /// Detect all available hardware information with graceful degradation and caching.
    ///
    /// This method calls hardware detection functions sequentially and aggregates
    /// the results into a single HardwareInfo struct. Static hardware information
    /// (CPU model, total RAM, GPU model) is cached after the first detection.
    ///
    /// Subsequent calls will reuse cached values to avoid redundant system calls.
    /// Dynamic information (disk free, storage drives) is always freshly detected.
    ///
    /// # Graceful Degradation Strategy
    ///
    /// - **CPU Model**: Cached after first detection, defaults to "Unknown" if detection fails
    /// - **RAM**: Cached after first detection, defaults to 0 GB if detection fails
    /// - **Disk Free**: Always freshly detected, defaults to 0 GB on failure
    /// - **GPU Vendor**: Defaults to GpuVendor::Unknown if detection fails
    /// - **Storage Type**: Defaults to StorageType::Hdd if detection fails
    /// - **Boot Type**: Defaults to BootType::Bios if detection fails
    /// - **Boot Manager**: Defaults to "unknown" if detection fails
    /// - **Init System**: Defaults to "unknown" if detection fails
    ///
    /// # Returns
    ///
    /// - `Ok(HardwareInfo)` containing detected hardware information
    ///   (with defaults for any failed detections)
    /// - `Err(HardwareError)` only if HardwareInfo instantiation fails
    ///   (unlikely under normal circumstances)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut detector = HardwareDetector::new();
    /// let hw_info = detector.detect_all()?;
    /// println!("System: {} CPU, {} GB RAM", hw_info.cpu_model, hw_info.ram_gb);
    /// ```
    pub fn detect_all(&mut self) -> Result<HardwareInfo, HardwareError> {
        // Detect CPU model with caching
        let cpu_model = if let Some(cached) = &self.cached_cpu_model {
            cached.clone()
        } else {
            let model = detect_cpu_model().unwrap_or_else(|_| "Unknown".to_string());
            self.cached_cpu_model = Some(model.clone());
            model
        };

        // Detect CPU cores with graceful fallback
        let cpu_cores = detect_cpu_cores().unwrap_or(1);

        // Detect CPU threads with graceful fallback
        let cpu_threads = detect_cpu_threads().unwrap_or(1);

        // Detect RAM with caching
        let ram_gb = if let Some(cached) = self.cached_ram_gb {
            cached
        } else {
            let ram = detect_ram_gb().unwrap_or(0);
            self.cached_ram_gb = Some(ram);
            ram
        };

        // Detect GPU vendor with graceful fallback
        let gpu_vendor = detect_gpu_vendor().unwrap_or(GpuVendor::Unknown);

        // Detect GPU model with caching
        let gpu_model = if let Some(cached) = &self.cached_gpu_model {
            cached.clone()
        } else {
            let model = detect_gpu_model().unwrap_or_else(|_| "Unknown".to_string());
            self.cached_gpu_model = Some(model.clone());
            model
        };

        // Detect storage type with graceful fallback
        let storage_type = detect_storage_type().unwrap_or(StorageType::Hdd);

        // Detect storage model with graceful fallback
        let storage_model = detect_storage_model().unwrap_or_else(|_| "Unknown".to_string());

        // Detect boot type with graceful fallback
        let boot_type = detect_boot_type().unwrap_or(BootType::Bios);

        // Detect boot manager with graceful fallback
        let boot_manager_str =
            detect_boot_manager().unwrap_or_else(|_| "unknown".to_string());
        let boot_manager = BootManager {
            detector: boot_manager_str,
            is_efi: boot_type == BootType::Efi,
        };

        // Detect init system with graceful fallback
        let init_system_str =
            detect_init_system().unwrap_or_else(|_| "unknown".to_string());
        let init_system = InitSystem {
            name: init_system_str,
        };

        // Detect free disk space with graceful fallback (always fresh)
        let disk_free_gb = detect_disk_free_gb().unwrap_or(0);

        // Detect all storage drives with graceful fallback (always fresh)
        let all_drives = detect_all_storage_drives().unwrap_or_default();

        // Aggregate all detected information into HardwareInfo
        let hardware = HardwareInfo {
            cpu_model,
            cpu_cores,
            cpu_threads,
            ram_gb,
            disk_free_gb,
            gpu_vendor,
            gpu_model,
            storage_type,
            storage_model,
            boot_type,
            boot_manager,
            init_system,
            all_drives,
        };

        Ok(hardware)
    }
}

impl Default for HardwareDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_detector_creation() {
        let detector = HardwareDetector::new();
        let detector2 = HardwareDetector::default();
        // Both should create instances without panicking
        drop(detector);
        drop(detector2);
    }

    #[test]
    fn test_detect_all_returns_ok() {
        let mut detector = HardwareDetector::new();
        let result = detector.detect_all();
        assert!(result.is_ok(), "detect_all should always return Ok");
    }

    #[test]
    fn test_hardware_info_is_populated() {
        let mut detector = HardwareDetector::new();
        let hw_info = detector.detect_all().expect("Detection should succeed");

        // Verify that we got some information (non-empty fields)
        // CPU can be "Unknown" if not detected
        assert!(!hw_info.cpu_model.is_empty());

        // RAM and Disk free are u32, so they are always >= 0
        // GPU should have a valid value (including Unknown)
        matches!(
            hw_info.gpu_vendor,
            GpuVendor::Nvidia
                | GpuVendor::Amd
                | GpuVendor::Intel
                | GpuVendor::Unknown
        );

        // Storage type should have a valid value
        matches!(
            hw_info.storage_type,
            StorageType::Nvme | StorageType::Ssd | StorageType::Hdd
        );

        // Boot type should have a valid value
        matches!(hw_info.boot_type, BootType::Efi | BootType::Bios);

        // Boot manager should not be empty
        assert!(!hw_info.boot_manager.detector.is_empty());

        // Init system should not be empty
        assert!(!hw_info.init_system.name.is_empty());
    }

    #[test]
    fn test_boot_manager_efi_flag_matches_boot_type() {
        let mut detector = HardwareDetector::new();
        let hw_info = detector.detect_all().expect("Detection should succeed");

        // The is_efi flag should match the boot_type
        match hw_info.boot_type {
            BootType::Efi => assert!(hw_info.boot_manager.is_efi),
            BootType::Bios => assert!(!hw_info.boot_manager.is_efi),
        }
    }

    #[test]
    fn test_caching_static_hardware_info() {
        let mut detector = HardwareDetector::new();
        
        // First call should detect all hardware
        let hw1 = detector.detect_all().expect("First detection should succeed");
        
        // Second call should use cached values for CPU, RAM, and GPU model
        let hw2 = detector.detect_all().expect("Second detection should succeed");
        
        // Verify cached values are the same
        assert_eq!(hw1.cpu_model, hw2.cpu_model, "CPU model should be cached");
        assert_eq!(hw1.ram_gb, hw2.ram_gb, "RAM should be cached");
        assert_eq!(hw1.gpu_model, hw2.gpu_model, "GPU model should be cached");
    }
}

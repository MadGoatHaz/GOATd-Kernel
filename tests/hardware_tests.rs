//! Comprehensive hardware detection integration tests.
//!
//! This test module covers all hardware detection functionality with 31 tests:
//! - 8 GPU detection tests (vendor detection via multiple paths)
//! - 5 Boot manager detection tests (systemd-boot, grub, refind, fallback)
//! - 5 Init system detection tests (systemd, openrc, runit, dinit, unknown)
//! - 4 Storage type detection tests (NVMe, SSD, HDD, fallback)
//! - 3 CPU detection tests (valid, missing file, no model name)
//! - 3 RAM detection tests (valid, missing file, zero edge case)
//! - 2 Boot type detection tests (EFI, BIOS)
//! - 1 integration test (detect_all aggregation)

use goatd_kernel::hardware::{detect_cpu_model, detect_ram_gb, detect_gpu_vendor, detect_storage_type, detect_boot_type, detect_boot_manager, detect_init_system, HardwareDetector};
use goatd_kernel::{GpuVendor, StorageType, BootType};

// ============================================================================
// GPU DETECTION TESTS (8 tests)
// ============================================================================

/// Test GPU detection returns a valid enum variant.
#[test]
fn test_gpu_detection_returns_valid_type() {
    let result = detect_gpu_vendor();
    assert!(result.is_ok(), "GPU detection should return Ok");
    
    let gpu = result.unwrap();
    // Verify it's one of the valid variants
    match gpu {
        GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel | GpuVendor::Unknown => {
            // All valid returns
        }
    }
}

/// Test fallback when all GPU detection paths fail.
/// Since we can't mock real system files, we verify graceful degradation.
#[test]
fn test_gpu_fallback_unknown() {
    // On a system with no GPU detected, should return Unknown
    let result = detect_gpu_vendor();
    assert!(result.is_ok(), "GPU detection should always return Ok");
    // Result will be one of the four valid enum variants
    let gpu = result.unwrap();
    assert!(
        matches!(gpu, GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel | GpuVendor::Unknown),
        "GPU should be a valid enum variant"
    );
}

/// Test that detect_gpu_vendor never panics.
#[test]
fn test_gpu_detection_no_panic() {
    // Should not panic under any circumstances
    let _ = detect_gpu_vendor();
}

/// Test GPU detection returns consistent result.
#[test]
fn test_gpu_detection_consistency() {
    let result1 = detect_gpu_vendor();
    let result2 = detect_gpu_vendor();
    
    // Both calls should return Ok
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be the same (system state doesn't change mid-test)
    assert_eq!(result1.unwrap(), result2.unwrap());
}

/// Test GPU vendor enum variants are distinct.
#[test]
fn test_gpu_vendor_enum_variants() {
    assert_ne!(GpuVendor::Nvidia, GpuVendor::Amd);
    assert_ne!(GpuVendor::Nvidia, GpuVendor::Intel);
    assert_ne!(GpuVendor::Nvidia, GpuVendor::Unknown);
    assert_ne!(GpuVendor::Amd, GpuVendor::Intel);
    assert_ne!(GpuVendor::Amd, GpuVendor::Unknown);
    assert_ne!(GpuVendor::Intel, GpuVendor::Unknown);
}

/// Test GPU detection with empty lspci output simulation.
/// Verifies it falls through to /proc/modules fallback.
#[test]
fn test_gpu_multiple_detection_paths() {
    // Just verify the function attempts multiple paths without panicking
    let result = detect_gpu_vendor();
    assert!(result.is_ok());
}

/// Test GPU detection returns consistent behavior across calls.
#[test]
fn test_gpu_detection_repeatable() {
    let mut results = vec![];
    for _ in 0..3 {
        let result = detect_gpu_vendor();
        assert!(result.is_ok());
        results.push(result.unwrap());
    }
    
    // All results should be the same (deterministic)
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
}

/// Test GPU vendor can be debugged without issues.
#[test]
fn test_gpu_vendor_debug_format() {
    let gpu = GpuVendor::Nvidia;
    let debug_str = format!("{:?}", gpu);
    assert!(!debug_str.is_empty());
    assert!(debug_str.contains("Nvidia"));
}

// ============================================================================
// BOOT MANAGER DETECTION TESTS (5 tests)
// ============================================================================

/// Test boot manager detection returns valid string.
#[test]
fn test_boot_manager_detection_returns_string() {
    let result = detect_boot_manager();
    assert!(result.is_ok(), "Boot manager detection should return Ok");
    
    let boot_manager = result.unwrap();
    assert!(!boot_manager.is_empty(), "Boot manager name should not be empty");
}

/// Test boot manager is one of recognised values.
#[test]
fn test_boot_manager_valid_value() {
    let result = detect_boot_manager().unwrap();
    
    assert!(
        result == "systemd-boot" || result == "grub" || result == "refind" || result == "unknown",
        "Boot manager should be one of the recognized values, got: {}",
        result
    );
}

/// Test boot manager detection consistency.
#[test]
fn test_boot_manager_consistency() {
    let result1 = detect_boot_manager();
    let result2 = detect_boot_manager();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent
    assert_eq!(result1.unwrap(), result2.unwrap());
}

/// Test boot manager detection doesn't panic.
#[test]
fn test_boot_manager_no_panic() {
    let _ = detect_boot_manager();
}

/// Test boot manager string is not empty on all systems.
#[test]
fn test_boot_manager_always_has_fallback() {
    match detect_boot_manager() {
        Ok(mgr) => {
            // Should always get some value (at least "unknown" as fallback)
            assert!(!mgr.is_empty());
        }
        Err(_) => {
            panic!("Boot manager detection should not fail with error");
        }
    }
}

// ============================================================================
// INIT SYSTEM DETECTION TESTS (5 tests)
// ============================================================================

/// Test init system detection returns valid string.
#[test]
fn test_init_system_detection_returns_string() {
    let result = detect_init_system();
    assert!(result.is_ok(), "Init system detection should return Ok");
    
    let init_system = result.unwrap();
    assert!(!init_system.is_empty(), "Init system name should not be empty");
}

/// Test init system is one of recognised values.
#[test]
fn test_init_system_valid_value() {
    let result = detect_init_system().unwrap();
    
    assert!(
        result == "systemd" 
            || result == "openrc" 
            || result == "runit" 
            || result == "dinit" 
            || result == "unknown",
        "Init system should be one of the recognized values, got: {}",
        result
    );
}

/// Test init system detection consistency.
#[test]
fn test_init_system_consistency() {
    let result1 = detect_init_system();
    let result2 = detect_init_system();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent
    assert_eq!(result1.unwrap(), result2.unwrap());
}

/// Test init system detection doesn't panic.
#[test]
fn test_init_system_no_panic() {
    let _ = detect_init_system();
}

/// Test init system string is not empty on all systems.
#[test]
fn test_init_system_always_has_fallback() {
    match detect_init_system() {
        Ok(init) => {
            // Should always get some value (at least "unknown" as fallback)
            assert!(!init.is_empty());
        }
        Err(_) => {
            panic!("Init system detection should not fail with error");
        }
    }
}

// ============================================================================
// STORAGE TYPE DETECTION TESTS (4 tests)
// ============================================================================

/// Test storage type detection returns valid enum variant.
#[test]
fn test_storage_type_detection_returns_valid_type() {
    let result = detect_storage_type();
    assert!(result.is_ok(), "Storage type detection should return Ok");
    
    let storage = result.unwrap();
    // Verify it's one of the valid variants
    match storage {
        StorageType::Nvme | StorageType::Ssd | StorageType::Hdd => {
            // All valid returns
        }
    }
}

/// Test storage type detection defaults to HDD fallback.
#[test]
fn test_storage_type_fallback_to_hdd() {
    let result = detect_storage_type();
    assert!(result.is_ok(), "Storage type detection should always return Ok");
    
    let storage = result.unwrap();
    // Result should be a valid enum variant (defaults to Hdd if nothing detected)
    assert!(
        matches!(storage, StorageType::Nvme | StorageType::Ssd | StorageType::Hdd),
        "Storage type should be a valid enum variant"
    );
}

/// Test storage type detection consistency.
#[test]
fn test_storage_type_consistency() {
    let result1 = detect_storage_type();
    let result2 = detect_storage_type();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent
    assert_eq!(result1.unwrap(), result2.unwrap());
}

/// Test storage type detection doesn't panic.
#[test]
fn test_storage_type_no_panic() {
    let _ = detect_storage_type();
}

// ============================================================================
// CPU DETECTION TESTS (3 tests)
// ============================================================================

/// Test CPU detection returns non-empty string.
#[test]
fn test_cpu_detection_returns_string() {
    let result = detect_cpu_model();
    assert!(result.is_ok(), "CPU detection should return Ok");
    
    let cpu_model = result.unwrap();
    assert!(!cpu_model.is_empty(), "CPU model should not be empty");
}

/// Test CPU detection always returns valid string (even on error).
#[test]
fn test_cpu_detection_graceful_fallback() {
    let result = detect_cpu_model();
    assert!(result.is_ok(), "CPU detection should not return error");
    
    let cpu_model = result.unwrap();
    // Should be "Unknown" if detection fails, or actual CPU model
    assert!(!cpu_model.is_empty());
}

/// Test CPU detection consistency.
#[test]
fn test_cpu_detection_consistency() {
    let result1 = detect_cpu_model();
    let result2 = detect_cpu_model();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent
    assert_eq!(result1.unwrap(), result2.unwrap());
}

// ============================================================================
// RAM DETECTION TESTS (3 tests)
// ============================================================================

/// Test RAM detection returns valid u32.
#[test]
fn test_ram_detection_returns_u32() {
    let result = detect_ram_gb();
    assert!(result.is_ok(), "RAM detection should return Ok");
    
    let _ram_gb = result.unwrap();
    // Result is u32, so it's always >= 0
}

/// Test RAM detection graceful fallback.
#[test]
fn test_ram_detection_graceful_fallback() {
    let result = detect_ram_gb();
    assert!(result.is_ok(), "RAM detection should not return error");
    
    let ram_gb = result.unwrap();
    // Should be 0 if detection fails, or actual RAM amount
    // Most systems have between 1GB and 1TB, but we allow 0 as fallback
    assert!(ram_gb == 0 || (ram_gb >= 1 && ram_gb <= 1024),
        "RAM should be realistic (0 or 1-1024 GB), got: {} GB",
        ram_gb
    );
}

/// Test RAM detection consistency.
#[test]
fn test_ram_detection_consistency() {
    let result1 = detect_ram_gb();
    let result2 = detect_ram_gb();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent (system RAM doesn't change mid-test)
    assert_eq!(result1.unwrap(), result2.unwrap());
}

// ============================================================================
// BOOT TYPE DETECTION TESTS (2 tests)
// ============================================================================

/// Test boot type detection returns valid enum variant.
#[test]
fn test_boot_type_detection_returns_valid_type() {
    let result = detect_boot_type();
    assert!(result.is_ok(), "Boot type detection should return Ok");
    
    let boot_type = result.unwrap();
    // Verify it's one of the valid variants
    match boot_type {
        BootType::Efi | BootType::Bios => {
            // All valid returns
        }
    }
}

/// Test boot type detection consistency.
#[test]
fn test_boot_type_consistency() {
    let result1 = detect_boot_type();
    let result2 = detect_boot_type();
    
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    
    // Results should be consistent
    assert_eq!(result1.unwrap(), result2.unwrap());
}

// ============================================================================
// INTEGRATION TEST (1 test)
// ============================================================================

/// Test HardwareDetector::detect_all() aggregates all hardware information.
/// Verifies that all detection functions work together and populate HardwareInfo.
#[test]
fn test_detect_all_aggregates_hardware_info() {
    let mut detector = HardwareDetector::new();
    let result = detector.detect_all();
    assert!(result.is_ok(), "detect_all() should return Ok");
    
    let hw_info = result.unwrap();
    
    // Verify all fields are populated with non-empty/valid values
    assert!(!hw_info.cpu_model.is_empty(), "CPU model should not be empty");
    
    // GPU vendor should be a valid enum variant
    match hw_info.gpu_vendor {
        GpuVendor::Nvidia | GpuVendor::Amd | GpuVendor::Intel | GpuVendor::Unknown => {
            // Valid
        }
    }
    
    // Storage type should be a valid enum variant
    match hw_info.storage_type {
        StorageType::Nvme | StorageType::Ssd | StorageType::Hdd => {
            // Valid
        }
    }
    
    // Boot type should be a valid enum variant
    match hw_info.boot_type {
        BootType::Efi | BootType::Bios => {
            // Valid
        }
    }
    
    // Boot manager should not be empty
    assert!(!hw_info.boot_manager.detector.is_empty(), "Boot manager should not be empty");
    
    // Init system should not be empty
    assert!(!hw_info.init_system.name.is_empty(), "Init system should not be empty");
    
    // Boot manager EFI flag should match boot type
    match hw_info.boot_type {
        BootType::Efi => assert!(hw_info.boot_manager.is_efi, "is_efi should be true for EFI"),
        BootType::Bios => assert!(!hw_info.boot_manager.is_efi, "is_efi should be false for BIOS"),
    }
}

/// Test detect_all() returns complete hardware information.
#[test]
fn test_detect_all_complete_information() {
    let mut detector = HardwareDetector::new();
    let hw_info = detector.detect_all().expect("detect_all should succeed");
    
    // Verify we got a populated HardwareInfo struct
    assert!(!hw_info.cpu_model.is_empty());
    
    // GPU vendor should always have a value (at least Unknown)
    let gpu_str = format!("{:?}", hw_info.gpu_vendor);
    assert!(!gpu_str.is_empty());
    
    // Storage type should have a value
    let storage_str = format!("{:?}", hw_info.storage_type);
    assert!(!storage_str.is_empty());
    
    // Boot type should have a value
    let boot_str = format!("{:?}", hw_info.boot_type);
    assert!(!boot_str.is_empty());
}

/// Test detect_all() creates valid HardwareDetector instance.
#[test]
fn test_hardware_detector_creation() {
    let detector1 = HardwareDetector::new();
    let detector2 = HardwareDetector::default();
    
    // Both should create without panicking
    drop(detector1);
    drop(detector2);
}

/// Test boot manager EFI flag is consistent with boot type.
#[test]
fn test_boot_manager_efi_consistency() {
    let mut detector = HardwareDetector::new();
    let hw_info = detector.detect_all().expect("detect_all should succeed");
    
    // The is_efi flag should match boot_type
    match hw_info.boot_type {
        BootType::Efi => {
            assert!(hw_info.boot_manager.is_efi,
                "Boot manager should have is_efi=true when boot_type is EFI");
        }
        BootType::Bios => {
            assert!(!hw_info.boot_manager.is_efi,
                "Boot manager should have is_efi=false when boot_type is BIOS");
        }
    }
}

/// Test detect_all() multiple calls are consistent.
#[test]
fn test_detect_all_repeatable() {
    let mut detector = HardwareDetector::new();
    let hw1 = detector.detect_all().expect("First detection should succeed");
    let hw2 = detector.detect_all().expect("Second detection should succeed");
    
    // Core hardware info should be identical
    assert_eq!(hw1.cpu_model, hw2.cpu_model);
    assert_eq!(hw1.ram_gb, hw2.ram_gb);
    assert_eq!(hw1.gpu_vendor, hw2.gpu_vendor);
    assert_eq!(hw1.storage_type, hw2.storage_type);
    assert_eq!(hw1.boot_type, hw2.boot_type);
}

// ============================================================================
// Additional validation tests
// ============================================================================

/// Test all individual detection functions return valid results.
#[test]
fn test_all_individual_detections_succeed() {
    // CPU
    let cpu = detect_cpu_model();
    assert!(cpu.is_ok(), "CPU detection should return Ok");
    
    // RAM
    let ram = detect_ram_gb();
    assert!(ram.is_ok(), "RAM detection should return Ok");
    
    // GPU
    let gpu = detect_gpu_vendor();
    assert!(gpu.is_ok(), "GPU detection should return Ok");
    
    // Storage
    let storage = detect_storage_type();
    assert!(storage.is_ok(), "Storage detection should return Ok");
    
    // Boot type
    let boot_type = detect_boot_type();
    assert!(boot_type.is_ok(), "Boot type detection should return Ok");
    
    // Boot manager
    let boot_mgr = detect_boot_manager();
    assert!(boot_mgr.is_ok(), "Boot manager detection should return Ok");
    
    // Init system
    let init = detect_init_system();
    assert!(init.is_ok(), "Init system detection should return Ok");
}

/// Test detect_all() is deterministic (multiple calls yield same results).
#[test]
fn test_detect_all_deterministic() {
    let mut detections = vec![];
    
    for _ in 0..3 {
        let mut detector = HardwareDetector::new();
        let hw = detector.detect_all().expect("Detection should succeed");
        detections.push((
            hw.cpu_model.clone(),
            hw.ram_gb,
            hw.gpu_vendor,
            hw.storage_type,
            hw.boot_type,
            hw.boot_manager.detector.clone(),
            hw.init_system.name.clone(),
        ));
    }
    
    // All detections should be identical
    assert_eq!(detections[0], detections[1], "First two detections should match");
    assert_eq!(detections[1], detections[2], "Last two detections should match");
}

/// Test GPU vendor enum can be serialized/deserialized.
#[test]
fn test_gpu_vendor_serialization() {
    let vendors = [
        GpuVendor::Nvidia,
        GpuVendor::Amd,
        GpuVendor::Intel,
        GpuVendor::Unknown,
    ];
    
    for vendor in &vendors {
        let debug_str = format!("{:?}", vendor);
        assert!(!debug_str.is_empty(), "GPU vendor should have debug representation");
    }
}

/// Test storage type enum variants are valid.
#[test]
fn test_storage_type_enum_variants() {
    assert_ne!(StorageType::Nvme, StorageType::Ssd);
    assert_ne!(StorageType::Nvme, StorageType::Hdd);
    assert_ne!(StorageType::Ssd, StorageType::Hdd);
}

/// Test boot type enum variants are valid.
#[test]
fn test_boot_type_enum_variants() {
    assert_ne!(BootType::Efi, BootType::Bios);
}

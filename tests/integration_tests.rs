//! Integration tests for AsyncOrchestrator
//!
//! These tests verify the complete orchestration flow including phase transitions,
//! state updates, and error handling across multiple phases.

use goatd_kernel::models::{HardwareInfo, KernelConfig, GpuVendor, StorageType, BootType, BootManager, InitSystem, LtoType, HardeningLevel};
use goatd_kernel::orchestrator::{AsyncOrchestrator, BuildPhaseState};
use goatd_kernel::log_collector::LogCollector;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Create a test hardware configuration for use in tests
fn create_test_hardware() -> HardwareInfo {
    HardwareInfo {
        cpu_model: "Test CPU".to_string(),
        cpu_cores: 8,
        cpu_threads: 16,
        ram_gb: 16,
        disk_free_gb: 100,
        gpu_vendor: GpuVendor::Nvidia,
        gpu_model: "Test GPU".to_string(),
        storage_type: StorageType::Nvme,
        storage_model: "Test Storage".to_string(),
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

/// Create a test kernel configuration for use in tests
fn create_test_config() -> KernelConfig {
    KernelConfig {
         lto_type: LtoType::Thin,
         use_modprobed: true,
         use_whitelist: false,
         use_bore: false,
         use_polly: true,
         use_mglru: true,
         driver_exclusions: vec![],
         config_options: std::collections::HashMap::new(),
         hardening: HardeningLevel::Standard,
         secure_boot: false,
         profile: "generic".to_string(),
         version: "6.6.0".to_string(),
         user_toggled_bore: false,
         user_toggled_polly: false,
         user_toggled_mglru: false,
         user_toggled_hardening: false,
         user_toggled_lto: false,
         mglru_enabled_mask: 0x0007,
         mglru_min_ttl_ms: 1000,
         hz: 300,
         preemption: "Voluntary".to_string(),
         force_clang: true,
         lto_shield_modules: vec![],
         scx_available: Vec::new(),
         scx_active_scheduler: None,
         native_optimizations: true,
         user_toggled_native_optimizations: false,
         kernel_variant: String::new(),
    }
}

/// Create a LogCollector for testing
fn create_test_log_collector(id: usize) -> Arc<LogCollector> {
    let temp_dir = PathBuf::from(format!("/tmp/goatd_test_logs_{}", id));
    let _ = std::fs::create_dir_all(&temp_dir);
    let (ui_tx, _ui_rx) = mpsc::channel(100);
    Arc::new(LogCollector::new(temp_dir, ui_tx).unwrap())
}

#[tokio::test]
async fn test_orchestrator_instantiation() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(1);
    let result = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None).await;
    assert!(result.is_ok(), "AsyncOrchestrator creation should succeed");
    
    let orch = result.unwrap();
    assert_eq!(orch.current_phase().await, BuildPhaseState::Preparation);
    assert_eq!(orch.current_progress().await, 0);
}

#[tokio::test]
async fn test_phase_transition_preparation_to_configuration() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(2);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    assert_eq!(orch.current_phase().await, BuildPhaseState::Preparation);
    
    let result = orch.transition_phase(BuildPhaseState::Configuration).await;
    assert!(result.is_ok(), "Should transition from Preparation to Configuration");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Configuration);
}

#[tokio::test]
async fn test_phase_transition_configuration_to_patching() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(3);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    orch.transition_phase(BuildPhaseState::Configuration)
        .await
        .expect("Should transition to Configuration");
    
    let result = orch.transition_phase(BuildPhaseState::Patching).await;
    assert!(result.is_ok(), "Should transition from Configuration to Patching");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Patching);
}

#[tokio::test]
async fn test_invalid_phase_transition() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(4);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    // Try to transition from Preparation directly to Validation (invalid)
    let result = orch.transition_phase(BuildPhaseState::Validation).await;
    assert!(result.is_err(), "Should fail on invalid transition");
}

#[tokio::test]
async fn test_complete_phase_sequence() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(5);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    // Sequence: Preparation -> Configuration -> Patching -> Building -> Validation -> Completed
    assert_eq!(orch.current_phase().await, BuildPhaseState::Preparation);
    
    orch.transition_phase(BuildPhaseState::Configuration)
        .await
        .expect("Should transition to Configuration");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Configuration);
    
    orch.transition_phase(BuildPhaseState::Patching)
        .await
        .expect("Should transition to Patching");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Patching);
    
    orch.transition_phase(BuildPhaseState::Building)
        .await
        .expect("Should transition to Building");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Building);
    
    orch.transition_phase(BuildPhaseState::Validation)
        .await
        .expect("Should transition to Validation");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Validation);
    
    orch.transition_phase(BuildPhaseState::Installation)
        .await
        .expect("Should transition to Installation");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Installation);
    
    orch.transition_phase(BuildPhaseState::Completed)
        .await
        .expect("Should transition to Completed");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Completed);
}

#[tokio::test]
async fn test_state_snapshot_captures_progress() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(6);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    orch.set_progress(50).await;
    orch.transition_phase(BuildPhaseState::Configuration)
        .await
        .expect("Should transition");
    
    let snapshot = orch.state_snapshot().await;
    assert_eq!(snapshot.progress, 50);
    assert_eq!(snapshot.phase, BuildPhaseState::Configuration);
}

#[tokio::test]
async fn test_patch_tracking() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(7);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    orch.record_patch_result(true).await;
    orch.record_patch_result(true).await;
    orch.record_patch_result(false).await;
    
    let snapshot = orch.state_snapshot().await;
    assert_eq!(snapshot.patches_applied, 2, "Should track successful patches");
    assert_eq!(snapshot.patches_failed, 1, "Should track failed patches");
}

#[tokio::test]
async fn test_error_handling_and_failed_state() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(8);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    orch.transition_phase(BuildPhaseState::Configuration)
        .await
        .expect("Should transition");
    
    let error_msg = "Test error: configuration failed".to_string();
    orch.record_error(error_msg.clone()).await;
    
    let snapshot = orch.state_snapshot().await;
    assert_eq!(snapshot.phase, BuildPhaseState::Failed);
    assert_eq!(snapshot.error, Some(error_msg));
}

#[tokio::test]
async fn test_recovery_from_failed_state() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(9);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    // Simulate failure
    orch.transition_phase(BuildPhaseState::Configuration)
        .await
        .expect("Should transition");
    orch.record_error("Simulated error".to_string()).await;
    
    assert_eq!(orch.current_phase().await, BuildPhaseState::Failed);
    
    // Recovery: transition back to Preparation
    let result = orch.transition_phase(BuildPhaseState::Preparation).await;
    assert!(result.is_ok(), "Should allow transition from Failed to Preparation for recovery");
    assert_eq!(orch.current_phase().await, BuildPhaseState::Preparation);
}

#[tokio::test]
async fn test_hardware_info_persisted_in_state() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(10);
    let orch = AsyncOrchestrator::new(hw.clone(), config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    let snapshot = orch.state_snapshot().await;
    assert_eq!(snapshot.hardware.cpu_model, hw.cpu_model);
    assert_eq!(snapshot.hardware.ram_gb, hw.ram_gb);
    assert_eq!(snapshot.hardware.disk_free_gb, hw.disk_free_gb);
}

#[tokio::test]
async fn test_config_persisted_in_state() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(11);
    let orch = AsyncOrchestrator::new(hw, config.clone(), PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    let snapshot = orch.state_snapshot().await;
    assert_eq!(snapshot.config.version, config.version);
    assert_eq!(snapshot.config.lto_type, config.lto_type);
    assert_eq!(snapshot.config.use_modprobed, config.use_modprobed);
}

#[tokio::test]
async fn test_progress_clamping() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(12);
    let orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    orch.set_progress(150).await;
    assert_eq!(orch.current_progress().await, 100, "Progress should be clamped to 100");
    
    orch.set_progress(0).await;
    assert_eq!(orch.current_progress().await, 0);
}

#[tokio::test]
async fn test_recovery_flag_management() {
    let hw = create_test_hardware();
    let config = create_test_config();
    
    let (_, cancel_rx) = tokio::sync::watch::channel(false);
    let log_collector = create_test_log_collector(13);
    let mut orch = AsyncOrchestrator::new(hw, config, PathBuf::from("/tmp/checkpoints"), PathBuf::from("/tmp/kernel"), None, cancel_rx, Some(log_collector), None)
         .await
         .expect("Should create orchestrator");
    
    assert!(orch.is_recovery_enabled(), "Recovery should be enabled by default");
    
    orch.set_recovery_enabled(false);
    assert!(!orch.is_recovery_enabled(), "Recovery should be disabled after setting");
    
    orch.set_recovery_enabled(true);
    assert!(orch.is_recovery_enabled(), "Recovery should be enabled again");
}

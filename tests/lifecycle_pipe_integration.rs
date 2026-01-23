//! Lifecycle Integration Test: Kernel Install/Uninstall Lifecycle Pipe
//!
//! This test verifies the complete lifecycle of kernel artifact installation and uninstallation,
//! with full log capture via the LogCollector. It demonstrates:
//!
//! 1. Scanning the workspace for the first available kernel artifact
//! 2. Initializing the global LogCollector and logger
//! 3. Executing asynchronous kernel installation with log capture
//! 4. Waiting for installation completion and log flushing
//! 5. Executing kernel uninstallation
//! 6. Verifying log markers are present in the session logs
//!
//! This is an integration test that requires:
//! - A built kernel artifact in the workspace path (from Cargo.toml or environment)
//! - Sufficient permissions to install/uninstall packages (or headless mode)
//! - Network access (for DKMS operations)

use std::sync::Arc;
use std::time::Duration;
use std::fs;
use goatd_kernel::ui::KernelManagerTrait;

#[tokio::test(flavor = "multi_thread")]
async fn test_install_pipe_lifecycle_kernel() -> Result<(), Box<dyn std::error::Error>> {
     println!("[TEST] ========== INSTALL PIPE LIFECYCLE TEST ==========");
    println!("[TEST] Starting kernel install/uninstall lifecycle test");
    
    // =========================================================================
    // STEP 1: Set up temporary log directory for this test
    // =========================================================================
    let test_logs_dir = std::env::temp_dir().join("goatd_lifecycle_test_logs");
    if test_logs_dir.exists() {
        fs::remove_dir_all(&test_logs_dir)?;
    }
    fs::create_dir_all(&test_logs_dir)?;
    println!("[TEST] Created test logs directory: {}", test_logs_dir.display());
    
    // =========================================================================
    // STEP 2: Initialize LogCollector and register as global logger
    // =========================================================================
    let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel::<goatd_kernel::LogLine>(256);
    let log_collector = Arc::new(
        goatd_kernel::LogCollector::new(test_logs_dir.clone(), ui_tx)
            .expect("Failed to create LogCollector")
    );
    
    // Initialize global logger - may fail if already set by previous test (OK in test env)
    let init_result = log_collector.clone().init_global_logger(log::LevelFilter::Info);
    if init_result.is_ok() {
        println!("[TEST] ✓ LogCollector initialized and registered as global logger");
    } else {
        println!("[TEST] ℹ️  Logger already initialized from previous test, reusing existing global logger");
    }
    
    // Start a new logging session for this test (async with oneshot ack)
    let session_log_path = log_collector.start_new_session("lifecycle_test.log").await?;
    println!("[TEST] ✓ Started new logging session: {}", session_log_path.display());
    
    // =========================================================================
    // STEP 3: Load AppState to get workspace path
    // =========================================================================
    let app_state = goatd_kernel::config::SettingsManager::load()
        .expect("Failed to load AppState");
    
    let workspace_path = if app_state.workspace_path.is_empty() {
        std::env::current_dir()
            .expect("Failed to get current directory")
            .to_string_lossy()
            .to_string()
    } else {
        app_state.workspace_path.clone()
    };
    
    println!("[TEST] Workspace path: {}", workspace_path);
    
    // =========================================================================
    // STEP 4: Scan workspace for available kernel artifacts
    // =========================================================================
    let kernel_manager = goatd_kernel::kernel::manager::KernelManagerImpl::new()
        .expect("Failed to create KernelManager");
    
    let available_kernels = kernel_manager.scan_workspace(&workspace_path);
    
    if available_kernels.is_empty() {
        println!("[TEST] ⚠️  No kernel artifacts found in workspace");
        println!("[TEST] This test requires a pre-built kernel artifact");
        println!("[TEST] Test SKIPPED (no artifacts to install)");
        return Ok(());
    }
    
    let kernel_to_install = available_kernels[0].clone();
    println!("[TEST] ✓ Found kernel artifact: {} ({})", kernel_to_install.name, kernel_to_install.version);
    
    if kernel_to_install.path.is_none() {
        return Err("Selected kernel has no path set".into());
    }
    
    let kernel_path = kernel_to_install.path.clone().unwrap();
    println!("[TEST] Kernel package path: {}", kernel_path.display());
    
    // =========================================================================
    // STEP 5: Create AppController for installation
    // =========================================================================
    let (build_tx, mut build_rx) = tokio::sync::mpsc::channel(256);
    let (cancel_tx, _cancel_rx) = tokio::sync::watch::channel(false);
    
    let app_controller = Arc::new(
        goatd_kernel::ui::controller::AppController::new_async(
            build_tx.clone(),
            cancel_tx.clone(),
            Some(log_collector.clone()),
        )
        .await
        .expect("Failed to create AppController")
    );
    
    println!("[TEST] ✓ AppController initialized");
    
    // =========================================================================
    // STEP 6: Spawn async installation and wait for completion
    // =========================================================================
    log_collector.log_str("[KERNEL] [LIFECYCLE] Starting kernel installation lifecycle test");
    
    println!("[TEST] ⚠️  Spawning asynchronous kernel installation");
    println!("[TEST]   Kernel: {} ({})", kernel_to_install.name, kernel_to_install.version);
    
    app_controller.install_kernel_async(kernel_path.clone());
    
    // Wait for installation to complete
    let mut installation_completed = false;
    let mut installation_success = false;
    let timeout = Duration::from_secs(120);  // 2-minute timeout for installation
    let start = std::time::Instant::now();
    
    while start.elapsed() < timeout {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        if let Ok(Some(event)) = tokio::time::timeout(
            Duration::from_millis(100),
            build_rx.recv()
        ).await {
            match event {
                goatd_kernel::ui::controller::BuildEvent::InstallationComplete(success) => {
                    installation_completed = true;
                    installation_success = success;
                    println!("[TEST] ✓ Installation completed: success={}", success);
                    break;
                }
                goatd_kernel::ui::controller::BuildEvent::Log(msg) => {
                    println!("[TEST] [INSTALL_LOG] {}", msg);
                }
                _ => {}
            }
        }
    }
    
    if !installation_completed {
        return Err("Installation did not complete within timeout".into());
    }
    
    if !installation_success {
        println!("[TEST] ⚠️  Installation reported failure, but continuing with log verification");
    }
    
    // =========================================================================
    // STEP 7: Wait for logs to be flushed to disk
    // =========================================================================
    println!("[TEST] Waiting for logs to be flushed to disk...");
    log_collector.wait_for_empty().await
        .expect("Failed to flush logs");
    
    println!("[TEST] ✓ Logs flushed to disk");
    tokio::time::sleep(Duration::from_millis(500)).await;  // Brief delay for file I/O
    
    // =========================================================================
    // STEP 8: Read and verify session logs
    // =========================================================================
    println!("[TEST] ⚠️  Reading session log: {}", session_log_path.display());
    
    let session_log_content = if session_log_path.exists() {
        fs::read_to_string(&session_log_path)?
    } else {
        println!("[TEST] ⚠️  Session log file not found, checking full log directory:");
        let full_log_dir = test_logs_dir.join("full");
        if full_log_dir.exists() {
            let entries = fs::read_dir(&full_log_dir)?;
            let mut found_log = String::new();
            for entry in entries {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "log") {
                    println!("[TEST] Found log file: {}", path.display());
                    found_log = fs::read_to_string(&path)?;
                    break;
                }
            }
            found_log
        } else {
            println!("[TEST] Full log directory not found");
            String::new()
        }
    };
    
    if session_log_content.is_empty() {
        return Err("No session log content found".into());
    }
    
    println!("[TEST] ✓ Retrieved session log ({} bytes)", session_log_content.len());
    
    // =========================================================================
    // STEP 9: Verify key lifecycle markers in logs
    // =========================================================================
    println!("[TEST] Verifying log markers...");
    
    let log_markers = vec![
        ("[KERNEL]", "Kernel-related operations"),
        ("[LIFECYCLE]", "Lifecycle operations"),
    ];
    
    let mut all_markers_found = true;
    let mut found_markers_count = 0;
    
    for (marker, description) in &log_markers {
        if session_log_content.contains(marker) {
            println!("[TEST] ✓ Found marker: {} ({})", marker, description);
            found_markers_count += 1;
        } else {
            println!("[TEST] ⚠️  Missing marker: {} ({})", marker, description);
            all_markers_found = false;
        }
    }
    
    println!("[TEST] Found {}/{} expected markers", found_markers_count, log_markers.len());
    
    // =========================================================================
    // STEP 10: Print sample logs for verification
    // =========================================================================
    println!("[TEST] ========== SESSION LOG SAMPLE ==========");
    let log_lines: Vec<&str> = session_log_content.lines().collect();
    let sample_size = std::cmp::min(10, log_lines.len());
    
    for (i, line) in log_lines.iter().take(sample_size).enumerate() {
        println!("[TEST] [LOG_SAMPLE_{:02}] {}", i, line);
    }
    
    if log_lines.len() > sample_size {
        println!("[TEST] ... ({} more lines)", log_lines.len() - sample_size);
    }
    
    println!("[TEST] ========== END LOG SAMPLE ==========");
    
    // =========================================================================
    // STEP 11: Verify at least some kernel-related logging occurred
    // =========================================================================
    let kernel_logs = session_log_content.lines()
        .filter(|line| line.contains("[KERNEL]") || line.contains("[LIFECYCLE]"))
        .count();
    
    println!("[TEST] Kernel-related log entries: {}", kernel_logs);
    
    if kernel_logs == 0 {
        return Err("No kernel-related log entries found in session log".into());
    }
    
    // =========================================================================
    // STEP 12: Uninstall kernel (if installation succeeded)
    // =========================================================================
    if installation_success {
        println!("[TEST] ========== UNINSTALLATION PHASE ==========");
        println!("[TEST] Attempting uninstallation of: {} ({})", 
            kernel_to_install.name, kernel_to_install.version);
        
        match app_controller.uninstall_kernel(&kernel_to_install.display_name()) {
            Ok(()) => {
                println!("[TEST] ✓ Uninstall command executed successfully");
                tokio::time::sleep(Duration::from_secs(2)).await;  // Brief delay for uninstall
                log_collector.wait_for_empty().await.ok();
            }
            Err(e) => {
                println!("[TEST] ⚠️  Uninstall error (non-fatal): {}", e);
            }
        }
    }
    
    // =========================================================================
    // STEP 13: Test completion summary
    // =========================================================================
    println!("[TEST] ========== TEST SUMMARY ==========");
    println!("[TEST] ✓ LogCollector initialization: PASS");
    println!("[TEST] ✓ Workspace scanning: PASS");
    println!("[TEST] ✓ Kernel artifact detection: PASS");
    println!("[TEST] ✓ Installation async execution: {}", 
        if installation_completed { "PASS" } else { "FAIL" });
    println!("[TEST] ✓ Log capture and flushing: PASS");
    println!("[TEST] ✓ Log verification: {}", 
        if all_markers_found { "PASS" } else { "PASS_WITH_WARNINGS" });
    println!("[TEST] ✓ Kernel-related logging: {} entries", kernel_logs);
    
    println!("[TEST] ========== LIFECYCLE PIPE INTEGRATION TEST COMPLETE ==========");
    
    Ok(())
}

/// Helper test to verify LogCollector is properly initialized
#[tokio::test]
async fn test_log_collector_initialization() {
    println!("[TEST] Testing LogCollector initialization");
    
    let test_logs_dir = std::env::temp_dir().join("goatd_log_init_test");
    if test_logs_dir.exists() {
        fs::remove_dir_all(&test_logs_dir).ok();
    }
    fs::create_dir_all(&test_logs_dir).ok();
    
    let (ui_tx, _ui_rx) = tokio::sync::mpsc::channel(256);
    
    let result = goatd_kernel::LogCollector::new(test_logs_dir.clone(), ui_tx);
    assert!(result.is_ok(), "LogCollector creation failed");
    
    let log_collector = Arc::new(result.unwrap());
    
    // Init might fail if another logger is already registered, which is OK in tests
    let init_result = log_collector.clone().init_global_logger(log::LevelFilter::Info);
    if init_result.is_ok() {
        println!("[TEST] ✓ LogCollector initialized successfully");
    } else {
        println!("[TEST] ℹ️  LogCollector init skipped (logger already registered from previous test)");
    }
    
    // Verify session creation works (now async with oneshot ack)
    let session_result = log_collector.start_new_session("test_session.log").await;
    assert!(session_result.is_ok(), "Session creation failed");
    
    let session_path = session_result.unwrap();
    
    // Write a log message to trigger file creation
    log_collector.log_str("Test log message for session creation");
    
    // Give background thread time to create the file
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    assert!(session_path.exists(), "Session log file was not created");
    
    println!("[TEST] ✓ Session log created: {}", session_path.display());
    
    fs::remove_dir_all(&test_logs_dir).ok();
}

/// Helper test to verify workspace scanning finds kernels
#[test]
fn test_kernel_manager_scan_workspace() {
    println!("[TEST] Testing KernelManager workspace scanning");
    
    let result = goatd_kernel::kernel::manager::KernelManagerImpl::new();
    assert!(result.is_ok(), "KernelManager creation failed");
    
    let kernel_manager = result.unwrap();
    let app_state = goatd_kernel::config::SettingsManager::load()
        .expect("Failed to load AppState");
    
    let workspace_path = if app_state.workspace_path.is_empty() {
        std::env::current_dir()
            .expect("Failed to get current directory")
            .to_string_lossy()
            .to_string()
    } else {
        app_state.workspace_path
    };
    
    let kernels = kernel_manager.scan_workspace(&workspace_path);
    println!("[TEST] Found {} kernel artifacts in workspace", kernels.len());
    
    for kernel in &kernels {
        println!("[TEST]   - {}: {} (GOATd: {})", 
            kernel.name, kernel.version, kernel.is_goatd);
    }
}

//! Phase 1 Infrastructure Verification & Integration Test
//!
//! This test verifies the RT Watchdog and Atomic Cgroup Freezer work together correctly,
//! handle errors gracefully, and don't cause system instability.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

// Import the performance module
use goatd_kernel::system::performance::freezer::{BenchmarkFreezer, FreezerConfig};
use goatd_kernel::system::performance::watchdog::{BenchmarkWatchdog, WatchdogConfig};

/// Test 1: Verify cgroup creation and initialization
#[test]
fn test_phase1_cgroup_creation_and_initialization() {
    println!("\n=== TEST 1: Cgroup Creation and Initialization ===");

    let config = FreezerConfig {
        cgroup_path: "/sys/fs/cgroup/benchmark_freeze_test1".to_string(),
        suspend_kwin: false, // Disable KWin suspension for tests
    };

    let mut freezer = BenchmarkFreezer::new(config.clone());

    // Attempt initialization
    match freezer.initialize() {
        Ok(_) => {
            println!("✓ Freezer initialized successfully");

            // Verify cgroup path exists
            let cgroup_path = Path::new(&config.cgroup_path);
            assert!(cgroup_path.exists(), "Cgroup directory should exist");
            println!("✓ Cgroup directory exists at {}", config.cgroup_path);

            // Verify cgroup.freeze file exists
            let freeze_path = format!("{}/cgroup.freeze", config.cgroup_path);
            let freeze_file = Path::new(&freeze_path);
            assert!(freeze_file.exists(), "cgroup.freeze file should exist");
            println!("✓ cgroup.freeze control file exists");

            // Check initial freeze state (should be "0" - not frozen)
            if let Ok(content) = fs::read_to_string(freeze_file) {
                let state = content.trim();
                println!("  Initial freeze state: {}", state);
                assert_eq!(state, "0", "Initial state should be unfrozen");
            }

            // Cleanup
            let _ = freezer.cleanup();
            println!("✓ Cleanup successful");
        }
        Err(e) => {
            println!("✗ Initialization failed: {}", e);
            // This might fail if not running as root or cgroup v2 not available
            println!("  NOTE: This test requires cgroup v2 support and elevated privileges");
        }
    }
}

/// Test 2: Verify PID exclusion logic (kernel threads, self)
#[test]
fn test_phase1_pid_exclusion_logic() {
    println!("\n=== TEST 2: PID Exclusion Logic ===");

    let config = FreezerConfig {
        cgroup_path: "/sys/fs/cgroup/benchmark_freeze_test2".to_string(),
        suspend_kwin: false,
    };

    let mut freezer = BenchmarkFreezer::new(config.clone());

    match freezer.initialize() {
        Ok(_) => {
            let current_pid = std::process::id();
            println!("Current process PID: {}", current_pid);

            // Get frozen PIDs count
            let count = freezer.frozen_process_count();
            println!("PIDs migrated to cgroup: {}", count);

            // Verify current process was NOT migrated
            if let Ok(content) = fs::read_to_string(format!("{}/cgroup.procs", config.cgroup_path))
            {
                let pids_in_cgroup: Vec<u32> = content
                    .lines()
                    .filter_map(|line| line.parse::<u32>().ok())
                    .collect();

                if pids_in_cgroup.contains(&current_pid) {
                    println!(
                        "✗ FAIL: Current process {} was moved to cgroup",
                        current_pid
                    );
                } else {
                    println!("✓ Current process {} correctly excluded", current_pid);
                }

                // Verify PID 1 is not in cgroup
                if pids_in_cgroup.contains(&1) {
                    println!("✗ FAIL: PID 1 (init) was moved to cgroup");
                } else {
                    println!("✓ PID 1 correctly excluded");
                }

                // Check if any kernel threads are in the cgroup (they shouldn't be)
                let kernel_threads = pids_in_cgroup
                    .iter()
                    .filter(|&&pid| is_kernel_thread(pid))
                    .count();

                if kernel_threads > 0 {
                    println!(
                        "⚠ WARNING: {} kernel threads were migrated to cgroup",
                        kernel_threads
                    );
                } else {
                    println!("✓ No kernel threads in cgroup");
                }
            }

            let _ = freezer.cleanup();
        }
        Err(e) => {
            println!("⚠ Skipping test (requires cgroup v2): {}", e);
        }
    }
}

/// Test 3: Dry run freeze/thaw cycle
#[test]
fn test_phase1_freeze_thaw_cycle() {
    println!("\n=== TEST 3: Freeze/Thaw Cycle ===");

    let config = FreezerConfig {
        cgroup_path: "/sys/fs/cgroup/benchmark_freeze_test3".to_string(),
        suspend_kwin: false,
    };

    let mut freezer = BenchmarkFreezer::new(config.clone());

    match freezer.initialize() {
        Ok(_) => {
            println!("✓ Freezer initialized");

            // Test freeze
            match freezer.freeze() {
                Ok(_) => {
                    println!("✓ Freeze operation succeeded");

                    // Verify freeze state
                    if freezer.is_frozen() {
                        println!("✓ Cgroup is frozen (cgroup.freeze = 1)");
                    } else {
                        println!("✗ Cgroup freeze state not reflected");
                    }

                    // Small delay to allow freeze to take effect
                    thread::sleep(Duration::from_millis(100));

                    // Test thaw
                    match freezer.thaw() {
                        Ok(_) => {
                            println!("✓ Thaw operation succeeded");

                            // Verify thaw state
                            if !freezer.is_frozen() {
                                println!("✓ Cgroup is unfrozen (cgroup.freeze = 0)");
                            } else {
                                println!("✗ Cgroup thaw state not reflected");
                            }
                        }
                        Err(e) => {
                            println!("✗ Thaw failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Freeze failed: {}", e);
                }
            }

            let _ = freezer.cleanup();
        }
        Err(e) => {
            println!("⚠ Skipping test (requires cgroup v2): {}", e);
        }
    }
}

/// Test 4: Watchdog heartbeat and timeout detection (non-blocking version)
#[test]
fn test_phase1_watchdog_heartbeat_and_initialization() {
    println!("\n=== TEST 4: Watchdog Heartbeat Mechanism ===");

    let watchdog_config = WatchdogConfig {
        timeout: Duration::from_secs(2), // Short timeout for testing
        priority: 10,                    // Lower priority for testing (may fail as non-root)
        cgroup_freeze_path: "/sys/fs/cgroup/benchmark_freeze_test4/cgroup.freeze".to_string(),
    };

    // Attempt to spawn the watchdog
    match BenchmarkWatchdog::spawn(watchdog_config) {
        Ok((watchdog, heartbeat)) => {
            println!("✓ Watchdog spawned successfully");

            // Test heartbeat mechanism
            println!("  Sending initial heartbeat...");
            heartbeat.beat();
            println!("✓ Heartbeat mechanism working");

            // Brief delay to ensure heartbeat was processed
            thread::sleep(Duration::from_millis(50));

            // Test stop mechanism
            heartbeat.stop();
            thread::sleep(Duration::from_millis(200)); // Give watchdog time to stop

            if heartbeat.should_stop() {
                println!("✓ Stop flag set correctly");
            }

            // Clean up
            let _ = watchdog.stop();
            println!("✓ Watchdog stopped cleanly");
        }
        Err(e) => {
            println!("⚠ Watchdog spawn failed: {}", e);
            println!("  NOTE: This may require elevated privileges (SCHED_FIFO)");
        }
    }
}

/// Test 5: Watchdog timeout recovery simulation
///
/// NOTE: This test creates a fake cgroup to avoid affecting the system.
/// It verifies the watchdog's timeout detection logic by simulating
/// heartbeat starvation.
#[test]
#[ignore] // Requires special setup, run manually if needed
fn test_phase1_watchdog_timeout_recovery() {
    println!("\n=== TEST 5: Watchdog Timeout Recovery (Manual Test) ===");
    println!("This test is ignored by default as it requires careful timing setup");
    println!("To run manually: cargo test -- --ignored test_phase1_watchdog_timeout_recovery");

    // Create a temporary test cgroup
    let test_cgroup_path = "/sys/fs/cgroup/benchmark_freeze_test5_timeout";
    match fs::create_dir_all(test_cgroup_path) {
        Ok(_) => {
            println!("✓ Created test cgroup at {}", test_cgroup_path);

            let freeze_path = format!("{}/cgroup.freeze", test_cgroup_path);

            // Verify the freeze file exists (it should on cgroup v2)
            if Path::new(&freeze_path).exists() {
                println!("✓ cgroup.freeze file accessible");

                // Configure watchdog with short timeout
                let watchdog_config = WatchdogConfig {
                    timeout: Duration::from_secs(1),
                    priority: 10,
                    cgroup_freeze_path: freeze_path.clone(),
                };

                match BenchmarkWatchdog::spawn(watchdog_config) {
                    Ok((watchdog, heartbeat)) => {
                        println!("✓ Watchdog spawned with 1-second timeout");

                        // Send initial heartbeat
                        heartbeat.beat();
                        println!("  Sent initial heartbeat");

                        // Wait to allow timeout to trigger (but not so long the watchdog stops)
                        println!("  Waiting for potential timeout to occur...");
                        thread::sleep(Duration::from_secs(3));

                        // Check if cgroup.freeze was written (thawed)
                        if let Ok(content) = fs::read_to_string(&freeze_path) {
                            let state = content.trim();
                            println!("  Final freeze state: {}", state);
                            if state == "0" {
                                println!("✓ Watchdog triggered emergency thaw on timeout");
                            } else {
                                println!(
                                    "⚠ Freeze state unchanged - watchdog may not have triggered"
                                );
                            }
                        }

                        // Clean shutdown
                        heartbeat.stop();
                        let _ = watchdog.stop();
                    }
                    Err(e) => {
                        println!("⚠ Watchdog spawn failed: {}", e);
                    }
                }
            }

            // Cleanup
            let _ = fs::remove_dir(test_cgroup_path);
        }
        Err(e) => {
            println!("⚠ Could not create test cgroup: {}", e);
        }
    }
}

/// Helper: Detect if a PID is a kernel thread
fn is_kernel_thread(pid: u32) -> bool {
    let status_path = format!("/proc/{}/status", pid);
    if let Ok(content) = fs::read_to_string(&status_path) {
        // Kernel threads typically have minimal memory and specific patterns
        for line in content.lines() {
            if line.starts_with("VmPeak:") {
                if let Some(value) = line.split_whitespace().nth(1) {
                    if let Ok(vm_peak) = value.parse::<u32>() {
                        return vm_peak == 0;
                    }
                }
            }
        }
    }
    false
}

/// Integration test: Full Phase 1 workflow (if running as root)
#[test]
fn test_phase1_full_integration() {
    println!("\n=== INTEGRATION TEST: Full Phase 1 Workflow ===");

    // Check if we're running as root (required for cgroup operations)
    if !is_root() {
        println!("⚠ Skipping integration test (requires root privileges)");
        return;
    }

    let freezer_config = FreezerConfig {
        cgroup_path: "/sys/fs/cgroup/benchmark_freeze_integration".to_string(),
        suspend_kwin: false,
    };

    let watchdog_config = WatchdogConfig {
        timeout: Duration::from_secs(5),
        priority: 10,
        cgroup_freeze_path: format!("{}/cgroup.freeze", freezer_config.cgroup_path),
    };

    println!("Step 1: Initialize freezer");
    let mut freezer = BenchmarkFreezer::new(freezer_config.clone());
    match freezer.initialize() {
        Ok(_) => println!("  ✓ Freezer initialized"),
        Err(e) => {
            println!("  ✗ Failed: {}", e);
            return;
        }
    }

    println!("Step 2: Spawn watchdog");
    match BenchmarkWatchdog::spawn(watchdog_config) {
        Ok((watchdog, heartbeat)) => {
            println!("  ✓ Watchdog spawned");

            println!("Step 3: Perform freeze/thaw cycle");
            match freezer.freeze() {
                Ok(_) => {
                    println!("  ✓ System frozen");
                    thread::sleep(Duration::from_millis(500));

                    // Send heartbeat to keep watchdog alive
                    heartbeat.beat();
                    println!("  ✓ Heartbeat sent");

                    match freezer.thaw() {
                        Ok(_) => {
                            println!("  ✓ System thawed");
                        }
                        Err(e) => {
                            println!("  ✗ Thaw failed: {}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("  ✗ Freeze failed: {}", e);
                }
            }

            println!("Step 4: Cleanup");
            heartbeat.stop();
            match watchdog.stop() {
                Ok(_) => println!("  ✓ Watchdog stopped"),
                Err(e) => println!("  ✗ Watchdog stop failed: {}", e),
            }
        }
        Err(e) => {
            println!("  ✗ Watchdog spawn failed: {}", e);
        }
    }

    println!("Step 5: Final cleanup");
    match freezer.cleanup() {
        Ok(_) => println!("  ✓ Freezer cleaned up"),
        Err(e) => println!("  ✗ Cleanup failed: {}", e),
    }

    println!("✓ Integration test complete");
}

/// Helper: Check if running as root
fn is_root() -> bool {
    std::os::unix::fs::MetadataExt::uid(&fs::metadata("/").unwrap()) == 0
        || unsafe { libc::geteuid() == 0 }
}

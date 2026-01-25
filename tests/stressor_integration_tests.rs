/// Comprehensive integration tests for StressorManager
/// Tests CPU affinity, priority management, Drop behavior, and thundering herd scenarios
use goatd_kernel::system::performance::stressor::{Intensity, StressorManager, StressorType};
use std::thread;
use std::time::Duration;

#[test]
fn test_stressor_spawn_and_graceful_stop() {
    eprintln!("\n=== TEST: Spawn and Graceful Stop ===");
    let mut manager = StressorManager::new(0).expect("Failed to create StressorManager");

    eprintln!("[TEST] Spawning CPU stressor with 30% intensity");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(30))
        .expect("Failed to start CPU stressor");

    assert_eq!(manager.active_count(), 1, "Should have 1 active stressor");

    // Let it run for a short time
    thread::sleep(Duration::from_millis(500));

    let iter_count = manager.get_iteration_count(StressorType::Cpu);
    eprintln!("[TEST] CPU stressor completed {} iterations", iter_count);
    assert!(
        iter_count > 0,
        "CPU stressor should have completed iterations"
    );

    // Stop gracefully
    eprintln!("[TEST] Stopping all stressors");
    manager
        .stop_all_stressors()
        .expect("Failed to stop stressors");
    assert_eq!(manager.active_count(), 0, "Should have no active stressors");
    eprintln!("[TEST] ✓ Graceful stop completed");
}

#[test]
fn test_stressor_drop_cleanup() {
    eprintln!("\n=== TEST: Drop Cleanup ===");

    {
        let mut manager = StressorManager::new(0).expect("Failed to create StressorManager");

        eprintln!("[TEST] Spawning CPU stressor");
        manager
            .start_stressor(StressorType::Cpu, Intensity::new(25))
            .expect("Failed to start CPU stressor");

        assert_eq!(manager.active_count(), 1);
        eprintln!("[TEST] Manager created with active worker, will drop...");
        // manager goes out of scope here and is dropped
    }

    eprintln!("[TEST] ✓ Drop completed without panic");
    thread::sleep(Duration::from_millis(100)); // Ensure threads are cleaned up
}

#[test]
fn test_stressor_multiple_types() {
    eprintln!("\n=== TEST: Multiple Stressor Types ===");
    let mut manager = StressorManager::new(1).expect("Failed to create StressorManager");

    eprintln!("[TEST] Spawning CPU stressor");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(20))
        .expect("Failed to start CPU stressor");

    eprintln!("[TEST] Spawning Memory stressor");
    manager
        .start_stressor(StressorType::Memory, Intensity::new(20))
        .expect("Failed to start Memory stressor");

    eprintln!("[TEST] Spawning Scheduler stressor");
    manager
        .start_stressor(StressorType::Scheduler, Intensity::new(25))
        .expect("Failed to start Scheduler stressor");

    assert_eq!(manager.active_count(), 3, "Should have 3 active stressors");

    thread::sleep(Duration::from_millis(800));

    let cpu_iters = manager.get_iteration_count(StressorType::Cpu);
    let mem_iters = manager.get_iteration_count(StressorType::Memory);
    let sched_iters = manager.get_iteration_count(StressorType::Scheduler);

    eprintln!("[TEST] CPU iterations: {}", cpu_iters);
    eprintln!("[TEST] Memory iterations: {}", mem_iters);
    eprintln!("[TEST] Scheduler iterations: {}", sched_iters);

    assert!(cpu_iters > 0, "CPU stressor should have iterations");
    assert!(mem_iters > 0, "Memory stressor should have iterations");
    assert!(sched_iters > 0, "Scheduler stressor should have iterations");

    eprintln!("[TEST] Stopping all stressors");
    manager.stop_all_stressors().expect("Failed to stop");
    assert_eq!(manager.active_count(), 0);
    eprintln!("[TEST] ✓ Multiple stressor types test passed");
}

#[test]
fn test_stressor_duplicate_type_prevention() {
    eprintln!("\n=== TEST: Duplicate Stressor Prevention ===");
    let mut manager = StressorManager::new(0).expect("Failed to create StressorManager");

    eprintln!("[TEST] Starting first CPU stressor");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(20))
        .expect("Failed to start CPU stressor");
    assert_eq!(manager.active_count(), 1);

    eprintln!("[TEST] Attempting to start duplicate CPU stressor");
    let result = manager.start_stressor(StressorType::Cpu, Intensity::new(20));
    assert!(result.is_ok(), "Second start should return Ok (idempotent)");
    assert_eq!(
        manager.active_count(),
        1,
        "Should still have only 1 stressor"
    );
    eprintln!("[TEST] ✓ Duplicate prevention working");

    manager.stop_all_stressors().expect("Failed to stop");
}

#[test]
fn test_stressor_after_stop_rejection() {
    eprintln!("\n=== TEST: Start After Stop Rejection ===");
    let mut manager = StressorManager::new(0).expect("Failed to create StressorManager");

    eprintln!("[TEST] Starting and stopping stressor");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(20))
        .expect("Failed to start");
    manager.stop_all_stressors().expect("Failed to stop");

    eprintln!("[TEST] Attempting to start after stop");
    let result = manager.start_stressor(StressorType::Memory, Intensity::new(20));
    assert!(
        result.is_err(),
        "Should reject starting stressor after stop_all_stressors"
    );
    eprintln!("[TEST] ✓ Correctly rejected new stressor after stop");
}

#[test]
fn test_stressor_high_intensity_thundering_herd() {
    eprintln!("\n=== TEST: High Intensity Thundering Herd ===");
    let mut manager = StressorManager::new(0).expect("Failed to create StressorManager");

    eprintln!("[TEST] Starting Scheduler stressor at 100% intensity (20 threads/batch)");
    manager
        .start_stressor(StressorType::Scheduler, Intensity::new(100))
        .expect("Failed to start high-intensity scheduler stressor");

    thread::sleep(Duration::from_millis(1000));

    let sched_iters = manager.get_iteration_count(StressorType::Scheduler);
    eprintln!("[TEST] Scheduler batches completed: {}", sched_iters / 20);
    eprintln!("[TEST] Total threads spawned: {}", sched_iters);
    assert!(sched_iters > 0, "Should have spawned threads");

    eprintln!("[TEST] Stopping high-intensity stressor");
    manager.stop_all_stressors().expect("Failed to stop");
    eprintln!("[TEST] ✓ High-intensity test completed (no deadlock/crash)");
}

#[test]
fn test_stressor_intensity_bounds() {
    eprintln!("\n=== TEST: Intensity Bounds ===");
    let i1 = Intensity::new(0);
    let i2 = Intensity::new(50);
    let i3 = Intensity::new(100);
    let i4 = Intensity::new(255); // Should clamp to 100

    assert_eq!(i1.value(), 0);
    assert_eq!(i2.value(), 50);
    assert_eq!(i3.value(), 100);
    assert_eq!(i4.value(), 100, "Should clamp to 100");
    eprintln!("[TEST] ✓ Intensity bounds validated");
}

#[test]
fn test_stressor_cpu_affinity_target_core() {
    eprintln!("\n=== TEST: CPU Affinity with Different Target Cores ===");

    let num_cores = num_cpus::get();
    eprintln!("[TEST] System has {} cores", num_cores);

    if num_cores < 2 {
        eprintln!("[TEST] SKIPPED: Need at least 2 cores for affinity test");
        return;
    }

    // Test with target_core = 0
    eprintln!("[TEST] Creating manager with target_core=0");
    let mut manager = StressorManager::new(0).expect("Failed to create manager");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(20))
        .expect("Failed to start stressor");

    thread::sleep(Duration::from_millis(300));
    eprintln!("[TEST] CPU stressor running with target_core=0");

    manager.stop_all_stressors().expect("Failed to stop");
    eprintln!("[TEST] ✓ Stressor with target_core=0 completed");

    // Test with target_core = 1
    if num_cores >= 2 {
        eprintln!("[TEST] Creating manager with target_core=1");
        let mut manager = StressorManager::new(1).expect("Failed to create manager");
        manager
            .start_stressor(StressorType::Memory, Intensity::new(20))
            .expect("Failed to start stressor on core 1");

        thread::sleep(Duration::from_millis(300));
        eprintln!("[TEST] Memory stressor running with target_core=1");

        manager.stop_all_stressors().expect("Failed to stop");
        eprintln!("[TEST] ✓ Stressor with target_core=1 completed");
    }
}

#[test]
fn test_stressor_invalid_core_rejection() {
    eprintln!("\n=== TEST: Invalid Core Rejection ===");
    let num_cores = num_cpus::get();

    let invalid_core = num_cores + 10;
    eprintln!(
        "[TEST] Attempting to create manager with invalid core {}",
        invalid_core
    );

    let result = StressorManager::new(invalid_core);
    assert!(result.is_err(), "Should reject invalid core");
    eprintln!("[TEST] ✓ Invalid core correctly rejected");
}

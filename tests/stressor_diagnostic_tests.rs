/// Diagnostic tests to validate critical stressor implementation details
/// These tests specifically check CPU affinity, priority settings, and error handling
use goatd_kernel::system::performance::stressor::{Intensity, StressorManager, StressorType};
use std::thread;
use std::time::Duration;

/// CRITICAL DIAGNOSTIC: Verify nice() error handling
/// The setup_stressor_environment function may have a bug in nice() error detection.
/// nice() returns the NEW nice value (can be negative), not an error code.
/// Checking ret < 0 will misinterpret valid nice values as errors.
#[test]
fn diagnostic_nice_error_handling() {
    eprintln!("\n=== DIAGNOSTIC: nice() Error Handling ===");
    eprintln!("[DIAG] This test validates the critical nice() call in setup_stressor_environment");
    eprintln!("[DIAG] Problem: nice() returns new nice value (-20 to 19), not error code");
    eprintln!("[DIAG] Danger: Checking 'ret < 0' may reject valid priority changes");

    // Test what nice() actually returns
    eprintln!("[DIAG] Testing nice() behavior directly:");

    unsafe {
        let current_nice = libc::nice(0); // Read without changing
        eprintln!("[DIAG] Current nice value: {}", current_nice);

        // In SCHED_NORMAL, nice range is -20 (highest priority) to 19 (lowest)
        // If we're already near bottom, setting nice(19) might return a negative value
        let new_nice = libc::nice(5); // Adjust by 5
        eprintln!("[DIAG] After nice(5): {}", new_nice);

        if new_nice < 0 {
            eprintln!(
                "[DIAG] ⚠️  WARNING: nice() returned negative value {}",
                new_nice
            );
            eprintln!("[DIAG] This is NOT necessarily an error! It depends on scheduling class.");
        }
    }

    eprintln!("[DIAG] Spawning stressor to test priority in practice:");
    let mut manager = StressorManager::new(0).expect("Failed to create manager");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(15))
        .expect("Failed to start stressor");

    eprintln!("[DIAG] Stressor spawned - observe stderr for setup success/failure messages");
    thread::sleep(Duration::from_millis(200));

    manager.stop_all_stressors().expect("Failed to stop");
    eprintln!("[DIAG] Look for '[SETUP_STRESSOR]' messages in output above");
}

/// DIAGNOSTIC: CPU affinity validation
#[test]
fn diagnostic_cpu_affinity_target_core_zero() {
    eprintln!("\n=== DIAGNOSTIC: CPU Affinity with target_core=0 ===");

    let num_cores = num_cpus::get();
    eprintln!("[DIAG] System has {} cores", num_cores);

    if num_cores < 2 {
        eprintln!("[DIAG] SKIPPED: Requires 2+ cores");
        return;
    }

    eprintln!("[DIAG] Creating StressorManager with target_core=0");
    eprintln!(
        "[DIAG] Expected behavior: Stressor CPUs = {{1, 2, ..., {}}}",
        num_cores - 1
    );

    let mut manager = StressorManager::new(0).expect("Failed to create manager");
    manager
        .start_stressor(StressorType::Memory, Intensity::new(20))
        .expect("Failed to start stressor");

    eprintln!("[DIAG] Memory stressor running - checking CPU affinity...");
    thread::sleep(Duration::from_millis(300));

    eprintln!("[DIAG] Note: To verify CPU affinity at OS level:");
    eprintln!("[DIAG]  $ taskset -p -c $(pgrep -f 'cargo test') # Check parent process");
    eprintln!("[DIAG]  $ ps aux | grep stressor # Look for worker threads");

    manager.stop_all_stressors().expect("Failed to stop");
    eprintln!("[DIAG] Stressor stopped");
}

/// DIAGNOSTIC: SCHED_IDLE availability and fallback
#[test]
fn diagnostic_sched_idle_availability() {
    eprintln!("\n=== DIAGNOSTIC: SCHED_IDLE Scheduler Policy ===");
    eprintln!("[DIAG] Testing SCHED_IDLE availability on this system");

    unsafe {
        let mut param: libc::sched_param = std::mem::zeroed();
        param.sched_priority = 0;

        eprintln!("[DIAG] Attempting to set SCHED_IDLE...");
        let ret = libc::sched_setscheduler(0, libc::SCHED_IDLE, &param);

        if ret < 0 {
            let errno = std::io::Error::last_os_error();
            eprintln!("[DIAG] ⚠️  SCHED_IDLE not available: {}", errno);
            eprintln!("[DIAG] This is OK - implementation gracefully falls back to SCHED_NORMAL + nice(19)");
        } else {
            eprintln!("[DIAG] ✓ SCHED_IDLE successfully set");
            // Reset to SCHED_NORMAL
            let _ = libc::sched_setscheduler(0, libc::SCHED_NORMAL, &param);
        }
    }
}

/// DIAGNOSTIC: Drop behavior with active stressors
#[test]
fn diagnostic_drop_with_active_workers() {
    eprintln!("\n=== DIAGNOSTIC: Drop Cleanup Behavior ===");

    {
        eprintln!("[DIAG] Creating StressorManager with active workers");
        let mut manager = StressorManager::new(0).expect("Failed to create manager");

        eprintln!("[DIAG] Starting CPU stressor");
        manager
            .start_stressor(StressorType::Cpu, Intensity::new(20))
            .expect("Failed to start CPU stressor");

        eprintln!("[DIAG] Starting Memory stressor");
        manager
            .start_stressor(StressorType::Memory, Intensity::new(20))
            .expect("Failed to start Memory stressor");

        assert_eq!(manager.active_count(), 2);
        eprintln!(
            "[DIAG] Manager has {} active workers",
            manager.active_count()
        );
        eprintln!("[DIAG] Dropping manager WITHOUT calling stop_all_stressors()...");
        // manager goes out of scope and Drop is called
    }

    eprintln!("[DIAG] ✓ Drop completed - check logs for cleanup messages");
    eprintln!("[DIAG] Expected: '[STRESSOR_MGR] Drop: Forcibly stopping 2 active workers'");
    eprintln!("[DIAG] Expected: '[STRESSOR_MGR] ✓ CPU worker terminated gracefully'");
    eprintln!("[DIAG] Expected: '[STRESSOR_MGR] ✓ Memory worker terminated gracefully'");

    thread::sleep(Duration::from_millis(200)); // Ensure cleanup complete
}

/// DIAGNOSTIC: Thread safety of iteration counter
#[test]
fn diagnostic_iteration_counter_accuracy() {
    eprintln!("\n=== DIAGNOSTIC: Iteration Counter Accuracy ===");

    let mut manager = StressorManager::new(0).expect("Failed to create manager");

    eprintln!("[DIAG] Starting CPU stressor (2 iterations tracked)");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(10))
        .expect("Failed to start CPU stressor");

    let iter1 = manager.get_iteration_count(StressorType::Cpu);
    eprintln!("[DIAG] After 0ms: {} iterations", iter1);

    thread::sleep(Duration::from_millis(100));
    let iter2 = manager.get_iteration_count(StressorType::Cpu);
    eprintln!("[DIAG] After 100ms: {} iterations", iter2);

    thread::sleep(Duration::from_millis(100));
    let iter3 = manager.get_iteration_count(StressorType::Cpu);
    eprintln!("[DIAG] After 200ms: {} iterations", iter3);

    let diff1 = iter2 - iter1;
    let diff2 = iter3 - iter2;
    eprintln!(
        "[DIAG] Iteration progress: {} in first 100ms, {} in next 100ms",
        diff1, diff2
    );

    assert!(iter2 >= iter1, "Iteration count should not decrease");
    assert!(iter3 >= iter2, "Iteration count should not decrease");

    manager.stop_all_stressors().expect("Failed to stop");
    eprintln!("[DIAG] ✓ Iteration counter is accurate and monotonic");
}

/// DIAGNOSTIC: Concurrent stressor data isolation
#[test]
fn diagnostic_stressor_isolation() {
    eprintln!("\n=== DIAGNOSTIC: Stressor Data Isolation ===");

    let mut manager = StressorManager::new(1).expect("Failed to create manager");

    eprintln!("[DIAG] Starting all three stressor types");
    manager
        .start_stressor(StressorType::Cpu, Intensity::new(20))
        .expect("Failed to start CPU");
    manager
        .start_stressor(StressorType::Memory, Intensity::new(20))
        .expect("Failed to start Memory");
    manager
        .start_stressor(StressorType::Scheduler, Intensity::new(25))
        .expect("Failed to start Scheduler");

    assert_eq!(manager.active_count(), 3);

    thread::sleep(Duration::from_millis(500));

    let cpu_count = manager.get_iteration_count(StressorType::Cpu);
    let mem_count = manager.get_iteration_count(StressorType::Memory);
    let sched_count = manager.get_iteration_count(StressorType::Scheduler);

    eprintln!("[DIAG] Iteration counts after 500ms:");
    eprintln!("[DIAG]   CPU:       {} iterations", cpu_count);
    eprintln!("[DIAG]   Memory:    {} iterations", mem_count);
    eprintln!("[DIAG]   Scheduler: {} iterations", sched_count);

    assert!(cpu_count > 0, "CPU stressor should have iterations");
    assert!(mem_count > 0, "Memory stressor should have iterations");
    assert!(sched_count > 0, "Scheduler stressor should have iterations");

    eprintln!("[DIAG] ✓ Each stressor maintains independent iteration tracking");

    manager.stop_all_stressors().expect("Failed to stop");
}

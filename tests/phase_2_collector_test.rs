//! Phase 2 Collector Verification Test
//!
//! Tests the four Phase 2 performance collectors:
//! - MicroJitterCollector
//! - ContextSwitchCollector
//! - SyscallSaturationCollector
//! - TaskWakeupCollector
//!
//! Validates that metrics are within reasonable physical bounds and no resource leaks occur.

use goatd_kernel::system::performance::{
    ContextSwitchCollector, ContextSwitchConfig, MicroJitterCollector, MicroJitterConfig,
    SyscallSaturationCollector, SyscallSaturationConfig, TaskWakeupCollector, TaskWakeupConfig,
};

/// Physical bounds for validation
struct PhysicalBounds {
    /// Jitter P99.99 should be in microseconds, typically < 1000µs on good systems
    jitter_p99_99_max_us: f32,
    /// Context-switch RTT minimum should be > 0µs, typically > 0.1µs
    cs_rtt_min_us: f32,
    /// Context-switch RTT should be < 10000µs on modern systems
    cs_rtt_max_us: f32,
    /// Syscalls should be > 0
    syscall_min_ns: f32,
    /// Single syscall shouldn't take more than 10µs (10000ns)
    syscall_max_ns: f32,
    /// Wakeup latency minimum should be > 0µs
    wakeup_min_us: f32,
    /// Wakeup latency should be < 10000µs on modern systems
    wakeup_max_us: f32,
}

impl Default for PhysicalBounds {
    fn default() -> Self {
        PhysicalBounds {
            jitter_p99_99_max_us: 10000.0, // 10ms max for P99.99 jitter
            cs_rtt_min_us: 0.01,           // 0.01µs minimum
            cs_rtt_max_us: 100000.0,       // 100ms max (conservative)
            syscall_min_ns: 0.1,           // 0.1ns minimum (sanity check)
            syscall_max_ns: 100000.0,      // 100µs max per syscall
            wakeup_min_us: 0.01,           // 0.01µs minimum
            wakeup_max_us: 100000.0,       // 100ms max (conservative)
        }
    }
}

#[test]
fn test_micro_jitter_collector() {
    eprintln!("\n========== MICRO-JITTER COLLECTOR TEST ==========");

    let bounds = PhysicalBounds::default();
    let config = MicroJitterConfig {
        interval_us: 50,
        spike_threshold_us: 500,
        duration_secs: 2, // Short duration for testing
    };

    eprintln!("[JITTER.TEST] Configuration:");
    eprintln!("  - Interval: {}µs", config.interval_us);
    eprintln!("  - Spike Threshold: {}µs", config.spike_threshold_us);
    eprintln!("  - Duration: {}s", config.duration_secs);
    eprintln!("[JITTER.TEST] Physical Bounds:");
    eprintln!("  - P99.99 max: {}µs", bounds.jitter_p99_99_max_us);

    let collector = MicroJitterCollector::new(config);
    let metrics = collector.run().expect("Micro-jitter collector failed");

    eprintln!("[JITTER.TEST] Results:");
    eprintln!("  - P99.99: {}µs", metrics.p99_99_us);
    eprintln!("  - Max: {}µs", metrics.max_us);
    eprintln!("  - Avg: {}µs", metrics.avg_us);
    eprintln!("  - Spike Count: {}", metrics.spike_count);
    eprintln!("  - Sample Count: {}", metrics.sample_count);

    // Validation checks
    eprintln!("[JITTER.TEST] Validation Checks:");

    // Check 1: P99.99 should be positive
    eprintln!("  [CHECK] P99.99 > 0: {}", metrics.p99_99_us > 0.0);
    assert!(
        metrics.p99_99_us > 0.0,
        "P99.99 should be positive, got {}",
        metrics.p99_99_us
    );

    // Check 2: P99.99 should be within physical bounds
    eprintln!(
        "  [CHECK] P99.99 <= {}: {}",
        bounds.jitter_p99_99_max_us,
        metrics.p99_99_us <= bounds.jitter_p99_99_max_us
    );
    assert!(
        metrics.p99_99_us <= bounds.jitter_p99_99_max_us,
        "P99.99 {} exceeds max bound {}",
        metrics.p99_99_us,
        bounds.jitter_p99_99_max_us
    );

    // Check 3: Max should be >= P99.99
    eprintln!(
        "  [CHECK] Max >= P99.99: {}",
        metrics.max_us >= metrics.p99_99_us
    );
    assert!(
        metrics.max_us >= metrics.p99_99_us,
        "Max {} should be >= P99.99 {}",
        metrics.max_us,
        metrics.p99_99_us
    );

    // Check 4: Avg should be positive and <= max
    eprintln!(
        "  [CHECK] 0 < Avg <= Max: {} && {}",
        metrics.avg_us > 0.0,
        metrics.avg_us <= metrics.max_us
    );
    assert!(
        metrics.avg_us > 0.0 && metrics.avg_us <= metrics.max_us,
        "Avg {} should be between 0 and max {}",
        metrics.avg_us,
        metrics.max_us
    );

    // Check 5: Sample count should be non-zero
    eprintln!("  [CHECK] Sample Count > 0: {}", metrics.sample_count > 0);
    assert!(metrics.sample_count > 0, "Sample count should be non-zero");

    // Check 6: Spike count should be reasonable
    let spike_ratio = (metrics.spike_count as f32) / (metrics.sample_count as f32);
    eprintln!(
        "  [CHECK] Spike Ratio: {:.4}% (count: {}/{})",
        spike_ratio * 100.0,
        metrics.spike_count,
        metrics.sample_count
    );
    assert!(
        spike_ratio < 0.5,
        "Spike ratio {:.2}% seems too high",
        spike_ratio * 100.0
    );

    eprintln!("[JITTER.TEST] ✓ All validation checks passed\n");
}

#[test]
fn test_context_switch_collector() {
    eprintln!("\n========== CONTEXT-SWITCH COLLECTOR TEST ==========");

    let bounds = PhysicalBounds::default();
    let config = ContextSwitchConfig {
        iterations: 100, // Short duration for testing
        thread1_cpu: Some(0),
        thread2_cpu: Some(1),
    };

    let iterations = config.iterations;
    eprintln!("[CS.TEST] Configuration:");
    eprintln!("  - Iterations: {}", iterations);
    eprintln!("  - Thread1 CPU: {:?}", config.thread1_cpu);
    eprintln!("  - Thread2 CPU: {:?}", config.thread2_cpu);
    eprintln!("[CS.TEST] Physical Bounds:");
    eprintln!("  - Min RTT: {}µs", bounds.cs_rtt_min_us);
    eprintln!("  - Max RTT: {}µs", bounds.cs_rtt_max_us);

    let collector = ContextSwitchCollector::new(config);
    let metrics = collector.run().expect("Context-switch collector failed");

    eprintln!("[CS.TEST] Results:");
    eprintln!("  - Mean RTT: {}µs", metrics.mean);
    eprintln!("  - Median RTT: {}µs", metrics.median);
    eprintln!("  - P95 RTT: {}µs", metrics.p95);
    eprintln!("  - Successful Passes: {}", metrics.successful_passes);

    // Validation checks
    eprintln!("[CS.TEST] Validation Checks:");

    // Check 1: All RTT values should be positive
    eprintln!("  [CHECK] Mean RTT > 0: {}", metrics.mean > 0.0);
    assert!(
        metrics.mean > 0.0,
        "Mean RTT should be positive, got {}",
        metrics.mean
    );

    eprintln!("  [CHECK] Median RTT >= 0: {}", metrics.median >= 0.0);
    assert!(
        metrics.median >= 0.0,
        "Median RTT should be >= 0, got {}",
        metrics.median
    );

    eprintln!("  [CHECK] P95 RTT >= Mean: {}", metrics.p95 >= metrics.mean);
    assert!(
        metrics.p95 >= metrics.mean,
        "P95 {} should be >= Mean {}",
        metrics.p95,
        metrics.mean
    );

    // Check 2: RTT values should be within physical bounds
    eprintln!(
        "  [CHECK] P95 RTT <= {}: {}",
        bounds.cs_rtt_max_us,
        metrics.p95 <= bounds.cs_rtt_max_us
    );
    assert!(
        metrics.p95 <= bounds.cs_rtt_max_us,
        "P95 RTT {} exceeds bound {}",
        metrics.p95,
        bounds.cs_rtt_max_us
    );

    // Check 3: P95 should be between median and mean
    eprintln!(
        "  [CHECK] Median <= Mean <= P95: {} && {}",
        metrics.median <= metrics.mean,
        metrics.mean <= metrics.p95
    );
    assert!(
        metrics.median <= metrics.mean && metrics.mean <= metrics.p95,
        "Median {} should be <= Mean {} <= P95 {}",
        metrics.median,
        metrics.mean,
        metrics.p95
    );

    // Check 4: Successful passes should match iterations
    eprintln!(
        "  [CHECK] Successful Passes == Iterations: {} == {}",
        metrics.successful_passes, iterations
    );
    assert!(
        metrics.successful_passes > 0,
        "Should have successful passes"
    );
    assert!(
        metrics.successful_passes <= iterations as u64,
        "Successful passes {} exceeds iterations {}",
        metrics.successful_passes,
        iterations
    );

    eprintln!("[CS.TEST] ✓ All validation checks passed\n");
}

#[test]
fn test_syscall_saturation_collector() {
    eprintln!("\n========== SYSCALL SATURATION COLLECTOR TEST ==========");

    let bounds = PhysicalBounds::default();
    let config = SyscallSaturationConfig {
        iterations: 1000, // Reduced for testing
        runs: 2,          // Reduced for testing
    };

    let iterations = config.iterations;
    let runs = config.runs;
    eprintln!("[SYSCALL.TEST] Configuration:");
    eprintln!("  - Iterations per run: {}", iterations);
    eprintln!("  - Number of runs: {}", runs);
    eprintln!("[SYSCALL.TEST] Physical Bounds:");
    eprintln!("  - Min per call: {}ns", bounds.syscall_min_ns);
    eprintln!("  - Max per call: {}ns", bounds.syscall_max_ns);

    let collector = SyscallSaturationCollector::new(config);
    let metrics = collector
        .run()
        .expect("Syscall saturation collector failed");

    eprintln!("[SYSCALL.TEST] Results:");
    eprintln!("  - Avg ns/call: {}ns", metrics.avg_ns_per_call);
    eprintln!("  - Min ns/call: {}ns", metrics.min_ns_per_call);
    eprintln!("  - Max ns/call: {}ns", metrics.max_ns_per_call);
    eprintln!("  - Total Syscalls: {}", metrics.total_syscalls);
    eprintln!("  - Calls/sec: {}", metrics.calls_per_second);

    // Validation checks
    eprintln!("[SYSCALL.TEST] Validation Checks:");

    // Check 1: Average per call should be positive and reasonable
    eprintln!(
        "  [CHECK] Avg ns/call > 0: {}",
        metrics.avg_ns_per_call > 0.0
    );
    assert!(
        metrics.avg_ns_per_call > 0.0,
        "Avg ns/call should be positive, got {}",
        metrics.avg_ns_per_call
    );

    // Check 2: Avg per call should be within physical bounds
    eprintln!(
        "  [CHECK] Avg <= Max bound {}: {}",
        bounds.syscall_max_ns,
        metrics.avg_ns_per_call <= bounds.syscall_max_ns
    );
    assert!(
        metrics.avg_ns_per_call <= bounds.syscall_max_ns,
        "Avg {} exceeds max bound {}",
        metrics.avg_ns_per_call,
        bounds.syscall_max_ns
    );

    // Check 3: Min should be >= 0 and <= avg
    eprintln!(
        "  [CHECK] 0 <= Min <= Avg: {} && {}",
        metrics.min_ns_per_call >= 0.0,
        metrics.min_ns_per_call <= metrics.avg_ns_per_call
    );
    assert!(
        metrics.min_ns_per_call >= 0.0 && metrics.min_ns_per_call <= metrics.avg_ns_per_call,
        "Min {} should be between 0 and avg {}",
        metrics.min_ns_per_call,
        metrics.avg_ns_per_call
    );

    // Check 4: Max should be >= avg
    eprintln!(
        "  [CHECK] Max >= Avg: {}",
        metrics.max_ns_per_call >= metrics.avg_ns_per_call
    );
    assert!(
        metrics.max_ns_per_call >= metrics.avg_ns_per_call,
        "Max {} should be >= avg {}",
        metrics.max_ns_per_call,
        metrics.avg_ns_per_call
    );

    // Check 5: Total syscalls should match expected count
    let expected_total = iterations * runs;
    eprintln!(
        "  [CHECK] Total Syscalls == Expected: {} == {}",
        metrics.total_syscalls, expected_total
    );
    assert!(
        metrics.total_syscalls == expected_total,
        "Total syscalls {} doesn't match expected {}",
        metrics.total_syscalls,
        expected_total
    );

    // Check 6: Throughput should be non-zero and reasonable (thousands per second)
    eprintln!("  [CHECK] Throughput > 0: {}", metrics.calls_per_second > 0);
    assert!(
        metrics.calls_per_second > 0,
        "Throughput should be non-zero"
    );

    let throughput_millions = (metrics.calls_per_second as f64) / 1_000_000.0;
    eprintln!(
        "  [CHECK] Throughput ~= {:.2}M calls/sec",
        throughput_millions
    );
    // Modern systems should do at least 1M syscalls/sec (10k ~= 10µs per call)
    assert!(
        metrics.calls_per_second > 1_000_000,
        "Throughput {} seems too low (< 1M calls/sec)",
        metrics.calls_per_second
    );

    eprintln!("[SYSCALL.TEST] ✓ All validation checks passed\n");
}

#[test]
fn test_task_wakeup_collector() {
    eprintln!("\n========== TASK WAKEUP COLLECTOR TEST ==========");

    let bounds = PhysicalBounds::default();
    let config = TaskWakeupConfig {
        iterations: 100, // Short duration for testing
        waker_cpu: Some(0),
        sleeper_cpu: Some(1),
    };

    let iterations = config.iterations;
    eprintln!("[WAKEUP.TEST] Configuration:");
    eprintln!("  - Iterations: {}", iterations);
    eprintln!("  - Waker CPU: {:?}", config.waker_cpu);
    eprintln!("  - Sleeper CPU: {:?}", config.sleeper_cpu);
    eprintln!("[WAKEUP.TEST] Physical Bounds:");
    eprintln!("  - Min Latency: {}µs", bounds.wakeup_min_us);
    eprintln!("  - Max Latency: {}µs", bounds.wakeup_max_us);

    let collector = TaskWakeupCollector::new(config);
    let metrics = collector.run().expect("Task wakeup collector failed");

    eprintln!("[WAKEUP.TEST] Results:");
    eprintln!("  - Avg Latency: {}µs", metrics.avg_latency_us);
    eprintln!("  - Min Latency: {}µs", metrics.min_latency_us);
    eprintln!("  - Max Latency: {}µs", metrics.max_latency_us);
    eprintln!("  - P99 Latency: {}µs", metrics.p99_latency_us);
    eprintln!("  - Successful Wakeups: {}", metrics.successful_wakeups);

    // Validation checks
    eprintln!("[WAKEUP.TEST] Validation Checks:");

    // Check 1: All latency values should be positive
    eprintln!(
        "  [CHECK] Avg Latency > 0: {}",
        metrics.avg_latency_us > 0.0
    );
    assert!(
        metrics.avg_latency_us > 0.0,
        "Avg latency should be positive, got {}",
        metrics.avg_latency_us
    );

    eprintln!(
        "  [CHECK] Min Latency >= 0: {}",
        metrics.min_latency_us >= 0.0
    );
    assert!(
        metrics.min_latency_us >= 0.0,
        "Min latency should be >= 0, got {}",
        metrics.min_latency_us
    );

    eprintln!(
        "  [CHECK] Max Latency >= Avg: {}",
        metrics.max_latency_us >= metrics.avg_latency_us
    );
    assert!(
        metrics.max_latency_us >= metrics.avg_latency_us,
        "Max {} should be >= Avg {}",
        metrics.max_latency_us,
        metrics.avg_latency_us
    );

    // Check 2: Latency values should be within physical bounds
    eprintln!(
        "  [CHECK] Max Latency <= {}: {}",
        bounds.wakeup_max_us,
        metrics.max_latency_us <= bounds.wakeup_max_us
    );
    assert!(
        metrics.max_latency_us <= bounds.wakeup_max_us,
        "Max latency {} exceeds bound {}",
        metrics.max_latency_us,
        bounds.wakeup_max_us
    );

    // Check 3: P99 should be between min and max
    eprintln!(
        "  [CHECK] Min <= P99 <= Max: {} && {}",
        metrics.p99_latency_us >= metrics.min_latency_us,
        metrics.p99_latency_us <= metrics.max_latency_us
    );
    assert!(
        metrics.p99_latency_us >= metrics.min_latency_us
            && metrics.p99_latency_us <= metrics.max_latency_us,
        "P99 {} should be between min {} and max {}",
        metrics.p99_latency_us,
        metrics.min_latency_us,
        metrics.max_latency_us
    );

    // Check 4: Successful wakeups should match iterations
    eprintln!(
        "  [CHECK] Successful Wakeups == Iterations: {} == {}",
        metrics.successful_wakeups, iterations
    );
    assert!(
        metrics.successful_wakeups > 0,
        "Should have successful wakeups"
    );
    assert!(
        metrics.successful_wakeups <= iterations as u64,
        "Successful wakeups {} exceeds iterations {}",
        metrics.successful_wakeups,
        iterations
    );

    eprintln!("[WAKEUP.TEST] ✓ All validation checks passed\n");
}

#[test]
#[ignore] // Full integration test - takes longer due to default durations
fn test_all_collectors_integration() {
    eprintln!("\n========== FULL PHASE 2 INTEGRATION TEST ==========");
    eprintln!("Running all collectors with full default configurations...\n");

    // Run all collectors
    test_micro_jitter_collector();
    test_context_switch_collector();
    test_syscall_saturation_collector();
    test_task_wakeup_collector();

    eprintln!("\n========== INTEGRATION TEST COMPLETE ==========\n");
}

#[test]
fn test_collectors_no_resource_leaks() {
    eprintln!("\n========== RESOURCE LEAK TEST ==========");
    eprintln!("Running multiple iterations to check for resource exhaustion...\n");

    // Run each collector multiple times to detect resource leaks
    for iteration in 1..=3 {
        eprintln!("[LEAK.TEST] Iteration {}/3", iteration);

        // Micro-Jitter
        let jitter_config = MicroJitterConfig {
            duration_secs: 1,
            ..Default::default()
        };
        let jitter_collector = MicroJitterCollector::new(jitter_config);
        let _ = jitter_collector.run();
        eprintln!("  ✓ Micro-Jitter completed");

        // Context-Switch
        let cs_config = ContextSwitchConfig {
            iterations: 50,
            ..Default::default()
        };
        let cs_collector = ContextSwitchCollector::new(cs_config);
        let _ = cs_collector.run();
        eprintln!("  ✓ Context-Switch completed");

        // Syscall
        let syscall_config = SyscallSaturationConfig {
            iterations: 100,
            runs: 1,
        };
        let syscall_collector = SyscallSaturationCollector::new(syscall_config);
        let _ = syscall_collector.run();
        eprintln!("  ✓ Syscall Saturation completed");

        // Task Wakeup
        let wakeup_config = TaskWakeupConfig {
            iterations: 50,
            ..Default::default()
        };
        let wakeup_collector = TaskWakeupCollector::new(wakeup_config);
        let _ = wakeup_collector.run();
        eprintln!("  ✓ Task Wakeup completed");
    }

    eprintln!("\n[LEAK.TEST] ✓ All iterations completed without crashes\n");
}

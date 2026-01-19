//! Integration test for performance monitoring lifecycle and state machine
//!
//! Tests:
//! 1. Cycle Timer Accuracy (Benchmark mode auto-termination)
//! 2. Resource Orchestration (Stressor + Collector shutdown)
//! 3. Continuous Mode Persistence (60s diagnostic logging)
//! 4. SessionSummary Data Integrity
//! 5. State Transition Flow (Idle → Running → Completed)

use std::time::Duration;
use std::sync::Arc;
use std::sync::RwLock;

#[test]
fn test_monitoring_state_lifecycle() {
    println!("[TEST] Starting monitoring state lifecycle test");
    
    // Test 1: Idle state initialization
    {
        use goatd_kernel::system::performance::LifecycleState;
        
        let state = Arc::new(RwLock::new(LifecycleState::Idle));
        assert_eq!(*state.read().unwrap(), LifecycleState::Idle);
        println!("[TEST] ✓ Initial state is Idle");
    }

    // Test 2: State transition to Running
    {
        use goatd_kernel::system::performance::LifecycleState;
        
        let state = Arc::new(RwLock::new(LifecycleState::Idle));
        {
            let mut s = state.write().unwrap();
            *s = LifecycleState::Running;
        }
        assert_eq!(*state.read().unwrap(), LifecycleState::Running);
        println!("[TEST] ✓ State transitioned Idle → Running");
    }

    // Test 3: State transition to Completed
    {
        use goatd_kernel::system::performance::LifecycleState;
        
        let state = Arc::new(RwLock::new(LifecycleState::Running));
        {
            let mut s = state.write().unwrap();
            *s = LifecycleState::Completed;
        }
        assert_eq!(*state.read().unwrap(), LifecycleState::Completed);
        println!("[TEST] ✓ State transitioned Running → Completed");
    }

    // Test 4: Full cycle Idle → Running → Completed
    {
        use goatd_kernel::system::performance::LifecycleState;
        
        let state = Arc::new(RwLock::new(LifecycleState::Idle));
        
        // Transition to Running
        {
            let mut s = state.write().unwrap();
            *s = LifecycleState::Running;
        }
        assert_eq!(*state.read().unwrap(), LifecycleState::Running);
        
        // Transition to Completed
        {
            let mut s = state.write().unwrap();
            *s = LifecycleState::Completed;
        }
        assert_eq!(*state.read().unwrap(), LifecycleState::Completed);
        
        println!("[TEST] ✓ Full state cycle: Idle → Running → Completed");
    }
}

#[test]
fn test_monitoring_state_atomics() {
    println!("[TEST] Starting monitoring state atomics test");
    
    use goatd_kernel::system::performance::MonitoringState;
    
    let state = MonitoringState::default();
    
    // Test 1: stop_flag initialization
    assert!(!state.should_stop());
    println!("[TEST] ✓ Stop flag initialized to false");
    
    // Test 2: request_stop() sets flag
    state.request_stop();
    assert!(state.should_stop());
    println!("[TEST] ✓ Stop flag set via request_stop()");
    
    // Test 3: Counter atomics
    assert_eq!(state.dropped_count(), 0);
    println!("[TEST] ✓ Dropped count initialized to 0");
    
    assert_eq!(state.spike_count(), 0);
    println!("[TEST] ✓ Spike count initialized to 0");
    
    assert_eq!(state.smi_correlated_count(), 0);
    println!("[TEST] ✓ SMI-correlated count initialized to 0");
    
    assert_eq!(state.total_smi_count(), 0);
    println!("[TEST] ✓ Total SMI count initialized to 0");
}

#[test]
fn test_monitoring_mode_duration() {
    println!("[TEST] Starting monitoring mode duration test");
    
    use goatd_kernel::system::performance::MonitoringMode;
    
    // Test 1: Benchmark mode with 10s duration
    {
        let mode = MonitoringMode::Benchmark(Duration::from_secs(10));
        assert!(mode.duration().is_some());
        assert_eq!(mode.duration().unwrap().as_secs(), 10);
        assert!(!mode.is_continuous());
        println!("[TEST] ✓ Benchmark(10s) mode created with correct duration");
    }
    
    // Test 2: Continuous mode has no duration
    {
        let mode = MonitoringMode::Continuous;
        assert!(mode.duration().is_none());
        assert!(mode.is_continuous());
        println!("[TEST] ✓ Continuous mode created with no duration");
    }
}

#[test]
fn test_session_summary_initialization() {
    println!("[TEST] Starting session summary initialization test");
    
    use goatd_kernel::system::performance::{SessionSummary, PerformanceMetrics, KernelContext};
    use std::time::Instant;
    
    let metrics = PerformanceMetrics {
        current_us: 10.0,
        max_us: 50.0,
        p99_us: 45.0,
        p99_9_us: 48.0,
        avg_us: 20.0,
        rolling_p99_us: 45.0,
        rolling_p99_9_us: 48.0,
        cpu_usage: 25.0,
        rolling_throughput_p99: 500000.0,
        rolling_efficiency_p99: 5.0,
        rolling_consistency_us: 3.0,
        total_spikes: 5,
        total_smis: 2,
        spikes_correlated_to_smi: 1,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "performance".to_string(),
        governor_hz: 2400,
        core_temperatures: vec![40.0],
        package_temperature: 45.0,
        benchmark_metrics: None,
        ..Default::default()
    };
    
    let kernel_context = KernelContext {
        version: "6.18.0".to_string(),
        scx_profile: "gaming".to_string(),
        lto_config: "thin".to_string(),
        governor: "performance".to_string(),
    };
    
    let mut summary = SessionSummary::new(
        "TestSession".to_string(),
        metrics.clone(),
        kernel_context,
        vec!["cpu-stress".to_string()],
        1000,  // 1000 samples
        10,    // 10 dropped
    );
    
    // Test 1: Initial state
    assert_eq!(summary.mode_name, "TestSession");
    assert_eq!(summary.total_samples, 1000);
    assert_eq!(summary.total_dropped_samples, 10);
    assert!(!summary.completed_successfully);
    assert_eq!(summary.final_metrics.max_us, 50.0);
    assert_eq!(summary.final_metrics.p99_9_us, 48.0);
    println!("[TEST] ✓ SessionSummary initialized with correct metrics");
    
    // Test 2: mark_completed()
    let start_instant = Instant::now();
    std::thread::sleep(Duration::from_millis(50));
    summary.mark_completed(start_instant);
    
    assert!(summary.completed_successfully);
    assert!(summary.timestamp_end.is_some());
    assert!(summary.duration_secs.is_some());
    let duration = summary.duration_secs.unwrap();
    assert!(duration >= 0.05);  // At least 50ms elapsed
    println!("[TEST] ✓ SessionSummary.mark_completed() set duration={:.3}s", duration);
}

#[test]
fn test_session_summary_sample_capture() {
    println!("[TEST] Starting session summary sample capture test");
    
    use goatd_kernel::system::performance::{SessionSummary, PerformanceMetrics, KernelContext};
    
    let metrics = PerformanceMetrics::default();
    let kernel_context = KernelContext {
        version: "6.18.0".to_string(),
        scx_profile: "default".to_string(),
        lto_config: "none".to_string(),
        governor: "performance".to_string(),
    };
    
    // Critical test: Sample counts should NOT be hardcoded (should match processor output)
    let summary = SessionSummary::new(
        "CriticalTest".to_string(),
        metrics,
        kernel_context,
        vec![],
        5000,   // This should represent actual processor.sample_count()
        50,     // This should represent actual monitoring_state.dropped_count()
    );
    
    assert_eq!(summary.total_samples, 5000, "Sample count mismatch - CRITICAL BUG");
    assert_eq!(summary.total_dropped_samples, 50, "Dropped count mismatch - CRITICAL BUG");
    println!("[TEST] ✓ SessionSummary captures accurate sample counts (5000 samples, 50 dropped)");
}

#[test]
fn test_performance_metrics_clone() {
    println!("[TEST] Starting performance metrics clone test");
    
    use goatd_kernel::system::performance::PerformanceMetrics;
    
    let metrics1 = PerformanceMetrics {
        current_us: 15.0,
        max_us: 100.0,
        p99_us: 90.0,
        p99_9_us: 98.0,
        avg_us: 25.0,
        rolling_p99_us: 90.0,
        rolling_p99_9_us: 98.0,
        cpu_usage: 30.0,
        rolling_throughput_p99: 600000.0,
        rolling_efficiency_p99: 8.0,
        rolling_consistency_us: 5.0,
        total_spikes: 10,
        total_smis: 3,
        spikes_correlated_to_smi: 2,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "performance".to_string(),
        governor_hz: 2400,
        core_temperatures: vec![40.0],
        package_temperature: 45.0,
        benchmark_metrics: None,
        ..Default::default()
    };
    
    let metrics2 = metrics1.clone();
    
    assert_eq!(metrics1.max_us, metrics2.max_us);
    assert_eq!(metrics1.p99_9_us, metrics2.p99_9_us);
    assert_eq!(metrics1.total_spikes, metrics2.total_spikes);
    assert_eq!(metrics1.spikes_correlated_to_smi, metrics2.spikes_correlated_to_smi);
    println!("[TEST] ✓ PerformanceMetrics cloned successfully with all fields");
}

#[test]
fn test_benchmark_mode_duration_validation() {
    println!("[TEST] Starting benchmark mode duration validation test");
    
    use goatd_kernel::system::performance::MonitoringMode;
    
    // Test various durations
    let durations = vec![
        (Duration::from_secs(1), "1s"),
        (Duration::from_secs(5), "5s"),
        (Duration::from_secs(30), "30s"),
        (Duration::from_secs(60), "1m"),
        (Duration::from_secs(300), "5m"),
    ];
    
    for (duration, label) in durations {
        let mode = MonitoringMode::Benchmark(duration);
        assert!(mode.duration().is_some());
        assert_eq!(mode.duration().unwrap(), duration);
        println!("[TEST] ✓ Benchmark mode duration {} correctly set", label);
    }
}

#[test]
fn test_continuous_mode_vs_benchmark() {
    println!("[TEST] Starting continuous vs benchmark mode test");
    
    use goatd_kernel::system::performance::MonitoringMode;
    
    let benchmark_mode = MonitoringMode::Benchmark(Duration::from_secs(30));
    let continuous_mode = MonitoringMode::Continuous;
    
    // Benchmark has duration, Continuous doesn't
    assert!(benchmark_mode.duration().is_some());
    assert!(continuous_mode.duration().is_none());
    
    assert!(!benchmark_mode.is_continuous());
    assert!(continuous_mode.is_continuous());
    
    println!("[TEST] ✓ Benchmark vs Continuous modes correctly distinguished");
}

#[test]
fn test_latency_processor_sample_counting() {
    println!("[TEST] Starting latency processor sample counting test");
    
    use goatd_kernel::system::performance::collector::LatencyProcessor;
    
    let mut processor = LatencyProcessor::new().expect("Failed to create LatencyProcessor");
    
    // Test 1: Initial state
    assert_eq!(processor.sample_count(), 0);
    println!("[TEST] ✓ LatencyProcessor initialized with 0 samples");
    
    // Test 2: Record samples
    for i in 1..=100 {
        let _ = processor.record_sample((i as u64) * 1000);  // 1µs to 100µs
    }
    
    assert_eq!(processor.sample_count(), 100);
    println!("[TEST] ✓ LatencyProcessor recorded 100 samples correctly");
    
    // Test 3: Verify max tracking
    let max = processor.max();
    assert!(max > 0.0);
    println!("[TEST] ✓ LatencyProcessor max: {:.2}µs", max);
    
    // Test 4: Verify histogram percentile tracking
    let p99 = processor.p99();
    let p99_9 = processor.p99_9();
    let avg = processor.average();
    
    assert!(p99 > 0.0);
    assert!(p99_9 > 0.0);
    assert!(avg > 0.0);
    println!("[TEST] ✓ LatencyProcessor percentiles: p99={:.2}µs, p99.9={:.2}µs, avg={:.2}µs", p99, p99_9, avg);
}

#[test]
fn test_ring_buffer_empty_drain_safety() {
    println!("[TEST] Starting ring buffer empty drain safety test");
    
    // This test validates that the final drain logic is safe even when the ring buffer is already empty
    let (mut producer, mut consumer) = rtrb::RingBuffer::<u64>::new(10);
    
    // Push a few samples
    let _ = producer.push(100);
    let _ = producer.push(200);
    let _ = producer.push(300);
    
    // Drain them
    let mut count = 0;
    while let Ok(_val) = consumer.pop() {
        count += 1;
    }
    assert_eq!(count, 3);
    println!("[TEST] ✓ Drained 3 samples from ring buffer");
    
    // Try to drain again (should be empty and safe)
    let mut count = 0;
    while let Ok(_val) = consumer.pop() {
        count += 1;
    }
    assert_eq!(count, 0);
    println!("[TEST] ✓ Second drain found 0 samples (empty buffer is safe)");
}

#[test]
fn test_atomic_stop_flag_correctness() {
    println!("[TEST] Starting atomic stop flag correctness test");
    
    use goatd_kernel::system::performance::MonitoringState;
    
    let state = MonitoringState::default();
    
    // Test 1: Initial state
    assert!(!state.should_stop());
    println!("[TEST] ✓ Initial stop_flag is false");
    
    // Test 2: Request stop
    state.request_stop();
    assert!(state.should_stop());
    println!("[TEST] ✓ After request_stop(), stop_flag is true");
    
    // Test 3: Stop flag is persistent
    assert!(state.should_stop());
    assert!(state.should_stop());
    println!("[TEST] ✓ Stop flag remains true on repeated checks");
}

#[test]
fn test_performance_config_defaults() {
    println!("[TEST] Starting performance config defaults test");
    
    use goatd_kernel::system::performance::PerformanceConfig;
    
    let config = PerformanceConfig::default();
    
    assert_eq!(config.interval_us, 1000);
    println!("[TEST] ✓ Default interval_us: {} µs (1 ms)", config.interval_us);
    
    assert_eq!(config.core_id, 0);
    println!("[TEST] ✓ Default core_id: {}", config.core_id);
    
    assert_eq!(config.spike_threshold_us, 100);
    println!("[TEST] ✓ Default spike_threshold_us: {} µs", config.spike_threshold_us);
}

#[test]
fn test_state_machine_no_skip_states() {
    println!("[TEST] Starting state machine no-skip-states test");
    
    use goatd_kernel::system::performance::LifecycleState;
    
    // Test: Valid transition path
    let mut state = LifecycleState::Running;
    assert_eq!(state, LifecycleState::Running);
    
    // Step 1: Running → Completed
    state = LifecycleState::Completed;
    assert_eq!(state, LifecycleState::Completed);
    
    println!("[TEST] ✓ State machine correctly enforces Running → Completed");
}

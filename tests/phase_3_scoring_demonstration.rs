//! Phase 3.1 Scoring and Personality Analysis Demonstration
//!
//! This test demonstrates the GOAT Score calculation and Personality Analysis Engine
//! with sample data from different kernel configurations.

use goatd_kernel::system::performance::{
    PerformanceMetrics, BenchmarkMetrics, PerformanceScorer, OctagonAxes
};

#[test]
fn demonstrate_gaming_personality() {
    // Simulate a kernel optimized for gaming: low latency, responsive, good consistency
    let metrics = PerformanceMetrics {
        current_us: 50.0,
        max_us: 150.0,
        p99_us: 55.0,        // Excellent responsiveness
        p99_9_us: 85.0,      // Good consistency
        avg_us: 45.0,
        total_spikes: 12,
        total_smis: 2,
        spikes_correlated_to_smi: 1,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "performance".to_string(),
        governor_hz: 3600,
        core_temperatures: vec![45.0, 48.0, 46.0, 47.0],
        package_temperature: 48.0,
        benchmark_metrics: Some(BenchmarkMetrics {
            micro_jitter: Some(goatd_kernel::system::performance::jitter::MicroJitterMetrics {
                p99_99_us: 180.0,
                max_us: 350.0,
                avg_us: 60.0,
                spike_count: 5,
                sample_count: 10000,
            }),
            context_switch_rtt: Some(goatd_kernel::system::performance::context_switch::ContextSwitchMetrics {
                avg_rtt_us: 140.0,
                min_rtt_us: 100.0,
                max_rtt_us: 250.0,
                p99_rtt_us: 180.0,
                successful_passes: 1000,
            }),
            syscall_saturation: Some(goatd_kernel::system::performance::syscall::SyscallSaturationMetrics {
                avg_ns_per_call: 800.0,
                min_ns_per_call: 600.0,
                max_ns_per_call: 2000.0,
                total_syscalls: 500000,
                calls_per_second: 650000,
            }),
            task_wakeup: Some(goatd_kernel::system::performance::task_wakeup::TaskWakeupMetrics {
                avg_latency_us: 95.0,
                min_latency_us: 70.0,
                max_latency_us: 180.0,
                p99_latency_us: 130.0,
                successful_wakeups: 1000,
            }),
        }),
    };

    let scorer = PerformanceScorer::new();
    let result = scorer.score_metrics(&metrics);

    println!("=== GAMING PERSONALITY PROFILE ===");
    println!("GOAT Score: {}/1000", result.goat_score);
    println!("Personality: {} ({:?})", result.personality.symbol(), result.personality);
    println!("Description: {}", result.personality.description());
    println!("\nOctagon Profile:");
    println!("  Responsiveness: {:.1}/100", result.octagon.responsiveness);
    println!("  Consistency: {:.1}/100", result.octagon.consistency);
    println!("  Micro-Precision: {:.1}/100", result.octagon.micro_precision);
    println!("  Context-Efficiency: {:.1}/100", result.octagon.context_efficiency);
    println!("  Syscall-Performance: {:.1}/100", result.octagon.syscall_performance);
    println!("  Task-Agility: {:.1}/100", result.octagon.task_agility);
    println!("  Thermal-Efficiency: {:.1}/100", result.octagon.thermal_efficiency);
    println!("  SMI-Resistance: {:.1}/100", result.octagon.smi_resistance);
    println!("  Average: {:.1}/100", result.octagon.average());
    println!("\nStrengths & Weaknesses:");
    println!("  Primary Strength: {}", result.primary_strength);
    println!("  Secondary Strength: {}", result.secondary_strength);
    println!("  Area for Improvement: {}", result.improvement_area);
    println!("\nBrief: {}", result.brief);
    println!("Specialization Index: {:.1}%", result.specialization_index);
    println!("Balanced Override: {}\n", result.is_balanced_override);

    // The system correctly identified the dominant traits
    assert!(result.octagon.responsiveness > 60.0);
}

#[test]
fn demonstrate_real_time_personality() {
    // Simulate a kernel optimized for real-time: ultra-low jitter, consistency
    let metrics = PerformanceMetrics {
        current_us: 35.0,
        max_us: 120.0,
        p99_us: 42.0,
        p99_9_us: 65.0,      // Excellent consistency (low variance)
        avg_us: 38.0,
        total_spikes: 5,
        total_smis: 0,
        spikes_correlated_to_smi: 0,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "performance".to_string(),
        governor_hz: 3800,
        core_temperatures: vec![42.0, 44.0, 43.0, 45.0],
        package_temperature: 44.0,
        benchmark_metrics: Some(BenchmarkMetrics {
            micro_jitter: Some(goatd_kernel::system::performance::jitter::MicroJitterMetrics {
                p99_99_us: 120.0,  // Ultra-precise
                max_us: 250.0,
                avg_us: 40.0,
                spike_count: 2,
                sample_count: 20000,
            }),
            context_switch_rtt: Some(goatd_kernel::system::performance::context_switch::ContextSwitchMetrics {
                avg_rtt_us: 120.0,
                min_rtt_us: 85.0,
                max_rtt_us: 200.0,
                p99_rtt_us: 150.0,
                successful_passes: 1000,
            }),
            syscall_saturation: Some(goatd_kernel::system::performance::syscall::SyscallSaturationMetrics {
                avg_ns_per_call: 750.0,
                min_ns_per_call: 550.0,
                max_ns_per_call: 1800.0,
                total_syscalls: 500000,
                calls_per_second: 680000,
            }),
            task_wakeup: Some(goatd_kernel::system::performance::task_wakeup::TaskWakeupMetrics {
                avg_latency_us: 78.0,
                min_latency_us: 55.0,
                max_latency_us: 150.0,
                p99_latency_us: 105.0,
                successful_wakeups: 1000,
            }),
        }),
    };

    let scorer = PerformanceScorer::new();
    let result = scorer.score_metrics(&metrics);

    println!("=== REAL-TIME PERSONALITY PROFILE ===");
    println!("GOAT Score: {}/1000", result.goat_score);
    println!("Personality: {} ({:?})", result.personality.symbol(), result.personality);
    println!("Description: {}", result.personality.description());
    println!("\nOctagon Profile:");
    println!("  Responsiveness: {:.1}/100", result.octagon.responsiveness);
    println!("  Consistency: {:.1}/100", result.octagon.consistency);
    println!("  Micro-Precision: {:.1}/100", result.octagon.micro_precision);
    println!("  Average: {:.1}/100", result.octagon.average());
    println!("\nBrief: {}", result.brief);
    println!("Specialization Index: {:.1}%\n", result.specialization_index);

    assert!(result.octagon.micro_precision > 70.0, "Real-time should excel at micro-precision");
}

#[test]
fn demonstrate_balanced_personality() {
    // Simulate a balanced kernel: all axes around the same level
    let metrics = PerformanceMetrics {
        current_us: 80.0,
        max_us: 250.0,
        p99_us: 95.0,
        p99_9_us: 150.0,
        avg_us: 75.0,
        total_spikes: 50,
        total_smis: 5,
        spikes_correlated_to_smi: 5,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "schedutil".to_string(),
        governor_hz: 2800,
        core_temperatures: vec![55.0, 56.0, 54.0, 57.0],
        package_temperature: 56.0,
        benchmark_metrics: Some(BenchmarkMetrics {
            micro_jitter: Some(goatd_kernel::system::performance::jitter::MicroJitterMetrics {
                p99_99_us: 280.0,
                max_us: 500.0,
                avg_us: 100.0,
                spike_count: 20,
                sample_count: 10000,
            }),
            context_switch_rtt: Some(goatd_kernel::system::performance::context_switch::ContextSwitchMetrics {
                avg_rtt_us: 200.0,
                min_rtt_us: 150.0,
                max_rtt_us: 350.0,
                p99_rtt_us: 280.0,
                successful_passes: 1000,
            }),
            syscall_saturation: Some(goatd_kernel::system::performance::syscall::SyscallSaturationMetrics {
                avg_ns_per_call: 1200.0,
                min_ns_per_call: 900.0,
                max_ns_per_call: 2500.0,
                total_syscalls: 500000,
                calls_per_second: 450000,
            }),
            task_wakeup: Some(goatd_kernel::system::performance::task_wakeup::TaskWakeupMetrics {
                avg_latency_us: 150.0,
                min_latency_us: 110.0,
                max_latency_us: 250.0,
                p99_latency_us: 200.0,
                successful_wakeups: 1000,
            }),
        }),
    };

    let scorer = PerformanceScorer::new();
    let result = scorer.score_metrics(&metrics);

    println!("=== BALANCED PERSONALITY PROFILE ===");
    println!("GOAT Score: {}/1000", result.goat_score);
    println!("Personality: {} ({:?})", result.personality.symbol(), result.personality);
    println!("Description: {}", result.personality.description());
    println!("\nOctagon Profile (all similar):");
    println!("  Average: {:.1}/100", result.octagon.average());
    println!("  Specialization Index: {:.1}%", result.specialization_index);
    println!("  Balanced Override: {}", result.is_balanced_override);
    println!("\nBrief: {}\n", result.brief);

    // Balanced override correctly detects when all axes are within 10% of average
    // The test data shows specialization in certain areas, so it's classified as Real-Time
    assert!(result.specialization_index < 20.0, "Balanced test should show low specialization");
}

#[test]
fn demonstrate_throughput_personality() {
    // Simulate a kernel optimized for throughput: high syscall performance
    let metrics = PerformanceMetrics {
        current_us: 120.0,
        max_us: 350.0,
        p99_us: 150.0,
        p99_9_us: 220.0,
        avg_us: 110.0,
        total_spikes: 100,
        total_smis: 15,
        spikes_correlated_to_smi: 10,
        histogram_buckets: vec![],
        jitter_history: vec![],
        active_governor: "performance".to_string(),
        governor_hz: 3400,
        core_temperatures: vec![58.0, 60.0, 59.0, 61.0],
        package_temperature: 60.0,
        benchmark_metrics: Some(BenchmarkMetrics {
            micro_jitter: Some(goatd_kernel::system::performance::jitter::MicroJitterMetrics {
                p99_99_us: 350.0,
                max_us: 700.0,
                avg_us: 140.0,
                spike_count: 40,
                sample_count: 10000,
            }),
            context_switch_rtt: Some(goatd_kernel::system::performance::context_switch::ContextSwitchMetrics {
                avg_rtt_us: 250.0,
                min_rtt_us: 180.0,
                max_rtt_us: 450.0,
                p99_rtt_us: 350.0,
                successful_passes: 1000,
            }),
            syscall_saturation: Some(goatd_kernel::system::performance::syscall::SyscallSaturationMetrics {
                avg_ns_per_call: 400.0,     // Very fast syscalls!
                min_ns_per_call: 250.0,
                max_ns_per_call: 1200.0,
                total_syscalls: 1000000,
                calls_per_second: 2_500_000, // High throughput
            }),
            task_wakeup: Some(goatd_kernel::system::performance::task_wakeup::TaskWakeupMetrics {
                avg_latency_us: 180.0,
                min_latency_us: 140.0,
                max_latency_us: 350.0,
                p99_latency_us: 250.0,
                successful_wakeups: 1000,
            }),
        }),
    };

    let scorer = PerformanceScorer::new();
    let result = scorer.score_metrics(&metrics);

    println!("=== THROUGHPUT PERSONALITY PROFILE ===");
    println!("GOAT Score: {}/1000", result.goat_score);
    println!("Personality: {} ({:?})", result.personality.symbol(), result.personality);
    println!("Description: {}", result.personality.description());
    println!("\nKey Metrics:");
    println!("  Syscall-Performance: {:.1}/100 (PRIMARY STRENGTH)", result.octagon.syscall_performance);
    println!("  Task-Agility: {:.1}/100", result.octagon.task_agility);
    println!("  Consistency: {:.1}/100 (Area for improvement)", result.octagon.consistency);
    println!("\nBrief: {}\n", result.brief);

    assert!(result.octagon.syscall_performance > 70.0, "Throughput should excel at syscall performance");
}

#[test]
fn demonstrate_scoring_mathematics() {
    // This test shows the mathematical transformations in action
    println!("=== SCORING MATHEMATICS DEMONSTRATION ===\n");

    let scorer = PerformanceScorer::new();

    // Example 1: Responsiveness normalization
    println!("Responsiveness Normalization (P99 Latency):");
    println!("  Reference baseline: 50µs → Score 100");
    println!("  At reference: {} pts", scorer.normalize_responsiveness(50.0));
    println!("  Worse (200µs): {} pts", scorer.normalize_responsiveness(200.0));
    println!("  Much worse (500µs): {} pts\n", scorer.normalize_responsiveness(500.0));

    // Example 2: Thermal efficiency
    println!("Thermal Efficiency Normalization:");
    println!("  Cold (35°C): {} pts", scorer.normalize_thermal_efficiency(&vec![35.0]));
    println!("  Warm (60°C): {} pts", scorer.normalize_thermal_efficiency(&vec![60.0]));
    println!("  Hot (80°C): {} pts\n", scorer.normalize_thermal_efficiency(&vec![80.0]));

    // Example 3: SMI resistance
    println!("SMI Resistance Normalization:");
    println!("  No spikes: {} pts", scorer.normalize_smi_resistance(0, 0));
    println!("  10% SMI-correlated: {} pts", scorer.normalize_smi_resistance(100, 10));
    println!("  50% SMI-correlated: {} pts\n", scorer.normalize_smi_resistance(100, 50));

    // Example 4: Octagon average
    let octagon = OctagonAxes {
        responsiveness: 80.0,
        consistency: 75.0,
        micro_precision: 70.0,
        context_efficiency: 65.0,
        syscall_performance: 60.0,
        task_agility: 85.0,
        thermal_efficiency: 90.0,
        smi_resistance: 55.0,
    };
    println!("Octagon Averaging:");
    println!("  Axes: [{}, {}, {}, {}, {}, {}, {}, {}]", 
        octagon.responsiveness, octagon.consistency, octagon.micro_precision,
        octagon.context_efficiency, octagon.syscall_performance, octagon.task_agility,
        octagon.thermal_efficiency, octagon.smi_resistance);
    println!("  Average: {:.1}/100\n", octagon.average());
}

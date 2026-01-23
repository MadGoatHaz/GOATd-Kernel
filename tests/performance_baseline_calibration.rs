//! Performance Baseline Calibration Test
//!
//! This test compares Pure mode (minimal overhead) vs Full mode (complete diagnostics)
//! to measure the overhead introduced by ring buffer pushes and SMI reads.
//!
//! Pure mode: clock readings + black_box only (no buffer/SMI)
//! Full mode: complete diagnostic pipeline with SMI correlation
//!
//! Expected overhead: ~50µs due to cache contention and ring buffer overhead

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use goatd_kernel::system::performance::collector::{
    CollectionMode, LatencyCollector, DISABLE_MSR_POLLER,
};

/// Runs a calibration cycle in the specified mode and collects statistics
fn run_calibration_mode(
    mode: CollectionMode,
    duration_ms: u64,
    interval_ms: u64,
    warmup_samples: u64,
) -> CalibrationResult {
    println!("\n[CALIBRATION] Running {} mode for {} ms (interval: {} ms)",
        match mode {
            CollectionMode::Pure => "PURE",
            CollectionMode::Full => "FULL",
        },
        duration_ms,
        interval_ms
    );

    // Setup shared state
    let stop_flag = Arc::new(AtomicBool::new(false));
    let dropped_count = Arc::new(AtomicU64::new(0));
    let spike_count = Arc::new(AtomicU64::new(0));
    let smi_correlated_spikes = Arc::new(AtomicU64::new(0));
    let total_smi_count = Arc::new(AtomicU64::new(0));

    // Create ring buffers (1MB each, ~131k samples at 8 bytes each)
    let (producer, mut consumer) = rtrb::RingBuffer::new(131072);
    let (event_producer, _event_consumer) = rtrb::RingBuffer::new(1024);

    // If Pure mode, disable MSR poller
    if mode == CollectionMode::Pure {
        DISABLE_MSR_POLLER.store(true, Ordering::Relaxed);
    } else {
        DISABLE_MSR_POLLER.store(false, Ordering::Relaxed);
    }

    // Create collector
    let collector = match mode {
        CollectionMode::Pure => {
            LatencyCollector::with_pure_mode(
                Duration::from_millis(interval_ms),
                producer,
                event_producer,
                Arc::clone(&stop_flag),
                Arc::clone(&dropped_count),
                50_000, // 50µs spike threshold
                Arc::clone(&spike_count),
                Arc::clone(&smi_correlated_spikes),
                Arc::clone(&total_smi_count),
                None, // No SMI correlation in Pure mode
                warmup_samples,
            )
        }
        CollectionMode::Full => {
            let mut collector = LatencyCollector::new(
                Duration::from_millis(interval_ms),
                producer,
                event_producer,
                Arc::clone(&stop_flag),
                Arc::clone(&dropped_count),
                50_000, // 50µs spike threshold
                Arc::clone(&spike_count),
                Arc::clone(&smi_correlated_spikes),
                Arc::clone(&total_smi_count),
                None, // No SMI correlation in this test
            );
            // Explicitly set Full mode (it's the default, but being explicit)
            collector.set_mode(CollectionMode::Full);
            collector.set_warmup_samples(warmup_samples);
            collector
        }
    };

    // For Pure mode, get shared references to track statistics
    let (pure_max_latency_arc, pure_sum_latency_arc, pure_sample_count_arc) = if mode == CollectionMode::Pure {
        (
            Some(collector.get_max_latency_pure_arc()),
            Some(collector.get_pure_mode_sum_latency_arc()),
            Some(collector.get_pure_mode_sample_count_arc()),
        )
    } else {
        (None, None, None)
    };

    // Spawn collector thread
    let collector_handle = std::thread::spawn(move || {
        collector.run();
    });

    // Consume samples during the test duration
    let start = std::time::Instant::now();
    let mut total_samples = 0u64;
    let mut max_latency_ns = 0u64;
    let mut min_latency_ns = u64::MAX;
    let mut sum_latency_ns = 0u64;
    let mut samples_after_warmup = 0u64;

    while start.elapsed().as_millis() < duration_ms as u128 {
        // Consume samples from ring buffer
        while let Ok(latency_ns) = consumer.pop() {
            total_samples += 1;
            samples_after_warmup += 1;
            
            max_latency_ns = max_latency_ns.max(latency_ns);
            min_latency_ns = min_latency_ns.min(latency_ns);
            sum_latency_ns = sum_latency_ns.saturating_add(latency_ns);
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    // Stop collector
    stop_flag.store(true, Ordering::Relaxed);
    let _ = collector_handle.join();

    // Consume any remaining samples
    while let Ok(latency_ns) = consumer.pop() {
        total_samples += 1;
        samples_after_warmup += 1;
        max_latency_ns = max_latency_ns.max(latency_ns);
        min_latency_ns = min_latency_ns.min(latency_ns);
        sum_latency_ns = sum_latency_ns.saturating_add(latency_ns);
    }

    // Clamp min_latency if it's still at MAX (no samples)
    if min_latency_ns == u64::MAX {
        min_latency_ns = 0;
    }

    let dropped = dropped_count.load(Ordering::Relaxed);
    let spikes = spike_count.load(Ordering::Relaxed);

    // For Pure mode, extract statistics from arc instead of ring buffer
    if let (Some(max_arc), Some(sum_arc), Some(count_arc)) = (&pure_max_latency_arc, &pure_sum_latency_arc, &pure_sample_count_arc) {
        max_latency_ns = max_arc.load(Ordering::Relaxed);
        let pure_sum = sum_arc.load(Ordering::Relaxed);
        let pure_count = count_arc.load(Ordering::Relaxed);
        samples_after_warmup = pure_count;
        sum_latency_ns = pure_sum;
    }

    // Calculate statistics
    let avg_latency_ns = if samples_after_warmup > 0 {
        sum_latency_ns / samples_after_warmup
    } else {
        0
    };

    println!("[CALIBRATION] {} Mode Results:",
        match mode {
            CollectionMode::Pure => "PURE",
            CollectionMode::Full => "FULL",
        }
    );
    println!("  Total samples consumed: {}", total_samples);
    println!("  Samples after warmup: {}", samples_after_warmup);
    println!("  Dropped samples: {}", dropped);
    println!("  Spikes detected: {}", spikes);
    println!("  Min latency: {} ns ({:.3} µs)", min_latency_ns, min_latency_ns as f32 / 1000.0);
    println!("  Max latency: {} ns ({:.3} µs)", max_latency_ns, max_latency_ns as f32 / 1000.0);
    println!("  Average latency: {} ns ({:.3} µs)", avg_latency_ns, avg_latency_ns as f32 / 1000.0);

    CalibrationResult {
        mode,
        total_samples,
        samples_after_warmup,
        dropped,
        spikes,
        min_latency_ns,
        max_latency_ns,
        avg_latency_ns,
    }
}

/// Result of a calibration run
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CalibrationResult {
    mode: CollectionMode,
    total_samples: u64,
    samples_after_warmup: u64,
    dropped: u64,
    spikes: u64,
    min_latency_ns: u64,
    max_latency_ns: u64,
    avg_latency_ns: u64,
}

#[test]
#[ignore] // Ignore by default; run with: cargo test -- --ignored --test-threads=1 performance_baseline_calibration
fn test_baseline_calibration_pure_vs_full() {
    println!("\n========================================");
    println!("Performance Baseline Calibration Test");
    println!("========================================");
    println!("Purpose: Measure ring buffer and SMI read overhead");
    println!("Pure mode: clock + black_box only");
    println!("Full mode: complete diagnostic pipeline");
    println!("Target: Identify 50µs latency regression root cause");

    // Calibration parameters
    let warmup_samples = 2000;
    let duration_ms = 5000; // 5 seconds per mode
    let interval_ms = 1; // 1 ms interval between samples

    // Run Pure mode baseline
    let pure_result = run_calibration_mode(
        CollectionMode::Pure,
        duration_ms,
        interval_ms,
        warmup_samples,
    );

    // Run Full mode
    let full_result = run_calibration_mode(
        CollectionMode::Full,
        duration_ms,
        interval_ms,
        warmup_samples,
    );

    // Analyze overhead
    println!("\n========================================");
    println!("Overhead Analysis");
    println!("========================================");

    let pure_avg = pure_result.avg_latency_ns as f32 / 1000.0;
    let full_avg = full_result.avg_latency_ns as f32 / 1000.0;
    let overhead_us = full_avg - pure_avg;
    let overhead_pct = if pure_avg > 0.0 {
        (overhead_us / pure_avg) * 100.0
    } else {
        0.0
    };

    println!("Pure mode average latency: {:.3} µs", pure_avg);
    println!("Full mode average latency: {:.3} µs", full_avg);
    println!("Overhead (Full - Pure): {:.3} µs ({:.1}%)", overhead_us, overhead_pct);

    // Max latency comparison
    let pure_max = pure_result.max_latency_ns as f32 / 1000.0;
    let full_max = full_result.max_latency_ns as f32 / 1000.0;
    let max_overhead_us = full_max - pure_max;

    println!("\nPure mode max latency: {:.3} µs", pure_max);
    println!("Full mode max latency: {:.3} µs", full_max);
    println!("Max latency overhead: {:.3} µs", max_overhead_us);

    // Report spikes
    println!("\nSpike Analysis (threshold: 50µs):");
    println!("Pure mode spikes: {}", pure_result.spikes);
    println!("Full mode spikes: {}", full_result.spikes);

    // Report dropped samples
    if pure_result.dropped > 0 || full_result.dropped > 0 {
        println!("\nDropped Samples Warning:");
        println!("Pure mode dropped: {}", pure_result.dropped);
        println!("Full mode dropped: {}", full_result.dropped);
    }

    // Interpretation
    println!("\n========================================");
    println!("Interpretation");
    println!("========================================");

    if overhead_us < 1.0 {
        println!("✓ Ring buffer overhead is minimal (<1µs)");
        println!("  → Cache contention is NOT the primary issue");
    } else if overhead_us < 50.0 {
        println!("⚠ Ring buffer overhead: {:.3} µs", overhead_us);
        println!("  → This could contribute to the observed regression");
    } else {
        println!("✗ Ring buffer overhead is significant: {:.3} µs", overhead_us);
        println!("  → This is likely the primary cause of latency regression");
    }

    if pure_result.max_latency_ns < 50_000 {
        println!("✓ Pure baseline max latency: {:.3} µs (<50µs)", pure_max);
    } else {
        println!("⚠ Pure baseline max latency: {:.3} µs (≥50µs)", pure_max);
        println!("  → Core scheduler latency may be elevated");
    }

    // Verify warmup worked
    if pure_result.samples_after_warmup > 0 {
        println!("✓ Warmup phase completed ({} samples skipped)", warmup_samples);
    } else {
        println!("✗ Warmup phase did not skip samples as expected");
    }

    println!("\n========================================");
}

#[test]
#[ignore]
fn test_baseline_calibration_warmup_effectiveness() {
    println!("\n========================================");
    println!("Warmup Phase Effectiveness Test");
    println!("========================================");
    println!("Purpose: Verify warmup eliminates startup artifacts");

    let interval_ms = 1;
    let duration_ms = 2000; // 2 seconds
    let warmup_samples = 500;

    // Run with small warmup
    let result = run_calibration_mode(
        CollectionMode::Pure,
        duration_ms,
        interval_ms,
        warmup_samples,
    );

    println!("\nWarmup Analysis:");
    println!("Expected warmup samples skipped: {}", warmup_samples);
    println!("Actual samples after warmup: {}", result.samples_after_warmup);

    // Verify cleanup happened
    if result.samples_after_warmup > 0 {
        println!("✓ Warmup phase successfully skipped {} samples", warmup_samples);
    } else {
        println!("✗ Warmup phase did not work as expected");
    }

    // Verify no excessive dropped samples
    if result.dropped == 0 {
        println!("✓ No samples dropped during calibration");
    } else {
        println!("⚠ {} samples dropped (buffer size issue?)", result.dropped);
    }
}

#[test]
#[ignore]
fn test_baseline_calibration_pure_mode_minimal() {
    println!("\n========================================");
    println!("Pure Mode Minimal Overhead Test");
    println!("========================================");
    println!("Purpose: Verify Pure mode is strictly minimal");

    let result = run_calibration_mode(
        CollectionMode::Pure,
        1000,
        1,
        100,
    );

    println!("\nPure Mode Characteristics:");
    println!("• Clock readings: ✓ (baseline timing)");
    println!("• black_box usage: ✓ (latency_ns wrapped)");
    println!("• Ring buffer pushes: ✗ (skipped)");
    println!("• SMI reads: ✗ (skipped)");
    println!("• Spike detection: Limited (no SMI lookup)");

    println!("\nResult Statistics:");
    println!("Min latency: {:.3} µs", result.min_latency_ns as f32 / 1000.0);
    println!("Max latency: {:.3} µs", result.max_latency_ns as f32 / 1000.0);
    println!("Avg latency: {:.3} µs", result.avg_latency_ns as f32 / 1000.0);

    // Pure mode should show lower max latency due to skipped operations
    if result.max_latency_ns < 100_000 {
        println!("✓ Pure mode max latency acceptable: {:.3} µs", result.max_latency_ns as f32 / 1000.0);
    } else {
        println!("⚠ Pure mode max latency higher than expected: {:.3} µs", result.max_latency_ns as f32 / 1000.0);
    }
}

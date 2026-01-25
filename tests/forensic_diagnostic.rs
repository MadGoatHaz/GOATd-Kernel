//! Forensic Diagnostic Test Suite
//!
//! This test module provides a standalone diagnostic harness for analyzing raw performance
//! collector data without UI interference. It's designed to validate that performance metrics
//! are trustworthy and not inflated by measurement artifacts.
//!
//! The test runs a full 10-second collection cycle and dumps nanosecond-precision raw data
//! for forensic analysis and comparison against system baseline expectations.

#[cfg(test)]
mod forensic_diagnostic {
    use std::thread;
    use std::time::{Duration, Instant};

    // Re-export collector and processor from the main crate
    use goatd_kernel::system::performance::collector::LatencyProcessor;
    use goatd_kernel::system::performance::{
        diagnostic_buffer, LatencyCollector, MonitoringState, SyscallSaturationCollector,
        SyscallSaturationConfig,
    };

    /// Forensic Test: Raw Latency Data Collection and Analysis
    ///
    /// This test:
    /// 1. Initializes a lock-free ring buffer for latency samples
    /// 2. Runs the LatencyCollector for 10 seconds at 1ms intervals
    /// 3. Captures nanosecond-precision samples without any filtering
    /// 4. Processes raw data to compute statistical metrics
    /// 5. Prints detailed results including percentiles, spikes, and SMI correlation
    ///
    /// Success Criteria:
    /// - Collects at least 9,000 samples in 10 seconds (accounting for timing jitter)
    /// - Max latency is reasonable for an idle system (< 1ms expected, < 10ms for loaded system)
    /// - P99.9 latency indicates tail percentile behavior
    /// - Spike count and SMI correlation provide diagnostic information
    #[test]
    fn test_forensic_raw_latency_collection() {
        let separator =
            "================================================================================";
        eprintln!("\n{}", separator);
        eprintln!("FORENSIC DIAGNOSTIC TEST: Raw Latency Data Collection");
        eprintln!("{}", separator);

        // ========================================================================
        // Initialize Ring Buffer and Monitoring State
        // ========================================================================
        eprintln!("\n[SETUP] Initializing lock-free ring buffer and monitoring state...");

        // Create a ring buffer with capacity for ~12,000 samples (10 sec + overhead)
        // At 1ms interval = 10,000 samples + buffer for timing variation
        let ring_buffer_capacity = 12_000;
        let (producer, mut consumer) = rtrb::RingBuffer::new(ring_buffer_capacity);

        // Create event ring buffer for diagnostic events
        let (event_producer, event_consumer) = rtrb::RingBuffer::new(1000);

        // Initialize monitoring state (atomic counters)
        let monitoring_state = MonitoringState::default();

        eprintln!(
            "[SETUP] Ring buffer created: capacity = {} samples",
            ring_buffer_capacity
        );

        // ========================================================================
        // Spawn Latency Collector Thread
        // ========================================================================
        eprintln!("[SETUP] Spawning latency collector thread...");

        let collector_state = monitoring_state.clone();

        // Initialize global diagnostic buffer for test visibility
        diagnostic_buffer::init_global_buffer(1024);

        // Spawn the event consumer to process SMI correlation and logs asynchronously
        let _consumer_handle = diagnostic_buffer::spawn_collector_event_consumer(
            event_consumer,
            collector_state.smi_correlated_spikes.clone(),
        );

        let collector_thread = thread::spawn(move || {
            let collector = LatencyCollector::new(
                Duration::from_millis(1), // 1ms interval
                producer,
                event_producer,
                collector_state.stop_flag.clone(),
                collector_state.dropped_samples.clone(),
                500_000, // 500µs spike threshold
                collector_state.spike_count.clone(),
                collector_state.smi_correlated_spikes.clone(),
                collector_state.total_smi_count.clone(),
                None, // SMI polling still happens background
            );
            collector.run();
        });

        // ========================================================================
        // Collection Phase: 10 seconds
        // ========================================================================
        eprintln!("[COLLECTION] Starting 10-second collection cycle...");
        let collection_start = Instant::now();
        let collection_duration = Duration::from_secs(10);

        // Collect samples from the ring buffer as they arrive
        let mut raw_samples: Vec<u64> = Vec::new();
        let mut status_interval = 0u64;

        while collection_start.elapsed() < collection_duration {
            // Try to read available samples from the ring buffer
            while let Ok(sample_ns) = consumer.pop() {
                raw_samples.push(sample_ns);
            }

            // Print progress every second
            let elapsed_secs = collection_start.elapsed().as_secs();
            if elapsed_secs > status_interval {
                status_interval = elapsed_secs;
                eprintln!(
                    "[COLLECTION] t={}s | Samples collected: {} | Dropped: {}",
                    elapsed_secs,
                    raw_samples.len(),
                    monitoring_state.dropped_count()
                );
            }

            // Sleep briefly to avoid busy-waiting
            thread::sleep(Duration::from_millis(10));
        }

        // Signal collector to stop and collect any remaining samples
        monitoring_state.request_stop();

        eprintln!("[COLLECTION] Requested stop. Collecting remaining buffered samples...");
        thread::sleep(Duration::from_millis(100)); // Give collector time to finish

        // Drain any remaining samples
        while let Ok(sample_ns) = consumer.pop() {
            raw_samples.push(sample_ns);
        }

        eprintln!(
            "[COLLECTION] Collection complete. Total samples collected: {}",
            raw_samples.len()
        );

        // Wait for the collector thread to terminate
        collector_thread.join().expect("Collector thread panicked");

        // [DRAIN] Wait for the asynchronous event consumer to process all final events
        eprintln!("[DRAIN] Waiting for event consumer to process final samples...");
        thread::sleep(Duration::from_millis(500)); // Generous sleep to ensure consumer is caught up

        // ========================================================================
        // Raw Data Analysis
        // ========================================================================
        eprintln!(
            "\n[ANALYSIS] Processing {} raw nanosecond samples...",
            raw_samples.len()
        );

        // Initialize latency processor
        let mut processor = LatencyProcessor::new().expect("Failed to initialize LatencyProcessor");

        // Record all samples into the processor
        let mut analysis_errors = 0u64;
        for (idx, &sample_ns) in raw_samples.iter().enumerate() {
            if let Err(_e) = processor.record_sample(sample_ns) {
                analysis_errors += 1;
                if analysis_errors <= 5 {
                    eprintln!(
                        "[ANALYSIS] Warning: Failed to record sample #{}: {} ns",
                        idx, sample_ns
                    );
                }
            }
        }

        if analysis_errors > 0 {
            eprintln!(
                "[ANALYSIS] Total record errors: {} (out of {} samples)",
                analysis_errors,
                raw_samples.len()
            );
        }

        // ========================================================================
        // Statistical Computation
        // ========================================================================
        eprintln!("\n[STATISTICS] Computing metrics from raw samples...");

        // Basic statistics
        let min_ns = raw_samples.iter().copied().min().unwrap_or(0);
        let max_ns = raw_samples.iter().copied().max().unwrap_or(0);
        let avg_ns = if !raw_samples.is_empty() {
            raw_samples.iter().sum::<u64>() / raw_samples.len() as u64
        } else {
            0
        };

        // Percentile computation
        let p99_us = processor.p99();
        let p99_9_us = processor.p99_9();
        let max_us = processor.max();
        let avg_us = processor.average();

        // Jitter analysis: compute rolling standard deviation
        let jitter_us = compute_rolling_jitter(&raw_samples[..raw_samples.len().min(1000)]);

        // Spike analysis
        let spike_count = monitoring_state.spike_count();
        let smi_correlated_spikes = monitoring_state.smi_correlated_count();
        let dropped_count = monitoring_state.dropped_count();

        // Throughput estimate: samples per second (1ms interval = 1000 samples/sec ideal)
        let estimated_throughput = if raw_samples.len() > 0 {
            (raw_samples.len() as f64 / 10.0) as u64 // x samples per second
        } else {
            0
        };

        // ========================================================================
        // Raw Data Dump (Forensic Output)
        // ========================================================================
        eprintln!("\n{}", separator);
        eprintln!("FORENSIC DATA DUMP: Raw Nanosecond Samples");
        eprintln!("{}", separator);
        eprintln!(
            "Total Samples: {} (expected ~10,000 at 1ms interval)",
            raw_samples.len()
        );
        eprintln!("Dropped Samples: {}", dropped_count);
        eprintln!(
            "Dropped Rate: {:.2}%",
            if raw_samples.len() > 0 {
                (dropped_count as f64 / (raw_samples.len() + dropped_count as usize) as f64) * 100.0
            } else {
                0.0
            }
        );
        eprintln!();
        eprintln!("RAW SAMPLE STATISTICS (nanoseconds):");
        eprintln!(
            "  Min:      {} ns ({:.3} µs)",
            min_ns,
            min_ns as f64 / 1000.0
        );
        eprintln!(
            "  Max:      {} ns ({:.3} µs)",
            max_ns,
            max_ns as f64 / 1000.0
        );
        eprintln!(
            "  Average:  {} ns ({:.3} µs)",
            avg_ns,
            avg_ns as f64 / 1000.0
        );
        eprintln!();
        eprintln!("PROCESSED STATISTICS (via LatencyProcessor):");
        eprintln!("  Min:      {:.3} µs", min_ns as f64 / 1000.0);
        eprintln!("  Max:      {:.3} µs (raw: {} ns)", max_us, max_ns);
        eprintln!("  Average:  {:.3} µs", avg_us);
        eprintln!("  P99:      {:.3} µs", p99_us);
        eprintln!("  P99.9:    {:.3} µs", p99_9_us);
        eprintln!();
        eprintln!("JITTER ANALYSIS:");
        eprintln!("  Rolling Jitter (1000-sample window): {:.3} µs", jitter_us);
        eprintln!("  Jitter Severity: {}", classify_jitter(jitter_us));
        eprintln!();
        eprintln!("SPIKE DETECTION:");
        eprintln!("  Total Spikes (>500µs): {}", spike_count);
        eprintln!("  SMI-Correlated Spikes:  {}", smi_correlated_spikes);
        eprintln!(
            "  Spike Rate: {:.2}%",
            if raw_samples.len() > 0 {
                (spike_count as f64 / raw_samples.len() as f64) * 100.0
            } else {
                0.0
            }
        );
        eprintln!();
        eprintln!("THROUGHPUT:");
        eprintln!(
            "  Estimated: {} samples/sec (~{} Hz)",
            estimated_throughput,
            estimated_throughput / 1000
        );
        eprintln!("  Expected:  1000 samples/sec (1 kHz at 1ms interval)");
        eprintln!(
            "  Loss:      {:.1}%",
            if estimated_throughput > 0 {
                let loss = 1000u64.saturating_sub(estimated_throughput) as f64;
                (loss / 1000.0) * 100.0
            } else {
                100.0
            }
        );

        // ========================================================================
        // Forensic Histogram
        // ========================================================================
        eprintln!("\n[HISTOGRAM] Distribution of samples across latency ranges:");
        print_latency_histogram(&raw_samples);

        // ========================================================================
        // Validation Checks
        // ========================================================================
        eprintln!("\n{}", separator);
        eprintln!("FORENSIC VALIDATION CHECKS");
        eprintln!("{}", separator);

        // Check 1: Sample count adequacy
        eprintln!("\n[CHECK 1] Sample Count Adequacy");
        let expected_min_samples = 9000; // Allowing for some timing jitter
        if raw_samples.len() >= expected_min_samples {
            eprintln!(
                "  ✓ PASS: Collected {} samples (expected >= {})",
                raw_samples.len(),
                expected_min_samples
            );
        } else {
            eprintln!(
                "  ✗ FAIL: Collected {} samples (expected >= {})",
                raw_samples.len(),
                expected_min_samples
            );
        }

        // Check 2: Latency sanity (idle system should have low latency)
        eprintln!("\n[CHECK 2] Latency Sanity Check (Idle System)");
        if max_us < 10_000.0 {
            // 10ms threshold for idle system
            eprintln!(
                "  ✓ PASS: Max latency {:.3} µs is reasonable for idle system",
                max_us
            );
        } else {
            eprintln!(
                "  ⚠ WARNING: Max latency {:.3} µs seems high for idle system",
                max_us
            );
        }

        // Check 3: P99.9 is above P99 (monotonic)
        eprintln!("\n[CHECK 3] Percentile Monotonicity");
        if p99_9_us >= p99_us {
            eprintln!(
                "  ✓ PASS: P99.9 ({:.3} µs) >= P99 ({:.3} µs)",
                p99_9_us, p99_us
            );
        } else {
            eprintln!(
                "  ✗ FAIL: P99.9 ({:.3} µs) < P99 ({:.3} µs) - percentiles inverted!",
                p99_9_us, p99_us
            );
        }

        // Check 4: No excessive droppage
        eprintln!("\n[CHECK 4] Buffer Droppage Check");
        let drop_rate = if raw_samples.len() + dropped_count as usize > 0 {
            (dropped_count as f64 / (raw_samples.len() + dropped_count as usize) as f64) * 100.0
        } else {
            0.0
        };

        if drop_rate < 5.0 {
            eprintln!(
                "  ✓ PASS: Drop rate {:.2}% is acceptable (threshold: 5%)",
                drop_rate
            );
        } else {
            eprintln!(
                "  ⚠ WARNING: Drop rate {:.2}% exceeds acceptable threshold (5%)",
                drop_rate
            );
        }

        // Check 5: SMI correlation reasonable
        eprintln!("\n[CHECK 5] SMI Correlation Sanity");
        if smi_correlated_spikes <= spike_count {
            eprintln!(
                "  ✓ PASS: SMI-correlated ({}) <= Total spikes ({})",
                smi_correlated_spikes, spike_count
            );
        } else {
            eprintln!(
                "  ✗ FAIL: SMI-correlated ({}) > Total spikes ({}) - logical inconsistency!",
                smi_correlated_spikes, spike_count
            );
        }

        eprintln!();
        eprintln!("{}", separator);
        eprintln!("END OF FORENSIC DIAGNOSTIC REPORT");
        eprintln!("{}", separator);
        eprintln!();
        eprintln!("INTERPRETATION GUIDE:");
        eprintln!("1. If Max latency is in µs range (< 1000 µs), the system is responsive");
        eprintln!("2. If P99.9 is significantly higher than P99, tail latency is concerning");
        eprintln!("3. If Spike count is high (>1% of samples), consider tuning spike_threshold");
        eprintln!("4. If Drop rate is >5%, the ring buffer may be too small or samples too fast");
        eprintln!("5. If SMI-correlated is 0%, SMI detection may not be available on this CPU");
        eprintln!();

        // Basic assertion to ensure test collected data
        assert!(
            !raw_samples.is_empty(),
            "Forensic test must collect at least one sample"
        );
    }

    /// Test: SyscallSaturationCollector Isolation
    ///
    /// This test runs the SyscallSaturationCollector separately to diagnose if it's
    /// contributing to latency measurements or causing interference.
    #[test]
    fn test_forensic_syscall_saturation_isolation() {
        let separator =
            "================================================================================";
        eprintln!("\n{}", separator);
        eprintln!("FORENSIC DIAGNOSTIC TEST: Syscall Saturation Collector Isolation");
        eprintln!("{}", separator);

        eprintln!("\n[SETUP] Initializing SyscallSaturationCollector...");

        let config = SyscallSaturationConfig {
            iterations: 10_000, // 10k getpid calls per run
            runs: 3,            // 3 runs for quick diagnostic
        };

        eprintln!(
            "[SETUP] Configuration: iterations={}, runs={}",
            config.iterations, config.runs
        );

        let collector = SyscallSaturationCollector::new(config.clone());

        eprintln!("[COLLECTION] Starting syscall saturation collection...");
        let start = Instant::now();

        match collector.run() {
            Ok(metrics) => {
                let elapsed = start.elapsed();
                eprintln!(
                    "[COLLECTION] Complete. Elapsed: {:.2}s",
                    elapsed.as_secs_f64()
                );

                eprintln!("\n[METRICS] Syscall Saturation Results:");
                eprintln!("  Avg ns/call:  {:.3} ns", metrics.avg_ns_per_call);
                eprintln!("  Min ns/call:  {:.3} ns", metrics.min_ns_per_call);
                eprintln!("  Max ns/call:  {:.3} ns", metrics.max_ns_per_call);
                eprintln!("  Total calls:  {}", metrics.total_syscalls);
                eprintln!("  Throughput:   {} calls/sec", metrics.calls_per_second);

                eprintln!("\n[ANALYSIS] Syscall overhead assessment:");
                if metrics.avg_ns_per_call < 1_000.0 {
                    // < 1µs
                    eprintln!("  ✓ GOOD: Syscall overhead is low (< 1µs)");
                } else if metrics.avg_ns_per_call < 10_000.0 {
                    // < 10µs
                    eprintln!(
                        "  ⚠ WARNING: Syscall overhead is moderate ({:.3} µs)",
                        metrics.avg_ns_per_call / 1000.0
                    );
                } else {
                    eprintln!(
                        "  ✗ CONCERN: Syscall overhead is high ({:.3} µs)",
                        metrics.avg_ns_per_call / 1000.0
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "[ERROR] Failed to collect syscall saturation metrics: {}",
                    e
                );
            }
        }

        eprintln!();
        eprintln!("{}", separator);
        eprintln!("END OF SYSCALL SATURATION ISOLATION TEST");
        eprintln!("{}", separator);
        eprintln!();
    }

    /// Helper: Compute rolling jitter from samples
    /// Jitter is the variation in latency between consecutive samples
    fn compute_rolling_jitter(samples: &[u64]) -> f32 {
        if samples.len() < 2 {
            return 0.0;
        }

        let mut deltas = Vec::with_capacity(samples.len() - 1);
        for i in 1..samples.len() {
            let delta = if samples[i] > samples[i - 1] {
                samples[i] - samples[i - 1]
            } else {
                samples[i - 1] - samples[i]
            };
            deltas.push(delta as f32 / 1000.0); // Convert to µs
        }

        // Compute standard deviation of deltas
        let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
        let variance =
            deltas.iter().map(|&d| (d - mean).powi(2)).sum::<f32>() / deltas.len() as f32;

        variance.sqrt()
    }

    /// Helper: Classify jitter severity
    fn classify_jitter(jitter_us: f32) -> &'static str {
        match jitter_us {
            j if j < 1.0 => "Excellent (< 1 µs)",
            j if j < 5.0 => "Good (1-5 µs)",
            j if j < 10.0 => "Acceptable (5-10 µs)",
            j if j < 50.0 => "Concerning (10-50 µs)",
            _ => "Critical (> 50 µs)",
        }
    }

    /// Helper: Print latency histogram
    fn print_latency_histogram(samples: &[u64]) {
        if samples.is_empty() {
            eprintln!("  (no samples to histogram)");
            return;
        }

        // Create 10 buckets from min to max
        let min = samples.iter().copied().min().unwrap_or(0);
        let max = samples.iter().copied().max().unwrap_or(0);

        if max == min || samples.is_empty() {
            eprintln!("  (all samples equal: {} ns)", min);
            return;
        }

        // Use saturating_sub to safely compute range, then divide to get bucket width
        let range = max.saturating_sub(min);
        // Ensure bucket_width is at least 1 to avoid divide-by-zero
        let bucket_width = (range / 10).max(1);
        let mut buckets = vec![0u64; 10];

        for &sample in samples {
            let bucket_idx = if sample >= max {
                9
            } else {
                let adjusted_sample = sample.saturating_sub(min);
                (((adjusted_sample / bucket_width).min(9)) as usize).min(9)
            };
            buckets[bucket_idx] += 1;
        }

        let max_bucket_count = buckets.iter().max().copied().unwrap_or(1);

        for (i, &count) in buckets.iter().enumerate() {
            let lower_ns = min.saturating_add((i as u64).saturating_mul(bucket_width));
            let upper_ns = min.saturating_add(((i + 1) as u64).saturating_mul(bucket_width));
            let bar_len = (count as f64 / max_bucket_count as f64 * 40.0) as usize;
            let bar = "█".repeat(bar_len);

            eprintln!(
                "  [{:5.0}-{:5.0} µs] {} ({} samples)",
                lower_ns as f64 / 1000.0,
                upper_ns as f64 / 1000.0,
                bar,
                count
            );
        }
    }
}

//! Standalone Latency Sanity Test
//!
//! This test measures raw kernel scheduling latency without UI or ring buffer interference.
//! It validates the accuracy of our latency measurement system by using the same primitives
//! as the main collector but in isolation.
//!
//! Usage: cargo run --bin latency_test
//!
//! Configuration:
//! - Duration: 10 seconds
//! - Frequency: 1000 Hz (1ms intervals)
//! - Sleep method: TIMER_ABSTIME (absolute wake-up timing)
//! - Measurements: Min, Max, Average, P99 latency

/// Helper: Add nanoseconds to a timespec.
/// Handles overflow correctly for multi-second additions.
fn add_ns_to_timespec(ts: &mut libc::timespec, ns: u64) {
    let new_nsec = ts.tv_nsec as u64 + ns;
    ts.tv_sec += (new_nsec / 1_000_000_000) as libc::time_t;
    ts.tv_nsec = (new_nsec % 1_000_000_000) as libc::c_long;
}

/// Helper: Compare two timespec values.
/// Returns: -1 if a < b, 0 if a == b, 1 if a > b
fn timespec_cmp(a: &libc::timespec, b: &libc::timespec) -> i32 {
    if a.tv_sec != b.tv_sec {
        if a.tv_sec < b.tv_sec {
            -1
        } else {
            1
        }
    } else if a.tv_nsec != b.tv_nsec {
        if a.tv_nsec < b.tv_nsec {
            -1
        } else {
            1
        }
    } else {
        0
    }
}

/// Helper: Calculate the difference between two timespec values in nanoseconds.
/// Returns (now - target) as u64. If now is before target (early wake), returns 0.
fn timespec_diff_ns(now: &libc::timespec, target: &libc::timespec) -> u64 {
    let sec_diff = now.tv_sec - target.tv_sec;
    let nsec_diff = now.tv_nsec - target.tv_nsec;

    // Convert to signed i64 to handle negative differences
    let diff_ns = (sec_diff as i64) * 1_000_000_000 + (nsec_diff as i64);

    // Handle early-wake scenarios explicitly
    // If now is before target (negative difference), return 0 for latency safety
    if diff_ns >= 0 {
        diff_ns as u64
    } else {
        0 // Early wake: no latency to report
    }
}

/// Latency statistics calculator
struct LatencyStats {
    samples: Vec<u64>,
    min: u64,
    max: u64,
    sum: u64,
}

impl LatencyStats {
    fn new() -> Self {
        LatencyStats {
            samples: Vec::new(),
            min: u64::MAX,
            max: 0,
            sum: 0,
        }
    }

    fn add(&mut self, latency_ns: u64) {
        self.samples.push(latency_ns);
        if latency_ns < self.min {
            self.min = latency_ns;
        }
        if latency_ns > self.max {
            self.max = latency_ns;
        }
        self.sum += latency_ns;
    }

    fn average(&self) -> u64 {
        if self.samples.is_empty() {
            0
        } else {
            self.sum / self.samples.len() as u64
        }
    }

    fn p99(&self) -> u64 {
        if self.samples.is_empty() {
            0
        } else {
            let mut sorted = self.samples.clone();
            sorted.sort_unstable();
            let idx = (self.samples.len() * 99) / 100;
            sorted[idx.min(sorted.len() - 1)]
        }
    }

    fn count(&self) -> usize {
        self.samples.len()
    }
}

fn main() {
    println!("=== GOATd Kernel Latency Sanity Test ===");
    println!("Duration: 10 seconds");
    println!("Frequency: 1000 Hz (1ms intervals)");
    println!("Method: TIMER_ABSTIME (absolute wake-up timing)");
    println!("Primitives: libc::clock_gettime, libc::clock_nanosleep");
    println!();

    let mut stats = LatencyStats::new();

    // Get the baseline time
    let mut next_wake: libc::timespec = unsafe {
        let mut ts = std::mem::zeroed::<libc::timespec>();
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
        ts
    };

    // Interval: 1ms = 1_000_000 nanoseconds
    let interval_ns = 1_000_000u64;

    // Target duration: 10 seconds
    let target_duration_s = 10i64;
    let target_end_sec = next_wake.tv_sec + target_duration_s;

    let start_time = std::time::Instant::now();

    println!("Test running...");

    // Main measurement loop
    loop {
        // Sleep until the target absolute time
        unsafe {
            libc::clock_nanosleep(
                libc::CLOCK_MONOTONIC,
                libc::TIMER_ABSTIME,
                &next_wake,
                std::ptr::null_mut(),
            );
        }

        // Get the actual wake-up time
        let now: libc::timespec = unsafe {
            let mut ts = std::mem::zeroed::<libc::timespec>();
            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
            ts
        };

        // Calculate latency: actual_wake - target_wake (in nanoseconds)
        let latency_ns = timespec_diff_ns(&now, &next_wake);
        stats.add(latency_ns);

        // Absolute Grid Alignment: always snap to the grid, even with phase-lag
        if timespec_cmp(&now, &next_wake) > 0 {
            // Late wake-up (Phase-Lag): calculate how many intervals to skip to land on the next grid point
            let late_ns = timespec_diff_ns(&now, &next_wake);
            let missed = (late_ns / interval_ns) + 1;
            add_ns_to_timespec(&mut next_wake, missed * interval_ns);
        } else {
            // On time: increment by exactly one interval to maintain the grid
            add_ns_to_timespec(&mut next_wake, interval_ns);
        }

        // Check if we've exceeded the target duration
        if now.tv_sec >= target_end_sec {
            break;
        }
    }

    let elapsed = start_time.elapsed();

    println!();
    println!("=== Test Complete ===");
    println!("Elapsed time: {:.3} seconds", elapsed.as_secs_f64());
    println!();
    println!("=== Latency Statistics ===");
    println!("Samples collected: {}", stats.count());
    println!(
        "Min latency:       {} ns ({:.3} µs)",
        stats.min,
        stats.min as f64 / 1000.0
    );
    println!(
        "Max latency:       {} ns ({:.3} µs)",
        stats.max,
        stats.max as f64 / 1000.0
    );
    println!(
        "Avg latency:       {} ns ({:.3} µs)",
        stats.average(),
        stats.average() as f64 / 1000.0
    );
    println!(
        "P99 latency:       {} ns ({:.3} µs)",
        stats.p99(),
        stats.p99() as f64 / 1000.0
    );
    println!();

    // Additional diagnostics
    println!("=== Diagnostics ===");
    let outliers = stats.samples.iter().filter(|&&s| s > 10_000_000).count();
    println!("Samples > 10ms (outliers): {}", outliers);

    let over_500us = stats.samples.iter().filter(|&&s| s > 500_000).count();
    println!("Samples > 500µs: {}", over_500us);

    let over_100us = stats.samples.iter().filter(|&&s| s > 100_000).count();
    println!("Samples > 100µs: {}", over_100us);

    println!();
    println!("Test completed successfully!");
}

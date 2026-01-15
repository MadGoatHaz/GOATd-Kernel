//! High-Precision Latency Collector
//!
//! Performs allocation-free latency measurement using:
//! - CLOCK_MONOTONIC for timing
//! - TIMER_ABSTIME for absolute wake-up timing
//! - rtrb lock-free ring buffer for communication
//! - Spike detection for latency anomalies
//! - Optional SMI correlation detection
//! - Periodic thermal data collection from sysfs

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use hdrhistogram::Histogram;

use super::SmiCorrelation;
use super::thermal;

/// The Collector performs high-precision latency measurement.
/// It runs an allocation-free hot loop that measures wake-up latency using CLOCK_MONOTONIC.
pub struct LatencyCollector {
    interval: Duration,
    producer: rtrb::Producer<u64>,
    stop_flag: Arc<AtomicBool>,
    dropped_count: Arc<AtomicU64>,
    spike_threshold: u64,
    spike_count: Arc<AtomicU64>,
    smi_correlation: Option<SmiCorrelation>,
    smi_correlated_spikes: Arc<AtomicU64>,
    total_smi_count: Arc<AtomicU64>,
}

impl LatencyCollector {
    /// Creates a new Collector with the given interval and ring buffer producer.
    pub fn new(
        interval: Duration,
        producer: rtrb::Producer<u64>,
        stop_flag: Arc<AtomicBool>,
        dropped_count: Arc<AtomicU64>,
        spike_threshold: u64,
        spike_count: Arc<AtomicU64>,
        smi_correlated_spikes: Arc<AtomicU64>,
        total_smi_count: Arc<AtomicU64>,
        cpu_id: usize,
    ) -> Self {
        // Initialize SmiCorrelation with references to authoritative atomics
        let smi_correlation = SmiCorrelation::new(
            cpu_id,
            Some(total_smi_count.clone()),
            Some(smi_correlated_spikes.clone()),
        );
        LatencyCollector {
            interval,
            producer,
            stop_flag,
            dropped_count,
            spike_threshold,
            spike_count,
            smi_correlation: Some(smi_correlation),
            smi_correlated_spikes,
            total_smi_count,
        }
    }

    /// Runs the measurement hot loop.
    /// This is allocation-free and uses TIMER_ABSTIME for absolute wake-up timing.
    /// Detects spikes (latency > threshold) and correlates with SMI if available.
    /// SMI correlation now updates authoritative atomics directly with 100ms cooldown.
    pub fn run(mut self) {
        eprintln!("[COLLECTOR] Starting hot loop: interval={:?}, spike_threshold={} ns", self.interval, self.spike_threshold);
        
        // Get the baseline time
        let mut next_wake: libc::timespec = unsafe {
            let mut ts = std::mem::zeroed::<libc::timespec>();
            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
            ts
        };

        // Convert interval to nanoseconds for easier manipulation
        let interval_ns = (self.interval.as_secs() as u64) * 1_000_000_000
            + (self.interval.subsec_nanos() as u64);

        let mut sample_count = 0u64;
        let mut dropped_total = 0u64;
        let mut spike_total = 0u64;
        let mut smi_correlated_total = 0u64;

        // Move smi_correlation out of self into a local variable to optimize hot loop
        let mut smi_corr = self.smi_correlation.take();

        // Hot loop - exit on stop_flag OR if producer is disconnected
        while !self.stop_flag.load(Ordering::Relaxed) {
             // Increment next_wake by the interval
             add_ns_to_timespec(&mut next_wake, interval_ns);

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
             sample_count += 1;

             // Detect spikes and correlate with SMI
              if latency_ns > self.spike_threshold {
                  let before_spike = self.spike_count.load(Ordering::Relaxed);
                  self.spike_count.fetch_add(1, Ordering::Release);
                  let after_spike = self.spike_count.load(Ordering::Relaxed);
                  spike_total += 1;

                  // Correlate with SMI if available
                  // record_spike() now:
                  // - implements 100ms cooldown internally
                  // - updates authoritative atomics directly
                  // - returns true if SMI was detected
                  if let Some(ref mut smi_corr_ref) = smi_corr {
                      if smi_corr_ref.record_spike() {
                          smi_correlated_total += 1;
                          let corr_count = self.smi_correlated_spikes.load(Ordering::Relaxed);
                          eprintln!("[COLLECTOR_SMI] SMI-correlated spike recorded! Total SMI-correlated: {} (atomic confirms: {})", smi_correlated_total, corr_count);
                      } else {
                          eprintln!("[COLLECTOR_SMI] Spike detected but cooldown prevented MSR check");
                      }
                  } else {
                      eprintln!("[COLLECTOR_SMI] WARNING: Spike detected but SMI correlation unavailable");
                  }

                  // Log spike events (diagnostic: only if notable)
                  if spike_total % 10 == 1 {
                      eprintln!("[COLLECTOR] Spike detected: latency={} ns (threshold={} ns), spike #{} [spike_count: {}→{}]", latency_ns, self.spike_threshold, spike_total, before_spike, after_spike);
                  }
              }

             // Try to push to the producer. If full, increment dropped counter.
             match self.producer.push(latency_ns) {
                 Ok(_) => {
                     // Successfully pushed
                 }
                 Err(_push_err) => {
                     // Buffer is full, increment dropped count
                     self.dropped_count.fetch_add(1, Ordering::Relaxed);
                     dropped_total += 1;
                     
                     if dropped_total % 100 == 1 {
                         eprintln!("[COLLECTOR] ⚠ Ring buffer full! Dropped sample #{}, total dropped: {}", dropped_total, dropped_total);
                     }
                 }
             }

             // Periodic status log every 1000 samples (~1 second at 1ms interval)
             if sample_count % 1000 == 0 {
                 eprintln!("[COLLECTOR] Samples: {}, Spikes: {}, SMI-correlated: {}, Dropped: {}", sample_count, spike_total, smi_correlated_total, dropped_total);
             }
        }

        eprintln!("[COLLECTOR] Hot loop stopped after {} samples (Spikes: {}, SMI-correlated: {}, Dropped: {})", sample_count, spike_total, smi_correlated_total, dropped_total);
        eprintln!("[COLLECTOR] Final atomic spike_count={}, smi_correlated_spikes={}",
            self.spike_count.load(Ordering::Relaxed),
            self.smi_correlated_spikes.load(Ordering::Relaxed));
    }

    /// Gets a reference to the spike count.
    pub fn spike_count(&self) -> Arc<AtomicU64> {
        let count = self.spike_count.load(Ordering::Relaxed);
        eprintln!("[COLLECTOR_ACCESS] Reading spike_count: {}", count);
        Arc::clone(&self.spike_count)
    }

    /// Gets a reference to the SMI-correlated spike count.
    pub fn smi_correlated_spikes(&self) -> Arc<AtomicU64> {
        let count = self.smi_correlated_spikes.load(Ordering::Relaxed);
        eprintln!("[COLLECTOR_ACCESS] Reading smi_correlated_spikes: {}", count);
        Arc::clone(&self.smi_correlated_spikes)
    }
}

/// Helper: Add nanoseconds to a timespec.
/// Handles overflow correctly for multi-second additions.
fn add_ns_to_timespec(ts: &mut libc::timespec, ns: u64) {
    let new_nsec = ts.tv_nsec as u64 + ns;
    ts.tv_sec += (new_nsec / 1_000_000_000) as libc::time_t;
    ts.tv_nsec = (new_nsec % 1_000_000_000) as libc::c_long;
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

/// Processes latency samples and maintains histogram statistics
pub struct LatencyProcessor {
    histogram: Histogram<u64>,
    last_sample: u64,
    max_latency: u64,
    sample_count: u64,
    /// 20-bucket logarithmic histogram for UI visualization
    buckets: [u64; 20],
    /// Maximum latency in the current cycle (reset every 100ms)
    cycle_max_ns: u64,
    /// Core temperatures in Celsius (captured periodically)
    core_temperatures: Vec<f32>,
    /// Package temperature in Celsius (captured periodically)
    package_temperature: f32,
    /// Last time thermal data was read (to throttle reads)
    last_thermal_read: std::time::Instant,
}

impl LatencyProcessor {
    /// Create a new latency processor with microsecond precision
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize histogram with 3 decimal places of accuracy (microsecond resolution)
        // Covers ranges from 1 ns to 1 hour
        let histogram = Histogram::<u64>::new(3)?;

        Ok(LatencyProcessor {
            histogram,
            last_sample: 0,
            max_latency: 0,
            sample_count: 0,
            buckets: [0u64; 20],
            cycle_max_ns: 0,
            core_temperatures: Vec::new(),
            package_temperature: 0.0,
            last_thermal_read: std::time::Instant::now(),
        })
    }

    /// Convert nanoseconds to bucket index using hybrid linear-logarithmic mapping
    ///
    /// - 0-9,999ns: Bucket = `ns / 1000` (Linear 1µs resolution) → buckets 0-9
    /// - 10,000ns+: Bucket = `10 + (log10(ns_float) - 4.0) * (10.0 / 3.0)` (Logarithmic) → buckets 10-19
    /// - Bounds result to 0..19
    fn latency_to_bucket_index(latency_ns: u64) -> usize {
        if latency_ns < 10_000 {
            // Linear region: 0-9,999 ns → buckets 0-9
            let index = latency_ns / 1000;
            (index as usize).min(9)
        } else {
            // Logarithmic region: 10,000+ ns → buckets 10-19
            let latency_f64 = latency_ns as f64;
            let log_val = latency_f64.log10();
            let bucket_f64 = 10.0 + (log_val - 4.0) * (10.0 / 3.0);
            let bucket_u = bucket_f64 as usize;
            (bucket_u).min(19)
        }
    }

    /// Record a latency sample (in nanoseconds)
    pub fn record_sample(&mut self, latency_ns: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Convert nanoseconds to microseconds for histogram (lose some precision but stay within range)
        let latency_us = (latency_ns + 500) / 1000; // Round to nearest microsecond

        self.histogram.record(latency_us)?;
        self.last_sample = latency_us;
        self.max_latency = self.max_latency.max(latency_us);
        self.sample_count += 1;
        
        // Update 20-bucket histogram
        let bucket_idx = Self::latency_to_bucket_index(latency_ns);
        self.buckets[bucket_idx] += 1;
        
        // Track maximum latency in the current cycle (for jitter timeline)
        self.cycle_max_ns = self.cycle_max_ns.max(latency_ns);

        Ok(())
    }

    /// Get the current maximum latency
    pub fn max(&self) -> f32 {
        self.max_latency as f32
    }

    /// Get the P99 percentile
    pub fn p99(&self) -> f32 {
        self.histogram.value_at_percentile(99.0) as f32
    }

    /// Get the P99.9 percentile
    pub fn p99_9(&self) -> f32 {
        self.histogram.value_at_percentile(99.9) as f32
    }

    /// Get the average latency
    pub fn average(&self) -> f32 {
        self.histogram.mean() as f32
    }

    /// Get the latest sample
    pub fn last_sample(&self) -> f32 {
        self.last_sample as f32
    }

    /// Get the total number of samples recorded
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        self.histogram.clear();
        self.last_sample = 0;
        self.max_latency = 0;
        self.sample_count = 0;
        self.buckets = [0u64; 20];
        self.cycle_max_ns = 0;
    }
    
    /// Set core temperatures directly (for seamless restart transition)
    /// This prevents heatmap blackout when monitoring is restarted
    pub fn set_core_temperatures(&mut self, temps: Vec<f32>) {
        self.core_temperatures = temps;
    }
    
    /// Update thermal data (called periodically, e.g., every 100ms)
    pub fn update_thermal_data(&mut self) {
        // Throttle thermal reads to every 100ms to avoid excessive sysfs access
        if self.last_thermal_read.elapsed() < std::time::Duration::from_millis(100) {
            return;
        }
        
        let thermal_data = thermal::read_thermal_data();
        self.core_temperatures = thermal_data.core_temperatures;
        self.package_temperature = thermal_data.package_temperature;
        self.last_thermal_read = std::time::Instant::now();
    }
    
    /// Get current core temperatures
    pub fn core_temperatures(&self) -> &[f32] {
        &self.core_temperatures
    }
    
    /// Get current package temperature
    pub fn package_temperature(&self) -> f32 {
        self.package_temperature
    }

    /// Get the 20-bucket histogram as normalized f32 values (0.0..1.0)
    /// Returns buckets normalized by the maximum bucket count
    pub fn get_histogram_buckets(&self) -> Vec<f32> {
        let max_count = self.buckets.iter().max().copied().unwrap_or(1) as f32;
        self.buckets
            .iter()
            .map(|&count| {
                let normalized = (count as f32) / max_count;
                normalized.min(1.0) // Ensure bounded to 0..1
            })
            .collect()
    }

    /// Get the current cycle maximum latency in microseconds
    /// This is the maximum latency observed since the last reset
    pub fn cycle_max_us(&self) -> f32 {
        ((self.cycle_max_ns + 500) / 1000) as f32
    }

    /// Reset cycle_max_ns for the next monitoring cycle
    pub fn reset_cycle_max(&mut self) {
        self.cycle_max_ns = 0;
    }

    /// Get histogram bucket distribution (for UI visualization)
    pub fn get_bucket_distribution(&self) -> Vec<(f32, f32, u64)> {
        let mut buckets = Vec::new();

        // Create 20 buckets logarithmically distributed from 1µs to max latency
        if self.max_latency == 0 {
            return buckets;
        }

        let max_log = (self.max_latency as f64).log10();
        for i in 0..20 {
            let lower_log = max_log * (i as f64 / 20.0);
            let upper_log = max_log * ((i + 1) as f64 / 20.0);

            let lower = 10_f64.powf(lower_log) as u64;
            let upper = 10_f64.powf(upper_log) as u64;

            let count = self.histogram.count_at(upper) - self.histogram.count_at(lower);

            buckets.push((lower as f32, upper as f32, count));
        }

        buckets
    }
}

impl Default for LatencyProcessor {
    fn default() -> Self {
        Self::new().expect("Failed to create default LatencyProcessor")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timespec_diff_ns() {
        // Test case 1: now > target (late wake)
        let target = libc::timespec {
            tv_sec: 1000,
            tv_nsec: 0,
        };
        let now = libc::timespec {
            tv_sec: 1000,
            tv_nsec: 1000000, // 1 ms later
        };
        let diff = timespec_diff_ns(&now, &target);
        assert_eq!(diff, 1_000_000);
    }

    #[test]
    fn test_add_ns_to_timespec() {
        let mut ts = libc::timespec {
            tv_sec: 1000,
            tv_nsec: 500_000_000,
        };
        add_ns_to_timespec(&mut ts, 600_000_000); // Add 0.6 seconds

        assert_eq!(ts.tv_sec, 1001);
        assert_eq!(ts.tv_nsec, 100_000_000);
    }

    #[test]
    fn test_latency_processor_creation() {
        let processor = LatencyProcessor::default();
        assert_eq!(processor.sample_count(), 0);
    }
}

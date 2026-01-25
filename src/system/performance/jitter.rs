//! Micro-Jitter Collector
//!
//! Performs high-frequency (50µs) wakeup monitoring to capture fine-grained kernel jitter.
//! Measures latency at P99.99 precision for real-time workload analysis.

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Configuration for micro-jitter monitoring
#[derive(Clone, Debug)]
pub struct MicroJitterConfig {
    /// Interval between samples in microseconds (typically 50µs)
    pub interval_us: u64,
    /// Spike threshold in microseconds for detecting anomalies
    pub spike_threshold_us: u64,
    /// Duration to run the jitter test in seconds
    pub duration_secs: u64,
}

impl Default for MicroJitterConfig {
    fn default() -> Self {
        MicroJitterConfig {
            interval_us: 50,         // 50µs high-frequency sampling
            spike_threshold_us: 500, // 500µs spike threshold
            duration_secs: 10,       // 10 second test
        }
    }
}

/// Metrics from micro-jitter measurement
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MicroJitterMetrics {
    /// P99.99 percentile latency in microseconds
    pub p99_99_us: f32,
    /// Maximum observed latency in microseconds
    pub max_us: f32,
    /// Average latency in microseconds
    pub avg_us: f32,
    /// Number of spikes detected (above threshold)
    pub spike_count: u64,
    /// Total number of samples collected
    pub sample_count: u64,
}

/// The Micro-Jitter Collector performs high-frequency latency measurement
/// to detect fine-grained kernel jitter patterns
#[derive(Clone)]
pub struct MicroJitterCollector {
    config: MicroJitterConfig,
    stop_flag: Arc<AtomicBool>,
}

impl MicroJitterCollector {
    /// Create a new micro-jitter collector with default configuration
    pub fn new(config: MicroJitterConfig) -> Self {
        MicroJitterCollector {
            config,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Run the micro-jitter measurement test
    /// Returns MicroJitterMetrics with P99.99, max, and spike information
    pub fn run(&self) -> Result<MicroJitterMetrics, Box<dyn std::error::Error>> {
        eprintln!(
            "[JITTER] Starting micro-jitter test: interval={}µs, threshold={}µs, duration={}s",
            self.config.interval_us, self.config.spike_threshold_us, self.config.duration_secs
        );

        // Initialize histogram with 3 decimal places (microsecond precision)
        let mut histogram = Histogram::<u64>::new(3)?;

        // Convert configuration to nanoseconds
        let interval_ns = (self.config.interval_us as u64) * 1000;
        let spike_threshold_ns = (self.config.spike_threshold_us as u64) * 1000;
        let duration_ns = (self.config.duration_secs as u64) * 1_000_000_000;

        // Get baseline time
        let mut next_wake: libc::timespec = unsafe {
            let mut ts = std::mem::zeroed::<libc::timespec>();
            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
            ts
        };

        let start_time: libc::timespec = next_wake;

        let mut sample_count = 0u64;
        let mut spike_count = 0u64;

        // Hot loop for jitter measurement
        while !self.stop_flag.load(Ordering::Relaxed) {
            // Add interval to next wake time
            add_ns_to_timespec(&mut next_wake, interval_ns);

            // Sleep until next wake-up
            unsafe {
                libc::clock_nanosleep(
                    libc::CLOCK_MONOTONIC,
                    libc::TIMER_ABSTIME,
                    &next_wake,
                    std::ptr::null_mut(),
                );
            }

            // Measure actual latency
            let now: libc::timespec = unsafe {
                let mut ts = std::mem::zeroed::<libc::timespec>();
                libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
                ts
            };

            let latency_ns = timespec_diff_ns(&now, &next_wake);
            let latency_us = (latency_ns + 500) / 1000; // Round to microseconds

            histogram.record(latency_us)?;
            sample_count += 1;

            // Detect spikes
            if latency_ns > spike_threshold_ns {
                spike_count += 1;
            }

            // Check if duration has elapsed
            let elapsed_ns = timespec_diff_ns(&now, &start_time);
            if elapsed_ns > duration_ns {
                break;
            }

            // Log status periodically (SILENCED to prevent spam)
            // if sample_count % 200 == 0 {
            //     log::debug!("[JITTER] Progress: {} samples, {} spikes", sample_count, spike_count);
            // }
        }

        let metrics = MicroJitterMetrics {
            p99_99_us: histogram.value_at_percentile(99.99) as f32,
            max_us: histogram.max() as f32,
            avg_us: histogram.mean() as f32,
            spike_count,
            sample_count,
        };

        eprintln!(
            "[JITTER] Test completed: P99.99={:.2}µs, Max={:.2}µs, Spikes={}",
            metrics.p99_99_us, metrics.max_us, metrics.spike_count
        );

        Ok(metrics)
    }

    /// Request the collector to stop
    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
    }
}

/// Helper: Add nanoseconds to a timespec
fn add_ns_to_timespec(ts: &mut libc::timespec, ns: u64) {
    let new_nsec = ts.tv_nsec as u64 + ns;
    ts.tv_sec += (new_nsec / 1_000_000_000) as libc::time_t;
    ts.tv_nsec = (new_nsec % 1_000_000_000) as libc::c_long;
}

/// Helper: Calculate the difference between two timespec values in nanoseconds
fn timespec_diff_ns(now: &libc::timespec, target: &libc::timespec) -> u64 {
    let sec_diff = now.tv_sec - target.tv_sec;
    let nsec_diff = now.tv_nsec - target.tv_nsec;
    let diff_ns = (sec_diff as i64) * 1_000_000_000 + (nsec_diff as i64);
    if diff_ns >= 0 {
        diff_ns as u64
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_micro_jitter_config_default() {
        let config = MicroJitterConfig::default();
        assert_eq!(config.interval_us, 50);
        assert_eq!(config.spike_threshold_us, 500);
        assert_eq!(config.duration_secs, 10);
    }

    #[test]
    fn test_micro_jitter_collector_creation() {
        let config = MicroJitterConfig::default();
        let collector = MicroJitterCollector::new(config);
        assert!(!collector.stop_flag.load(Ordering::Relaxed));
    }
}

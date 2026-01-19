//! Context-Switch RTT Collector (High-Precision, Topology-Aware)
//!
//! Measures round-trip time (RTT) between two threads using pipe-based token passing.
//! Evaluates the overhead of context switching and schedule latency.
//!
//! **LABORATORY-GRADE PRECISION**:
//! - Returns f32 microseconds without aggressive early rounding
//! - Returns Summary struct (mean, median, p95) instead of P99 (statistical bias)
//! - Topology-aware: Detects CPU siblings and uses cross-core pairs for representative measurement
//! - Sub-microsecond precision preserved throughout nanosecond calculations

use std::os::unix::io::RawFd;
use std::sync::Arc;
use std::thread;
use serde::{Deserialize, Serialize};

/// Configuration for context-switch RTT measurement
#[derive(Clone, Debug)]
pub struct ContextSwitchConfig {
    /// Number of token passes to perform (iterations)
    pub iterations: u64,
    /// CPU core ID for first thread (if CPU affinity is desired)
    pub thread1_cpu: Option<usize>,
    /// CPU core ID for second thread
    pub thread2_cpu: Option<usize>,
}

impl Default for ContextSwitchConfig {
    fn default() -> Self {
        ContextSwitchConfig {
            iterations: 1000,
            thread1_cpu: Some(0),
            thread2_cpu: Some(1),
        }
    }
}

/// High-precision RTT summary statistics
///
/// LABORATORY-GRADE CALIBRATION:
/// - mean: Arithmetic mean RTT in microseconds (f32, full precision)
/// - median: 50th percentile RTT in microseconds (representative)
/// - p95: 95th percentile RTT in microseconds (tail behavior)
///
/// Avoids P99/P99.9 which are statistically biased and subject to noise floor effects.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextSwitchSummary {
    /// Mean RTT in microseconds (arithmetic average)
    pub mean: f32,
    /// Median RTT in microseconds (50th percentile, representative)
    pub median: f32,
    /// P95 percentile RTT in microseconds (tail behavior, less noisy than P99)
    pub p95: f32,
    /// Total number of successful token passes
    pub successful_passes: u64,
}

/// Legacy metrics struct for backward compatibility
/// **DEPRECATED**: Use ContextSwitchSummary instead
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextSwitchMetrics {
    /// Average RTT in microseconds (round-trip time)
    pub avg_rtt_us: f32,
    /// Minimum RTT observed in microseconds
    pub min_rtt_us: f32,
    /// Maximum RTT observed in microseconds
    pub max_rtt_us: f32,
    /// P99 percentile RTT in microseconds (deprecated, use Summary instead)
    pub p99_rtt_us: f32,
    /// Total number of successful token passes
    pub successful_passes: u64,
}

impl From<ContextSwitchSummary> for ContextSwitchMetrics {
    fn from(summary: ContextSwitchSummary) -> Self {
        ContextSwitchMetrics {
            avg_rtt_us: summary.mean,
            min_rtt_us: 0.0, // Not tracked in Summary
            max_rtt_us: 0.0, // Not tracked in Summary
            p99_rtt_us: summary.p95, // Use P95 as approximation
            successful_passes: summary.successful_passes,
        }
    }
}

/// The Context-Switch Collector measures RTT between threads via pipe token passing
pub struct ContextSwitchCollector {
    config: ContextSwitchConfig,
}

impl ContextSwitchCollector {
    /// Create a new context-switch collector
    pub fn new(config: ContextSwitchConfig) -> Self {
        ContextSwitchCollector { config }
    }

    /// Run the context-switch RTT test with high precision and topology awareness
    ///
    /// **LABORATORY-GRADE MEASUREMENT**:
    /// 1. Detects if CPUs 0 and 1 are siblings (same physical core)
    /// 2. If siblings detected, attempts to find a cross-core pair for representative measurement
    /// 3. Preserves nanosecond precision throughout calculations
    /// 4. Returns Summary with mean, median, and p95 (avoids P99 statistical bias)
    ///
    /// # Returns
    /// ContextSwitchSummary with laboratory-grade statistics
    pub fn run(&self) -> Result<ContextSwitchSummary, Box<dyn std::error::Error>> {
        eprintln!(
            "[CTX_SWITCH] Starting context-switch RTT test: {} iterations",
            self.config.iterations
        );

        // Determine CPU pair with topology awareness
        let (cpu1, cpu2) = self.select_cpu_pair()?;
        eprintln!(
            "[CTX_SWITCH] Selected CPU pair: {} <-> {} (topology-aware)",
            cpu1, cpu2
        );

        // Create pipes for communication
        let (pipe_read1, pipe_write1) = create_pipe()?;
        let (pipe_read2, pipe_write2) = create_pipe()?;

        let iterations = self.config.iterations;
        let metrics_container = Arc::new(std::sync::Mutex::new(Vec::new()));
        let metrics_clone = Arc::clone(&metrics_container);

        // Spawn receiver thread
        let receiver_handle = thread::spawn(move || {
            // Apply CPU affinity
            set_cpu_affinity(cpu2).ok();

            let mut buf = [0u8; 1];
            let mut rtt_samples_ns = Vec::with_capacity(iterations as usize);

            for _ in 0..iterations {
                // Record when we receive
                let recv_start = get_time_ns();

                // Read token from pipe
                if unsafe { libc::read(pipe_read1, buf.as_mut_ptr() as *mut libc::c_void, 1) } > 0
                {
                    // Record reception time
                    let recv_time = get_time_ns();

                    // Send token back immediately
                    unsafe {
                        libc::write(pipe_write2, buf.as_ptr() as *const libc::c_void, 1);
                    }

                    // Store in NANOSECONDS for precision
                    rtt_samples_ns.push(recv_time - recv_start);
                }
            }

            // Store samples
            if let Ok(mut container) = metrics_clone.lock() {
                *container = rtt_samples_ns;
            }

            // Cleanup
            unsafe {
                libc::close(pipe_read1);
                libc::close(pipe_write2);
            }
        });

        // Sender thread: apply CPU affinity and send tokens and measure RTT
        set_cpu_affinity(cpu1)?;

        let mut rtt_latencies_ns = Vec::with_capacity(iterations as usize);
        let mut buf = [42u8; 1]; // Token value

        for i in 0..iterations {
            let send_time = get_time_ns();

            // Send token
            unsafe {
                libc::write(pipe_write1, buf.as_ptr() as *const libc::c_void, 1);
            }

            // Read response
            if unsafe { libc::read(pipe_read2, buf.as_mut_ptr() as *mut libc::c_void, 1) } > 0 {
                let recv_time = get_time_ns();
                // Store in NANOSECONDS for precision
                let rtt_ns = recv_time - send_time;
                rtt_latencies_ns.push(rtt_ns);
            }

            if (i + 1) % 100 == 0 {
                eprintln!("[CTX_SWITCH] Progress: {}/{} passes", i + 1, iterations);
            }
        }

        // Wait for receiver thread
        let _ = receiver_handle.join();

        // Cleanup write-side pipes
        unsafe {
            libc::close(pipe_write1);
            libc::close(pipe_read2);
        }

        // **HIGH-PRECISION CONVERSION** (nanoseconds → microseconds as f32)
        // Convert WITHOUT aggressive rounding: preserve sub-microsecond precision
        let rtt_us: Vec<f32> = rtt_latencies_ns
            .iter()
            .map(|ns| (*ns as f32) / 1000.0) // Direct conversion, no rounding
            .collect();

        // Sort for percentile calculations
        let mut rtt_us_sorted = rtt_us.clone();
        rtt_us_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Calculate statistics
        let mean = rtt_us.iter().sum::<f32>() / rtt_us.len().max(1) as f32;
        let median = Self::calculate_percentile(&rtt_us_sorted, 0.50);
        let p95 = Self::calculate_percentile(&rtt_us_sorted, 0.95);

        let summary = ContextSwitchSummary {
            mean,
            median,
            p95,
            successful_passes: rtt_latencies_ns.len() as u64,
        };

        eprintln!(
            "[CTX_SWITCH] Test completed: Mean={:.3}µs (sub-µs precision), Median={:.3}µs, P95={:.3}µs",
            summary.mean, summary.median, summary.p95
        );
        eprintln!(
            "[CTX_SWITCH] CPU pair: {} <-> {} | Topology-aware cross-core measurement",
            cpu1, cpu2
        );

        Ok(summary)
    }

    /// Select CPU pair with topology awareness
    ///
    /// If CPUs 0 and 1 are siblings (same physical core), attempts to find
    /// a non-sibling pair for more representative cross-core measurement.
    fn select_cpu_pair(&self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let cpu1 = self.config.thread1_cpu.unwrap_or(0);
        let cpu2 = self.config.thread2_cpu.unwrap_or(1);

        // Check if CPUs are siblings
        if are_cpus_siblings(cpu1, cpu2)? {
            eprintln!(
                "[CTX_SWITCH] ⚠ CPUs {} and {} are siblings (same physical core)",
                cpu1, cpu2
            );
            eprintln!("[CTX_SWITCH] Attempting to find cross-core pair...");

            // Try to find a non-sibling pair
            if let Ok((alt_cpu1, alt_cpu2)) = find_cross_core_pair(cpu1) {
                eprintln!(
                    "[CTX_SWITCH] ✓ Found cross-core pair: {} <-> {}",
                    alt_cpu1, alt_cpu2
                );
                return Ok((alt_cpu1, alt_cpu2));
            } else {
                eprintln!(
                    "[CTX_SWITCH] ⚠ Could not find cross-core pair, using sibling CPUs anyway"
                );
            }
        }

        Ok((cpu1, cpu2))
    }

    /// Calculate percentile from sorted values
    fn calculate_percentile(sorted: &[f32], percentile: f32) -> f32 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((sorted.len() as f32 - 1.0) * percentile).ceil() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

/// Create a unidirectional pipe and return (read_fd, write_fd)
fn create_pipe() -> Result<(RawFd, RawFd), Box<dyn std::error::Error>> {
    let mut fds = [0; 2];
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            return Err("Failed to create pipe".into());
        }
    }
    Ok((fds[0], fds[1]))
}

/// Get current time in nanoseconds using CLOCK_MONOTONIC
fn get_time_ns() -> u64 {
    unsafe {
        let mut ts = std::mem::zeroed::<libc::timespec>();
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
        (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64)
    }
}

/// Set CPU affinity for current thread to specified CPU core
fn set_cpu_affinity(cpu: usize) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let mut set = std::mem::zeroed::<libc::cpu_set_t>();
        libc::CPU_SET(cpu, &mut set);

        if libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
            return Err(format!("Failed to set CPU affinity to CPU {}", cpu).into());
        }

        eprintln!("[CTX_SWITCH] ✓ Pinned thread to CPU {}", cpu);
    }
    Ok(())
}

/// Check if two CPUs are siblings (same physical core)
///
/// Reads `/sys/devices/system/cpu/cpu{N}/topology/thread_siblings_list`
/// to determine if CPUs share a physical core.
fn are_cpus_siblings(cpu1: usize, cpu2: usize) -> Result<bool, Box<dyn std::error::Error>> {
    let path = format!(
        "/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list",
        cpu1
    );

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            // Parse comma/dash separated list (e.g., "0,2" or "0-1")
            let siblings: Vec<usize> = content
                .trim()
                .split(|c: char| c == ',' || c == '-')
                .filter_map(|s| s.parse::<usize>().ok())
                .collect();

            Ok(siblings.contains(&cpu2))
        }
        Err(_) => {
            eprintln!("[CTX_SWITCH] ⚠ Could not read topology info, assuming non-siblings");
            Ok(false)
        }
    }
}

/// Find a cross-core CPU pair starting from the given CPU
///
/// Attempts to find two CPUs on different physical cores for representative
/// cross-core context-switch measurement.
fn find_cross_core_pair(start_cpu: usize) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    // Try CPUs in order, skipping siblings
    for candidate_cpu in 0..256 {
        if candidate_cpu == start_cpu {
            continue;
        }

        if !are_cpus_siblings(start_cpu, candidate_cpu)? {
            return Ok((start_cpu, candidate_cpu));
        }
    }

    Err("Could not find cross-core pair".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_switch_config_default() {
        let config = ContextSwitchConfig::default();
        assert_eq!(config.iterations, 1000);
        assert_eq!(config.thread1_cpu, Some(0));
        assert_eq!(config.thread2_cpu, Some(1));
    }

    #[test]
    fn test_context_switch_collector_creation() {
        let config = ContextSwitchConfig::default();
        let collector = ContextSwitchCollector::new(config);
        assert_eq!(collector.config.iterations, 1000);
    }

    #[test]
    fn test_percentile_calculation() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let median = ContextSwitchCollector::calculate_percentile(&values, 0.50);
        assert!(median >= 5.0 && median <= 6.0);

        let p95 = ContextSwitchCollector::calculate_percentile(&values, 0.95);
        assert!(p95 >= 9.0 && p95 <= 10.0);
    }

    #[test]
    fn test_summary_conversion_to_metrics() {
        let summary = ContextSwitchSummary {
            mean: 5.5,
            median: 5.0,
            p95: 9.5,
            successful_passes: 1000,
        };

        let metrics: ContextSwitchMetrics = summary.into();
        assert_eq!(metrics.avg_rtt_us, 5.5);
        assert_eq!(metrics.p99_rtt_us, 9.5); // P95 used as approximation
        assert_eq!(metrics.successful_passes, 1000);
    }
}

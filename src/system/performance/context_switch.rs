//! Context-Switch RTT Collector
//!
//! Measures round-trip time (RTT) between two threads using pipe-based token passing.
//! Evaluates the overhead of context switching and schedule latency.

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

/// Metrics from context-switch RTT measurement
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ContextSwitchMetrics {
    /// Average RTT in microseconds (round-trip time)
    pub avg_rtt_us: f32,
    /// Minimum RTT observed in microseconds
    pub min_rtt_us: f32,
    /// Maximum RTT observed in microseconds
    pub max_rtt_us: f32,
    /// P99 percentile RTT in microseconds
    pub p99_rtt_us: f32,
    /// Total number of successful token passes
    pub successful_passes: u64,
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

    /// Run the context-switch RTT test
    /// Creates two threads that pass a token through a pipe
    pub fn run(&self) -> Result<ContextSwitchMetrics, Box<dyn std::error::Error>> {
        eprintln!(
            "[CTX_SWITCH] Starting context-switch RTT test: {} iterations",
            self.config.iterations
        );

        // Create pipes for communication
        let (pipe_read1, pipe_write1) = create_pipe()?;
        let (pipe_read2, pipe_write2) = create_pipe()?;

        let iterations = self.config.iterations;
        let metrics_container = Arc::new(std::sync::Mutex::new(Vec::new()));
        let metrics_clone = Arc::clone(&metrics_container);

        // Spawn receiver thread
        let receiver_handle = thread::spawn(move || {
            let mut buf = [0u8; 1];
            let mut rtt_samples = Vec::with_capacity(iterations as usize);

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

                    rtt_samples.push(recv_time - recv_start);
                }
            }

            // Store samples
            if let Ok(mut container) = metrics_clone.lock() {
                *container = rtt_samples;
            }

            // Cleanup
            unsafe {
                libc::close(pipe_read1);
                libc::close(pipe_write2);
            }
        });

        // Sender thread: send tokens and measure RTT
        let mut rtt_latencies = Vec::with_capacity(iterations as usize);
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
                let rtt_ns = recv_time - send_time;
                rtt_latencies.push(rtt_ns);
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

        // Convert nanoseconds to microseconds and compute statistics
        let mut rtt_us: Vec<u64> = rtt_latencies
            .iter()
            .map(|ns| (ns + 500) / 1000) // Round to nearest microsecond
            .collect();

        rtt_us.sort();

        let avg_rtt = if !rtt_us.is_empty() {
            (rtt_us.iter().sum::<u64>() as f32) / (rtt_us.len() as f32)
        } else {
            0.0
        };

        let min_rtt = (*rtt_us.first().unwrap_or(&0)) as f32;
        let max_rtt = (*rtt_us.last().unwrap_or(&0)) as f32;
        let p99_idx = ((rtt_us.len() as f32 * 0.99) as usize).min(rtt_us.len() - 1);
        let p99_rtt = (rtt_us[p99_idx]) as f32;

        let metrics = ContextSwitchMetrics {
            avg_rtt_us: avg_rtt,
            min_rtt_us: min_rtt,
            max_rtt_us: max_rtt,
            p99_rtt_us: p99_rtt,
            successful_passes: rtt_latencies.len() as u64,
        };

        eprintln!(
            "[CTX_SWITCH] Test completed: Avg RTT={:.2}µs, Min={:.2}µs, Max={:.2}µs, P99={:.2}µs",
            metrics.avg_rtt_us, metrics.min_rtt_us, metrics.max_rtt_us, metrics.p99_rtt_us
        );

        Ok(metrics)
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
}

//! Syscall Saturation Collector
//!
//! Measures syscall entry/exit overhead using tight loops of fast syscalls (getpid).
//! Evaluates how syscall overhead scales under system load.

use serde::{Deserialize, Serialize};

/// Configuration for syscall saturation
#[derive(Clone, Debug)]
pub struct SyscallSaturationConfig {
    /// Number of getpid() calls per test iteration
    pub iterations: u64,
    /// Number of sequential test runs to perform
    pub runs: u64,
}

impl Default for SyscallSaturationConfig {
    fn default() -> Self {
        SyscallSaturationConfig {
            iterations: 100_000,  // 100k getpid calls per run
            runs: 5,              // 5 sequential runs
        }
    }
}

/// Metrics from syscall saturation measurement
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SyscallSaturationMetrics {
    /// Average time per syscall in nanoseconds
    pub avg_ns_per_call: f32,
    /// Minimum time per syscall observed
    pub min_ns_per_call: f32,
    /// Maximum time per syscall observed
    pub max_ns_per_call: f32,
    /// Total syscalls executed
    pub total_syscalls: u64,
    /// Estimated syscalls per second (throughput)
    pub calls_per_second: u64,
}

/// The Syscall Saturation Collector measures getpid() overhead
pub struct SyscallSaturationCollector {
    config: SyscallSaturationConfig,
}

impl SyscallSaturationCollector {
    /// Create a new syscall saturation collector
    pub fn new(config: SyscallSaturationConfig) -> Self {
        SyscallSaturationCollector { config }
    }

    /// Run the syscall saturation test
    /// Executes tight loops of getpid() to measure syscall overhead
    pub fn run(&self) -> Result<SyscallSaturationMetrics, Box<dyn std::error::Error>> {
        eprintln!(
            "[SYSCALL] Starting syscall saturation test: {} iterations x {} runs",
            self.config.iterations, self.config.runs
        );

        let mut all_timings = Vec::new();
        let mut total_syscalls = 0u64;
        let mut min_run_time = u64::MAX;
        let mut max_run_time = 0u64;

        for run in 0..self.config.runs {
            let run_timings = self.run_iteration()?;
            let run_time: u64 = run_timings.iter().sum();
            
            all_timings.extend(&run_timings);
            total_syscalls += self.config.iterations;
            min_run_time = min_run_time.min(run_time);
            max_run_time = max_run_time.max(run_time);

            let avg_per_call = (run_time as f32) / (self.config.iterations as f32);
            eprintln!(
                "[SYSCALL] Run {}/{}: {:.2} ns/call, total={}ns",
                run + 1,
                self.config.runs,
                avg_per_call,
                run_time
            );
        }

        all_timings.sort();

        let avg_ns = if !all_timings.is_empty() {
            (all_timings.iter().sum::<u64>() as f32) / (all_timings.len() as f32)
        } else {
            0.0
        };

        let min_ns = (*all_timings.first().unwrap_or(&0)) as f32;
        let max_ns = (*all_timings.last().unwrap_or(&0)) as f32;

        // Calculate throughput: syscalls per second
        let total_ns = min_run_time * self.config.runs; // Conservative estimate
        let calls_per_sec = if total_ns > 0 {
            ((total_syscalls as f64) / (total_ns as f64) * 1_000_000_000.0) as u64
        } else {
            0
        };

        let metrics = SyscallSaturationMetrics {
            avg_ns_per_call: avg_ns,
            min_ns_per_call: min_ns,
            max_ns_per_call: max_ns,
            total_syscalls,
            calls_per_second: calls_per_sec,
        };

        eprintln!(
            "[SYSCALL] Test completed: Avg={:.2}ns, Min={:.2}ns, Max={:.2}ns, Throughput={}k/sec",
            metrics.avg_ns_per_call,
            metrics.min_ns_per_call,
            metrics.max_ns_per_call,
            calls_per_sec / 1000
        );

        Ok(metrics)
    }

    /// Run a single iteration of getpid() calls
    /// Returns a vector of timings (one per call)
    fn run_iteration(&self) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
        let mut timings = Vec::with_capacity(self.config.iterations as usize);

        for _ in 0..self.config.iterations {
            let start = get_time_ns();
            unsafe {
                libc::getpid();
            }
            let end = get_time_ns();
            timings.push(end - start);
        }

        Ok(timings)
    }
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
    fn test_syscall_saturation_config_default() {
        let config = SyscallSaturationConfig::default();
        assert_eq!(config.iterations, 100_000);
        assert_eq!(config.runs, 5);
    }

    #[test]
    fn test_syscall_saturation_collector_creation() {
        let config = SyscallSaturationConfig::default();
        let collector = SyscallSaturationCollector::new(config);
        assert_eq!(collector.config.iterations, 100_000);
    }
}

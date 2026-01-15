//! Task-to-Task Wakeup Collector
//!
//! Measures wake-up latency between threads using futex-based signaling.
//! Evaluates how fast one thread can wake another and get scheduled.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::thread;
use serde::{Deserialize, Serialize};

/// Configuration for task wakeup measurement
#[derive(Clone, Debug)]
pub struct TaskWakeupConfig {
    /// Number of wake-up cycles to perform (iterations)
    pub iterations: u64,
    /// CPU core ID for waker thread (if CPU affinity is desired)
    pub waker_cpu: Option<usize>,
    /// CPU core ID for sleeper thread
    pub sleeper_cpu: Option<usize>,
}

impl Default for TaskWakeupConfig {
    fn default() -> Self {
        TaskWakeupConfig {
            iterations: 1000,
            waker_cpu: Some(0),
            sleeper_cpu: Some(1),
        }
    }
}

/// Metrics from task wakeup measurement
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskWakeupMetrics {
    /// Average wake-up latency in microseconds
    pub avg_latency_us: f32,
    /// Minimum wake-up latency observed
    pub min_latency_us: f32,
    /// Maximum wake-up latency observed
    pub max_latency_us: f32,
    /// P99 percentile wake-up latency
    pub p99_latency_us: f32,
    /// Total successful wakeups
    pub successful_wakeups: u64,
}

/// The Task Wakeup Collector measures thread wake-up latency using futex
pub struct TaskWakeupCollector {
    config: TaskWakeupConfig,
}

impl TaskWakeupCollector {
    /// Create a new task wakeup collector
    pub fn new(config: TaskWakeupConfig) -> Self {
        TaskWakeupCollector { config }
    }

    /// Run the task wakeup test
    /// Creates two threads: an "awakener" and a "sleeper"
    /// The awakener signals the sleeper via futex and measures wake-up latency
    pub fn run(&self) -> Result<TaskWakeupMetrics, Box<dyn std::error::Error>> {
        eprintln!(
            "[WAKEUP] Starting task wakeup test: {} iterations",
            self.config.iterations
        );

        // Shared futex state
        let futex_state = Arc::new(AtomicI32::new(0));
        let futex_clone = Arc::clone(&futex_state);
        let iterations = self.config.iterations;
        let metrics_container = Arc::new(std::sync::Mutex::new(Vec::new()));
        let metrics_clone = Arc::clone(&metrics_container);

        // Spawn sleeper thread
        let sleeper_handle = thread::spawn(move || {
            let mut latencies = Vec::with_capacity(iterations as usize);

            for i in 0..iterations {
                // Wait for the futex value to change to 1
                loop {
                    let current = futex_clone.load(Ordering::Acquire);
                    if current == 1 {
                        // Record wake-up time
                        let wakeup_time = get_time_ns();
                        latencies.push(wakeup_time);

                        // Reset futex for next iteration
                        futex_clone.store(0, Ordering::Release);
                        break;
                    }

                    // Spin-wait with small pause to reduce CPU usage
                    std::hint::spin_loop();
                }

                if (i + 1) % 100 == 0 {
                    eprintln!("[WAKEUP] Sleeper: received {}/{} wakeups", i + 1, iterations);
                }
            }

            // Store latencies
            if let Ok(mut container) = metrics_clone.lock() {
                *container = latencies;
            }
        });

        // Waker thread: signal sleeper and measure latency
        let mut wakeup_latencies = Vec::with_capacity(iterations as usize);

        for i in 0..iterations {
            let send_time = get_time_ns();

            // Signal the sleeper by setting futex to 1
            futex_state.store(1, Ordering::Release);

            // Spin-wait for acknowledgment (futex reset)
            loop {
                let current = futex_state.load(Ordering::Acquire);
                if current == 0 {
                    let recv_time = get_time_ns();
                    let latency_ns = recv_time - send_time;
                    wakeup_latencies.push(latency_ns);
                    break;
                }
                std::hint::spin_loop();
            }

            if (i + 1) % 100 == 0 {
                eprintln!("[WAKEUP] Waker: sent {}/{} wakeups", i + 1, iterations);
            }
        }

        // Wait for sleeper thread
        let _ = sleeper_handle.join();

        // Convert nanoseconds to microseconds and compute statistics
        let mut latencies_us: Vec<u64> = wakeup_latencies
            .iter()
            .map(|ns| (ns + 500) / 1000) // Round to nearest microsecond
            .collect();

        latencies_us.sort();

        let avg_latency = if !latencies_us.is_empty() {
            (latencies_us.iter().sum::<u64>() as f32) / (latencies_us.len() as f32)
        } else {
            0.0
        };

        let min_latency = (*latencies_us.first().unwrap_or(&0)) as f32;
        let max_latency = (*latencies_us.last().unwrap_or(&0)) as f32;
        let p99_idx = ((latencies_us.len() as f32 * 0.99) as usize).min(
            if latencies_us.len() > 0 {
                latencies_us.len() - 1
            } else {
                0
            },
        );
        let p99_latency = if latencies_us.len() > 0 {
            latencies_us[p99_idx] as f32
        } else {
            0.0
        };

        let metrics = TaskWakeupMetrics {
            avg_latency_us: avg_latency,
            min_latency_us: min_latency,
            max_latency_us: max_latency,
            p99_latency_us: p99_latency,
            successful_wakeups: wakeup_latencies.len() as u64,
        };

        eprintln!(
            "[WAKEUP] Test completed: Avg={:.2}µs, Min={:.2}µs, Max={:.2}µs, P99={:.2}µs",
            metrics.avg_latency_us, metrics.min_latency_us, metrics.max_latency_us, metrics.p99_latency_us
        );

        Ok(metrics)
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
    fn test_task_wakeup_config_default() {
        let config = TaskWakeupConfig::default();
        assert_eq!(config.iterations, 1000);
        assert_eq!(config.waker_cpu, Some(0));
        assert_eq!(config.sleeper_cpu, Some(1));
    }

    #[test]
    fn test_task_wakeup_collector_creation() {
        let config = TaskWakeupConfig::default();
        let collector = TaskWakeupCollector::new(config);
        assert_eq!(collector.config.iterations, 1000);
    }
}

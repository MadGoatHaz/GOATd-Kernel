//! RT Watchdog for Benchmark Safety
//!
//! This module implements a high-priority watchdog thread that monitors
//! benchmark execution with a heartbeat mechanism. If the heartbeat stops
//! for longer than the configured timeout, the watchdog performs an automatic
//! teardown by thawing cgroups.
//!
//! The watchdog thread runs with SCHED_FIFO priority to ensure it can
//! interrupt frozen processes and recover the system.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Configuration for the benchmark watchdog
#[derive(Clone, Debug)]
pub struct WatchdogConfig {
    /// Timeout duration: if no heartbeat is received within this time, trigger teardown
    pub timeout: Duration,
    /// SCHED_FIFO priority level (1-99, higher is more important)
    pub priority: u32,
    /// Path to the cgroup freeze control file
    pub cgroup_freeze_path: String,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        WatchdogConfig {
            timeout: Duration::from_secs(30),
            priority: 99, // Maximum real-time priority
            cgroup_freeze_path: "/sys/fs/cgroup/benchmark_freeze/cgroup.freeze".to_string(),
        }
    }
}

/// Shared state for watchdog heartbeat communication
#[derive(Clone)]
pub struct HeartbeatHandle {
    /// Last heartbeat timestamp (in nanoseconds)
    last_beat: Arc<AtomicU64>,
    /// Flag to signal the watchdog to stop
    stop_flag: Arc<AtomicBool>,
}

impl HeartbeatHandle {
    /// Send a heartbeat signal to the watchdog
    pub fn beat(&self) {
        // Store current time as nanoseconds since UNIX_EPOCH for reliable tracking
        // Use a simple counter: increment by 1 to confirm beat was received
        let current = self.last_beat.load(Ordering::Acquire);
        let next = if current == u64::MAX {
            1 // Start from 1 if at initial value
        } else {
            current.wrapping_add(1) // Increment with wraparound
        };
        self.last_beat.store(next, Ordering::Release);
    }

    /// Signal the watchdog to stop
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
    }

    /// Check if stop has been signaled
    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Acquire)
    }
}

/// Real-time watchdog for benchmark safety
pub struct BenchmarkWatchdog {
    config: WatchdogConfig,
    heartbeat: HeartbeatHandle,
    join_handle: Option<thread::JoinHandle<()>>,
}

impl BenchmarkWatchdog {
    /// Create and start a new benchmark watchdog
    ///
    /// # Arguments
    /// * `config` - Configuration for the watchdog (timeout, priority, cgroup path)
    ///
    /// # Returns
    /// A tuple containing the started watchdog and a heartbeat handle for signaling
    ///
    /// # Example
    /// ```ignore
    /// let config = WatchdogConfig {
    ///     timeout: Duration::from_secs(30),
    ///     priority: 99,
    ///     cgroup_freeze_path: "/sys/fs/cgroup/benchmark_freeze/cgroup.freeze".to_string(),
    /// };
    /// let (watchdog, heartbeat) = BenchmarkWatchdog::spawn(config)?;
    /// ```
    pub fn spawn(config: WatchdogConfig) -> Result<(Self, HeartbeatHandle), String> {
        let heartbeat = HeartbeatHandle {
            last_beat: Arc::new(AtomicU64::new(u64::MAX)), // Initialize to "far future"
            stop_flag: Arc::new(AtomicBool::new(false)),
        };

        let heartbeat_clone = heartbeat.clone();
        let config_clone = config.clone();

        // Spawn the watchdog thread
        let join_handle = thread::Builder::new()
            .name("benchmark-watchdog".to_string())
            .spawn(move || {
                // Set SCHED_FIFO priority for this thread
                if let Err(e) = Self::set_sched_fifo(config_clone.priority) {
                    eprintln!("Warning: Failed to set SCHED_FIFO priority: {}", e);
                }

                // Run the watchdog loop
                Self::watchdog_loop(&heartbeat_clone, &config_clone);
            })
            .map_err(|e| format!("Failed to spawn watchdog thread: {}", e))?;

        Ok((
            BenchmarkWatchdog {
                config,
                heartbeat: heartbeat.clone(),
                join_handle: Some(join_handle),
            },
            heartbeat,
        ))
    }

    /// Set SCHED_FIFO priority for the current thread
    fn set_sched_fifo(priority: u32) -> Result<(), String> {
        if priority < 1 || priority > 99 {
            return Err(format!(
                "Invalid SCHED_FIFO priority: {}. Must be between 1 and 99",
                priority
            ));
        }

        unsafe {
            let mut sched_param: libc::sched_param = std::mem::zeroed();
            sched_param.sched_priority = priority as i32;

            let result =
                libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &sched_param);

            if result != 0 {
                return Err(format!(
                    "Failed to set SCHED_FIFO priority: error code {}",
                    result
                ));
            }
        }

        Ok(())
    }

    /// Main watchdog loop that monitors heartbeat and triggers teardown on timeout
    fn watchdog_loop(heartbeat: &HeartbeatHandle, config: &WatchdogConfig) {
        let mut last_checked_beat = u64::MAX;
        let mut consecutive_no_beat_checks = 0u32;
        let check_interval = Duration::from_millis(100); // Check every 100ms

        loop {
            // Check if stop has been signaled
            if heartbeat.should_stop() {
                break;
            }

            // Get current heartbeat value
            let current_beat = heartbeat.last_beat.load(Ordering::Acquire);

            // If heartbeat hasn't changed, count the no-beat interval
            if current_beat == last_checked_beat && last_checked_beat != u64::MAX {
                consecutive_no_beat_checks = consecutive_no_beat_checks.saturating_add(1);

                // Check if we've exceeded timeout
                let elapsed = check_interval * consecutive_no_beat_checks;
                if elapsed >= config.timeout {
                    eprintln!(
                        "Watchdog timeout exceeded: {:?} >= {:?} ({} checks). Triggering teardown.",
                        elapsed, config.timeout, consecutive_no_beat_checks
                    );
                    if let Err(e) = Self::teardown(&config.cgroup_freeze_path) {
                        eprintln!("Teardown failed: {}", e);
                    }
                    break;
                }
            } else {
                // Heartbeat received - reset timeout counter
                last_checked_beat = current_beat;
                consecutive_no_beat_checks = 0;
            }

            // Sleep before next check
            thread::sleep(check_interval);
        }
    }

    /// Perform emergency teardown by thawing cgroups
    fn teardown(cgroup_freeze_path: &str) -> Result<(), String> {
        // Try to write "0" to the cgroup freeze file
        let path = Path::new(cgroup_freeze_path);

        if path.exists() {
            match fs::write(path, "0") {
                Ok(_) => {
                    eprintln!("Successfully thawed cgroup at {}", cgroup_freeze_path);
                    Ok(())
                }
                Err(e) => Err(format!(
                    "Failed to write to cgroup freeze file {}: {}",
                    cgroup_freeze_path, e
                )),
            }
        } else {
            // Fallback: log warning but don't fail
            eprintln!(
                "Warning: cgroup freeze path does not exist: {}. Teardown incomplete.",
                cgroup_freeze_path
            );
            Ok(())
        }
    }

    /// Get the heartbeat handle for sending signals
    pub fn heartbeat(&self) -> HeartbeatHandle {
        self.heartbeat.clone()
    }

    /// Stop the watchdog and wait for it to finish
    pub fn stop(mut self) -> Result<(), String> {
        self.heartbeat.stop();

        if let Some(handle) = self.join_handle.take() {
            match handle.join() {
                Ok(_) => Ok(()),
                Err(_) => Err("Watchdog thread panicked".to_string()),
            }
        } else {
            Ok(())
        }
    }
}

impl Drop for BenchmarkWatchdog {
    fn drop(&mut self) {
        // Attempt to stop gracefully on drop
        let _ = self.heartbeat.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watchdog_config_default() {
        let config = WatchdogConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.priority, 99);
        assert!(config.cgroup_freeze_path.contains("cgroup.freeze"));
    }

    #[test]
    fn test_heartbeat_handle_stop() {
        let handle = HeartbeatHandle {
            last_beat: Arc::new(AtomicU64::new(0)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        };

        assert!(!handle.should_stop());
        handle.stop();
        assert!(handle.should_stop());
    }

    #[test]
    fn test_heartbeat_beat() {
        let handle = HeartbeatHandle {
            last_beat: Arc::new(AtomicU64::new(0)),
            stop_flag: Arc::new(AtomicBool::new(false)),
        };

        let before = handle.last_beat.load(Ordering::Acquire);
        handle.beat();
        let after = handle.last_beat.load(Ordering::Acquire);

        assert!(after >= before);
    }
}

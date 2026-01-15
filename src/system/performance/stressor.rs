//! Advanced Stressor Orchestration Engine
//!
//! Implements multi-threaded stress testing to validate kernel determinism under load.
//! Stressors are spawned as background worker threads with:
//! - Nice 19 priority (lowest CPU priority)
//! - SCHED_IDLE scheduling policy
//! - CPU affinity masking (excluded from target_core)
//!
//! ## Stressor Types:
//! - **CPU**: SIMD/Matrix math loop for computational load
//! - **Memory**: Volatile random writes for cache thrashing
//! - **Scheduler**: High-frequency spawn/yield for runqueue flooding

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use nix::sched::sched_setaffinity;
use nix::sched::CpuSet;
use nix::unistd::Pid;

/// Enumeration of available stressor types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StressorType {
    /// CPU-intensive SIMD/matrix math operations
    Cpu,
    /// Memory-intensive cache thrashing via volatile writes
    Memory,
    /// Scheduler-intensive spawn/yield flooding
    Scheduler,
}

impl std::fmt::Display for StressorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StressorType::Cpu => write!(f, "CPU"),
            StressorType::Memory => write!(f, "Memory"),
            StressorType::Scheduler => write!(f, "Scheduler"),
        }
    }
}

/// Intensity level for stressor load (0-100)
#[derive(Clone, Copy, Debug)]
pub struct Intensity(u8);

impl Intensity {
    /// Create a new intensity (clamped to 0-100)
    pub fn new(value: u8) -> Self {
        Intensity(value.min(100))
    }

    /// Get the intensity value
    pub fn value(&self) -> u8 {
        self.0
    }
}

impl Default for Intensity {
    fn default() -> Self {
        Intensity(50) // 50% default intensity
    }
}

/// Information about an active stressor worker
#[derive(Debug)]
struct StressorWorker {
    stressor_type: StressorType,
    handle: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
    iteration_count: Arc<AtomicUsize>,
}

/// The StressorManager orchestrates all background stress workers.
/// It spawns and manages the lifecycle of CPU, Memory, and Scheduler stressors.
pub struct StressorManager {
    /// Map of active stressors by type
    workers: Vec<StressorWorker>,
    /// Target core to exclude from stressor affinity
    target_core: usize,
    /// Total number of available cores
    num_cores: usize,
    /// Tracks whether the manager has been stopped
    stopped: bool,
}

impl StressorManager {
    /// Create a new StressorManager with knowledge of the target measurement core
    pub fn new(target_core: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let num_cores = num_cpus::get();
        if target_core >= num_cores {
            return Err(format!("target_core {} out of range (available: {})", target_core, num_cores).into());
        }

        eprintln!("[STRESSOR_MGR] Initialized: target_core={}, num_cores={}", target_core, num_cores);

        Ok(StressorManager {
            workers: Vec::new(),
            target_core,
            num_cores,
            stopped: false,
        })
    }

    /// Spawn a new stressor with the given type and intensity
    pub fn start_stressor(
        &mut self,
        stressor_type: StressorType,
        intensity: Intensity,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.stopped {
            return Err("Cannot start stressor after stop_all_stressors() called".into());
        }

        // Check if this stressor type is already running
        if self.workers.iter().any(|w| w.stressor_type == stressor_type) {
            eprintln!("[STRESSOR_MGR] {} stressor already running", stressor_type);
            return Ok(());
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let iteration_count = Arc::new(AtomicUsize::new(0));

        let target_core = self.target_core;
        let num_cores = self.num_cores;
        let stop_flag_clone = Arc::clone(&stop_flag);
        let iteration_count_clone = Arc::clone(&iteration_count);

        let handle = thread::spawn(move || {
            eprintln!("[STRESSOR_{}] Worker starting (intensity={}%)", stressor_type, intensity.value());

            // Set up the stressor worker environment
            if let Err(e) = setup_stressor_environment(target_core, num_cores) {
                eprintln!("[STRESSOR_{}] ✗ Failed to setup environment: {}", stressor_type, e);
                return;
            }

            eprintln!("[STRESSOR_{}] ✓ Environment setup complete", stressor_type);

            // Run the appropriate stressor routine
            match stressor_type {
                StressorType::Cpu => {
                    cpu_stressor_routine(
                        intensity,
                        &stop_flag_clone,
                        &iteration_count_clone,
                    );
                }
                StressorType::Memory => {
                    memory_stressor_routine(
                        intensity,
                        &stop_flag_clone,
                        &iteration_count_clone,
                    );
                }
                StressorType::Scheduler => {
                    scheduler_stressor_routine(
                        intensity,
                        &stop_flag_clone,
                        &iteration_count_clone,
                    );
                }
            }

            eprintln!(
                "[STRESSOR_{}] Worker stopped after {} iterations",
                stressor_type,
                iteration_count_clone.load(Ordering::Relaxed)
            );
        });

        let worker = StressorWorker {
            stressor_type,
            handle: Some(handle),
            stop_flag,
            iteration_count,
        };

        self.workers.push(worker);
        eprintln!("[STRESSOR_MGR] Started {} stressor (intensity={}%)", stressor_type, intensity.value());
        Ok(())
    }

    /// Stop all active stressors and wait for graceful termination
    pub fn stop_all_stressors(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!("[STRESSOR_MGR] Stopping all {} stressors", self.workers.len());

        // Signal all workers to stop
        for worker in &self.workers {
            worker.stop_flag.store(true, Ordering::Release);
        }

        // Wait for all workers to finish
        for worker in &mut self.workers {
            if let Some(handle) = worker.handle.take() {
                match handle.join() {
                    Ok(_) => {
                        eprintln!(
                            "[STRESSOR_MGR] ✓ {} worker terminated gracefully ({}k iterations)",
                            worker.stressor_type,
                            worker.iteration_count.load(Ordering::Relaxed) / 1000
                        );
                    }
                    Err(_) => {
                        eprintln!("[STRESSOR_MGR] ✗ {} worker panicked during shutdown", worker.stressor_type);
                    }
                }
            }
        }

        self.workers.clear();
        self.stopped = true;
        eprintln!("[STRESSOR_MGR] All stressors stopped");

        Ok(())
    }

    /// Get the count of active stressors
    pub fn active_count(&self) -> usize {
        self.workers.len()
    }

    /// Get iteration count for a specific stressor type
    pub fn get_iteration_count(&self, stressor_type: StressorType) -> usize {
        self.workers
            .iter()
            .find(|w| w.stressor_type == stressor_type)
            .map(|w| w.iteration_count.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

impl Drop for StressorManager {
    fn drop(&mut self) {
        if !self.stopped && !self.workers.is_empty() {
            eprintln!("[STRESSOR_MGR] Drop: Forcibly stopping {} active workers", self.workers.len());
            if let Err(e) = self.stop_all_stressors() {
                eprintln!("[STRESSOR_MGR] ✗ Error during drop cleanup: {}", e);
            }
        }
    }
}

/// Setup the environment for a stressor worker:
/// - Set nice priority to 19 (lowest CPU priority)
/// - Set SCHED_IDLE scheduling policy
/// - Set CPU affinity to all cores except target_core
fn setup_stressor_environment(target_core: usize, num_cores: usize) -> Result<(), Box<dyn std::error::Error>> {
    // Set nice priority to 19 (lowest CPU priority)
    // Using getrusage/setrusage via nix is not directly available, so we use libc
    unsafe {
        let ret = libc::nice(19);
        if ret < 0 {
            return Err(format!("nice() failed with errno: {}", std::io::Error::last_os_error()).into());
        }
    }

    // Set SCHED_IDLE scheduling policy with priority 0
    unsafe {
        let mut param: libc::sched_param = std::mem::zeroed();
        param.sched_priority = 0;

        let ret = libc::sched_setscheduler(0, libc::SCHED_IDLE, &param);
        if ret < 0 {
            eprintln!("[SETUP_STRESSOR] Warning: SCHED_IDLE not available, continuing with default scheduler");
            // Don't fail - SCHED_IDLE might not be available, but nice priority is enough
        }
    }

    // Set CPU affinity to all cores except target_core
    let mut cpu_set = CpuSet::new();
    for core in 0..num_cores {
        if core != target_core {
            cpu_set.set(core)?;
        }
    }

    sched_setaffinity(Pid::from_raw(0), &cpu_set)?;

    Ok(())
}

/// CPU Stressor: SIMD/Matrix math loop
/// Performs intensive floating-point operations to load the CPU
fn cpu_stressor_routine(
    intensity: Intensity,
    stop_flag: &Arc<AtomicBool>,
    iteration_count: &Arc<AtomicUsize>,
) {
    let intensity_factor = (intensity.value() as f64) / 100.0;
    let iterations_per_batch = (10000.0 * intensity_factor) as usize;

    eprintln!("[CPU_STRESSOR] Starting: intensity_factor={:.2}, iterations={}", intensity_factor, iterations_per_batch);

    // Pre-allocate working memory for the matrix
    let matrix_size = 64;
    let mut a: Vec<Vec<f64>> = vec![vec![0.0; matrix_size]; matrix_size];
    let mut b: Vec<Vec<f64>> = vec![vec![0.0; matrix_size]; matrix_size];
    let mut c: Vec<Vec<f64>> = vec![vec![0.0; matrix_size]; matrix_size];

    // Initialize matrices
    for i in 0..matrix_size {
        for j in 0..matrix_size {
            a[i][j] = (i as f64) * (j as f64) * 0.001;
            b[i][j] = (i as f64 + j as f64) * 0.001;
        }
    }

    let mut batch_count = 0;
    while !stop_flag.load(Ordering::Relaxed) {
        // Perform matrix multiplication
        for i in 0..matrix_size {
            for j in 0..matrix_size {
                let mut sum = 0.0;
                for k in 0..matrix_size {
                    sum += a[i][k] * b[k][j];
                }
                // Use volatile write to prevent compiler optimization
                unsafe {
                    std::ptr::write_volatile(&mut c[i][j], sum);
                }
            }
        }

        // Perform SIMD-like operations with manual loop
        for i in 0..iterations_per_batch {
            let x = (i as f64).sin() * (i as f64).cos();
            let y = x.sqrt().abs();
            let z = y * y + x;
            unsafe {
                std::ptr::write_volatile(&mut a[0][0], z);
            }
        }

        batch_count += 1;
        iteration_count.fetch_add(iterations_per_batch, Ordering::Relaxed);

        if batch_count % 10 == 0 {
            eprintln!("[CPU_STRESSOR] Progress: {} iterations", batch_count * iterations_per_batch);
        }
    }

    eprintln!("[CPU_STRESSOR] Completed {} batches", batch_count);
}

/// Memory Stressor: Volatile random writes for cache thrashing
/// Continuously writes to random locations in a large buffer
fn memory_stressor_routine(
    intensity: Intensity,
    stop_flag: &Arc<AtomicBool>,
    iteration_count: &Arc<AtomicUsize>,
) {
    let intensity_factor = (intensity.value() as f64) / 100.0;
    // Allocate memory: 8MB base + intensity scaling
    let buffer_size = (8 * 1024 * 1024) + ((intensity_factor * 56.0 * 1024.0 * 1024.0) as usize);

    eprintln!(
        "[MEMORY_STRESSOR] Starting: buffer_size={} MB, intensity_factor={:.2}",
        buffer_size / (1024 * 1024),
        intensity_factor
    );

    // Allocate large buffer on heap
    let mut buffer = vec![0u8; buffer_size];

    // Use a simple linear congruential generator for pseudo-random access
    let mut seed: u64 = 0xdeadbeef;
    let multiplier: u64 = 6364136223846793005;
    let increment: u64 = 1442695040888963407;

    let mut iteration = 0u64;
    while !stop_flag.load(Ordering::Relaxed) {
        // Generate pseudo-random index
        seed = seed.wrapping_mul(multiplier).wrapping_add(increment);
        let index = ((seed >> 32) as usize) % buffer_size;

        // Volatile write to prevent optimization
        unsafe {
            std::ptr::write_volatile(&mut buffer[index], (seed & 0xFF) as u8);
        }

        iteration += 1;
        if iteration % 1000 == 0 {
            iteration_count.fetch_add(1000, Ordering::Relaxed);
        }

        if iteration % 100_000 == 0 {
            eprintln!("[MEMORY_STRESSOR] Progress: {} iterations", iteration);
        }
    }

    iteration_count.fetch_add((iteration % 1000) as usize, Ordering::Relaxed);
    eprintln!("[MEMORY_STRESSOR] Completed {} memory accesses", iteration);
}

/// Scheduler Stressor: High-frequency spawn/yield for runqueue flooding
/// Spawns many short-lived threads to flood the scheduler's runqueue
fn scheduler_stressor_routine(
    intensity: Intensity,
    stop_flag: &Arc<AtomicBool>,
    iteration_count: &Arc<AtomicUsize>,
) {
    let intensity_factor = (intensity.value() as f64) / 100.0;
    let threads_per_batch = ((20.0 * intensity_factor) as usize).max(1);

    eprintln!(
        "[SCHEDULER_STRESSOR] Starting: threads_per_batch={}, intensity_factor={:.2}",
        threads_per_batch, intensity_factor
    );

    let mut batch_count = 0;
    while !stop_flag.load(Ordering::Relaxed) {
        let mut handles = Vec::with_capacity(threads_per_batch);

        // Spawn a batch of short-lived threads
        for _ in 0..threads_per_batch {
            let handle = thread::spawn(|| {
                // Do minimal work to create scheduler contention
                thread::yield_now();
                for _ in 0..1000 {
                    // Volatile to prevent optimization
                    unsafe {
                        std::ptr::write_volatile(&mut [0u8; 1][0], 1);
                    }
                }
                thread::yield_now();
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            let _ = handle.join();
        }

        batch_count += 1;
        iteration_count.fetch_add(threads_per_batch, Ordering::Relaxed);

        if batch_count % 100 == 0 {
            eprintln!("[SCHEDULER_STRESSOR] Progress: {} batches ({} threads spawned)", batch_count, batch_count * threads_per_batch);
        }
    }

    eprintln!("[SCHEDULER_STRESSOR] Completed {} batches ({} threads total)", batch_count, batch_count * threads_per_batch);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intensity_bounds() {
        let intensity = Intensity::new(150); // Should clamp to 100
        assert_eq!(intensity.value(), 100);
    }

    #[test]
    fn test_intensity_default() {
        let intensity = Intensity::default();
        assert_eq!(intensity.value(), 50);
    }

    #[test]
    fn test_stressor_manager_creation() {
        let manager = StressorManager::new(0);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_stressor_manager_invalid_core() {
        let num_cores = num_cpus::get();
        let manager = StressorManager::new(num_cores + 1);
        assert!(manager.is_err());
    }

    #[test]
    fn test_stressor_manager_active_count() {
        let manager = StressorManager::new(0);
        assert!(manager.is_ok());
        let manager = manager.unwrap();
        assert_eq!(manager.active_count(), 0);
    }
}

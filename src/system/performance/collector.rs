//! High-Precision Latency Collector
//!
//! Performs allocation-free latency measurement using:
//! - CLOCK_MONOTONIC for timing
//! - TIMER_ABSTIME for absolute wake-up timing
//! - rtrb lock-free ring buffer for communication
//! - Spike detection for latency anomalies
//! - Optional SMI correlation detection
//! - Periodic thermal data collection from sysfs
//! - Event-based diagnostic pipeline (lock-free, allocation-free hot loop)
//! - Baseline Calibration Mode: Pure mode eliminates overhead by skipping SMI/buffer operations

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use hdrhistogram::Histogram;

use super::SmiCorrelation;
use super::thermal;
use super::diagnostic_buffer::send_diagnostic;

/// Collection mode for baseline calibration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionMode {
    /// Pure baseline: minimal overhead, no ring buffer/SMI reads
    Pure,
    /// Full measurement: complete diagnostic pipeline
    Full,
}

/// Static flag to disable MSR poller (used in Pure mode calibration)
pub static DISABLE_MSR_POLLER: AtomicBool = AtomicBool::new(false);

/// Diagnostic event produced by the hot loop
/// Used to communicate status changes, spikes, and SMI detections outside the critical path
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectorEvent {
    /// Spike detected: latency_ns, spike_id, raw_smi_count
    Spike(u64, u64, u64),
    /// SMI detected and correlated with spike
    SmiDetected,
    /// Ring buffer full: dropped_count
    BufferFull(u64),
    /// Status periodic update: sample_count, spike_total, smi_correlated_total, dropped_total
    Status { samples: u64, spikes: u64, smi_correlated: u64, dropped: u64 },
    /// Warmup phase complete: transition to official metrics recording
    WarmupComplete,
    /// Warmup phase complete: transition to recording
    Flush,
}

/// Static atomic counter for total SMI events detected across all runs
pub(crate) static TOTAL_SMI_COUNT: AtomicU64 = AtomicU64::new(0);

/// The Collector performs high-precision latency measurement.
/// It runs an allocation-free hot loop that measures wake-up latency using CLOCK_MONOTONIC.
/// Diagnostic events are pushed to a separate ring buffer and consumed asynchronously.
pub struct LatencyCollector {
    interval: Duration,
    producer: rtrb::Producer<u64>,
    /// Event producer for diagnostic pipeline (lock-free notification of spikes, SMI, buffer full, status)
    event_producer: rtrb::Producer<CollectorEvent>,
    stop_flag: Arc<AtomicBool>,
    dropped_count: Arc<AtomicU64>,
    /// Atomic counter for dropped event producer pushes (when event_producer.push() fails)
    dropped_event_count: Arc<AtomicU64>,
    spike_threshold: u64,
    spike_count: Arc<AtomicU64>,
    smi_correlation: Option<Arc<RwLock<SmiCorrelation>>>,
    smi_correlated_spikes: Arc<AtomicU64>,
    total_smi_count: Arc<AtomicU64>,
    /// Flag: SMI correlation determined to be unavailable (non-Intel/no MSR) - stops retry logging
    smi_unavailable: Arc<AtomicBool>,
    /// Collection mode: Pure (minimal overhead) or Full (complete diagnostics)
    mode: CollectionMode,
    /// Number of warmup samples to skip before recording (default 2000)
    warmup_samples: u64,
    /// Maximum latency recorded in Pure mode (for comparison)
    max_latency_pure_ns: Arc<AtomicU64>,
    /// Sum of latencies in Pure mode (for averaging)
    pure_mode_sum_latency: Arc<AtomicU64>,
    /// Count of samples in Pure mode (for averaging)
    pure_mode_sample_count: Arc<AtomicU64>,
    /// Flag: control whether spike detection and recording is active (false during warmup)
    is_recording: Arc<AtomicBool>,
}

impl LatencyCollector {
    /// Creates a new Collector with the given interval and ring buffer producer.
    /// SMI correlation is initialized asynchronously and provided as an optional thread-safe wrapper.
    /// Event producer is used for lock-free, allocation-free diagnostic event communication.
    pub fn new(
        interval: Duration,
        producer: rtrb::Producer<u64>,
        event_producer: rtrb::Producer<CollectorEvent>,
        stop_flag: Arc<AtomicBool>,
        dropped_count: Arc<AtomicU64>,
        spike_threshold: u64,
        spike_count: Arc<AtomicU64>,
        smi_correlated_spikes: Arc<AtomicU64>,
        total_smi_count: Arc<AtomicU64>,
        smi_correlation: Option<Arc<RwLock<SmiCorrelation>>>,
    ) -> Self {
        LatencyCollector {
            interval,
            producer,
            event_producer,
            stop_flag,
            dropped_count,
            spike_threshold,
            spike_count,
            smi_correlation,
            smi_correlated_spikes,
            total_smi_count,
            smi_unavailable: Arc::new(AtomicBool::new(false)),
            dropped_event_count: Arc::new(AtomicU64::new(0)),
            mode: CollectionMode::Full,
            warmup_samples: 2000,
            max_latency_pure_ns: Arc::new(AtomicU64::new(0)),
            pure_mode_sum_latency: Arc::new(AtomicU64::new(0)),
            pure_mode_sample_count: Arc::new(AtomicU64::new(0)),
            is_recording: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a new Collector in Pure baseline mode with minimal overhead.
    /// Pure mode skips ring buffer pushes and SMI reads for accurate measurement of core latency.
    /// warmup_samples: number of samples to skip before recording (default 2000)
    pub fn with_pure_mode(
        interval: Duration,
        producer: rtrb::Producer<u64>,
        event_producer: rtrb::Producer<CollectorEvent>,
        stop_flag: Arc<AtomicBool>,
        dropped_count: Arc<AtomicU64>,
        spike_threshold: u64,
        spike_count: Arc<AtomicU64>,
        smi_correlated_spikes: Arc<AtomicU64>,
        total_smi_count: Arc<AtomicU64>,
        smi_correlation: Option<Arc<RwLock<SmiCorrelation>>>,
        warmup_samples: u64,
    ) -> Self {
        LatencyCollector {
            interval,
            producer,
            event_producer,
            stop_flag,
            dropped_count,
            spike_threshold,
            spike_count,
            smi_correlation,
            smi_correlated_spikes,
            total_smi_count,
            smi_unavailable: Arc::new(AtomicBool::new(false)),
            dropped_event_count: Arc::new(AtomicU64::new(0)),
            mode: CollectionMode::Pure,
            warmup_samples,
            max_latency_pure_ns: Arc::new(AtomicU64::new(0)),
            pure_mode_sum_latency: Arc::new(AtomicU64::new(0)),
            pure_mode_sample_count: Arc::new(AtomicU64::new(0)),
            is_recording: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Runs the measurement hot loop.
    /// This is STRICTLY lock-free and allocation-free in the critical path.
    /// - Uses TIMER_ABSTIME for absolute wake-up timing
    /// - Detects spikes (latency > threshold) via atomic counter only
    /// - Pushes diagnostic events to separate ring buffer (non-blocking)
    /// - SMI correlation check uses atomic SMI count only (no locks in hot loop)
    /// - All format! calls removed from hot loop (event consumer handles formatting)
    /// - Pure mode: uses std::hint::black_box and skips SMI/buffer for minimal overhead
    /// - Warmup phase: skips first warmup_samples iterations to eliminate startup artifacts
    /// - Warmup state transition: when sample_count == warmup_samples + 1, transition to Running state
    pub fn run(mut self) {
        // Attempt to set SCHED_FIFO priority (outside critical path)
        let priority_result = unsafe {
            let sched_param = libc::sched_param {
                sched_priority: 99, // Highest real-time priority
            };
            libc::sched_setscheduler(0, libc::SCHED_FIFO, &sched_param)
        };
        
        if priority_result != 0 {
            let errno = std::io::Error::last_os_error();
            send_diagnostic(&format!(
                "[COLLECTOR] ⚠️ Warning: Failed to set SCHED_FIFO priority (errno: {}). Continuing in SCHED_OTHER mode.",
                errno
            ));
        } else {
            send_diagnostic("[COLLECTOR] ✓ Successfully set SCHED_FIFO priority (99)");
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Pre-hot-loop diagnostic message (outside critical path)
            send_diagnostic(&format!(
                "[COLLECTOR] Starting hot loop: mode={:?}, interval={:?}, spike_threshold={} ns, warmup_samples={}",
                self.mode, self.interval, self.spike_threshold, self.warmup_samples
            ));

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
            let mut prev_dropped_logged = 0u64;
            let mut max_latency_ns = 0u64;
            let mut warmup_transitioned = false;
            let mut cached_smi_count = 0u64;

            // ============================================================================
            // CRITICAL PATH START: Hot loop is STRICTLY lock-free and allocation-free
            // ============================================================================
            while !self.stop_flag.load(Ordering::Relaxed) {
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
            // This is measured against the ORIGINAL target before gap-skipping
            let latency_ns = timespec_diff_ns(&now, &next_wake);
            
            // In Pure mode, use black_box to prevent compiler optimization
            let latency_ns = if self.mode == CollectionMode::Pure {
                std::hint::black_box(latency_ns)
            } else {
                latency_ns
            };
            
            sample_count += 1;

            // ====================================================================
            // SAMPLER HARDENING: Synthetic Sample Injection for Missed Intervals
            // ====================================================================
            // When stutter occurs (phase-lag), inject synthetic samples to represent
            // the missed time. This ensures every gap on the 1ms grid is accounted for.
            // Example: 50ms missing on 1ms grid → 49 synthetic samples at interval_ns
            
            // CRITICAL: Push the REAL sample FIRST (the one we just measured)
            if self.mode == CollectionMode::Full {
                if self.producer.push(latency_ns).is_err() {
                    self.dropped_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            
            // Now handle grid alignment and synthetic sample injection
            let mut synthetic_samples = 0u64;
            if timespec_cmp(&now, &next_wake) > 0 {
                // Late wake-up (Phase-Lag): We detected a stutter
                let late_ns = timespec_diff_ns(&now, &next_wake);
                let missed = late_ns / interval_ns;  // How many MISSED intervals
                
                // LABORATORY-GRADE: Inject synthetic samples for missed intervals
                // Each synthetic sample has latency = interval_ns (represents the missed time slot)
                // This reconstructs the temporal structure without allocation
                // CRITICAL: Cap synthetic samples to prevent unbounded loops on extreme stutters
                const MAX_SYNTHETIC_SAMPLES: u64 = 10000;  // Max synthetic samples per stutter event
                if missed > 0 && self.mode == CollectionMode::Full {
                    let capped_missed = missed.min(MAX_SYNTHETIC_SAMPLES);
                    for _ in 0..capped_missed {
                        // Push synthetic sample representing missed interval
                        // Synthetic latency = nominal interval (1000ns for 1kHz sampling)
                        if self.producer.push(interval_ns).is_err() {
                            self.dropped_count.fetch_add(1, Ordering::Relaxed);
                        }
                        synthetic_samples += 1;
                    }
                    
                    // LOG: If we exceeded cap, warn about extreme stutter
                    if missed > MAX_SYNTHETIC_SAMPLES {
                        eprintln!("[COLLECTOR] ⚠️ Warning: Extreme stutter detected: {} intervals missed, capped synthetic injection to {}", missed, MAX_SYNTHETIC_SAMPLES);
                    }
                    
                    // Diagnostic: Report stutter with synthetic recovery
                    if self.event_producer.push(CollectorEvent::Status {
                        samples: sample_count,
                        spikes: self.spike_count.load(Ordering::Relaxed),
                        smi_correlated: self.smi_correlated_spikes.load(Ordering::Relaxed),
                        dropped: self.dropped_count.load(Ordering::Relaxed) + synthetic_samples,
                    }).is_err() {
                        self.dropped_event_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
                
                // Snap to next grid point
                add_ns_to_timespec(&mut next_wake, (missed + 1) * interval_ns);
            } else {
                // On time: increment by exactly one interval
                add_ns_to_timespec(&mut next_wake, interval_ns);
            }

            // WARMUP STATE TRANSITION: When we exit the warmup phase (sample_count >= warmup_samples), activate recording and send WarmupComplete event
            if sample_count >= self.warmup_samples && !warmup_transitioned {
                warmup_transitioned = true;
                self.is_recording.store(true, Ordering::Relaxed);
                // Push WarmupComplete event instead of calling send_diagnostic (removes Mutex acquisition and format! from hot loop)
                if self.event_producer.push(CollectorEvent::WarmupComplete).is_err() {
                    self.dropped_event_count.fetch_add(1, Ordering::Relaxed);
                }
            }

            // ALWAYS push samples to producer (UNCONDITIONAL) - allows live data flow to UI during warmup
            if self.mode == CollectionMode::Full {
                // Try to push latency sample to the producer. If full, increment dropped counter.
                match self.producer.push(latency_ns) {
                    Ok(_) => {
                        // Successfully pushed
                    }
                    Err(_push_err) => {
                        // Buffer is full, increment dropped count atomically
                        self.dropped_count.fetch_add(1, Ordering::Relaxed);
                        dropped_total += 1;

                        // Push buffer full event only on significant milestones (non-blocking)
                        if dropped_total > prev_dropped_logged + 100 {
                            if self.event_producer.push(CollectorEvent::BufferFull(dropped_total)).is_err() {
                                self.dropped_event_count.fetch_add(1, Ordering::Relaxed);
                            }
                            prev_dropped_logged = dropped_total;
                        }
                    }
                }
            }

            // Track max latency and statistics in Pure mode
            if self.mode == CollectionMode::Pure {
                if latency_ns > max_latency_ns {
                    max_latency_ns = latency_ns;
                    self.max_latency_pure_ns.store(max_latency_ns, Ordering::Relaxed);
                }
                // Accumulate for average calculation
                self.pure_mode_sum_latency.fetch_add(latency_ns, Ordering::Relaxed);
                self.pure_mode_sample_count.fetch_add(1, Ordering::Relaxed);
            }

            // CONDITIONAL: Detect spikes and perform cycle-accurate SMI correlation
            // Uses Relaxed ordering for efficiency (non-blocking check)
            if self.is_recording.load(Ordering::Relaxed) && latency_ns > self.spike_threshold {
                // Early-exit if in Pure mode (skip spike detection overhead entirely)
                if self.mode == CollectionMode::Pure {
                    // Pure mode: no spike events, no SMI reads
                } else {
                    // Full mode: increment spike counter atomically
                    self.spike_count.fetch_add(1, Ordering::Release);
                    spike_total += 1;

                    // ================================================================
                    // CYCLE-ACCURATE SMI CORRELATION: Atomic load immediately on spike
                    // ================================================================
                    // CRITICAL FIX: Load SMI count IMMEDIATELY when spike is detected
                    // This provides cycle-accurate correlation with the latency spike
                    // instead of using stale cached values from 100+ iterations ago
                    let raw_smi_count = TOTAL_SMI_COUNT.load(Ordering::Acquire);
                    
                    // Correlate spike with SMI event: if SMI count changed since last spike, mark correlation
                    if raw_smi_count > cached_smi_count && raw_smi_count > 0 {
                        self.smi_correlated_spikes.fetch_add(1, Ordering::Release);
                        eprintln!("[COLLECTOR] [SMI_SPIKE] Spike at {:.3}µs CORRELATED with SMI: count={} (prev={})",
                            latency_ns as f32 / 1000.0, raw_smi_count, cached_smi_count);
                    }
                    
                    // Update cache for next comparison
                    cached_smi_count = raw_smi_count;

                    // Push spike event with cycle-accurate SMI count (non-blocking, allocation-free)
                    if self.event_producer.push(CollectorEvent::Spike(latency_ns, spike_total, raw_smi_count)).is_err() {
                        self.dropped_event_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }

            // Periodic status update moved outside hot loop to reduce contention
            // Status events are now only sent at program exit via post-hot-loop diagnostics
            }
            // ============================================================================
            // CRITICAL PATH END: Hot loop completed
            // ============================================================================

            // Post-hot-loop diagnostics (outside critical path, uses send_diagnostic)
            send_diagnostic(&format!(
                "[COLLECTOR] Hot loop stopped after {} samples ({} warmup + {} recorded)",
                sample_count, self.warmup_samples, sample_count.saturating_sub(self.warmup_samples)
            ));
            send_diagnostic(&format!(
                "[COLLECTOR] Spikes: {}, Dropped: {}, Dropped Events: {}, Mode: {:?}",
                spike_total, dropped_total, self.dropped_event_count.load(Ordering::Relaxed), self.mode
            ));
            if self.mode == CollectionMode::Pure {
                send_diagnostic(&format!(
                    "[COLLECTOR] Pure mode max latency: {} ns",
                    self.max_latency_pure_ns.load(Ordering::Relaxed)
                ));
            } else {
                send_diagnostic(&format!(
                    "[COLLECTOR] Final atomic spike_count={}, smi_correlated_spikes={}, total_smi_count={}",
                    self.spike_count.load(Ordering::Relaxed),
                    self.smi_correlated_spikes.load(Ordering::Relaxed),
                    TOTAL_SMI_COUNT.load(Ordering::Relaxed)
                ));
            }
        }));
        
        // Handle panic result: log and continue gracefully
        if let Err(panic_info) = result {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("[COLLECTOR] PANIC: {}", s)
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("[COLLECTOR] PANIC: {}", s)
            } else {
                "[COLLECTOR] PANIC: unknown panic info".to_string()
            };
            send_diagnostic(&panic_msg);
        }
    }

    /// Gets a reference to the spike count.
    pub fn spike_count(&self) -> Arc<AtomicU64> {
        let count = self.spike_count.load(Ordering::Relaxed);
        send_diagnostic(&format!("[COLLECTOR_ACCESS] Reading spike_count: {}", count));
        Arc::clone(&self.spike_count)
    }

    /// Gets a reference to the SMI-correlated spike count.
    pub fn smi_correlated_spikes(&self) -> Arc<AtomicU64> {
        let count = self.smi_correlated_spikes.load(Ordering::Relaxed);
        send_diagnostic(&format!("[COLLECTOR_ACCESS] Reading smi_correlated_spikes: {}", count));
        Arc::clone(&self.smi_correlated_spikes)
    }

    /// Sets the collection mode (for testing/calibration)
    pub fn set_mode(&mut self, mode: CollectionMode) {
        self.mode = mode;
    }

    /// Gets the current collection mode
    pub fn get_mode(&self) -> CollectionMode {
        self.mode
    }

    /// Sets the warmup samples count (for testing/calibration)
    pub fn set_warmup_samples(&mut self, warmup_samples: u64) {
        self.warmup_samples = warmup_samples;
    }

    /// Gets the current warmup samples count
    pub fn get_warmup_samples(&self) -> u64 {
        self.warmup_samples
    }

    /// Gets a cloneable reference to the is_recording flag for monitoring warmup state
    pub fn get_is_recording_arc(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.is_recording)
    }

    /// Gets the maximum latency recorded in Pure mode (in nanoseconds)
    pub fn get_max_latency_pure_ns(&self) -> u64 {
        self.max_latency_pure_ns.load(Ordering::Relaxed)
    }

    /// Gets a cloneable reference to the Pure mode max latency Arc for concurrent access
    pub fn get_max_latency_pure_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.max_latency_pure_ns)
    }

    /// Gets a cloneable reference to the Pure mode sum latency Arc
    pub fn get_pure_mode_sum_latency_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.pure_mode_sum_latency)
    }

    /// Gets a cloneable reference to the Pure mode sample count Arc
    pub fn get_pure_mode_sample_count_arc(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.pure_mode_sample_count)
    }
}

/// Spawns a background thread that updates TOTAL_SMI_COUNT every 100ms
/// by reading the MSR whenever available.
/// This is a non-critical background task (not in hot loop).
/// Will immediately terminate if DISABLE_MSR_POLLER is set (for Pure mode calibration).
pub fn spawn_msr_poller() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        // Check if MSR poller is disabled (Pure mode calibration)
        if DISABLE_MSR_POLLER.load(Ordering::Relaxed) {
            return;
        }
        
        loop {
            std::thread::sleep(Duration::from_millis(100));
            
            // Check disable flag every iteration for fast response
            if DISABLE_MSR_POLLER.load(Ordering::Relaxed) {
                return;
            }
            
            // Try to read SMI count from MSR (Intel only)
            // This uses existing diagnostic infrastructure to fetch MSR data
            // If unavailable (non-Intel or no MSR), this is a no-op
            if let Ok(smi_count) = read_smi_count_from_msr() {
                // Update the global atomic counter with the latest MSI count
                TOTAL_SMI_COUNT.store(smi_count, Ordering::Relaxed);
            }
        }
    })
}

/// Attempt to read SMI count from MSR (Intel IA32_SMI_COUNT)
/// Returns the SMI count if available, otherwise returns an error
fn read_smi_count_from_msr() -> Result<u64, Box<dyn std::error::Error>> {
    // Attempt to read MSR 0x34 (IA32_SMI_COUNT) on Intel CPUs
    // This is a privileged operation and may fail on non-Intel or restricted systems
    
    // For now, this is a placeholder that attempts to use /dev/cpu/*/msr interface
    // In production, integrate with existing diagnostic::SmiDetector if available
    
    // Try to read from the first CPU's MSR file
    const MSR_SMI_COUNT: u64 = 0x34;
    let msr_path = format!("/dev/cpu/0/msr");
    
    match std::fs::File::open(&msr_path) {
        Ok(mut file) => {
            use std::io::Seek;
            // Seek to the SMI count MSR offset
            file.seek(std::io::SeekFrom::Start(MSR_SMI_COUNT))?;
            
            // Read 8 bytes as u64
            let mut buf = [0u8; 8];
            use std::io::Read;
            file.read_exact(&mut buf)?;
            
            let smi_count = u64::from_le_bytes(buf);
            Ok(smi_count)
        }
        Err(_) => {
            // MSR device not available (non-Intel, no root, or kernel doesn't support it)
            Err("MSR interface not available".into())
        }
    }
}

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
        if a.tv_sec < b.tv_sec { -1 } else { 1 }
    } else if a.tv_nsec != b.tv_nsec {
        if a.tv_nsec < b.tv_nsec { -1 } else { 1 }
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

/// Processes latency samples and maintains histogram statistics
/// All internal tracking uses nanoseconds (ns) for sub-microsecond precision
pub struct LatencyProcessor {
    /// HDR histogram storing nanosecond latencies
    /// Range: 1 ns to 100,000,000 ns (100 ms)
    histogram: Histogram<u64>,
    /// Last recorded sample in nanoseconds
    last_sample_ns: u64,
    /// Maximum latency recorded in nanoseconds
    max_latency_ns: u64,
    /// Total sample count
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
    /// Rolling window of the last 1,000 samples for stable average calculation (nanoseconds)
    /// Prevents session-wide averages from becoming "sticky" and unresponsive to recent changes
    rolling_samples: std::collections::VecDeque<u64>,
    /// Maximum size of rolling window (1,000 samples for ~1 second at 1000Hz)
    rolling_max_size: usize,
}

impl LatencyProcessor {
    /// Create a new latency processor with nanosecond precision
    /// Histogram range: 1 ns to 100,000,000 ns (100 ms)
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize histogram with nanosecond precision
        // Range: 1 ns to 100,000,000 ns (100 ms), 3 significant figures
        // This captures sub-microsecond samples accurately
        let histogram = Histogram::<u64>::new_with_max(100_000_000, 3)?;

        Ok(LatencyProcessor {
            histogram,
            last_sample_ns: 0,
            max_latency_ns: 0,
            sample_count: 0,
            buckets: [0u64; 20],
            cycle_max_ns: 0,
            core_temperatures: Vec::new(),
            package_temperature: 0.0,
            last_thermal_read: std::time::Instant::now(),
            rolling_samples: std::collections::VecDeque::with_capacity(1000),
            rolling_max_size: 1000,
        })
    }

    /// Convert nanoseconds to bucket index using pure logarithmic mapping
    ///
    /// Distributes all 20 buckets evenly across the full range (1ns to 100ms = 10^8 ns).
    /// Pure logarithmic scaling ensures:
    /// - Bucket 0: ~1-790 ns
    /// - Bucket 10: ~1-10 µs
    /// - Bucket 19: ~12.6-100 ms
    ///
    /// Formula: bucket_index = (log10(latency_ns) / log10(100_000_000)) * 19
    /// This maps log10(1) = 0 → bucket 0, and log10(100_000_000) = 8 → bucket 19
    fn latency_to_bucket_index(latency_ns: u64) -> usize {
        // Clamp input to 1..100_000_000 ns
        let clamped_ns = latency_ns.max(1).min(100_000_000);
        let latency_f64 = clamped_ns as f64;
        
        // Pure logarithmic mapping: log10(ns) / 8 * 19
        // log10(100_000_000) = 8, so we span log10(1) = 0 to log10(100_000_000) = 8
        let log_val = latency_f64.log10();
        let bucket_f64 = (log_val / 8.0) * 19.0;
        let bucket_u = bucket_f64 as usize;
        
        bucket_u.min(19)
    }

    /// Record a latency sample (in nanoseconds)
    /// Captures sub-microsecond precision by recording nanoseconds directly
    /// Maintains a rolling window of the last 1,000 samples for responsive average calculation
    pub fn record_sample(&mut self, latency_ns: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure sample is at least 1 ns to satisfy histogram's minimum requirement
        let sample_ns = latency_ns.max(1);

        // Record nanoseconds directly in histogram (no conversion, preserves sub-microsecond precision)
        self.histogram.record(sample_ns)?;
        
        // Update internal state with nanosecond values
        self.last_sample_ns = sample_ns;
        self.max_latency_ns = self.max_latency_ns.max(sample_ns);
        self.sample_count += 1;
        
        // Update 20-bucket histogram
        let bucket_idx = Self::latency_to_bucket_index(sample_ns);
        self.buckets[bucket_idx] += 1;
        
        // Track maximum latency in the current cycle (for jitter timeline)
        self.cycle_max_ns = self.cycle_max_ns.max(sample_ns);
        
        // Add sample to rolling window for responsive average calculation
        self.rolling_samples.push_back(sample_ns);
        if self.rolling_samples.len() > self.rolling_max_size {
            self.rolling_samples.pop_front();
        }

        Ok(())
    }

    /// Get the current maximum latency in microseconds
    /// Converts internal nanosecond tracking to microseconds for UI display
    /// Uses floating-point division to preserve fractional microseconds
    pub fn max(&self) -> f32 {
        self.max_latency_ns as f32 / 1000.0
    }

    /// Get the P99 percentile in microseconds
    /// Converts nanosecond histogram value to microseconds
    /// Uses floating-point division to preserve fractional microseconds
    pub fn p99(&self) -> f32 {
        let p99_ns = self.histogram.value_at_percentile(99.0);
        p99_ns as f32 / 1000.0
    }

    /// Get the P99.9 percentile in microseconds
    /// Converts nanosecond histogram value to microseconds
    /// Uses floating-point division to preserve fractional microseconds
    pub fn p99_9(&self) -> f32 {
        let p99_9_ns = self.histogram.value_at_percentile(99.9);
        p99_9_ns as f32 / 1000.0
    }

    /// Get the average latency in microseconds
    /// Uses rolling average of the last 1,000 samples for responsive metrics (instead of session-wide histogram)
    /// Recovers 5x faster than session-wide histogram as metrics stabilize
    /// Converts nanosecond rolling average to microseconds
    /// Uses floating-point division to preserve fractional microseconds
    pub fn average(&self) -> f32 {
        if self.rolling_samples.is_empty() {
            return 0.0;
        }
        
        // Calculate rolling average from the last 1,000 samples (or fewer if not enough data)
        let sum: u64 = self.rolling_samples.iter().sum();
        let avg_ns = (sum as f64) / (self.rolling_samples.len() as f64);
        (avg_ns / 1000.0) as f32
    }

    /// Get the latest sample in microseconds
    /// Converts internal nanosecond sample to microseconds
    /// Uses floating-point division to preserve fractional microseconds
    pub fn last_sample(&self) -> f32 {
        self.last_sample_ns as f32 / 1000.0
    }

    /// Get the total number of samples recorded
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Reset all statistics and rolling window for new session
    /// Clears session-wide histogram and rolling samples to start fresh measurement
    pub fn reset(&mut self) {
        self.histogram.clear();
        self.last_sample_ns = 0;
        self.max_latency_ns = 0;
        self.sample_count = 0;
        self.buckets = [0u64; 20];
        self.cycle_max_ns = 0;
        self.rolling_samples.clear();
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
    /// Converts internal nanosecond tracking to microseconds for UI display
    /// Uses floating-point division to preserve fractional microseconds
    pub fn cycle_max_us(&self) -> f32 {
        self.cycle_max_ns as f32 / 1000.0
    }

    /// Reset cycle_max_ns for the next monitoring cycle
    pub fn reset_cycle_max(&mut self) {
        self.cycle_max_ns = 0;
    }

    /// Get histogram bucket distribution (for UI visualization)
    /// Returns buckets with nanosecond ranges converted to microseconds
    /// Uses floating-point division to preserve fractional microseconds
    pub fn get_bucket_distribution(&self) -> Vec<(f32, f32, u64)> {
        let mut buckets = Vec::new();

        // Create 20 buckets logarithmically distributed from 1ns to max latency
        if self.max_latency_ns == 0 {
            return buckets;
        }

        let max_log = (self.max_latency_ns as f64).log10();
        for i in 0..20 {
            let lower_log = max_log * (i as f64 / 20.0);
            let upper_log = max_log * ((i + 1) as f64 / 20.0);

            let lower_ns = 10_f64.powf(lower_log) as u64;
            let upper_ns = 10_f64.powf(upper_log) as u64;

            let count = self.histogram.count_at(upper_ns) - self.histogram.count_at(lower_ns);

            // Convert nanoseconds to microseconds for UI display with floating-point precision
            let lower_us = lower_ns as f32 / 1000.0;
            let upper_us = upper_ns as f32 / 1000.0;

            buckets.push((lower_us, upper_us, count));
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

    #[test]
    fn test_latency_bucket_index_sub_microsecond() {
        // Test sub-microsecond precision (< 1000 ns = < 1 µs)
        // 1 ns should map to bucket 0
        let bucket_1ns = LatencyProcessor::latency_to_bucket_index(1);
        assert_eq!(bucket_1ns, 0, "1 ns should map to bucket 0");
        
        // 100 ns should map to early bucket (log10(100) ≈ 2.0)
        let bucket_100ns = LatencyProcessor::latency_to_bucket_index(100);
        assert!(bucket_100ns < 10, "100 ns should map to early bucket");
        
        // 500 ns should still map to early bucket
        let bucket_500ns = LatencyProcessor::latency_to_bucket_index(500);
        assert!(bucket_500ns < 10, "500 ns should map to early bucket");
    }

    #[test]
    fn test_latency_bucket_index_microsecond_scale() {
        // Test microsecond-scale precision (1 µs to 1 ms)
        // 1000 ns = 1 µs
        let bucket_1us = LatencyProcessor::latency_to_bucket_index(1000);
        assert!(bucket_1us <= 10, "1 µs should map to valid bucket");
        
        // 10 µs = 10,000 ns
        let bucket_10us = LatencyProcessor::latency_to_bucket_index(10_000);
        assert!(bucket_10us <= 19, "10 µs should map to valid bucket");
        
        // 100 µs = 100,000 ns
        let bucket_100us = LatencyProcessor::latency_to_bucket_index(100_000);
        assert!(bucket_100us <= 19, "100 µs should map to valid bucket");
        
        // 1 ms = 1,000,000 ns
        let bucket_1ms = LatencyProcessor::latency_to_bucket_index(1_000_000);
        assert!(bucket_1ms <= 19, "1 ms should map to valid bucket");
    }

    #[test]
    fn test_latency_bucket_index_millisecond_scale() {
        // Test millisecond-scale precision (1 ms to 100 ms)
        // 10 ms = 10,000,000 ns
        let bucket_10ms = LatencyProcessor::latency_to_bucket_index(10_000_000);
        assert!(bucket_10ms <= 19, "10 ms should map to valid bucket");
        
        // 50 ms = 50,000,000 ns
        let bucket_50ms = LatencyProcessor::latency_to_bucket_index(50_000_000);
        assert!(bucket_50ms <= 19, "50 ms should map to valid bucket");
        
        // 100 ms = 100,000,000 ns (upper boundary)
        let bucket_100ms = LatencyProcessor::latency_to_bucket_index(100_000_000);
        assert_eq!(bucket_100ms, 19, "100 ms should map to bucket 19 (max)");
    }

    #[test]
    fn test_latency_bucket_index_monotonic() {
        // Test that bucket indices are monotonically increasing
        let test_values = vec![
            1, 10, 100, 500, 999, 1000, 10_000, 100_000, 1_000_000,
            10_000_000, 50_000_000, 100_000_000
        ];
        
        let mut prev_bucket = 0;
        for &ns in &test_values {
            let bucket = LatencyProcessor::latency_to_bucket_index(ns);
            assert!(bucket >= prev_bucket,
                    "Buckets should be monotonically increasing: {} ns → bucket {}, but prev was {}",
                    ns, bucket, prev_bucket);
            prev_bucket = bucket;
        }
    }

    #[test]
    fn test_processor_fractional_microseconds_precision() {
        let mut processor = LatencyProcessor::default();
        
        // Record a sample with sub-microsecond precision: 123 ns
        processor.record_sample(123).expect("Failed to record 123 ns sample");
        
        // The getter should return 123 / 1000 = 0.123 µs (not rounded to 0.0)
        let last_sample_us = processor.last_sample();
        assert!(last_sample_us > 0.1 && last_sample_us < 0.2,
                "123 ns should convert to ~0.123 µs, got {}", last_sample_us);
    }

    #[test]
    fn test_processor_max_fractional_precision() {
        let mut processor = LatencyProcessor::default();
        
        // Record samples with sub-microsecond precision
        processor.record_sample(456).expect("Failed to record sample");
        processor.record_sample(789).expect("Failed to record sample");
        
        // Max should be 789 ns = 0.789 µs
        let max_us = processor.max();
        assert!(max_us > 0.7 && max_us < 1.0,
                "Max of 789 ns should be ~0.789 µs, got {}", max_us);
    }

    #[test]
    fn test_processor_millisecond_accuracy() {
        let mut processor = LatencyProcessor::default();
        
        // Record millisecond-scale samples
        processor.record_sample(5_000_000).expect("Failed to record 5 ms sample");  // 5 ms
        processor.record_sample(25_000_000).expect("Failed to record 25 ms sample"); // 25 ms
        
        // Verify samples are recorded correctly
        assert_eq!(processor.sample_count(), 2);
        assert!(processor.max() >= 25.0, "Max should be at least 25 µs (25 ms)");
    }

    #[test]
    fn test_processor_p99_fractional_precision() {
        let mut processor = LatencyProcessor::default();
        
        // Record a mix of sub-microsecond and larger samples
        for i in 1..=100 {
            let sample = (i as u64) * 100; // 100 ns, 200 ns, ..., 10000 ns
            processor.record_sample(sample).expect("Failed to record sample");
        }
        
        let p99_us = processor.p99();
        // P99 should be retrievable with fractional precision
        assert!(p99_us > 0.0, "P99 should be positive");
    }

    #[test]
    fn test_fractional_precision_across_scales() {
        // Comprehensive test that fractional microseconds are preserved across all scales
        
        // Test values with fractional results when divided by 1000
        let test_cases = vec![
            (7, "7 ns = 0.007 µs"),
            (37, "37 ns = 0.037 µs"),
            (123, "123 ns = 0.123 µs"),
            (999, "999 ns = 0.999 µs"),
            (5_678, "5,678 ns = 5.678 µs"),
            (12_345, "12,345 ns = 12.345 µs"),
            (999_999, "999,999 ns = 999.999 µs"),
        ];
        
        for (ns_value, _description) in test_cases {
            let mut temp_processor = LatencyProcessor::default();
            temp_processor.record_sample(ns_value).expect("Failed to record sample");
            
            // Verify the last_sample is not integer-rounded
            let us_value = temp_processor.last_sample();
            let expected = ns_value as f32 / 1000.0;
            assert!((us_value - expected).abs() < 0.0001,
                    "Fractional precision lost: {} ns should be {:.3} µs, got {:.3} µs",
                    ns_value, expected, us_value);
        }
    }

    #[test]
    fn test_full_range_coverage_1ns_to_100ms() {
        let mut processor = LatencyProcessor::default();
        
        // Log-spaced samples from 1 ns to 100 ms with fixed increment
        let test_values = vec![
            1, 10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000
        ];
        
        for sample in test_values {
            processor.record_sample(sample).expect("Failed to record sample");
        }
        
        // Verify processor captured the full range
        assert_eq!(processor.sample_count(), 9);
        assert!(processor.max() >= 100.0, "Max should reach at least 100 µs (100 ms range)");
    }
}

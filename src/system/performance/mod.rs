//! High-Precision Performance Diagnostics Module
//!
//! This module provides real-time latency measurement, system tuning,
//! performance metrics collection, and stress testing for the GOATd Kernel Builder.
//!
//! ## Architecture
//! - **Tuner**: Prepares system environment (mlockall, SCHED_FIFO, CPU affinity)
//! - **Collector**: Measures latency with nanosecond precision using lock-free rtrb ring buffer
//! - **Diagnostic**: Detects SMI (System Management Interrupt) correlations via MSR
//! - **History**: Persists performance snapshots for trend analysis
//! - **Stressor**: Orchestrates background workers (CPU, Memory, Scheduler) for load testing

pub mod tuner;
pub mod collector;
pub mod diagnostic;
pub mod history;
pub mod stressor;
pub mod thermal;
pub mod watchdog;
pub mod freezer;
pub mod jitter;
pub mod context_switch;
pub mod syscall;
pub mod task_wakeup;
pub mod scoring;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, Duration, Instant};
use std::fmt;

pub use tuner::{Tuner, PmQosGuard};
pub use collector::LatencyCollector;
pub use diagnostic::{SmiCorrelation, SmiDetector};
pub use history::{PerformanceHistory, PerformanceSnapshot, HistoryManager, BenchmarkRun, BenchmarkRunManager};
pub use stressor::{StressorManager, StressorType, Intensity};
pub use thermal::{ThermalData, read_thermal_data};
pub use watchdog::{BenchmarkWatchdog, WatchdogConfig, HeartbeatHandle};
pub use freezer::{BenchmarkFreezer, FreezerConfig};
pub use jitter::{MicroJitterCollector, MicroJitterConfig, MicroJitterMetrics};
pub use context_switch::{ContextSwitchCollector, ContextSwitchConfig, ContextSwitchMetrics};
pub use syscall::{SyscallSaturationCollector, SyscallSaturationConfig, SyscallSaturationMetrics};
pub use task_wakeup::{TaskWakeupCollector, TaskWakeupConfig, TaskWakeupMetrics};
pub use scoring::{
    PerformanceScorer, ReferenceBenchmarks, PersonalityType, ScoringResult,
};

// BenchmarkPhase is defined inline above

/// Configuration for performance monitoring
#[derive(Clone, Debug)]
pub struct PerformanceConfig {
    /// Measurement interval in microseconds (e.g., 1000 µs = 1 ms)
    pub interval_us: u64,
    /// Target CPU core for isolated measurements
    pub core_id: usize,
    /// Latency threshold for spike detection (microseconds)
    pub spike_threshold_us: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        PerformanceConfig {
            interval_us: 1000,      // 1 ms default
            core_id: 0,             // CPU 0 default
            spike_threshold_us: 100, // 100 µs spike threshold
        }
    }
}

/// Real-time performance metrics snapshot
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PerformanceMetrics {
    /// Current latency in microseconds (latest sample)
    pub current_us: f32,
    /// Maximum observed latency in microseconds
    pub max_us: f32,
    /// P99 percentile latency
    pub p99_us: f32,
    /// P99.9 percentile latency
    pub p99_9_us: f32,
    /// Average latency
    pub avg_us: f32,
    /// Rolling 1000-sample P99 latency (allows score recovery)
    #[serde(default)]
    pub rolling_p99_us: f32,
    /// Rolling 1000-sample P99.9 latency (for rolling consistency calculation)
    #[serde(default)]
    pub rolling_p99_9_us: f32,
    /// Rolling 1000-sample P99 throughput (operations per second)
    #[serde(default)]
    pub rolling_throughput_p99: f32,
    /// Rolling 1000-sample P99 efficiency (context-switch overhead in µs)
    #[serde(default)]
    pub rolling_efficiency_p99: f32,
    /// Rolling 1000-sample P99 consistency (delta between P99.9 and P99 for stability)
    #[serde(default)]
    pub rolling_consistency_us: f32,
    /// Total number of spikes detected (> threshold)
    pub total_spikes: u64,
    /// Total number of SMI events detected
    pub total_smis: u64,
    /// Number of spikes correlated with SMI
    pub spikes_correlated_to_smi: u64,
    /// 20-bucket logarithmic histogram (normalized 0.0..1.0)
    #[serde(default)]
    pub histogram_buckets: Vec<f32>,
    /// Jitter timeline: last 300 samples of cycle_max values (µs)
    #[serde(default)]
    pub jitter_history: Vec<f32>,
    /// Active CPU frequency governor (e.g., "powersave", "performance", "schedutil")
    #[serde(default)]
    pub active_governor: String,
    /// Current CPU frequency in MHz
    #[serde(default)]
    pub governor_hz: i32,
    /// Core temperatures in Celsius (per physical core)
    #[serde(default)]
    pub core_temperatures: Vec<f32>,
    /// Package temperature in Celsius
    #[serde(default)]
    pub package_temperature: f32,
    /// Advanced benchmark metrics from Phase 2.1 collectors
    #[serde(default)]
    pub benchmark_metrics: Option<BenchmarkMetrics>,
}

impl PerformanceMetrics {
    /// Reset metrics to default values for new session
    /// Clears all metric values while preserving the structure
    pub fn reset(&mut self) {
        self.current_us = 0.0;
        self.max_us = 0.0;
        self.p99_us = 0.0;
        self.p99_9_us = 0.0;
        self.avg_us = 0.0;
        self.rolling_p99_us = 0.0;
        self.rolling_p99_9_us = 0.0;
        self.rolling_throughput_p99 = 0.0;
        self.rolling_efficiency_p99 = 0.0;
        self.rolling_consistency_us = 0.0;
        self.total_spikes = 0;
        self.total_smis = 0;
        self.spikes_correlated_to_smi = 0;
        self.histogram_buckets.clear();
        self.jitter_history.clear();
        self.active_governor.clear();
        self.governor_hz = 0;
        self.core_temperatures.clear();
        self.package_temperature = 0.0;
        self.benchmark_metrics = None;
    }
}

/// Benchmark metrics from advanced performance collectors (Phase 2.1)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BenchmarkMetrics {
    /// Micro-jitter metrics (P99.99 detection)
    #[serde(default)]
    pub micro_jitter: Option<jitter::MicroJitterMetrics>,
    /// Context-switch RTT metrics
    #[serde(default)]
    pub context_switch_rtt: Option<context_switch::ContextSwitchMetrics>,
    /// Syscall saturation metrics
    #[serde(default)]
    pub syscall_saturation: Option<syscall::SyscallSaturationMetrics>,
    /// Task-to-task wakeup latency metrics
    #[serde(default)]
    pub task_wakeup: Option<task_wakeup::TaskWakeupMetrics>,
}

impl BenchmarkMetrics {
    /// Create a new empty BenchmarkMetrics struct
    pub fn new() -> Self {
        BenchmarkMetrics {
            micro_jitter: None,
            context_switch_rtt: None,
            syscall_saturation: None,
            task_wakeup: None,
        }
    }

    /// Check if all benchmark tests have been collected
    pub fn is_complete(&self) -> bool {
        self.micro_jitter.is_some()
            && self.context_switch_rtt.is_some()
            && self.syscall_saturation.is_some()
            && self.task_wakeup.is_some()
    }

    /// Get a summary of which benchmarks have been collected
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.micro_jitter.is_some() {
            parts.push("micro-jitter");
        }
        if self.context_switch_rtt.is_some() {
            parts.push("context-switch-rtt");
        }
        if self.syscall_saturation.is_some() {
            parts.push("syscall-saturation");
        }
        if self.task_wakeup.is_some() {
            parts.push("task-wakeup");
        }
        format!("[{}]", parts.join(", "))
    }
}

/// Kernel context information for performance records
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KernelContext {
    /// Kernel version string
    pub version: String,
    /// Active SCX scheduler profile (e.g., "gaming", "powersave", "disabled")
    pub scx_profile: String,
    /// LTO configuration (e.g., "thin", "full", "none")
    pub lto_config: String,
    /// Active CPU frequency governor (e.g., "powersave", "performance", "schedutil")
    #[serde(default)]
    pub governor: String,
}

/// A point-in-time performance record for JSON persistence
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PerformanceRecord {
    /// Timestamp of this record
    #[serde(default = "SystemTime::now")]
    pub timestamp: SystemTime,
    /// Kernel context (version, scheduler, LTO)
    pub kernel_context: KernelContext,
    /// Latency statistics
    pub metrics: PerformanceMetrics,
    /// Active stressors at time of measurement
    pub active_stressors: Vec<String>,
    /// Raw histogram bucket data for distribution visualization
    pub histogram_buckets: Vec<HistogramBucket>,
    /// Custom label or name for this benchmark (e.g., "Gaming Profile 1", "Baseline Test")
    /// If None, display will use timestamp as fallback
    #[serde(default)]
    pub label: Option<String>,
}

/// Histogram bucket for latency distribution
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HistogramBucket {
    /// Lower bound of latency range (µs)
    pub lower_us: f32,
    /// Upper bound of latency range (µs)
    pub upper_us: f32,
    /// Count of samples in this bucket
    pub count: u64,
}

/// Shared state for monitoring lifecycle
#[derive(Clone)]
pub struct MonitoringState {
    /// Flag to signal measurement thread to stop
    pub stop_flag: Arc<AtomicBool>,
    /// Count of dropped samples (ring buffer full)
    pub dropped_samples: Arc<AtomicU64>,
    /// Spike count counter
    pub spike_count: Arc<AtomicU64>,
    /// SMI-correlated spike count
    pub smi_correlated_spikes: Arc<AtomicU64>,
    /// Total SMI event count
    pub total_smi_count: Arc<AtomicU64>,
    /// Total samples collected (set when monitoring stops)
    pub final_sample_count: Arc<AtomicU64>,
    /// Total dropped samples at session end (set when monitoring stops)
    pub final_dropped_count: Arc<AtomicU64>,
}

impl Default for MonitoringState {
    fn default() -> Self {
        MonitoringState {
            stop_flag: Arc::new(AtomicBool::new(false)),
            dropped_samples: Arc::new(AtomicU64::new(0)),
            spike_count: Arc::new(AtomicU64::new(0)),
            smi_correlated_spikes: Arc::new(AtomicU64::new(0)),
            total_smi_count: Arc::new(AtomicU64::new(0)),
            final_sample_count: Arc::new(AtomicU64::new(0)),
            final_dropped_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl MonitoringState {
    /// Signal the measurement thread to stop
    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
    }

    /// Check if stop has been requested
    pub fn should_stop(&self) -> bool {
        self.stop_flag.load(Ordering::Acquire)
    }

    /// Get the current dropped sample count
    pub fn dropped_count(&self) -> u64 {
        self.dropped_samples.load(Ordering::Relaxed)
    }

    /// Get the current spike count
    pub fn spike_count(&self) -> u64 {
        self.spike_count.load(Ordering::Relaxed)
    }

    /// Get the current SMI-correlated spike count
    pub fn smi_correlated_count(&self) -> u64 {
        self.smi_correlated_spikes.load(Ordering::Relaxed)
    }

    /// Get the total SMI event count
    pub fn total_smi_count(&self) -> u64 {
        self.total_smi_count.load(Ordering::Relaxed)
    }
}

/// Benchmark phase: 6 phases of the GOATd Full Benchmark
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BenchmarkPhase {
    /// Phase 1: Baseline (no stressors, 0-10s)
    Baseline,
    /// Phase 2: Computational Heat (CPU 100%, 10-20s)
    ComputationalHeat,
    /// Phase 3: Memory Saturation (Memory 100%, 20-30s)
    MemorySaturation,
    /// Phase 4: Scheduler Flood (Scheduler 100%, 30-40s)
    SchedulerFlood,
    /// Phase 5: Gaming Simulator (CPU 50% + Scheduler 50%, 40-50s)
    GamingSimulator,
    /// Phase 6: The Gauntlet (CPU 100% + Memory 100% + Scheduler 100%, 50-60s)
    TheGauntlet,
}

impl fmt::Display for BenchmarkPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchmarkPhase::Baseline => write!(f, "Baseline"),
            BenchmarkPhase::ComputationalHeat => write!(f, "Computational Heat"),
            BenchmarkPhase::MemorySaturation => write!(f, "Memory Saturation"),
            BenchmarkPhase::SchedulerFlood => write!(f, "Scheduler Flood"),
            BenchmarkPhase::GamingSimulator => write!(f, "Gaming Simulator"),
            BenchmarkPhase::TheGauntlet => write!(f, "The Gauntlet"),
        }
    }
}

impl BenchmarkPhase {
    /// Get the start time in seconds for this phase
    pub fn start_time(&self) -> u64 {
        match self {
            BenchmarkPhase::Baseline => 0,
            BenchmarkPhase::ComputationalHeat => 10,
            BenchmarkPhase::MemorySaturation => 20,
            BenchmarkPhase::SchedulerFlood => 30,
            BenchmarkPhase::GamingSimulator => 40,
            BenchmarkPhase::TheGauntlet => 50,
        }
    }

    /// Get the duration of this phase in seconds
    pub fn duration_secs(&self) -> u64 {
        10 // All phases are 10 seconds
    }

    /// Get the end time in seconds for this phase
    pub fn end_time(&self) -> u64 {
        self.start_time() + self.duration_secs()
    }

    /// Get the next phase, or None if this is the last phase
    pub fn next_phase(&self) -> Option<BenchmarkPhase> {
        match self {
            BenchmarkPhase::Baseline => Some(BenchmarkPhase::ComputationalHeat),
            BenchmarkPhase::ComputationalHeat => Some(BenchmarkPhase::MemorySaturation),
            BenchmarkPhase::MemorySaturation => Some(BenchmarkPhase::SchedulerFlood),
            BenchmarkPhase::SchedulerFlood => Some(BenchmarkPhase::GamingSimulator),
            BenchmarkPhase::GamingSimulator => Some(BenchmarkPhase::TheGauntlet),
            BenchmarkPhase::TheGauntlet => None,
        }
    }
}

/// Monitoring mode: either a fixed-duration benchmark, system benchmark, or continuous monitoring
#[derive(Clone, Debug)]
pub enum MonitoringMode {
    /// Benchmark mode: runs for a specified duration then auto-stops
    Benchmark(Duration),
    /// System benchmark: GOATd Full Benchmark with 6 phases
    SystemBenchmark,
    /// Continuous mode: runs until manually stopped with periodic diagnostics
    Continuous,
}

impl MonitoringMode {
    /// Get the duration if this is a benchmark mode
    pub fn duration(&self) -> Option<Duration> {
        match self {
            MonitoringMode::Benchmark(d) => Some(*d),
            MonitoringMode::SystemBenchmark => Some(Duration::from_secs(60)),
            MonitoringMode::Continuous => None,
        }
    }

    /// Check if this is continuous mode
    pub fn is_continuous(&self) -> bool {
        matches!(self, MonitoringMode::Continuous)
    }

    /// Check if this is system benchmark mode
    pub fn is_system_benchmark(&self) -> bool {
        matches!(self, MonitoringMode::SystemBenchmark)
    }
}

/// Lifecycle state for monitoring sessions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleState {
    /// Idle: no monitoring active
    Idle,
    /// Running: monitoring is actively collecting samples
    Running,
    /// Paused: monitoring is paused but can be resumed
    Paused,
    /// Completed: monitoring finished and results are finalized
    Completed,
}

/// Summary of a completed monitoring session
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionSummary {
    /// Timestamp when the session started
    pub timestamp_start: SystemTime,
    /// Timestamp when the session ended
    pub timestamp_end: Option<SystemTime>,
    /// Monitoring mode used for this session
    pub mode_name: String,
    /// Duration the session ran (if completed)
    pub duration_secs: Option<f64>,
    /// Final metrics from the session
    pub final_metrics: PerformanceMetrics,
    /// Kernel context at time of session
    pub kernel_context: KernelContext,
    /// List of active stressors during session
    pub active_stressors: Vec<String>,
    /// Whether the session completed successfully
    pub completed_successfully: bool,
    /// Final sample count
    pub total_samples: u64,
    /// Final dropped sample count
    pub total_dropped_samples: u64,
    /// Custom label/name for this benchmark result
    #[serde(default)]
    pub label: Option<String>,
}

impl SessionSummary {
    /// Create a new session summary
    pub fn new(
        mode_name: String,
        final_metrics: PerformanceMetrics,
        kernel_context: KernelContext,
        active_stressors: Vec<String>,
        total_samples: u64,
        total_dropped_samples: u64,
    ) -> Self {
        SessionSummary {
            timestamp_start: SystemTime::now(),
            timestamp_end: None,
            mode_name,
            duration_secs: None,
            final_metrics,
            kernel_context,
            active_stressors,
            completed_successfully: false,
            total_samples,
            total_dropped_samples,
            label: None,
        }
    }

    /// Create a new session summary with a custom label
    pub fn with_label(
        mode_name: String,
        final_metrics: PerformanceMetrics,
        kernel_context: KernelContext,
        active_stressors: Vec<String>,
        total_samples: u64,
        total_dropped_samples: u64,
        label: Option<String>,
    ) -> Self {
        SessionSummary {
            timestamp_start: SystemTime::now(),
            timestamp_end: None,
            mode_name,
            duration_secs: None,
            final_metrics,
            kernel_context,
            active_stressors,
            completed_successfully: false,
            total_samples,
            total_dropped_samples,
            label,
        }
    }

    /// Mark the session as completed and calculate duration
    pub fn mark_completed(&mut self, start_instant: Instant) {
        self.timestamp_end = Some(SystemTime::now());
        self.completed_successfully = true;
        let elapsed = start_instant.elapsed();
        self.duration_secs = Some(elapsed.as_secs_f64());
    }
}

/// Benchmark Orchestrator: Manages the 60-second GOATd Full Benchmark sequence
///
/// Coordinates 6 consecutive 10-second phases with phase-specific stressor configurations:
/// - Phase 1 (0-10s): Baseline (no stressors)
/// - Phase 2 (10-20s): CPU 100%
/// - Phase 3 (20-30s): Memory 100%
/// - Phase 4 (30-40s): Scheduler 100%
/// - Phase 5 (40-50s): CPU 50% + Scheduler 50%
/// - Phase 6 (50-60s): CPU 100% + Memory 100% + Scheduler 100%
///
/// The orchestrator handles stressor transitions and collects phase-specific metrics.
#[derive(Clone, Debug)]
pub struct BenchmarkOrchestrator {
    /// Current phase
    pub current_phase: BenchmarkPhase,
    /// Session start time
    pub session_start: Instant,
    /// Phase metrics collection: phase name -> metrics snapshot
    pub phase_metrics: Vec<(String, PerformanceMetrics)>,
}

impl BenchmarkOrchestrator {
    /// Create a new benchmark orchestrator
    pub fn new() -> Self {
        BenchmarkOrchestrator {
            current_phase: BenchmarkPhase::Baseline,
            session_start: Instant::now(),
            phase_metrics: Vec::with_capacity(6),
        }
    }

    /// Get the elapsed time in seconds
    pub fn elapsed_secs(&self) -> u64 {
        self.session_start.elapsed().as_secs()
    }

    /// Check if the benchmark is complete (60 seconds elapsed)
    pub fn is_complete(&self) -> bool {
        self.elapsed_secs() >= 60
    }

    /// Transition to the next phase and return the new phase or None if complete
    pub fn advance_phase(&mut self) -> Option<BenchmarkPhase> {
        if let Some(next) = self.current_phase.next_phase() {
            self.current_phase = next;
            Some(next)
        } else {
            None
        }
    }

    /// Get stressors for the current phase
    pub fn get_phase_stressors(&self) -> Vec<(StressorType, Intensity)> {
        match self.current_phase {
            BenchmarkPhase::Baseline => {
                // No stressors
                vec![]
            }
            BenchmarkPhase::ComputationalHeat => {
                // CPU 100%
                vec![(StressorType::Cpu, Intensity::new(100))]
            }
            BenchmarkPhase::MemorySaturation => {
                // Memory 100%
                vec![(StressorType::Memory, Intensity::new(100))]
            }
            BenchmarkPhase::SchedulerFlood => {
                // Scheduler 100%
                vec![(StressorType::Scheduler, Intensity::new(100))]
            }
            BenchmarkPhase::GamingSimulator => {
                // CPU 50% + Scheduler 50%
                vec![
                    (StressorType::Cpu, Intensity::new(50)),
                    (StressorType::Scheduler, Intensity::new(50)),
                ]
            }
            BenchmarkPhase::TheGauntlet => {
                // CPU 100% + Memory 100% + Scheduler 100%
                vec![
                    (StressorType::Cpu, Intensity::new(100)),
                    (StressorType::Memory, Intensity::new(100)),
                    (StressorType::Scheduler, Intensity::new(100)),
                ]
            }
        }
    }

    /// Record metrics for the current phase
    pub fn record_phase_metrics(&mut self, metrics: PerformanceMetrics) {
        self.phase_metrics.push((self.current_phase.to_string(), metrics));
    }

    /// Calculate the final GOAT Score from aggregated phase metrics
    ///
    /// Aggregates all 6 phase metrics by mathematically averaging them:
    /// - Averages max, p99, p99.9, avg latencies across all phases
    /// - Averages spike counts and SMI correlations
    /// - Uses rolling metrics when available (p99, p99.9, consistency) for accuracy
    /// - Preserves benchmark_metrics from last phase for detailed scoring
    ///
    /// This ensures a cumulative/standardized score that represents the kernel's
    /// overall performance across all 6 stress phases, not just a snapshot.
    pub fn calculate_final_score(&self) -> Option<u16> {
        if self.phase_metrics.is_empty() {
            return None;
        }

        // Aggregate metrics across all phases with numerical averaging
        let mut aggregated = PerformanceMetrics::default();
        let phase_count = self.phase_metrics.len() as f32;

        for (_, metrics) in &self.phase_metrics {
            // Accumulate latency metrics
            aggregated.max_us += metrics.max_us;
            aggregated.p99_us += metrics.p99_us;
            aggregated.p99_9_us += metrics.p99_9_us;
            aggregated.avg_us += metrics.avg_us;
            
            // Accumulate rolling metrics (already in µs, can be directly averaged)
            aggregated.rolling_p99_us += metrics.rolling_p99_us;
            aggregated.rolling_p99_9_us += metrics.rolling_p99_9_us;
            aggregated.rolling_consistency_us += metrics.rolling_consistency_us;
            
            // Accumulate spike/SMI metrics
            aggregated.total_spikes += metrics.total_spikes;
            aggregated.total_smis += metrics.total_smis;
            aggregated.spikes_correlated_to_smi += metrics.spikes_correlated_to_smi;
        }

        // Calculate average by dividing by phase count
        aggregated.max_us /= phase_count;
        aggregated.p99_us /= phase_count;
        aggregated.p99_9_us /= phase_count;
        aggregated.avg_us /= phase_count;
        aggregated.rolling_p99_us /= phase_count;
        aggregated.rolling_p99_9_us /= phase_count;
        aggregated.rolling_consistency_us /= phase_count;
        aggregated.total_spikes = (aggregated.total_spikes as f32 / phase_count) as u64;
        aggregated.total_smis = (aggregated.total_smis as f32 / phase_count) as u64;
        aggregated.spikes_correlated_to_smi = (aggregated.spikes_correlated_to_smi as f32 / phase_count) as u64;

        // Preserve benchmark_metrics from last phase for detailed scoring
        if let Some((_, last_metrics)) = self.phase_metrics.last() {
            aggregated.benchmark_metrics = last_metrics.benchmark_metrics.clone();
        }

        // Score the aggregated metrics with PerformanceScorer
        // This applies the standard 7-metric weighting (27% Latency, 18% Consistency, etc.)
        let scorer = PerformanceScorer::new();
        let result = scorer.score_metrics(&aggregated);
        
        eprintln!("[BENCHMARK_ORCHESTRATOR] Final GOAT Score from cumulative metrics: {} ({} phases averaged)",
            result.goat_score, self.phase_metrics.len());
        
        Some(result.goat_score)
    }
}

impl Default for BenchmarkOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_config_default() {
        let config = PerformanceConfig::default();
        assert_eq!(config.interval_us, 1000);
        assert_eq!(config.core_id, 0);
        assert_eq!(config.spike_threshold_us, 100);
    }

    #[test]
    fn test_monitoring_state_lifecycle() {
        let state = MonitoringState::default();
        assert!(!state.should_stop());
        state.request_stop();
        assert!(state.should_stop());
    }

    #[test]
    fn test_benchmark_metrics_new() {
        let metrics = BenchmarkMetrics::new();
        assert!(metrics.micro_jitter.is_none());
        assert!(metrics.context_switch_rtt.is_none());
        assert!(metrics.syscall_saturation.is_none());
        assert!(metrics.task_wakeup.is_none());
        assert!(!metrics.is_complete());
    }

    #[test]
    fn test_benchmark_metrics_summary() {
        let mut metrics = BenchmarkMetrics::new();
        assert_eq!(metrics.summary(), "[]");

        metrics.micro_jitter = Some(jitter::MicroJitterMetrics {
            p99_99_us: 100.0,
            max_us: 500.0,
            avg_us: 50.0,
            spike_count: 10,
            sample_count: 1000,
        });

        assert!(metrics.summary().contains("micro-jitter"));
    }

    #[test]
    fn test_benchmark_metrics_complete() {
        let mut metrics = BenchmarkMetrics::new();
        assert!(!metrics.is_complete());

        metrics.micro_jitter = Some(jitter::MicroJitterMetrics::default());
        metrics.context_switch_rtt = Some(context_switch::ContextSwitchMetrics::default());
        metrics.syscall_saturation = Some(syscall::SyscallSaturationMetrics::default());
        metrics.task_wakeup = Some(task_wakeup::TaskWakeupMetrics::default());

        assert!(metrics.is_complete());
    }
}

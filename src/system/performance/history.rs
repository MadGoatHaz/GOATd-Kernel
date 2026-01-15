//! Performance History Management
//!
//! Captures periodic snapshots of performance metrics and persists them to JSON
//! for trend analysis and historical review.
//!
//! ## HistoryManager
//! The `HistoryManager` provides persistent storage of test results as timestamped JSON files
//! in `~/.config/goatdkernel/performance/records/`. Each record contains:
//! - Timestamp of the session
//! - Kernel context (version, SCX profile, LTO)
//! - Final session metrics
//! - Active stressors at time of test
//! - Histogram distribution data
//!
//! ## BenchmarkRunManager
//! The `BenchmarkRunManager` stores analyzed benchmark runs with scoring results in
//! `~/.config/goatd/benchmarks/run_{timestamp}.json` for historical comparison.

use super::{PerformanceMetrics, PerformanceRecord, KernelContext, HistogramBucket, SessionSummary};
use super::scoring::ScoringResult;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Metadata for a performance record - used for UI display without loading full record
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceRecordMetadata {
    /// Unique identifier (filename without path)
    pub id: String,
    /// Custom label if provided, otherwise timestamp
    pub label: String,
    /// Raw timestamp for sorting
    pub timestamp: SystemTime,
    /// Formatted display name: "Label (YYYY-MM-DD HH:MM:SS)"
    pub display_name: String,
}

impl PerformanceRecordMetadata {
    /// Create metadata from a record ID and label/timestamp
    pub fn new(id: String, label: Option<String>, timestamp: SystemTime) -> Self {
        let timestamp_str = match timestamp.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => {
                // Format as YYYY-MM-DD HH:MM:SS
                let secs = duration.as_secs();
                let datetime = chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));
                datetime.format("%Y-%m-%d %H:%M:%S").to_string()
            }
            Err(_) => "Unknown".to_string(),
        };

        let display_label = label.unwrap_or_else(|| "Unnamed".to_string());
        let display_name = format!("{} ({})", display_label, timestamp_str);

        PerformanceRecordMetadata {
            id,
            label: display_label,
            timestamp,
            display_name,
        }
    }
}

/// A snapshot of performance metrics at a point in time
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// Timestamp when this snapshot was taken
    pub timestamp: SystemTime,
    /// Performance metrics at this moment
    pub metrics: PerformanceMetrics,
    /// Kernel context information
    pub kernel_context: KernelContext,
}

impl PerformanceSnapshot {
    /// Create a new performance snapshot
    pub fn new(
        metrics: PerformanceMetrics,
        kernel_context: KernelContext,
    ) -> Self {
        PerformanceSnapshot {
            timestamp: SystemTime::now(),
            metrics,
            kernel_context,
        }
    }
}

/// Rolling window buffer for calculating P99 statistics and consistency
/// Maintains exactly 1000 samples for Latency, Throughput, Efficiency, and Consistency metrics
pub struct RollingWindow {
    /// Latency samples (microseconds)
    pub latency_samples: VecDeque<f32>,
    /// Throughput samples (operations per second)
    pub throughput_samples: VecDeque<f32>,
    /// Efficiency samples (microseconds or normalized units)
    pub efficiency_samples: VecDeque<f32>,
    /// Consistency samples (P99.9 - P99 delta for rolling consistency tracking)
    pub consistency_samples: VecDeque<f32>,
    /// Maximum window size (1000 samples)
    const_max_size: usize,
}

impl RollingWindow {
    /// Create a new rolling window buffer
    pub fn new() -> Self {
        RollingWindow {
            latency_samples: VecDeque::with_capacity(1000),
            throughput_samples: VecDeque::with_capacity(1000),
            efficiency_samples: VecDeque::with_capacity(1000),
            consistency_samples: VecDeque::with_capacity(1000),
            const_max_size: 1000,
        }
    }

    /// Add a latency sample and maintain 1000-sample window
    pub fn add_latency(&mut self, value: f32) {
        self.latency_samples.push_back(value);
        if self.latency_samples.len() > self.const_max_size {
            self.latency_samples.pop_front();
        }
    }

    /// Add a throughput sample and maintain 1000-sample window
    pub fn add_throughput(&mut self, value: f32) {
        self.throughput_samples.push_back(value);
        if self.throughput_samples.len() > self.const_max_size {
            self.throughput_samples.pop_front();
        }
    }

    /// Add an efficiency sample and maintain 1000-sample window
    pub fn add_efficiency(&mut self, value: f32) {
        self.efficiency_samples.push_back(value);
        if self.efficiency_samples.len() > self.const_max_size {
            self.efficiency_samples.pop_front();
        }
    }

    /// Add a consistency sample (delta between P99.9 and P99) and maintain 1000-sample window
    pub fn add_consistency(&mut self, value: f32) {
        self.consistency_samples.push_back(value);
        if self.consistency_samples.len() > self.const_max_size {
            self.consistency_samples.pop_front();
        }
    }

    /// Calculate P99 (99th percentile) from the latency window
    /// Returns the value at index 990 (99th percentile of 1000 samples)
    pub fn calculate_p99_latency(&self) -> f32 {
        if self.latency_samples.is_empty() {
            return 0.0;
        }
        
        let mut sorted: Vec<f32> = self.latency_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        // P99 = index 990 (0-indexed) out of 1000 samples
        let p99_index = (sorted.len() as f32 * 0.99) as usize;
        let index = p99_index.min(sorted.len() - 1);
        sorted[index]
    }

    /// Calculate P99.9 (99.9th percentile) from the latency window
    /// Returns the value at index 999 (99.9th percentile of 1000 samples)
    pub fn calculate_p99_9_latency(&self) -> f32 {
        if self.latency_samples.is_empty() {
            return 0.0;
        }
        
        let mut sorted: Vec<f32> = self.latency_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        // P99.9 = index 999 (0-indexed) out of 1000 samples
        let p99_9_index = (sorted.len() as f32 * 0.999) as usize;
        let index = p99_9_index.min(sorted.len() - 1);
        sorted[index]
    }

    /// Calculate P99 consistency (P99.9 - P99) from the consistency samples
    /// Measures stability by tracking the gap between tail percentiles
    pub fn calculate_p99_consistency(&self) -> f32 {
        if self.consistency_samples.is_empty() {
            return 0.0;
        }
        
        let mut sorted: Vec<f32> = self.consistency_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        // P99 of consistency delta = index 990 out of 1000 samples
        let p99_index = (sorted.len() as f32 * 0.99) as usize;
        let index = p99_index.min(sorted.len() - 1);
        sorted[index]
    }

    /// Calculate Coefficient of Variation (CV) from latency samples
    /// CV = Standard Deviation / Mean
    /// Measures consistency: lower CV = more consistent performance
    /// Optimal: 0.05 (5%), Poor: 0.50 (50%)
    pub fn calculate_cv(&self) -> f32 {
        if self.latency_samples.len() < 2 {
            return 0.0;
        }
        
        // Calculate mean
        let mean = self.latency_samples.iter().sum::<f32>() / self.latency_samples.len() as f32;
        
        if mean == 0.0 {
            return 0.0;
        }
        
        // Calculate standard deviation
        let variance = self.latency_samples.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f32>() / self.latency_samples.len() as f32;
        let std_dev = variance.sqrt();
        
        // Calculate and return CV
        std_dev / mean
    }

    /// Calculate Standard Deviation from latency samples in microseconds
    /// Measures variability of latency: lower std_dev = more consistent performance
    /// Professional calibration: 5µs (Optimal) → 50µs (Poor)
    /// This is the preferred metric for "µs Var" display in KernBench tiers
    pub fn calculate_std_dev(&self) -> f32 {
        if self.latency_samples.len() < 2 {
            return 0.0;
        }
        
        // Calculate mean
        let mean = self.latency_samples.iter().sum::<f32>() / self.latency_samples.len() as f32;
        
        // Calculate standard deviation directly
        let variance = self.latency_samples.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f32>() / self.latency_samples.len() as f32;
        let std_dev = variance.sqrt();
        
        // Return standard deviation in microseconds (already in µs from input samples)
        std_dev
    }

    /// Calculate P99 from the throughput window
    pub fn calculate_p99_throughput(&self) -> f32 {
        if self.throughput_samples.is_empty() {
            return 0.0;
        }
        
        let mut sorted: Vec<f32> = self.throughput_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        let p99_index = (sorted.len() as f32 * 0.99) as usize;
        let index = p99_index.min(sorted.len() - 1);
        sorted[index]
    }

    /// Calculate P99 from the efficiency window
    pub fn calculate_p99_efficiency(&self) -> f32 {
        if self.efficiency_samples.is_empty() {
            return 0.0;
        }
        
        let mut sorted: Vec<f32> = self.efficiency_samples.iter().copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        let p99_index = (sorted.len() as f32 * 0.99) as usize;
        let index = p99_index.min(sorted.len() - 1);
        sorted[index]
    }

    /// Clear all samples
    pub fn clear(&mut self) {
        self.latency_samples.clear();
        self.throughput_samples.clear();
        self.efficiency_samples.clear();
        self.consistency_samples.clear();
    }
}

impl Default for RollingWindow {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages a sliding window of performance history
pub struct PerformanceHistory {
    /// Queue of snapshots (FIFO, newest at back)
    snapshots: VecDeque<PerformanceSnapshot>,
    /// Maximum number of snapshots to retain (default: 12 for ~1 minute at 5s intervals)
    max_snapshots: usize,
    /// Path to persist history to disk
    persist_path: Option<String>,
    /// 1000-sample rolling window for P99 calculation
    pub rolling_window: RollingWindow,
}

impl PerformanceHistory {
    /// Create a new history manager with the specified retention size
    pub fn new(max_snapshots: usize) -> Self {
        PerformanceHistory {
            snapshots: VecDeque::with_capacity(max_snapshots),
            max_snapshots,
            persist_path: None,
            rolling_window: RollingWindow::new(),
        }
    }

    /// Create history with persistence enabled
    pub fn with_persistence(max_snapshots: usize, path: String) -> Self {
        PerformanceHistory {
            snapshots: VecDeque::with_capacity(max_snapshots),
            max_snapshots,
            persist_path: Some(path),
            rolling_window: RollingWindow::new(),
        }
    }

    /// Add a new snapshot to the history
    pub fn add_snapshot(&mut self, snapshot: PerformanceSnapshot) {
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);

        // Persist to disk if enabled
        if let Some(ref path) = self.persist_path {
            let _ = self.save_to_disk(path);
        }
    }

    /// Get all snapshots in chronological order (oldest first)
    pub fn snapshots(&self) -> Vec<PerformanceSnapshot> {
        self.snapshots.iter().cloned().collect()
    }

    /// Get the most recent snapshot
    pub fn latest(&self) -> Option<PerformanceSnapshot> {
        self.snapshots.back().cloned()
    }

    /// Get the count of snapshots currently in history
    pub fn count(&self) -> usize {
        self.snapshots.len()
    }

    /// Calculate trend (change from oldest to newest snapshot)
    pub fn trend_max_latency(&self) -> Option<f32> {
        if self.snapshots.len() < 2 {
            return None;
        }

        let oldest = &self.snapshots[0].metrics.max_us;
        let newest = &self.snapshots[self.snapshots.len() - 1].metrics.max_us;

        Some(newest - oldest)
    }

    /// Calculate average latency across all snapshots
    pub fn average_latency_across_history(&self) -> Option<f32> {
        if self.snapshots.is_empty() {
            return None;
        }

        let sum: f32 = self.snapshots.iter().map(|s| s.metrics.avg_us).sum();
        Some(sum / self.snapshots.len() as f32)
    }

    /// Save history to disk as JSON
    pub fn save_to_disk(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let path_ref = path.as_ref();
        eprintln!("[HISTORY] Saving {} snapshots to {}", self.snapshots.len(), path_ref.display());
        
        // Ensure parent directory exists
        if let Some(parent) = path_ref.parent() {
            eprintln!("[HISTORY] Ensuring parent directory exists: {}", parent.display());
            fs::create_dir_all(parent)?;
            eprintln!("[HISTORY] ✓ Parent directory ready");
        }
        
        let json = serde_json::to_string_pretty(&self.snapshots)?;
        eprintln!("[HISTORY] Serialized JSON size: {} bytes", json.len());
        
        fs::write(path_ref, &json)?;
        eprintln!("[HISTORY] ✓ Successfully persisted history to disk");
        Ok(())
    }

    /// Load history from disk
    pub fn load_from_disk(&mut self, path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
        let path_ref = path.as_ref();
        eprintln!("[HISTORY] Loading history from {}", path_ref.display());
        
        if !path_ref.exists() {
            eprintln!("[HISTORY] File does not exist yet (new profile), skipping load");
            return Ok(());
        }
        
        let json = fs::read_to_string(path_ref)?;
        eprintln!("[HISTORY] Loaded JSON file: {} bytes", json.len());
        
        let loaded: Vec<PerformanceSnapshot> = serde_json::from_str(&json)?;
        eprintln!("[HISTORY] Deserialized {} snapshots from JSON", loaded.len());

        self.snapshots.clear();
        for snapshot in loaded.into_iter().take(self.max_snapshots) {
            self.snapshots.push_back(snapshot);
        }
        
        eprintln!("[HISTORY] ✓ Loaded {} snapshots (limited to max {})", self.snapshots.len(), self.max_snapshots);
        Ok(())
    }

    /// Clear all snapshots
    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    /// Reset history and rolling window for new session
    /// Clears all snapshots and resets the rolling window to start fresh
    pub fn reset(&mut self) {
        self.snapshots.clear();
        self.rolling_window.clear();
    }

    /// Export full performance record with histogram distribution
    pub fn export_record(
        &self,
        active_stressors: Vec<String>,
        histogram_buckets: Vec<HistogramBucket>,
    ) -> Option<PerformanceRecord> {
        self.latest().map(|snapshot| {
            PerformanceRecord {
                timestamp: snapshot.timestamp,
                kernel_context: snapshot.kernel_context,
                metrics: snapshot.metrics,
                active_stressors,
                histogram_buckets,
                label: None,
            }
        })
    }
}

impl Default for PerformanceHistory {
    fn default() -> Self {
        Self::new(12) // Default: 12 snapshots (1 minute at 5s intervals)
    }
}

/// HistoryManager for persistent storage of performance test records
///
/// Saves test results as timestamped JSON files in `~/.config/goatdkernel/performance/records/`
/// for later comparison and trend analysis.
pub struct HistoryManager {
    /// Base directory for storing performance records
    records_dir: PathBuf,
}

impl HistoryManager {
    /// Create a new HistoryManager
    ///
    /// Initializes with the standard GOATd config directory.
    /// Creates the records directory if it doesn't exist.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let records_dir = Self::get_records_dir();
        fs::create_dir_all(&records_dir)?;
        Ok(HistoryManager { records_dir })
    }

    /// Get the base records directory path
    fn get_records_dir() -> PathBuf {
        // Use XDG_CONFIG_HOME or default to ~/.config
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    PathBuf::from(h).join(".config")
                })
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/.config"))
            .join("goatdkernel")
            .join("performance")
            .join("records");
        
        config_dir
    }

    /// Generate a unique filename based on timestamp and session ID
    fn generate_filename() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        
        format!("perf_record_{}.json", now)
    }

    /// Save a session summary as a persistent performance record
    ///
    /// Captures real-time metadata (Kernel version, SCX profile, LTO type)
    /// and serializes the complete session to a timestamped JSON file.
    /// Preserves the custom label from the SessionSummary for later display.
    ///
    /// Returns the ID (filename) of the saved record.
    pub fn save_record(&self, summary: SessionSummary, histogram_buckets: Vec<HistogramBucket>) -> Result<String, Box<dyn std::error::Error>> {
        // Build the complete performance record, preserving the label
        let record = PerformanceRecord {
            timestamp: summary.timestamp_start,
            kernel_context: summary.kernel_context.clone(),
            metrics: summary.final_metrics.clone(),
            active_stressors: summary.active_stressors.clone(),
            histogram_buckets,
            label: summary.label.clone(),
        };

        // Generate unique filename
        let filename = Self::generate_filename();
        let filepath = self.records_dir.join(&filename);

        eprintln!("[HISTORY_MANAGER] Saving performance record to: {}", filepath.display());
        if let Some(ref lbl) = record.label {
            eprintln!("[HISTORY_MANAGER] Record label: {}", lbl);
        }

        // Serialize to pretty JSON for user readability
        let json = serde_json::to_string_pretty(&record)?;
        eprintln!("[HISTORY_MANAGER] Serialized record size: {} bytes", json.len());

        // Write to disk
        fs::write(&filepath, &json)?;
        eprintln!("[HISTORY_MANAGER] ✓ Record persisted: {} (label: {})", filename, record.label.as_ref().unwrap_or(&"None".to_string()));

        Ok(filename)
    }

    /// List all available test record IDs (filenames/timestamps)
    ///
    /// Returns a sorted list of record filenames in reverse chronological order (newest first).
    pub fn list_records(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut records = Vec::new();

        if !self.records_dir.exists() {
            eprintln!("[HISTORY_MANAGER] Records directory does not exist: {}", self.records_dir.display());
            return Ok(records);
        }

        for entry in fs::read_dir(&self.records_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only include JSON files matching the pattern
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    if filename_str.starts_with("perf_record_") && filename_str.ends_with(".json") {
                        records.push(filename_str.to_string());
                    }
                }
            }
        }

        // Sort in reverse order (newest first)
        records.sort_by(|a, b| b.cmp(a));
        eprintln!("[HISTORY_MANAGER] Listed {} records", records.len());

        Ok(records)
    }

    /// List all available test records with metadata (labels and timestamps for UI display)
    ///
    /// Returns a sorted list of `PerformanceRecordMetadata` with display names in reverse chronological order.
    /// This method performs minimal JSON parsing to extract only label and timestamp without loading full metrics.
    pub fn list_records_metadata(&self) -> Result<Vec<PerformanceRecordMetadata>, Box<dyn std::error::Error>> {
        #[derive(Deserialize)]
        struct LabeledRecord {
            timestamp: SystemTime,
            #[serde(default)]
            label: Option<String>,
        }

        let mut metadata = Vec::new();

        if !self.records_dir.exists() {
            eprintln!("[HISTORY_MANAGER] Records directory does not exist: {}", self.records_dir.display());
            return Ok(metadata);
        }

        for entry in fs::read_dir(&self.records_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only include JSON files matching the pattern
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    if filename_str.starts_with("perf_record_") && filename_str.ends_with(".json") {
                        // Read and parse minimal fields
                        match fs::read_to_string(&path) {
                            Ok(json) => {
                                match serde_json::from_str::<LabeledRecord>(&json) {
                                    Ok(record) => {
                                        let meta = PerformanceRecordMetadata::new(
                                            filename_str.to_string(),
                                            record.label,
                                            record.timestamp,
                                        );
                                        metadata.push(meta);
                                    }
                                    Err(e) => {
                                        eprintln!("[HISTORY_MANAGER] Warning: Failed to parse metadata from {}: {}", filename_str, e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("[HISTORY_MANAGER] Warning: Failed to read {}: {}", filename_str, e);
                            }
                        }
                    }
                }
            }
        }

        // Sort in reverse order by timestamp (newest first)
        metadata.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        eprintln!("[HISTORY_MANAGER] Listed {} record metadata entries", metadata.len());

        Ok(metadata)
    }

    /// Load a performance record by ID (filename)
    ///
    /// Returns the deserialized `PerformanceRecord` or an error if not found.
    pub fn load_record(&self, id: &str) -> Result<PerformanceRecord, Box<dyn std::error::Error>> {
        let filepath = self.records_dir.join(id);

        eprintln!("[HISTORY_MANAGER] Loading record from: {}", filepath.display());

        if !filepath.exists() {
            return Err(format!("Record not found: {}", id).into());
        }

        let json = fs::read_to_string(&filepath)?;
        eprintln!("[HISTORY_MANAGER] Loaded JSON file: {} bytes", json.len());

        let record: PerformanceRecord = serde_json::from_str(&json)?;
        eprintln!("[HISTORY_MANAGER] ✓ Record deserialized successfully");

        Ok(record)
    }

    /// Delete a performance record by ID
    pub fn delete_record(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let filepath = self.records_dir.join(id);

        if !filepath.exists() {
            return Err(format!("Record not found: {}", id).into());
        }

        fs::remove_file(&filepath)?;
        eprintln!("[HISTORY_MANAGER] ✓ Record deleted: {}", id);

        Ok(())
    }

    /// Get the count of stored records
    pub fn record_count(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let records = self.list_records()?;
        Ok(records.len())
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize HistoryManager")
    }
}

/// A complete benchmark run with performance metrics and scoring analysis
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BenchmarkRun {
    /// Timestamp when this benchmark run completed
    pub timestamp: SystemTime,
    /// Kernel context at time of benchmark
    pub kernel_context: KernelContext,
    /// Performance metrics from the benchmark
    pub metrics: PerformanceMetrics,
    /// Scoring result and analysis
    pub scoring: ScoringResult,
    /// Active stressors during the run
    pub active_stressors: Vec<String>,
    /// Duration of the benchmark in seconds
    pub duration_secs: Option<f64>,
    /// Human-readable label for the run
    pub label: Option<String>,
}

impl BenchmarkRun {
    /// Create a new benchmark run with all components
    pub fn new(
        kernel_context: KernelContext,
        metrics: PerformanceMetrics,
        scoring: ScoringResult,
        active_stressors: Vec<String>,
        duration_secs: Option<f64>,
        label: Option<String>,
    ) -> Self {
        BenchmarkRun {
            timestamp: SystemTime::now(),
            kernel_context,
            metrics,
            scoring,
            active_stressors,
            duration_secs,
            label,
        }
    }
}

/// Manages persistent storage of analyzed benchmark runs
///
/// Saves benchmark runs with scoring results as JSON files in `~/.config/goatd/benchmarks/`
/// for later comparison and historical tracking.
pub struct BenchmarkRunManager {
    /// Base directory for storing benchmark runs
    benchmarks_dir: PathBuf,
}

impl BenchmarkRunManager {
    /// Create a new BenchmarkRunManager
    ///
    /// Initializes with the standard GOATd config directory.
    /// Creates the benchmarks directory if it doesn't exist.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let benchmarks_dir = Self::get_benchmarks_dir();
        fs::create_dir_all(&benchmarks_dir)?;
        Ok(BenchmarkRunManager { benchmarks_dir })
    }

    /// Get the base benchmarks directory path
    fn get_benchmarks_dir() -> PathBuf {
        // Use XDG_CONFIG_HOME or default to ~/.config
        let config_dir = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    PathBuf::from(h).join(".config")
                })
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/.config"))
            .join("goatd")
            .join("benchmarks");
        
        config_dir
    }

    /// Generate unique filename based on timestamp
    fn generate_filename() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        
        format!("run_{}.json", now)
    }

    /// Save a benchmark run
    ///
    /// Persists the complete benchmark run with metrics and scoring to disk.
    /// Returns the ID (filename without extension) of the saved run.
    pub fn save_run(&self, run: BenchmarkRun) -> Result<String, Box<dyn std::error::Error>> {
        let filename = Self::generate_filename();
        let filepath = self.benchmarks_dir.join(&filename);

        eprintln!("[BENCHMARK_MANAGER] Saving benchmark run to: {}", filepath.display());

        // Serialize to pretty JSON for user readability
        let json = serde_json::to_string_pretty(&run)?;
        eprintln!("[BENCHMARK_MANAGER] Serialized run size: {} bytes", json.len());

        // Write to disk
        fs::write(&filepath, &json)?;
        eprintln!("[BENCHMARK_MANAGER] ✓ Benchmark run persisted: {}", filename);

        // Return ID without extension for consistency with HistoryManager
        Ok(filename.replace(".json", ""))
    }

    /// List all available benchmark run IDs (filenames/timestamps)
    ///
    /// Returns a sorted list of run filenames in reverse chronological order (newest first).
    pub fn list_runs(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut runs = Vec::new();

        if !self.benchmarks_dir.exists() {
            eprintln!("[BENCHMARK_MANAGER] Benchmarks directory does not exist: {}", self.benchmarks_dir.display());
            return Ok(runs);
        }

        for entry in fs::read_dir(&self.benchmarks_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only include JSON files matching the pattern
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    if filename_str.starts_with("run_") && filename_str.ends_with(".json") {
                        runs.push(filename_str.to_string());
                    }
                }
            }
        }

        // Sort in reverse order (newest first)
        runs.sort_by(|a, b| b.cmp(a));
        eprintln!("[BENCHMARK_MANAGER] Listed {} benchmark runs", runs.len());

        Ok(runs)
    }

    /// Load a benchmark run by ID (filename)
    ///
    /// Returns the deserialized `BenchmarkRun` or an error if not found.
    pub fn load_run(&self, id: &str) -> Result<BenchmarkRun, Box<dyn std::error::Error>> {
        // Handle both with and without .json extension
        let filename = if id.ends_with(".json") {
            id.to_string()
        } else {
            format!("{}.json", id)
        };
        
        let filepath = self.benchmarks_dir.join(&filename);

        eprintln!("[BENCHMARK_MANAGER] Loading benchmark run from: {}", filepath.display());

        if !filepath.exists() {
            return Err(format!("Benchmark run not found: {}", id).into());
        }

        let json = fs::read_to_string(&filepath)?;
        eprintln!("[BENCHMARK_MANAGER] Loaded JSON file: {} bytes", json.len());

        let run: BenchmarkRun = serde_json::from_str(&json)?;
        eprintln!("[BENCHMARK_MANAGER] ✓ Benchmark run deserialized successfully");

        Ok(run)
    }

    /// Delete a benchmark run by ID
    pub fn delete_run(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let filename = if id.ends_with(".json") {
            id.to_string()
        } else {
            format!("{}.json", id)
        };
        
        let filepath = self.benchmarks_dir.join(&filename);

        if !filepath.exists() {
            return Err(format!("Benchmark run not found: {}", id).into());
        }

        fs::remove_file(&filepath)?;
        eprintln!("[BENCHMARK_MANAGER] ✓ Benchmark run deleted: {}", id);

        Ok(())
    }

    /// Get the count of stored benchmark runs
    pub fn run_count(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let runs = self.list_runs()?;
        Ok(runs.len())
    }
}

impl Default for BenchmarkRunManager {
    fn default() -> Self {
        Self::new().expect("Failed to initialize BenchmarkRunManager")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot() -> PerformanceSnapshot {
        PerformanceSnapshot::new(
            PerformanceMetrics {
                current_us: 10.0,
                max_us: 50.0,
                p99_us: 45.0,
                p99_9_us: 48.0,
                avg_us: 15.0,
                total_spikes: 2,
                total_smis: 0,
                spikes_correlated_to_smi: 0,
                histogram_buckets: vec![],
                jitter_history: vec![],
                active_governor: "schedutil".to_string(),
                governor_hz: 2400,
                core_temperatures: vec![],
                package_temperature: 0.0,
            },
            KernelContext {
                version: "6.7.0".to_string(),
                scx_profile: "gaming".to_string(),
                lto_config: "thin".to_string(),
                governor: "schedutil".to_string(),
            },
        )
    }

    #[test]
    fn test_history_creation() {
        let history = PerformanceHistory::new(12);
        assert_eq!(history.count(), 0);
    }

    #[test]
    fn test_add_snapshot() {
        let mut history = PerformanceHistory::new(3);
        let snapshot = create_test_snapshot();

        history.add_snapshot(snapshot);
        assert_eq!(history.count(), 1);
    }

    #[test]
    fn test_max_snapshots_enforcement() {
        let mut history = PerformanceHistory::new(2);

        for _ in 0..5 {
            history.add_snapshot(create_test_snapshot());
        }

        assert_eq!(history.count(), 2);
    }

    #[test]
    fn test_latest_snapshot() {
        let mut history = PerformanceHistory::new(5);
        let snapshot = create_test_snapshot();

        history.add_snapshot(snapshot.clone());
        assert!(history.latest().is_some());
        assert_eq!(history.latest().unwrap().metrics.max_us, snapshot.metrics.max_us);
    }

    #[test]
    fn test_average_latency() {
        let mut history = PerformanceHistory::new(5);

        for _ in 0..3 {
            history.add_snapshot(create_test_snapshot());
        }

        let avg = history.average_latency_across_history();
        assert!(avg.is_some());
        assert_eq!(avg.unwrap(), 15.0); // All snapshots have avg_us = 15.0
    }

    #[test]
    fn test_clear() {
        let mut history = PerformanceHistory::new(5);
        history.add_snapshot(create_test_snapshot());

        assert_eq!(history.count(), 1);
        history.clear();
        assert_eq!(history.count(), 0);
    }

    #[test]
    fn test_export_record() {
        let mut history = PerformanceHistory::new(5);
        history.add_snapshot(create_test_snapshot());

        let stressors = vec!["stress-ng".to_string()];
        let buckets = vec![];

        let record = history.export_record(stressors.clone(), buckets);
        assert!(record.is_some());
        assert_eq!(record.unwrap().active_stressors, stressors);
    }

    #[test]
    fn test_benchmark_run_creation() {
        use crate::system::performance::scoring::PersonalityType;
        
        let run = BenchmarkRun::new(
            KernelContext {
                version: "6.7.0".to_string(),
                scx_profile: "gaming".to_string(),
                lto_config: "thin".to_string(),
                governor: "schedutil".to_string(),
            },
            PerformanceMetrics {
                current_us: 10.0,
                max_us: 50.0,
                p99_us: 45.0,
                p99_9_us: 48.0,
                avg_us: 15.0,
                rolling_p99_us: 45.0,
                rolling_p99_9_us: 48.0,
                rolling_throughput_p99: 500000.0,
                rolling_efficiency_p99: 10.0,
                rolling_consistency_us: 5.0,
                total_spikes: 2,
                total_smis: 0,
                spikes_correlated_to_smi: 0,
                histogram_buckets: vec![],
                jitter_history: vec![],
                active_governor: "schedutil".to_string(),
                governor_hz: 2400,
                core_temperatures: vec![],
                package_temperature: 0.0,
                benchmark_metrics: None,
            },
            ScoringResult {
                goat_score: 750,
                personality: PersonalityType::Balanced,
                primary_strength: "Latency: 75.0/100".to_string(),
                secondary_strength: "Consistency: 70.0/100".to_string(),
                improvement_area: "Thermal: 65.0/100".to_string(),
                brief: "Balanced personality profile (⚖️) delivers very good performance overall. Strongest in Latency.".to_string(),
                is_balanced_override: false,
                specialization_index: 5.0,
            },
            vec!["stress-ng".to_string()],
            Some(60.0),
            Some("test_run".to_string()),
        );

        assert_eq!(run.scoring.goat_score, 750);
        assert!(run.label.is_some());
    }
}

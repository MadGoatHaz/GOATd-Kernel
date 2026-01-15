/// Performance Dashboard View with Spectrum Visualization
///
/// Displays real-time performance metrics, jitter history, CPU heatmap,
/// and the "Performance Spectrum" (7 horizontal metric strips) for comprehensive
/// performance analysis.
///
/// Features:
/// - Simplified Benchmark Controls (compact, radio/checkbox only)
/// - Performance Spectrum: 7 cyberpunk-styled metric strips with micro-sparklines
/// - High-density dashboard with status-colored progress indicators
/// - Live metrics from AppController

use eframe::egui;
use std::sync::Arc;
use std::cell::RefCell;
use std::time::{Instant, Duration};
use tokio::sync::RwLock;
use crate::ui::controller::AppController;
use crate::log_info;
use super::widgets;
use crate::system::performance::{MonitoringMode, StressorType};

/// Performance Spectrum Strip definition
#[derive(Clone, Debug)]
pub struct SpectrumStrip {
    pub label: &'static str,
    pub value: f32,
    pub max_value: f32,
    pub color: egui::Color32,
    pub history: Vec<f32>, // 10-second sparkline history
    pub moving_avg: f32,   // 10-sample moving average for "Pulse" indicator
    pub normalized_score: f32, // 0.0-1.0 normalized score for color gradient
    pub raw_value_display: String, // Raw metric value for digital display (e.g., "29.8µs")
}

impl SpectrumStrip {
    pub fn new(label: &'static str, max_value: f32, color: egui::Color32) -> Self {
        SpectrumStrip {
            label,
            value: 0.0,
            max_value,
            color,
            history: Vec::new(),
            moving_avg: 0.0,
            normalized_score: 0.0,
            raw_value_display: String::new(),
        }
    }

    /// Update value and add to history (max 100 samples for 10s at 10Hz)
    /// Also calculates moving average for pulse indicator
    pub fn update(&mut self, value: f32) {
        self.value = value;
        self.history.push(value);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        
        // DIAGNOSTIC: Track allocation frequency
        static UPDATE_CALL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let call_count = UPDATE_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if call_count % 500 == 0 && call_count > 0 {
            log_info!("[SPECTRUM_UPDATE] {:5} alloc cycles | label={} | history_len={:3} | avg={:.2}",
                call_count, self.label, self.history.len(), self.moving_avg);
        }
        
        // Calculate 10-sample moving average for "Pulse"
        let window_size = 10;
        let window: Vec<f32> = self.history.iter().rev().take(window_size).cloned().collect();
        if !window.is_empty() {
            self.moving_avg = window.iter().sum::<f32>() / window.len() as f32;
        }
    }

    /// Get normalized progress (0.0 to 1.0)
    pub fn progress(&self) -> f32 {
        (self.value / self.max_value).max(0.0).min(1.0)
    }
}

/// Cached comparison result for UI display
#[derive(Clone, Debug)]
struct ComparisonCacheEntry {
    test_a_id: String,
    test_b_id: String,
    kernel_a: String,
    kernel_b: String,
    // Metric values from test A (tuple format: kernel, scx, lto, min, max, avg, p99_9, smi_count, stall_count)
    min_us_a: f32,
    max_us_a: f32,
    avg_us_a: f32,
    p99_9_us_a: f32,
    smi_count_a: i32,
    stall_count_a: i32,
    // Metric values from test B
    min_us_b: f32,
    max_us_b: f32,
    avg_us_b: f32,
    p99_9_us_b: f32,
    smi_count_b: i32,
    stall_count_b: i32,
    // Delta percentages (min_delta, max_delta, avg_delta, p99.9_delta, smi_delta, stall_delta)
    min_delta: f32,
    max_delta: f32,
    avg_delta: f32,
    p99_9_delta: f32,
    smi_delta: f32,
    stall_delta: f32,
}

/// Performance UI state
pub struct PerformanceUIState {
    /// Last time metrics were updated from AppController
    last_update: RefCell<Instant>,
    /// Throttle interval (100ms)
    throttle_interval: Duration,
    /// Benchmark duration selection state (in seconds: 0=continuous, 30=30s, 60=1m, 300=5m)
    benchmark_duration_seconds: RefCell<u32>,
    /// Stressor toggles
    stressor_cpu_enabled: RefCell<bool>,
    stressor_memory_enabled: RefCell<bool>,
    stressor_scheduler_enabled: RefCell<bool>,
    /// Benchmark countdown timer
    benchmark_countdown: RefCell<Option<f64>>,
    /// Live monitoring mode flag (diagnostic mode)
    live_monitoring_active: RefCell<bool>,
    /// Comparison UI state
    comparison_test_a_selected: RefCell<Option<String>>,
    comparison_test_b_selected: RefCell<Option<String>>,
    comparison_available_tests: RefCell<Vec<crate::system::performance::history::PerformanceRecordMetadata>>,
    comparison_result_cache: Arc<std::sync::Mutex<Option<ComparisonCacheEntry>>>,
    /// Track last loaded test IDs to prevent redundant fetches
    comparison_last_loaded_ids: RefCell<(Option<String>, Option<String>)>,
    /// Last time comparison test list was refreshed
    last_records_refresh: RefCell<Instant>,
    /// Show comparison popup window
    show_comparison_popup: RefCell<bool>,
    /// Show benchmark name prompt
    show_name_benchmark_prompt: RefCell<bool>,
    /// Track if naming prompt was already triggered for current completion
    naming_prompt_triggered: RefCell<bool>,
    /// Custom benchmark name input buffer
    benchmark_name_input: RefCell<String>,
    /// Test selected for deletion in comparison window
    test_to_delete: RefCell<Option<String>>,
    
    /// === SPECTRUM METRICS STATE ===
    /// Performance Spectrum strips (7 metrics: Latency, Throughput, Jitter, CPU Eff, Thermal, Consistency, SMI Res)
    spectrum_strips: RefCell<Vec<SpectrumStrip>>,
    /// Cached GOAT Score (0-1000)
    goat_score: RefCell<u16>,
}

impl PerformanceUIState {
    pub fn new() -> Self {
        let spectrum_strips = vec![
            SpectrumStrip::new("Latency", 100.0, egui::Color32::from_rgb(0x51, 0xaf, 0xef)), // Cyan
            SpectrumStrip::new("Throughput", 100.0, egui::Color32::from_rgb(0xff, 0xaa, 0x00)), // Orange
            SpectrumStrip::new("Jitter", 100.0, egui::Color32::from_rgb(0xff, 0xff, 0x00)), // Yellow
            SpectrumStrip::new("Efficiency", 100.0, egui::Color32::from_rgb(0x00, 0xff, 0x00)), // Green
            SpectrumStrip::new("Thermal", 100.0, egui::Color32::from_rgb(0xff, 0x00, 0x00)), // Red
            SpectrumStrip::new("Consistency", 100.0, egui::Color32::from_rgb(0xff, 0x00, 0xff)), // Magenta
            SpectrumStrip::new("SMI Res.", 100.0, egui::Color32::from_rgb(0xff, 0xff, 0xff)), // White
        ];

        PerformanceUIState {
            last_update: RefCell::new(Instant::now()),
            throttle_interval: Duration::from_millis(100),
            benchmark_duration_seconds: RefCell::new(60),
            stressor_cpu_enabled: RefCell::new(true),
            stressor_memory_enabled: RefCell::new(false),
            stressor_scheduler_enabled: RefCell::new(false),
            benchmark_countdown: RefCell::new(None),
            live_monitoring_active: RefCell::new(false),
            comparison_test_a_selected: RefCell::new(None),
            comparison_test_b_selected: RefCell::new(None),
            comparison_available_tests: RefCell::new(Vec::new()),
            comparison_result_cache: Arc::new(std::sync::Mutex::new(None)),
            comparison_last_loaded_ids: RefCell::new((None, None)),
            last_records_refresh: RefCell::new(Instant::now()),
            show_comparison_popup: RefCell::new(false),
            show_name_benchmark_prompt: RefCell::new(false),
            naming_prompt_triggered: RefCell::new(false),
            benchmark_name_input: RefCell::new(String::new()),
            test_to_delete: RefCell::new(None),
            spectrum_strips: RefCell::new(spectrum_strips),
            goat_score: RefCell::new(0),
        }
    }
    
    /// Check if metrics should be refreshed based on throttle interval
    pub fn should_update(&self) -> bool {
        self.last_update.borrow().elapsed() >= self.throttle_interval
    }
    
    /// Record that an update occurred
    pub fn mark_updated(&self) {
        *self.last_update.borrow_mut() = Instant::now();
    }
    
    /// Get selected benchmark duration in seconds
    /// Returns None for Continuous mode
    /// Returns Some(seconds) for Benchmark mode
    /// Special handling: 999 maps to SystemBenchmark mode (handled via get_monitoring_mode)
    pub fn get_benchmark_duration_secs(&self) -> Option<u64> {
        let seconds = *self.benchmark_duration_seconds.borrow();
        match seconds {
            0 => None, // Continuous
            999 => None, // SystemBenchmark (handled separately via get_monitoring_mode)
            _ => Some(seconds as u64),
        }
    }
    
    /// Get the monitoring mode based on current duration selection
    /// Maps duration values to appropriate MonitoringMode variants
    pub fn get_monitoring_mode(&self) -> MonitoringMode {
        let seconds = *self.benchmark_duration_seconds.borrow();
        match seconds {
            0 => MonitoringMode::Continuous,
            999 => MonitoringMode::SystemBenchmark,
            secs => MonitoringMode::Benchmark(std::time::Duration::from_secs(secs as u64)),
        }
    }
    
    /// Get selected stressors
    pub fn get_selected_stressors(&self) -> Vec<StressorType> {
        let mut stressors = Vec::new();
        if *self.stressor_cpu_enabled.borrow() {
            stressors.push(StressorType::Cpu);
        }
        if *self.stressor_memory_enabled.borrow() {
            stressors.push(StressorType::Memory);
        }
        if *self.stressor_scheduler_enabled.borrow() {
            stressors.push(StressorType::Scheduler);
        }
        stressors
    }

    /// Update spectrum metrics from performance data
    ///
    /// Normalized Score Formula (0.0-1.0):
    /// - Lower is Better (L.i.B): `(1.0 - (val - optimal) / (poor - optimal)).clamp(0, 1)`
    /// - Higher is Better (H.i.B): `((val - poor) / (optimal - poor)).clamp(0, 1)`
    ///
    /// All bars fill left-to-right as performance improves (score increases to 1.0).
    /// Uses 1000-sample rolling P99 values for score calculation to allow recovery.
    /// Professional "KernBench" Tier Calibration:
    /// - P99.9 Latency: 0-200µs (Green <20µs, Yellow 50µs, Red >100µs) for micro-stutter detection
    /// - Max Latency: 0-1000µs (Green <50µs, Yellow 150µs, Red >500µs) for peak performance
    pub fn update_spectrum_from_metrics(&self, metrics: &crate::system::performance::PerformanceMetrics) {
        let mut strips = self.spectrum_strips.borrow_mut();

        // ===== METRIC 0: Latency (L.i.B) - HIGH-END RIG CALIBRATION =====
        // Using rolling 1000-sample P99 ONLY - NO session max fallback for recovery
        // High-end calibration (9800X3D/265K): 10µs (Optimal/1.0) → 500µs (Poor/0.0)
        // This reflects "Holy Grail" performance for ultra-responsive systems
        let latency_val = metrics.rolling_p99_us.max(10.0).min(500.0);
        let latency_norm = (1.0 - ((latency_val - 10.0) / 490.0)).clamp(0.001, 1.0);
        strips[0].update(latency_norm * 100.0);
        strips[0].normalized_score = latency_norm;
        strips[0].raw_value_display = format!("{:.1}µs", latency_val);

        // ===== METRIC 1: Throughput (H.i.B) =====
        // Using rolling 1000-sample P99 throughput ONLY - NO session max fallback for recovery
        // Higher is better: 1.0M ops/s (Optimal/1.0) → 100k ops/s (Poor/0.0)
        let (throughput_norm, throughput_display_str) = if metrics.rolling_throughput_p99 > 0.0 {
            let norm = ((metrics.rolling_throughput_p99 - 100_000.0) / 900_000.0).clamp(0.0, 1.0);
            (norm, format!("{:.0}k/s", metrics.rolling_throughput_p99 / 1000.0))
        } else if let Some(bm) = metrics.benchmark_metrics.as_ref() {
            if let Some(syscall) = bm.syscall_saturation.as_ref() {
                let val_f32 = syscall.calls_per_second as f32;
                let norm = ((val_f32 - 100_000.0) / 900_000.0).clamp(0.0, 1.0);
                (norm, format!("{:.0}k/s", val_f32 / 1000.0))
            } else {
                // Fallback: inverse rolling latency mapping (not max)
                let norm = (50.0 / metrics.rolling_p99_us.max(50.0)).clamp(0.0, 1.0);
                let estimated_ops = 1_000_000.0 / metrics.rolling_p99_us.max(1.0);
                (norm, format!("{:.0}k/s", estimated_ops / 1000.0))
            }
        } else {
            // Fallback: inverse rolling latency mapping (not max)
            let norm = (50.0 / metrics.rolling_p99_us.max(50.0)).clamp(0.0, 1.0);
            let estimated_ops = 1_000_000.0 / metrics.rolling_p99_us.max(1.0);
            (norm, format!("{:.0}k/s", estimated_ops / 1000.0))
        };
        strips[1].update(throughput_norm * 100.0);
        strips[1].normalized_score = throughput_norm.clamp(0.001, 1.0);
        strips[1].raw_value_display = throughput_display_str;

        // ===== METRIC 2: Jitter (L.i.B) - CLAMPED RELATIVE JITTER (Purity of Scheduling) =====
        // Clamped Relative Jitter Formula: (1.0 - ((std_dev / mean) - 0.05) / 0.25).clamp(0.001, 1.0)
        // Optimal: 5% Noise (1.0) - Holy Grail of Scheduling Purity
        // Poor: 30% Noise (0.001) - Objectively High Jitter
        // Maps scheduling consistency across all rigs (accounts for baseline latency)
        let (jitter_val, jitter_norm) = if metrics.jitter_history.is_empty() {
            (0.0, 1.0) // Default to optimal if no data
        } else {
            let mean = metrics.jitter_history.iter().sum::<f32>() / metrics.jitter_history.len() as f32;
            if mean <= 0.0 {
                (0.0, 1.0) // Avoid division by zero
            } else {
                let variance = metrics.jitter_history.iter()
                    .map(|x| (x - mean).powi(2))
                    .sum::<f32>() / metrics.jitter_history.len() as f32;
                let std_dev = variance.sqrt();
                let relative_jitter = std_dev / mean;  // Coefficient of Variation
                let norm = (1.0 - ((relative_jitter - 0.05) / 0.25)).clamp(0.001, 1.0);
                (std_dev, norm)
            }
        };
        strips[2].update(jitter_norm * 100.0);
        strips[2].normalized_score = jitter_norm;
        strips[2].raw_value_display = format!("{:.1}µs", jitter_val);

        // ===== METRIC 3: CPU Efficiency (L.i.B) - Context-Switch Overhead =====
        // Using rolling 1000-sample P99 efficiency ONLY - NO session max fallback for recovery
        // Lower efficiency overhead = Higher score: 1µs (Optimal/1.0) → 100µs (Poor/0.0)
        let (cpu_eff_norm, cpu_eff_display) = if metrics.rolling_efficiency_p99 > 0.0 {
            // Use rolling P99 efficiency value (context-switch overhead in µs)
            let efficiency_val = metrics.rolling_efficiency_p99.max(1.0).min(100.0);
            let norm = (1.0 - ((efficiency_val - 1.0) / 99.0)).clamp(0.001, 1.0);
            (norm, format!("{:.1}µs", efficiency_val))
        } else if let Some(bm) = metrics.benchmark_metrics.as_ref() {
            if let Some(ctx_switch) = bm.context_switch_rtt.as_ref() {
                // Use rolling equivalent if available, not session max
                let efficiency = ctx_switch.avg_rtt_us.max(1.0).min(100.0);
                let norm = (1.0 - ((efficiency - 1.0) / 99.0)).clamp(0.001, 1.0);
                (norm, format!("{:.1}µs", efficiency))
            } else {
                // Fallback: use rolling P99 latency for estimation (not max)
                let norm = (50.0 / metrics.rolling_p99_us.max(50.0)).clamp(0.001, 1.0);
                let estimated_eff = metrics.rolling_p99_us.max(1.0);
                (norm, format!("{:.1}µs", estimated_eff))
            }
        } else {
            // Fallback: use rolling P99 latency for estimation (not max)
            let norm = (50.0 / metrics.rolling_p99_us.max(50.0)).clamp(0.001, 1.0);
            let estimated_eff = metrics.rolling_p99_us.max(1.0);
            (norm, format!("{:.1}µs", estimated_eff))
        };
        strips[3].update(cpu_eff_norm * 100.0);
        strips[3].normalized_score = cpu_eff_norm;
        strips[3].raw_value_display = cpu_eff_display;

        // ===== METRIC 4: Thermal (L.i.B) =====
        // Lower is better: 40°C (Optimal/1.0) → 90°C (Poor/0.0)
        // Uses MAX core temperature to match backend implementation in scoring.rs
        let max_temp = if metrics.core_temperatures.is_empty() {
            45.0 // Default to neutral if no data
        } else {
            metrics.core_temperatures.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        };
        let thermal_norm = if max_temp == 0.0 {
            0.5 // Neutral default
        } else {
            (1.0 - ((max_temp - 40.0) / 50.0)).clamp(0.001, 1.0)
        };
        strips[4].update(thermal_norm * 100.0);
        strips[4].normalized_score = thermal_norm;
        strips[4].raw_value_display = format!("{:.1}°C", max_temp);

        // ===== METRIC 5: Consistency (L.i.B) - LABORATORY GRADE CV % SCALE =====
        // Using Coefficient of Variation (CV % = std_dev / mean) for frame pacing precision
        // CV % reflects relative jitter independent of baseline latency
        // Laboratory Grade Calibration:
        // - CV <= 5% (< 0.05): 1.0 score (Laboratory Grade / "Silent" Kernels)
        // - CV >= 30% (>= 0.30): 0.001 score (Poor / Frame Pacing Issues)
        // Linear Normalization: Score = (1.0 - (CV - 0.05) / 0.25).clamp(0.001, 1.0)
        let std_dev_us = metrics.rolling_consistency_us;  // Standard deviation from rolling window
        let (consistency_norm, _cv_percent) = if metrics.rolling_p99_us > 0.0 && std_dev_us > 0.0 {
            // Calculate CV from rolling latency mean and std_dev
            let cv = std_dev_us / metrics.rolling_p99_us;  // Coefficient of Variation
            let norm = (1.0 - ((cv - 0.05) / 0.25)).clamp(0.001, 1.0);
            (norm, cv * 100.0)
        } else {
            (1.0, 0.0)  // Default to optimal if data unavailable
        };
        strips[5].update(consistency_norm * 100.0);
        strips[5].normalized_score = consistency_norm;
        // Display: Show only the standard deviation in µs (NO CV% label)
        strips[5].raw_value_display = format!("{:.1}µs", std_dev_us);

        // ===== METRIC 6: SMI Resilience (H.i.B) =====
        // Higher is better: 0 SMIs (Optimal/1.0) → 10+ SMIs (Poor/0.0)
        // Returns 1.0 (Full/Green) if no SMIs are detected
        let smi_norm = if metrics.total_smis == 0 {
            1.0 // Perfect score if no SMIs detected
        } else {
            let ratio = metrics.spikes_correlated_to_smi as f32 / metrics.total_smis as f32;
            (1.0 - ratio.min(1.0)).clamp(0.001, 1.0)
        };
        strips[6].update(smi_norm * 100.0);
        strips[6].normalized_score = smi_norm;
        strips[6].raw_value_display = if metrics.total_smis == 0 {
            "0 SMIs".to_string()
        } else {
            format!("{}/{}", metrics.spikes_correlated_to_smi, metrics.total_smis)
        };

        // ===== CALCULATE GOAT SCORE (0-1000) =====
        // Weighted sum of normalized metrics (0.0-1.0), scaled to 0-1000
        // 7 metrics with rebalanced weights:
        // Latency (27%), Consistency (18%), Jitter (15%), Throughput (10%), CPU Eff (10%), Thermal (10%), SMI Res (10%)
        let goat_score = calculate_goat_score(
            latency_norm,       // 27% weight - responsiveness
            consistency_norm,   // 18% weight - stability
            jitter_norm,        // 15% weight - micro-precision
            throughput_norm,    // 10% weight - syscall throughput
            cpu_eff_norm,       // 10% weight - context-switch efficiency
            thermal_norm,       // 10% weight - thermal stability
            smi_norm,           // 10% weight - interrupt mitigation
        );
        *self.goat_score.borrow_mut() = goat_score;
    }
}

thread_local! {
    static PERF_UI_STATE: PerformanceUIState = PerformanceUIState::new();
}

/// Calculate jitter (standard deviation) from latency samples
fn calculate_jitter(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let variance = samples
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f32>() / samples.len() as f32;
    
    variance.sqrt()
}

/// Helper: Get color based on score
fn progress_bar_color(value: f32) -> egui::Color32 {
    if value >= 80.0 {
        egui::Color32::from_rgb(0x98, 0xbe, 0x65) // Green
    } else if value >= 60.0 {
        egui::Color32::from_rgb(0xda, 0x85, 0x48) // Orange
    } else {
        egui::Color32::from_rgb(0xff, 0x64, 0x64) // Red
    }
}

/// Helper: Determine delta color based on improvement/regression
/// For latency/SMI/stalls: negative/lower = GREEN (improvement), positive/higher = RED (regression)
/// For throughput: positive/higher = GREEN (improvement), negative/lower = RED (regression)
fn get_delta_color(delta_percent: f32, is_lower_better: bool) -> egui::Color32 {
    let threshold = 0.5; // Minimal change threshold
    
    if is_lower_better {
        // For latency, SMI, stalls: negative is good (improvement)
        if delta_percent < -threshold {
            egui::Color32::from_rgb(0x98, 0xbe, 0x65) // Green - improvement
        } else if delta_percent > threshold {
            egui::Color32::from_rgb(0xff, 0x64, 0x64) // Red - regression
        } else {
            egui::Color32::from_rgb(0xa0, 0xa0, 0xa0) // Gray - minimal change
        }
    } else {
        // For throughput: positive is good (improvement)
        if delta_percent > threshold {
            egui::Color32::from_rgb(0x98, 0xbe, 0x65) // Green - improvement
        } else if delta_percent < -threshold {
            egui::Color32::from_rgb(0xff, 0x64, 0x64) // Red - regression
        } else {
            egui::Color32::from_rgb(0xa0, 0xa0, 0xa0) // Gray - minimal change
        }
    }
}

/// Render horizontal bar chart for comparison delta
/// Shows value A vs value B with visual bar indicating relative performance
///
/// Parameters:
/// - ui: egui context
/// - val_a: Value from test A
/// - val_b: Value from test B
/// - delta_percent: Change percentage (calculated as (B - A) / A * 100)
/// - is_lower_better: If true, negative delta is improvement (latency/SMI); if false, positive is improvement (throughput)
/// - max_bar_width: Width allocated for the bar visualization
fn render_comparison_bar(
    ui: &mut egui::Ui,
    val_a: f32,
    val_b: f32,
    delta_percent: f32,
    is_lower_better: bool,
    max_bar_width: f32,
) {
    ui.horizontal(|ui| {
        ui.set_max_width(max_bar_width);
        
        // Values display (30% of width)
        ui.vertical(|ui| {
            ui.set_max_width(max_bar_width * 0.30);
            ui.label(egui::RichText::new(format!("{:.2}", val_a)).monospace().small());
            ui.label(egui::RichText::new(format!("{:.2}", val_b)).monospace().small());
        });
        
        ui.separator();
        
        // Bar visualization (70% of width)
        let bar_area_width = max_bar_width * 0.70;
        let bar_height = 40.0;
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(bar_area_width, bar_height),
            egui::Sense::hover(),
        );
        
        let bar_rect = response.rect;
        let center_x = bar_rect.min.x + bar_rect.width() / 2.0;
        
        // Get color based on delta
        let bar_color = get_delta_color(delta_percent, is_lower_better);
        
        // Draw background (center line)
        painter.line_segment(
            [egui::pos2(center_x, bar_rect.min.y + 5.0), egui::pos2(center_x, bar_rect.max.y - 5.0)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        );
        
        // Draw delta bar
        let delta_clamped = delta_percent.max(-100.0).min(100.0); // Clamp to reasonable range
        let bar_width = (delta_clamped.abs() / 100.0) * (bar_rect.width() / 2.0 - 10.0);
        
        let bar_x_min = if delta_clamped < 0.0 {
            center_x - bar_width
        } else {
            center_x
        };
        let bar_x_max = if delta_clamped < 0.0 {
            center_x
        } else {
            center_x + bar_width
        };
        
        let bar_fill_rect = egui::Rect::from_min_max(
            egui::pos2(bar_x_min, bar_rect.min.y + 12.0),
            egui::pos2(bar_x_max, bar_rect.max.y - 12.0),
        );
        
        painter.rect_filled(bar_fill_rect, 2.0, bar_color);
        painter.rect_stroke(bar_fill_rect, 2.0, egui::Stroke::new(1.0, bar_color));
        
        // Draw delta percentage text in center
        let delta_text = format!("{:+.1}%", delta_percent);
        painter.text(
            bar_rect.center(),
            egui::Align2::CENTER_CENTER,
            &delta_text,
            egui::FontId::new(11.0, egui::FontFamily::Monospace),
            bar_color,
        );
    });
}

/// Get gradient color based on normalized score (0.0-1.0)
/// Correct interpolation: Red (0.0) -> Orange (0.40) -> Yellow (0.75) -> Green (1.0)
fn get_score_color(score: f32) -> egui::Color32 {
    let clamped = score.max(0.0).min(1.0);
    
    if clamped <= 0.40 {
        // Red to Orange: 0.0-0.40
        let t = clamped / 0.40;
        let r = ((0xff as f32 * (1.0 - t)) + (0xda as f32 * t)) as u8;
        let g = ((0x64 as f32 * (1.0 - t)) + (0x85 as f32 * t)) as u8;
        let b = ((0x64 as f32 * (1.0 - t)) + (0x48 as f32 * t)) as u8;
        egui::Color32::from_rgb(r, g, b)
    } else if clamped <= 0.75 {
        // Orange to Yellow: 0.40-0.75
        let t = (clamped - 0.40) / 0.35;
        let r = ((0xda as f32 * (1.0 - t)) + (0xEC as f32 * t)) as u8;
        let g = ((0x85 as f32 * (1.0 - t)) + (0xbe as f32 * t)) as u8;
        let b = ((0x48 as f32 * (1.0 - t)) + (0x7B as f32 * t)) as u8;
        egui::Color32::from_rgb(r, g, b)
    } else {
        // Yellow to Green: 0.75-1.0
        let t = (clamped - 0.75) / 0.25;
        let r = ((0xEC as f32 * (1.0 - t)) + (0x98 as f32 * t)) as u8;
        let g = ((0xbe as f32 * (1.0 - t)) + (0xbe as f32 * t)) as u8;
        let b = ((0x7B as f32 * (1.0 - t)) + (0x65 as f32 * t)) as u8;
        egui::Color32::from_rgb(r, g, b)
    }
}

/// Calculate GOAT Score (0-1000) from normalized metric scores (0.0-1.0)
/// Weights per spec (7 metrics):
/// - Latency: 27% (responsiveness)
/// - Consistency: 18%
/// - Jitter: 15% (micro-precision)
/// - Throughput: 10%
/// - CPU Eff: 10%
/// - Thermal: 10%
/// - SMI Res: 10%
fn calculate_goat_score(
    latency_norm: f32,
    consistency_norm: f32,
    jitter_norm: f32,
    throughput_norm: f32,
    cpu_eff_norm: f32,
    thermal_norm: f32,
    smi_norm: f32,
) -> u16 {
    let weighted_score =
        (latency_norm * 0.27) +
        (consistency_norm * 0.18) +
        (jitter_norm * 0.15) +
        (throughput_norm * 0.10) +
        (cpu_eff_norm * 0.10) +
        (thermal_norm * 0.10) +
        (smi_norm * 0.10);
    
    // Convert from 0.0-1.0 to 0-1000 with specialization multiplier
    ((weighted_score * 1000.0).min(1000.0)) as u16
}

/// Get performance tier label and color based on GOAT Score
fn get_performance_tier(goat_score: u16) -> (&'static str, egui::Color32) {
    match goat_score {
        900..=1000 => ("S-TIER", egui::Color32::from_rgb(0x00, 0xFF, 0x00)), // Neon Green
        800..=899 => ("A-TIER", egui::Color32::from_rgb(0x51, 0xaf, 0xef)), // Cyan
        700..=799 => ("B-TIER", egui::Color32::from_rgb(0xEC, 0xBE, 0x7B)), // Yellow
        _ => ("C-TIER", egui::Color32::from_rgb(0xda, 0x85, 0x48)), // Orange/Red
    }
}

/// Get temperature color using 5-point gradient: Blue (20°C) → Green → Yellow → Orange → Red (95°C)
fn get_temp_color(temp: f32) -> egui::Color32 {
    let normalized = ((temp - 20.0) / (95.0 - 20.0)).max(0.0).min(1.0);
    
    if normalized < 0.25 {
        // Blue (20°C) to Green
        let t = normalized / 0.25;
        let r = (0 as f32 * (1.0 - t) + 0 as f32 * t) as u8;
        let g = (100 as f32 * (1.0 - t) + 200 as f32 * t) as u8;
        let b = (200 as f32 * (1.0 - t) + 100 as f32 * t) as u8;
        egui::Color32::from_rgb(r, g, b)
    } else if normalized < 0.5 {
        // Green to Yellow
        let t = (normalized - 0.25) / 0.25;
        let r = (0 as f32 * (1.0 - t) + 255 as f32 * t) as u8;
        let g = (200 as f32 * (1.0 - t) + 255 as f32 * t) as u8;
        let b = (100 as f32 * (1.0 - t) + 0 as f32 * t) as u8;
        egui::Color32::from_rgb(r, g, b)
    } else if normalized < 0.75 {
        // Yellow to Orange
        let t = (normalized - 0.5) / 0.25;
        let r = (255 as f32 * (1.0 - t) + 255 as f32 * t) as u8;
        let g = (255 as f32 * (1.0 - t) + 165 as f32 * t) as u8;
        let b = (0 as f32 * (1.0 - t) + 0 as f32 * t) as u8;
        egui::Color32::from_rgb(r, g, b)
    } else {
        // Orange to Red
        let t = (normalized - 0.75) / 0.25;
        let r = (255 as f32 * (1.0 - t) + 255 as f32 * t) as u8;
        let g = (165 as f32 * (1.0 - t) + 50 as f32 * t) as u8;
        let b = (0 as f32 * (1.0 - t) + 50 as f32 * t) as u8;
        egui::Color32::from_rgb(r, g, b)
    }
}

/// Render compact Mini Heatmap (8-column grid of temperature blocks)
/// Layout: 8 blocks across, 2 rows for 16 cores, refined rectangles (48x35px)
fn render_mini_heatmap(ui: &mut egui::Ui, core_temps: &[f32]) {
     if core_temps.is_empty() {
         ui.label("No temperature data");
         return;
     }
     
     // Fixed grid: 8 columns, tight spacing, refined block dimensions
     let block_width = 48.0;  // Width of each block (-5% from 50.6px)
     let block_height = 35.0; // Height of each block (+5% from 33.6px)
    let tight_spacing = 2.0; // Tight spacing between blocks
    
    // Fixed 8 columns layout
    let cols = 8;
    
    // Center the entire grid both horizontally and vertically within allocated space
    ui.vertical_centered(|ui| {
        for chunk in core_temps.chunks(cols) {
            ui.horizontal(|ui| {
                ui.set_max_height(block_height);
                ui.spacing_mut().item_spacing = egui::vec2(tight_spacing, tight_spacing);
                ui.horizontal_centered(|ui| {
                    for &temp in chunk.iter() {
                        // Use 5-point gradient color based on absolute temperature (20°C to 95°C)
                        let bg_color = get_temp_color(temp);
                        
                        // Determine text color based on temperature
                        let text_color = if temp < 50.0 {
                            egui::Color32::from_rgb(255, 255, 255) // Light text for cool temps
                        } else {
                            egui::Color32::from_rgb(0, 0, 0) // Dark text for warm temps
                        };
                        
                        // Allocate space and draw block
                        let block_size_vec = egui::Vec2::new(block_width, block_height);
                        let (response, painter) = ui.allocate_painter(block_size_vec, egui::Sense::hover());
                        
                        // Fill block
                        painter.rect_filled(response.rect, 2.0, bg_color);
                        painter.rect_stroke(response.rect, 2.0, egui::Stroke::new(0.5, egui::Color32::DARK_GRAY));
                        
                        // Draw temperature text inside block (smaller font for compact layout)
                        let temp_text = format!("{:.0}°", temp);
                        painter.text(
                            response.rect.center(),
                            egui::Align2::CENTER_CENTER,
                            &temp_text,
                            egui::FontId::new(9.0, egui::FontFamily::Monospace),
                            text_color,
                        );
                    }
                });
            });
        }
    });
}

/// Render a single Performance Spectrum strip with Signal & Pulse design
/// Layout: [Label (65px)] [Digital Value (50px)] [Signal Bar (fill)]
/// Integrated sparkline + moving average pulse overlay within signal bar
/// Uses dynamic color gradient based on normalized_score (0.0-1.0)
///
/// Optional phase_highlight: If Some, pulsing effect is applied to this strip if it's the active metric for the phase
fn render_spectrum_strip(ui: &mut egui::Ui, strip: &SpectrumStrip, phase_highlight: Option<&str>) {
    // Get dynamic color based on normalized score
    let mut dynamic_color = get_score_color(strip.normalized_score);
    
    // Apply pulsing effect if this strip is highlighted for the current phase
    // Pulsing is achieved by brightening the color every 500ms
    if let Some(highlight_metric) = phase_highlight {
        if strip.label.eq_ignore_ascii_case(highlight_metric) {
            let pulse_phase = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() / 250) % 4;
            
            // Brighten color on alternating cycles (creates pulsing effect)
            if pulse_phase < 2 {
                let brightness_mult = 1.3;
                let r = ((dynamic_color.r() as f32 * brightness_mult) as u8).min(255);
                let g = ((dynamic_color.g() as f32 * brightness_mult) as u8).min(255);
                let b = ((dynamic_color.b() as f32 * brightness_mult) as u8).min(255);
                dynamic_color = egui::Color32::from_rgb(r, g, b);
            }
        }
    }
    
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0; // Reduced spacing between elements

        // Label (70px)
        ui.vertical(|ui| {
            ui.set_max_width(70.0);
            ui.set_min_width(70.0);
            ui.label(egui::RichText::new(strip.label).monospace().small());
        });

        // Digital Value (60px) - Display raw value, not the score
        ui.vertical(|ui| {
            ui.set_max_width(60.0);
            ui.set_min_width(60.0);
            let value_text = if strip.raw_value_display.is_empty() {
                format!("{:.1}", strip.value)
            } else {
                strip.raw_value_display.clone()
            };
            ui.colored_label(
                dynamic_color,
                egui::RichText::new(value_text).monospace().strong(),
            );
        });

        // Signal & Pulse Bar (remaining space - custom painted)
        ui.vertical(|ui| {
            let available_width = ui.available_width();
            // DIAGNOSTIC: Log width underflow detection
            if available_width < 50.0 {
                log_info!("[SPECTRUM_STRIP] ⚠️ WIDTH UNDERFLOW ALERT: label={}, available_width={:.1}px (min 50px required)", strip.label, available_width);
            }
            ui.set_max_width(available_width);
            
            // Allocate painter for the signal bar
            let bar_height = 30.0;
            let (response, painter) = ui.allocate_painter(
                egui::Vec2::new(available_width, bar_height),
                egui::Sense::hover(),
            );

            // Background fill
            painter.rect_filled(response.rect, 2.0, egui::Color32::from_black_alpha(50));
            painter.rect_stroke(response.rect, 2.0, egui::Stroke::new(0.5, egui::Color32::DARK_GRAY));

            let bar_width = response.rect.width();
            let bar_height = response.rect.height();
            let min_x = response.rect.min.x;
            let min_y = response.rect.min.y;
            let max_y = response.rect.max.y;

            // ===== DRAW BACKGROUND SPARKLINE (subtle area-filled) =====
            if !strip.history.is_empty() {
                let min_val = strip.history.iter().copied().fold(f32::INFINITY, f32::min);
                let max_val = strip.history.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let range = (max_val - min_val).max(0.1);

                // Create polygon points for area-filled sparkline
                let mut sparkline_points = Vec::new();
                
                // Top edge of sparkline
                for (i, &val) in strip.history.iter().enumerate() {
                    let x = (min_x + (i as f32 / strip.history.len().max(1) as f32) * bar_width).floor();
                    let normalized = (val - min_val) / range;
                    let y = (max_y - normalized * (bar_height * 0.7)).floor(); // Use 70% of bar height
                    sparkline_points.push(egui::pos2(x, y));
                }
                
                // Bottom edge (reverse) to close the polygon
                let last_x = (min_x + bar_width).floor();
                sparkline_points.push(egui::pos2(last_x, max_y));
                sparkline_points.push(egui::pos2(min_x, max_y));

                // Draw subtle filled area with clipping
                if sparkline_points.len() > 2 {
                    let sparkline_color = egui::Color32::from_rgba_premultiplied(
                        dynamic_color.r(), dynamic_color.g(), dynamic_color.b(), 30
                    );
                    // Shrink the clip rect by 1px on all sides to prevent bleeding
                    let clip_rect = response.rect.shrink(1.0);
                    let clipped_painter = painter.with_clip_rect(clip_rect);
                    clipped_painter.add(egui::Shape::convex_polygon(sparkline_points, sparkline_color, egui::Stroke::NONE));
                }
            }

            // ===== DRAW SEGMENTED SIGNAL BLOCKS =====
            let num_segments = 25; // 20-30 blocks as per spec
            let segment_padding = 2.0; // Strict 2px padding between segments
            let usable_width = bar_width - (segment_padding * (num_segments as f32 - 1.0));
            let segment_width = (usable_width / num_segments as f32).max(1.0);

            let progress = strip.progress();
            let filled_segments = (progress * num_segments as f32).ceil() as usize;

            for i in 0..num_segments {
                // Integer-based step calculation for non-overlapping segments
                let segment_x = (min_x + i as f32 * (segment_width + segment_padding)).floor();
                let segment_max_x = (segment_x + segment_width).floor();
                let segment_rect = egui::Rect::from_min_max(
                    egui::pos2(segment_x, (min_y + 4.0).floor()),
                    egui::pos2(segment_max_x, (max_y - 4.0).floor()),
                );

                if i < filled_segments {
                    // Filled segment: apply glow to peak blocks (last 2-3)
                    let is_peak = filled_segments > 0 && i >= (filled_segments.saturating_sub(3));
                    let segment_color = if is_peak {
                        // Reduced brightness boost for peak blocks (1.3 -> 1.15)
                        let brightness_boost = 1.15;
                        let r = ((dynamic_color.r() as f32 * brightness_boost) as u8).min(255);
                        let g = ((dynamic_color.g() as f32 * brightness_boost) as u8).min(255);
                        let b = ((dynamic_color.b() as f32 * brightness_boost) as u8).min(255);
                        egui::Color32::from_rgb(r, g, b)
                    } else {
                        dynamic_color
                    };

                    painter.rect_filled(segment_rect, 1.0, segment_color);
                    
                    // Add reduced glow stroke to peak blocks (stroke thickness 1.0 -> 0.5)
                    if is_peak {
                        painter.rect_stroke(segment_rect, 1.0, egui::Stroke::new(0.5, segment_color));
                    }
                } else {
                    // Unfilled segment: dark semi-transparent background
                    painter.rect_filled(segment_rect, 1.0, egui::Color32::from_black_alpha(100));
                }
            }

            // ===== OVERLAY PULSE INDICATOR (moving average line) =====
            if !strip.history.is_empty() {
                let min_val = strip.history.iter().copied().fold(f32::INFINITY, f32::min);
                let max_val = strip.history.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                let range = (max_val - min_val).max(0.1);

                // Calculate pulse position based on moving average
                let avg_normalized = (strip.moving_avg - min_val) / range;
                let pulse_y = (max_y - avg_normalized * (bar_height * 0.7)).round();
                
                // DIAGNOSTIC: Detect out-of-bounds pulse indicators that may indicate data anomalies
                if pulse_y < min_y || pulse_y > max_y {
                    log_info!("[SPECTRUM_PULSE] 🚨 OUT-OF-BOUNDS: label={} | pulse_y={:.1} | bounds=[{:.1}, {:.1}]",
                        strip.label, pulse_y, min_y, max_y);
                }
                
                // Draw pulse indicator as a thin horizontal line with slight glow
                let pulse_color = egui::Color32::from_rgb(0x51, 0xaf, 0xef); // Cyan glow
                painter.line_segment(
                    [egui::pos2((min_x + 2.0).round(), pulse_y), egui::pos2((min_x + bar_width - 2.0).round(), pulse_y)],
                    egui::Stroke::new(2.0, pulse_color),
                );
            }
        });
    });
}

/// Render benchmark completion summary with final GOAT Score and phase metrics
fn render_benchmark_completion_summary(
    ui: &mut egui::Ui,
    controller: &Arc<RwLock<AppController>>,
    state: &PerformanceUIState,
) {
    ui.group(|ui| {
        ui.heading("🏆 BENCHMARK COMPLETE");
        ui.separator();
        
        // Display final GOAT Score prominently
        let goat_score = *state.goat_score.borrow();
        let (tier_label, tier_color) = get_performance_tier(goat_score);
        
        ui.horizontal(|ui| {
            ui.colored_label(
                tier_color,
                egui::RichText::new(format!("Final GOAT Score: {}/1000", goat_score))
                    .monospace()
                    .strong()
                    .size(18.0),
            );
            ui.colored_label(
                tier_color,
                egui::RichText::new(tier_label)
                    .monospace()
                    .strong()
                    .size(16.0),
            );
        });
        
        ui.separator();
        
        // Display phase metrics if available from orchestrator
        if let Ok(ctrl) = controller.try_read() {
            if let Ok(orch_lock) = ctrl.benchmark_orchestrator.read() {
                if let Some(ref orch) = *orch_lock {
                    if !orch.phase_metrics.is_empty() {
                        ui.label("Phase Results:");
                        for (i, (phase_name, metrics)) in orch.phase_metrics.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Phase {}: {}", i + 1, phase_name));
                                ui.separator();
                                ui.label(format!(
                                    "Max: {:.1}µs | P99: {:.1}µs | P99.9: {:.1}µs",
                                    metrics.max_us, metrics.p99_us, metrics.p99_9_us
                                ));
                            });
                        }
                    }
                }
            }
        }
    });
}

/// Render the benchmark phase status display
/// Shows current phase name, number, and countdown timer during SystemBenchmark mode
fn render_phase_status(ui: &mut egui::Ui, controller: &Arc<RwLock<AppController>>) {
    if let Ok(ctrl) = controller.try_read() {
        if let Ok(orch_lock) = ctrl.benchmark_orchestrator.read() {
            if let Some(ref orch) = *orch_lock {
                let elapsed = orch.elapsed_secs();
                let phase = orch.current_phase;
                let phase_start = phase.start_time();
                let phase_end = phase.end_time();
                let time_in_phase = elapsed.saturating_sub(phase_start);
                let time_remaining = phase_end.saturating_sub(elapsed);
                
                // Determine phase number (1-6)
                let phase_num = match phase {
                    crate::system::performance::BenchmarkPhase::Baseline => 1,
                    crate::system::performance::BenchmarkPhase::ComputationalHeat => 2,
                    crate::system::performance::BenchmarkPhase::MemorySaturation => 3,
                    crate::system::performance::BenchmarkPhase::SchedulerFlood => 4,
                    crate::system::performance::BenchmarkPhase::GamingSimulator => 5,
                    crate::system::performance::BenchmarkPhase::TheGauntlet => 6,
                };
                
                // Render phase status with colored background
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        // Phase label
                        let phase_label = format!("PHASE {}/6: {}", phase_num, phase);
                        ui.colored_label(
                            egui::Color32::from_rgb(0x51, 0xaf, 0xef), // Cyan
                            egui::RichText::new(&phase_label).monospace().strong(),
                        );
                        
                        // Spacer
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Time remaining
                            let countdown_text = format!("{}s remaining", time_remaining);
                            ui.colored_label(
                                egui::Color32::from_rgb(0xff, 0xaa, 0x00), // Orange
                                egui::RichText::new(&countdown_text).monospace().small(),
                            );
                        });
                    });
                    
                    // Progress bar showing phase progress
                    let progress = (time_in_phase as f32) / 10.0; // 10 seconds per phase
                    ui.add(
                        egui::ProgressBar::new(progress.min(1.0))
                            .show_percentage()
                            .text(format!("{}/10s", time_in_phase))
                    );
                });
            }
        }
    }
}

/// Render the Performance Spectrum card (7 horizontal metric strips)
/// SYNCHRONIZED WIDTH: Matches the KPI card above by using same column width constraint
/// All internal strips scale to fit the forced width perfectly (no bleeding)
fn render_performance_spectrum(ui: &mut egui::Ui, state: &PerformanceUIState, phase_highlight: Option<&str>) {
    ui.group(|ui| {
        // CRITICAL: Get available width at start and use it as fixed constraint
        // This ensures the spectrum card matches the KPI card width exactly
        let column_width = ui.available_width();
        ui.set_max_width(column_width);
        ui.set_min_width(column_width);
        
        // === Header with GOAT Score and Tier ===
        ui.horizontal(|ui| {
            ui.heading("⚡ Performance Spectrum");
            
            // SPACER
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let goat_score = *state.goat_score.borrow();
                let (tier_label, tier_color) = get_performance_tier(goat_score);
                
                // PROMINENT SCORE DISPLAY: Larger text for better visibility
                // This is where completion feedback is shown
                let score_text = format!("🎯 {} / 1000", goat_score);
                ui.colored_label(
                    tier_color,
                    egui::RichText::new(&score_text)
                        .monospace()
                        .strong()
                        .size(16.0),
                );
                
                ui.colored_label(
                    tier_color,
                    egui::RichText::new(tier_label)
                        .monospace()
                        .strong()
                        .size(14.0),
                );
            });
        });
        ui.separator();

        let strips = state.spectrum_strips.borrow();
        for strip in strips.iter() {
            render_spectrum_strip(ui, strip, phase_highlight);
        }
    });
}

/// Render the Performance tab with live metrics from AppController
pub fn render_performance(
    ui: &mut egui::Ui,
    controller: &Arc<RwLock<AppController>>,
) {
    ui.heading("Performance Dashboard");
    ui.separator();
    
    // Check atomic dirty flag: if metrics were updated by background processor, request repaint
    if let Ok(ctrl) = controller.try_read() {
        if ctrl.atomic_perf_dirty.load(std::sync::atomic::Ordering::Acquire) {
            // Clear the dirty flag now that we're repainting
            ctrl.atomic_perf_dirty.store(false, std::sync::atomic::Ordering::Release);
            // Request repaint to keep UI responsive while monitoring is active
            ui.ctx().request_repaint();
        }
    }
    
    // Extract latest metrics from AppController using try_read (non-blocking)
    let metrics = {
        if let Ok(ctrl) = controller.try_read() {
            ctrl.get_current_performance_metrics().ok()
        } else {
            None
        }
    };
    
    // Extract jitter history from AppController (always use latest, no throttling for render)
    let (max_latency, p99_9_latency, jitter_history, core_temps) =
        PERF_UI_STATE.with(|state| {
            // FIXED: Always show latest metrics in UI, only throttle data collection at controller level
            // This ensures gauges/charts update every frame while data collection is still throttled
            let max_lat = metrics.as_ref().map(|m| m.max_us).unwrap_or(0.0);
            let p99_9_lat = metrics.as_ref().map(|m| m.p99_9_us).unwrap_or(0.0);
            let jitter_vec = metrics.as_ref().map(|m| m.jitter_history.clone()).unwrap_or_default();
            let core_temps = metrics.as_ref().map(|m| m.core_temperatures.clone()).unwrap_or_default();
            
            // Update spectrum from current metrics
            if let Some(ref m) = metrics {
                state.update_spectrum_from_metrics(m);
            }
            
            (max_lat, p99_9_lat, jitter_vec, core_temps)
        });
    
    // Extract monitoring status
    let (is_monitoring, lifecycle_state) = {
        if let Ok(ctrl) = controller.try_read() {
            ctrl.get_monitoring_status()
        } else {
            (false, "Unknown".to_string())
        }
    };
    
    // Display monitoring status with completion indicator
    let (status_text, status_color) = if lifecycle_state == "Completed" {
        ("✅ BENCHMARK COMPLETE".to_string(), egui::Color32::from_rgb(0x98, 0xbe, 0x65))
    } else if is_monitoring {
        (format!("🟢 MONITORING ACTIVE ({})", lifecycle_state), egui::Color32::GREEN)
    } else {
        ("⏸ Idle".to_string(), egui::Color32::GRAY)
    };
    
    ui.colored_label(
        status_color,
        egui::RichText::new(&status_text)
            .monospace()
            .strong()
            .size(14.0)
    );
    
    ui.separator();
    
    // === PHASE STATUS DISPLAY (for GOATd Full Benchmark / SystemBenchmark) ===
    PERF_UI_STATE.with(|state| {
        let duration = *state.benchmark_duration_seconds.borrow();
        if duration == 999 {  // SystemBenchmark mode
            if lifecycle_state == "Completed" {
                render_benchmark_completion_summary(ui, controller, state);
                
                // Auto-show name benchmark prompt when benchmark completes
                // Only trigger once per completion cycle
                let mut naming_triggered = state.naming_prompt_triggered.borrow_mut();
                if !*naming_triggered {
                    *state.show_name_benchmark_prompt.borrow_mut() = true;
                    *naming_triggered = true;
                }
            } else {
                render_phase_status(ui, controller);
                // Reset trigger flag when benchmark is no longer completed
                *state.naming_prompt_triggered.borrow_mut() = false;
            }
            ui.separator();
        }
    });
    
    // === MAIN LAYOUT: KPIs, Spectrum, Perpetual Jitter History, and Controls ===
    let jitter_val = calculate_jitter(&jitter_history);
    let avg_temp = core_temps.iter().sum::<f32>() / core_temps.len().max(1) as f32;
    
    ui.columns(2, |cols| {
        // ===== LEFT COLUMN: KPIs & Performance Spectrum =====
        cols[0].group(|ui| {
            ui.set_max_height(168.0); // Match Jitter History height exactly
            ui.set_min_height(168.0); // Lock height for precise vertical alignment
            
            ui.label("Real-Time KPIs (Professional Tiers)");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    // Max Latency KPI: Professional tier 0-1000µs (Green<50, Yellow 150, Red>500)
                    widgets::radial_gauge(ui, max_latency, 0.0..1000.0, "Max Latency (µs)");
                });
                ui.vertical(|ui| {
                    // P99.9 Latency KPI: Professional tier 0-200µs (Green<20, Yellow 50, Red>100)
                    widgets::radial_gauge(ui, p99_9_latency, 0.0..200.0, "P99.9 Latency (µs)");
                });
                ui.vertical(|ui| {
                    widgets::radial_gauge(ui, jitter_val, 0.0..50.0, "Jitter (σ)");
                });
                ui.vertical(|ui| {
                    widgets::radial_gauge(ui, avg_temp, 0.0..100.0, "Package Temp (°C)");
                });
            });
        });

        // === Performance Spectrum Display (Single column, full width) ===
         PERF_UI_STATE.with(|state| {
             // Determine phase-specific metric highlight for pulsing effect
             let phase_highlight = {
                 if let Ok(ctrl) = controller.try_read() {
                     if let Ok(orch_lock) = ctrl.benchmark_orchestrator.read() {
                         if let Some(ref orch) = *orch_lock {
                             use crate::system::performance::BenchmarkPhase;
                             let phase = orch.current_phase;
                             Some(match phase {
                                 BenchmarkPhase::Baseline => "Latency",  // Phase 1: baseline latency baseline
                                 BenchmarkPhase::ComputationalHeat => "Latency",  // Phase 2: CPU heat = latency stress
                                 BenchmarkPhase::MemorySaturation => "Throughput",  // Phase 3: memory = throughput stress
                                 BenchmarkPhase::SchedulerFlood => "Jitter",  // Phase 4: scheduler = jitter stress
                                 BenchmarkPhase::GamingSimulator => "Efficiency",  // Phase 5: gaming = efficiency
                                 BenchmarkPhase::TheGauntlet => "Consistency",  // Phase 6: ultimate = consistency
                             }).map(|s| s.to_string())
                         } else {
                             None
                         }
                     } else {
                         None
                     }
                 } else {
                     None
                 }
             };
             
             let phase_ref = phase_highlight.as_deref();
             render_performance_spectrum(&mut cols[0], state, phase_ref);
         });
        
        
        // ===== RIGHT COLUMN: Perpetual Jitter History, Benchmark Controls with Mini Heatmap =====
        
        // === PERPETUAL Jitter History (Always Visible, Restored to 168px) ===
         // Vertical height restored to 168px with internal graph expanded to fill card
         cols[1].group(|ui| {
             ui.set_max_height(168.0); // Restored to 168px
             ui.set_min_height(168.0); // Lock height
             
             ui.label("📈 Jitter History & Analysis");
             if jitter_history.is_empty() {
                 ui.label("Monitoring idle - no data yet");
             } else {
                 // Expanded sparkline: allocate all vertical space flush with bottom
                 ui.vertical_centered(|ui| {
                     ui.set_max_height(f32::INFINITY); // Allow sparkline to expand fully
                     ui.spacing_mut().item_spacing.y = 0.0; // Remove internal bottom gaps
                     widgets::sparkline(ui, &jitter_history,
                         &format!("Samples: {}, Min: {:.2}µs, Max: {:.2}µs",
                             jitter_history.len(),
                             jitter_history.iter().fold(f32::INFINITY, |a, &b| a.min(b)),
                             jitter_history.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b))
                         )
                     );
                 });
             }
         });
        
        // === SIMPLIFIED Benchmark Controls with Integrated Mini Heatmap ===
        cols[1].group(|ui| {
            ui.label("⚙️ Benchmark Controls & Temps");
            ui.separator();
            
            PERF_UI_STATE.with(|state| {
                ui.label("Duration:");
                let duration = *state.benchmark_duration_seconds.borrow();
                
                // Compact horizontal radio buttons
                ui.horizontal(|ui| {
                    if ui.radio(duration == 0, "Continuous").clicked() {
                        *state.benchmark_duration_seconds.borrow_mut() = 0;
                    }
                    if ui.radio(duration == 30, "30s").clicked() {
                        *state.benchmark_duration_seconds.borrow_mut() = 30;
                    }
                    if ui.radio(duration == 60, "1m").clicked() {
                        *state.benchmark_duration_seconds.borrow_mut() = 60;
                    }
                    if ui.radio(duration == 300, "5m").clicked() {
                        *state.benchmark_duration_seconds.borrow_mut() = 300;
                    }
                    if ui.radio(duration == 999, "GOATd Benchmark (60s)").clicked() {
                        *state.benchmark_duration_seconds.borrow_mut() = 999;
                    }
                });
                
                ui.separator();
                
                // === Stressors & Temperature Grid: Horizontal Layout ===
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(150.0);
                        ui.label("Stressors:");
                        let mut cpu_enabled = *state.stressor_cpu_enabled.borrow();
                        let mut mem_enabled = *state.stressor_memory_enabled.borrow();
                        let mut sched_enabled = *state.stressor_scheduler_enabled.borrow();
                        
                        if ui.checkbox(&mut cpu_enabled, "CPU").changed() {
                            *state.stressor_cpu_enabled.borrow_mut() = cpu_enabled;
                        }
                        if ui.checkbox(&mut mem_enabled, "Memory").changed() {
                            *state.stressor_memory_enabled.borrow_mut() = mem_enabled;
                        }
                        if ui.checkbox(&mut sched_enabled, "Scheduler").changed() {
                            *state.stressor_scheduler_enabled.borrow_mut() = sched_enabled;
                        }
                    });
                    
                    ui.separator();
                    
                    // Temperature Grid moved to right of Stressors
                    if !core_temps.is_empty() {
                        ui.vertical(|ui| {
                            ui.set_max_width(280.0);
                            render_mini_heatmap(ui, &core_temps);
                        });
                    }
                });
                
                ui.separator();
                
                let (is_monitoring, _) = {
                    if let Ok(ctrl) = controller.try_read() {
                        ctrl.get_monitoring_status()
                    } else {
                        (false, "Unknown".to_string())
                    }
                };
                
                // Start/Stop button - text changes based on mode
                let button_text = if is_monitoring {
                    "Stop Monitoring"
                } else {
                    let duration = *state.benchmark_duration_seconds.borrow();
                    if duration == 999 {
                        "Run GOATd Gauntlet"
                    } else {
                        "Start Benchmark"
                    }
                };
                let button_color = if is_monitoring {
                    egui::Color32::from_rgb(200, 50, 50)
                } else {
                    egui::Color32::from_rgb(50, 200, 50)
                };
                
                if ui.button(egui::RichText::new(button_text).color(button_color)).clicked() {
                    if is_monitoring {
                        let controller_clone = controller.clone();
                        tokio::spawn(async move {
                            if let Ok(ctrl) = controller_clone.try_read() {
                                let _ = ctrl.handle_stop_monitoring();
                            }
                        });
                    } else {
                        let controller_clone = controller.clone();
                        let stressors = state.get_selected_stressors();
                        let monitoring_mode = state.get_monitoring_mode();
                        
                        tokio::spawn(async move {
                            if let Ok(ctrl) = controller_clone.try_read() {
                                let _ = ctrl.handle_trigger_monitoring(monitoring_mode, stressors);
                            }
                        });
                    }
                }
                
                ui.separator();
                
                // Live Monitoring toggle
                let live_monitoring = *state.live_monitoring_active.borrow();
                let live_button_text = if live_monitoring { "■ Stop Live" } else { "▶ Live Monitor" };
                
                if ui.button(live_button_text).clicked() {
                    if live_monitoring {
                        let controller_clone = controller.clone();
                        *state.live_monitoring_active.borrow_mut() = false;
                        tokio::spawn(async move {
                            if let Ok(ctrl) = controller_clone.try_read() {
                                let _ = ctrl.handle_stop_monitoring();
                            }
                        });
                    } else {
                        let controller_clone = controller.clone();
                        *state.live_monitoring_active.borrow_mut() = true;
                        tokio::spawn(async move {
                            if let Ok(ctrl) = controller_clone.try_read() {
                                let _ = ctrl.handle_trigger_monitoring(MonitoringMode::Continuous, vec![]);
                            }
                        });
                    }
                }
                
                ui.separator();
                
                // Compare Results button - toggle comparison popup
                if ui.button("📊 Compare Results").clicked() {
                    let show_popup = *state.show_comparison_popup.borrow();
                    *state.show_comparison_popup.borrow_mut() = !show_popup;
                }
                
            });
        });
        
    });
    
    // === COMPARISON RESULTS POPUP ===
    PERF_UI_STATE.with(|state| {
        let show_popup = *state.show_comparison_popup.borrow();
        
        if show_popup {
            let mut is_open = true;
            egui::Window::new("Compare Performance Results")
                .open(&mut is_open)
                .resizable(true)
                .default_width(900.0)
                .show(ui.ctx(), |ui| {
                    ui.label("Select two performance tests to compare");
                    ui.separator();
                    
                    // Fetch available test records with metadata (throttled to max once per 2 seconds)
                    let test_records = {
                        let mut should_refresh = false;
                        {
                            let last_refresh = state.last_records_refresh.borrow();
                            if last_refresh.elapsed() >= Duration::from_secs(2) {
                                should_refresh = true;
                            }
                        }
                        
                        if should_refresh {
                            if let Ok(ctrl) = controller.try_read() {
                                let records = ctrl.get_comparison_test_ids().unwrap_or_default();
                                *state.last_records_refresh.borrow_mut() = Instant::now();
                                *state.comparison_available_tests.borrow_mut() = records.clone();
                                records
                            } else {
                                state.comparison_available_tests.borrow().clone()
                            }
                        } else {
                            state.comparison_available_tests.borrow().clone()
                        }
                    };
                    
                    if test_records.is_empty() {
                        ui.colored_label(
                            egui::Color32::GRAY,
                            "No saved performance tests available - run benchmarks first"
                        );
                    } else {
                        // === TWO-COLUMN LAYOUT: Comparison Selection (Left) + Management (Right) ===
                        // Clone test IDs outside the column scope so they're available for comparison logic
                        let (test_a_id_cloned, test_b_id_cloned) = {
                            let test_a_selected = state.comparison_test_a_selected.borrow_mut();
                            let test_a_id = test_a_selected.clone();
                            drop(test_a_selected);
                            
                            let test_b_selected = state.comparison_test_b_selected.borrow_mut();
                            let test_b_id = test_b_selected.clone();
                            drop(test_b_selected);
                            
                            (test_a_id, test_b_id)
                        };
                        
                        ui.columns(2, |cols| {
                            // LEFT COLUMN: Test A & Test B selection
                            cols[0].label("Test A (Baseline):");
                            let mut test_a_selected = state.comparison_test_a_selected.borrow_mut();
                            
                            // Find display name from current selection
                            let test_a_display = test_a_selected.as_ref()
                                .and_then(|id| test_records.iter().find(|r| &r.id == id).map(|r| r.display_name.clone()))
                                .unwrap_or_else(|| "-- Select --".to_string());
                            
                            egui::ComboBox::from_id_source("test_a_combo")
                                .selected_text(&test_a_display)
                                .show_ui(&mut cols[0], |ui| {
                                    for record in &test_records {
                                        ui.selectable_value(&mut *test_a_selected, Some(record.id.clone()), &record.display_name);
                                    }
                                });
                            drop(test_a_selected);
                            
                            cols[0].label("Test B (Compare):");
                            let mut test_b_selected = state.comparison_test_b_selected.borrow_mut();
                            
                            // Find display name from current selection
                            let test_b_display = test_b_selected.as_ref()
                                .and_then(|id| test_records.iter().find(|r| &r.id == id).map(|r| r.display_name.clone()))
                                .unwrap_or_else(|| "-- Select --".to_string());
                            
                            egui::ComboBox::from_id_source("test_b_combo")
                                .selected_text(&test_b_display)
                                .show_ui(&mut cols[0], |ui| {
                                    for record in &test_records {
                                        ui.selectable_value(&mut *test_b_selected, Some(record.id.clone()), &record.display_name);
                                    }
                                });
                            drop(test_b_selected);

                            // RIGHT COLUMN: Management Controls (Delete)
                            cols[1].label("🗑️ Manage Results");
                            cols[1].separator();
                            
                            cols[1].label("Select test to delete:");
                            let mut test_to_delete = state.test_to_delete.borrow_mut();
                            
                            // Find display name from current selection
                            let delete_display = test_to_delete.as_ref()
                                .and_then(|id| test_records.iter().find(|r| &r.id == id).map(|r| r.display_name.clone()))
                                .unwrap_or_else(|| "-- Select --".to_string());
                            
                            egui::ComboBox::from_id_source("delete_combo")
                                .selected_text(&delete_display)
                                .show_ui(&mut cols[1], |ui| {
                                    for record in &test_records {
                                        ui.selectable_value(&mut *test_to_delete, Some(record.id.clone()), &record.display_name);
                                    }
                                });
                            
                            let test_to_delete_cloned = test_to_delete.clone();
                            drop(test_to_delete);
                            
                            if cols[1].button("Delete Result").clicked() {
                                if let Some(test_id) = test_to_delete_cloned {
                                    let controller_clone = controller.clone();
                                    let state_clone_for_refresh = state.test_to_delete.clone();
                                    
                                    tokio::spawn(async move {
                                        if let Ok(ctrl) = controller_clone.try_read() {
                                            match ctrl.handle_delete_performance_record(&test_id) {
                                                Ok(()) => {
                                                    log_info!("[PERF] [UI] ✅ Record deleted successfully: {}", test_id);
                                                }
                                                Err(e) => {
                                                    log_info!("[PERF] [UI] ❌ Failed to delete record: {}", e);
                                                }
                                            }
                                        }
                                        
                                        // Clear selection and force refresh
                                        *state_clone_for_refresh.borrow_mut() = None;
                                    });
                                    
                                    // Force immediate UI refresh of test list (set to past time)
                                    *state.last_records_refresh.borrow_mut() = Instant::now() - Duration::from_secs(3);
                                }
                            }
                        });
                        
                        // Comparison table when both tests are selected
                        if let (Some(a_id), Some(b_id)) = (&test_a_id_cloned, &test_b_id_cloned) {
                            ui.separator();
                            
                            // Enhanced header with kernel comparison summary
                            let cached_for_header = state.comparison_result_cache.lock().ok().and_then(|guard| guard.clone());
                            if let Some(cached) = cached_for_header {
                                ui.horizontal(|ui| {
                                    ui.heading("📊 Comparison Results");
                                    ui.separator();
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0x51, 0xaf, 0xef),
                                        egui::RichText::new(format!("{} vs {}", cached.kernel_a, cached.kernel_b))
                                            .monospace()
                                            .strong(),
                                    );
                                });
                            } else {
                                ui.heading("📊 Comparison Results");
                            }
                            ui.separator();
                            
                            // CRITICAL FIX: Check if currently selected IDs differ from last loaded IDs
                            // Only trigger a new load if the selection has changed
                            let need_reload = {
                                let mut last_loaded = state.comparison_last_loaded_ids.borrow_mut();
                                let current_a: Option<String> = Some(a_id.clone());
                                let current_b: Option<String> = Some(b_id.clone());
                                
                                // Load if selection differs from what was last loaded
                                if last_loaded.0 != current_a || last_loaded.1 != current_b {
                                    // Update immediately so we don't spam requests while this is fetching
                                    *last_loaded = (current_a.clone(), current_b.clone());
                                    true
                                } else {
                                    false
                                }
                            };
                            
                            // Only load comparison when selection CHANGES, never on every frame
                            if need_reload {
                                let controller_clone = controller.clone();
                                let a_id_copy = a_id.clone();
                                let b_id_copy = b_id.clone();
                                let cache_arc = Arc::clone(&state.comparison_result_cache);
                                
                                tokio::spawn(async move {
                                    if let Ok(ctrl) = controller_clone.try_read() {
                                        match ctrl.handle_compare_tests_request(&a_id_copy, &b_id_copy) {
                                            Ok((test_a, test_b, deltas)) => {
                                                log::debug!("[COMPARE] Comparison loaded: A={} vs B={}", a_id_copy, b_id_copy);
                                                
                                                // Cache the result with all metric values
                                                // test_a/test_b tuple: (kernel, scx, lto, min, max, avg, p99_9, smi_count, stall_count)
                                                // deltas tuple: (min_delta, max_delta, avg_delta, p99_9_delta, smi_delta, stall_delta)
                                                let cached = ComparisonCacheEntry {
                                                    test_a_id: a_id_copy.clone(),
                                                    test_b_id: b_id_copy.clone(),
                                                    kernel_a: test_a.0.clone(),
                                                    kernel_b: test_b.0.clone(),
                                                    min_us_a: test_a.3,
                                                    max_us_a: test_a.4,
                                                    avg_us_a: test_a.5,
                                                    p99_9_us_a: test_a.6,
                                                    smi_count_a: test_a.7,
                                                    stall_count_a: test_a.8,
                                                    min_us_b: test_b.3,
                                                    max_us_b: test_b.4,
                                                    avg_us_b: test_b.5,
                                                    p99_9_us_b: test_b.6,
                                                    smi_count_b: test_b.7,
                                                    stall_count_b: test_b.8,
                                                    min_delta: deltas.0,
                                                    max_delta: deltas.1,
                                                    avg_delta: deltas.2,
                                                    p99_9_delta: deltas.3,
                                                    smi_delta: deltas.4,
                                                    stall_delta: deltas.5,
                                                };
                                                
                                                if let Ok(mut c) = cache_arc.lock() {
                                                    *c = Some(cached);
                                                }
                                            }
                                            Err(e) => {
                                                log::debug!("[COMPARE] Comparison failed: {}", e);
                                            }
                                        }
                                    }
                                });
                            }
                            
                            // CRITICAL FIX: Clone cache data immediately and release lock
                            // This prevents holding the lock across UI rendering, which blocks
                            // the async task from updating the cache (causes stuck "Loading..." state)
                            let cached_entry = state.comparison_result_cache.lock().ok().and_then(|guard| guard.clone());
                            
                            if let Some(cached) = cached_entry {
                                // Build metric comparison data with raw values, deltas, and tooltips
                                // Format: (label, val_a, val_b, delta_percent, is_lower_better, tooltip)
                                let metric_rows: Vec<(&str, f32, f32, f32, bool, &str)> = vec![
                                    ("Min Latency (µs)", cached.min_us_a, cached.min_us_b, cached.min_delta, true, "Best-case scheduling response time"),
                                    ("Max Latency (µs)", cached.max_us_a, cached.max_us_b, cached.max_delta, true, "Worst-case scheduling response time"),
                                    ("Avg Latency (µs)", cached.avg_us_a, cached.avg_us_b, cached.avg_delta, true, "Mean scheduling response across all samples"),
                                    ("P99.9 (µs)", cached.p99_9_us_a, cached.p99_9_us_b, cached.p99_9_delta, true, "99.9th percentile latency (micro-stutter detection)"),
                                    ("SMI Count", cached.smi_count_a as f32, cached.smi_count_b as f32, cached.smi_delta, true, "System Management Interrupt occurrences"),
                                    ("Stall Correlated", cached.stall_count_a as f32, cached.stall_count_b as f32, cached.stall_delta, true, "Latency spikes correlated to SMI events"),
                                ];
                                
                                // Header row for comparison metrics
                                ui.label("Detailed Metric Comparison:");
                                ui.separator();
                                
                                // Reduce spacing between metric cards
                                ui.spacing_mut().item_spacing.y = 3.0;
                                
                                // Render each metric with full-width card design
                                for (metric_label, val_a, val_b, delta, is_lower_better, tooltip) in metric_rows {
                                    let card_bg_color = egui::Color32::from_rgba_unmultiplied(30, 35, 40, 200);
                                    
                                    // Full-width card with subtle background
                                    ui.group(|ui| {
                                        // Paint background for the card
                                        let available_width = ui.available_width();
                                        let (response, painter) = ui.allocate_painter(
                                            egui::Vec2::new(available_width, 48.0),
                                            egui::Sense::hover(),
                                        );
                                        
                                        // Draw card background with rounded corners
                                        painter.rect_filled(response.rect, 6.0, card_bg_color);
                                        painter.rect_stroke(
                                            response.rect,
                                            6.0,
                                            egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 90, 100)),
                                        );
                                        
                                        // Draw content within the card
                                        let inner_margin = 4.0;
                                        let content_rect = response.rect.shrink(inner_margin);
                                        
                                        // Layout: Col1 (Metric Name) | Col2 (Values) | Col3 (Delta Bar) | Col4 (Delta %)
                                        let col1_width = available_width * 0.20; // 20% for metric name
                                        let col2_width = available_width * 0.18; // 18% for values
                                        let col3_width = available_width * 0.40; // 40% for bar
                                        let _col4_width = available_width * 0.15; // 15% for delta %
                                        
                                        // Column 1: Metric Name (Monospace, Bold) with tooltip
                                        let metric_pos = egui::pos2(content_rect.min.x + 4.0, content_rect.min.y + 8.0);
                                        let metric_rect = egui::Rect::from_min_size(metric_pos, egui::vec2(col1_width, 20.0));
                                        
                                        painter.text(
                                            metric_pos,
                                            egui::Align2::LEFT_TOP,
                                            metric_label,
                                            egui::FontId::new(12.0, egui::FontFamily::Monospace),
                                            egui::Color32::from_rgb(200, 210, 220),
                                        );
                                        
                                        // Add tooltip on hover
                                        if response.hovered() && metric_rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) {
                                            egui::show_tooltip_at(
                                                ui.ctx(),
                                                egui::Id::new(("metric_tooltip", metric_label)),
                                                Some(ui.ctx().pointer_latest_pos().unwrap_or_default() + egui::vec2(10.0, 10.0)),
                                                |ui| {
                                                    ui.label(egui::RichText::new(tooltip).small().color(egui::Color32::from_rgb(220, 220, 220)));
                                                },
                                            );
                                        }
                                        
                                        // Column 2: Values (A vs B, subdued)
                                        let values_text = format!("{:.1} vs {:.1}", val_a, val_b);
                                        let values_pos = egui::pos2(content_rect.min.x + col1_width + 8.0, content_rect.min.y + 24.0);
                                        painter.text(
                                            values_pos,
                                            egui::Align2::LEFT_TOP,
                                            &values_text,
                                            egui::FontId::new(11.0, egui::FontFamily::Monospace),
                                            egui::Color32::from_rgb(150, 160, 170),
                                        );
                                        
                                        // Column 3: Delta Bar (centered at zero)
                                        let bar_x = content_rect.min.x + col1_width + col2_width + 4.0;
                                        let bar_y_top = content_rect.min.y + 8.0;
                                        let bar_width = col3_width - 8.0;
                                        let bar_height = 8.0;
                                        let bar_bottom = bar_y_top + bar_height + 14.0;
                                        
                                        // Draw center line (zero)
                                        let center_x = bar_x + bar_width / 2.0;
                                        painter.line_segment(
                                            [egui::pos2(center_x, bar_y_top), egui::pos2(center_x, bar_bottom)],
                                            egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 110, 120)),
                                        );
                                        
                                        // Draw delta bar
                                        let delta_clamped = delta.max(-100.0).min(100.0);
                                        let filled_width = (delta_clamped.abs() / 100.0) * (bar_width / 2.0 - 4.0);
                                        let bar_color = get_delta_color(delta, is_lower_better);
                                        
                                        let bar_rect = if delta_clamped < 0.0 {
                                            egui::Rect::from_min_max(
                                                egui::pos2(center_x - filled_width, bar_y_top + 2.0),
                                                egui::pos2(center_x, bar_bottom - 2.0),
                                            )
                                        } else {
                                            egui::Rect::from_min_max(
                                                egui::pos2(center_x, bar_y_top + 2.0),
                                                egui::pos2(center_x + filled_width, bar_bottom - 2.0),
                                            )
                                        };
                                        
                                        painter.rect_filled(bar_rect, 2.0, bar_color);
                                        painter.rect_stroke(bar_rect, 2.0, egui::Stroke::new(0.5, bar_color));
                                        
                                        // Column 4: Delta % (Large, Bold, Color-Coded)
                                        let delta_text = format!("{:+.1}%", delta);
                                        let delta_pos = egui::pos2(
                                            content_rect.min.x + col1_width + col2_width + col3_width + 4.0,
                                            content_rect.min.y + 18.0,
                                        );
                                        painter.text(
                                            delta_pos,
                                            egui::Align2::LEFT_TOP,
                                            &delta_text,
                                            egui::FontId::new(14.0, egui::FontFamily::Monospace),
                                            bar_color, // Use delta color for percentage
                                        );
                                    });
                                    
                                    ui.separator();
                                }
                            } else {
                                ui.label("Loading comparison data...");
                            }
                        }
                    }
                });
            
            // Update state based on window close
            if !is_open {
                *state.show_comparison_popup.borrow_mut() = false;
            }
        }
    });
    
    // === NAME BENCHMARK PROMPT ===
    PERF_UI_STATE.with(|state| {
        // FIX: Check visibility WITHOUT holding a mutable borrow across window rendering
        let should_show = *state.show_name_benchmark_prompt.borrow();
        
        if should_show {
            let should_close_window = std::cell::RefCell::new(false);
            let mut is_open = true;
            
            egui::Window::new("Name Your Benchmark")
                .open(&mut is_open)
                .resizable(false)
                .default_width(400.0)
                .show(ui.ctx(), |ui| {
                    ui.label("Enter a name for this benchmark result:");
                    ui.label(egui::RichText::new("(Empty = date/time format: YYYY-MM-DD HH:MM:SS)").small().italics());
                    
                    // FIX: Use temporary variable approach with proper scoping
                    // Get current value from RefCell, use temp var in UI, only update on change
                    let mut temp_name = {
                        state.benchmark_name_input.borrow().clone()
                    }; // Borrow immediately released
                    
                    let response = ui.text_edit_singleline(&mut temp_name);
                    
                    // Sync back to state only if changed
                    {
                        let mut name_input = state.benchmark_name_input.borrow_mut();
                        if *name_input != temp_name {
                            *name_input = temp_name.clone();
                        }
                    } // Borrow immediately released
                    
                    // Check for Enter key press
                    let mut should_save = false;
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        should_save = true;
                    }
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("Save Record").clicked() || should_save {
                            // Signal window to close IMMEDIATELY in this frame
                            *should_close_window.borrow_mut() = true;
                            
                            // Extract name BEFORE any async operations
                            let name = {
                                let name_input = state.benchmark_name_input.borrow();
                                if name_input.is_empty() {
                                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
                                } else {
                                    name_input.clone()
                                }
                            };
                            
                            // Spawn async save task (happens in background, window closes NOW)
                            let controller_clone = controller.clone();
                            tokio::spawn(async move {
                                if let Ok(ctrl) = controller_clone.try_read() {
                                    match ctrl.handle_save_performance_record(&name) {
                                        Ok(()) => {
                                            log_info!("[BENCHMARK_NAME] ✅ Record saved as: {}", name);
                                        }
                                        Err(e) => {
                                            log_info!("[BENCHMARK_NAME] ❌ Failed to save record: {}", e);
                                        }
                                    }
                                }
                            });
                            
                            // CRITICAL: Close window state IMMEDIATELY - happens before save completes
                            *state.show_name_benchmark_prompt.borrow_mut() = false;
                            
                            // Refresh records for comparison popup
                            *state.last_records_refresh.borrow_mut() = Instant::now() - Duration::from_secs(3);
                            
                            // Open comparison popup for next window
                            *state.show_comparison_popup.borrow_mut() = true;
                            
                            // Clear input for next use
                            state.benchmark_name_input.borrow_mut().clear();
                        }
                        
                        if ui.button("Cancel").clicked() {
                            *should_close_window.borrow_mut() = true;
                            *state.show_name_benchmark_prompt.borrow_mut() = false;
                            state.benchmark_name_input.borrow_mut().clear();
                        }
                    });
                });
            
            // Close window if button was clicked OR X was clicked
            if *should_close_window.borrow() {
                *state.show_name_benchmark_prompt.borrow_mut() = false;
            }
            if !is_open {
                *state.show_name_benchmark_prompt.borrow_mut() = false;
            }
        }
    });
}

/// Save current performance record to persistent history for future comparisons
fn save_performance_record(controller: &Arc<tokio::sync::RwLock<AppController>>) {
    let controller_clone = controller.clone();
    tokio::spawn(async move {
        if let Ok(ctrl) = controller_clone.try_read() {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let label = format!("perf_test_{}", timestamp);
            
            match ctrl.handle_save_performance_record(&label) {
                Ok(()) => {
                    log_info!("[SAVE_RECORD] ✓ Performance record saved successfully");
                    log_info!("[SAVE_RECORD]    Record ID: {}", label);
                }
                Err(e) => {
                    log_info!("[SAVE_RECORD] ❌ Failed to save record: {}", e);
                }
            }
        }
    });
}

/// Export current performance metrics to CSV and JSON files
fn export_performance_metrics(controller: &Arc<tokio::sync::RwLock<AppController>>) {
    let controller_clone = controller.clone();
    tokio::spawn(async move {
        if let Ok(ctrl) = controller_clone.try_read() {
            match ctrl.get_current_performance_metrics().ok() {
                Some(metrics) => {
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let csv_filename = format!("perf_metrics_{}.csv", timestamp);
                    let json_filename = format!("perf_metrics_{}.json", timestamp);
                    
                    // Export to JSON
                    match serde_json::to_string_pretty(&metrics) {
                        Ok(json_str) => {
                            match std::fs::write(&json_filename, &json_str) {
                                Ok(()) => {
                                    log_info!("[EXPORT] ✓ JSON: {}", json_filename);
                                }
                                Err(e) => {
                                    log_info!("[EXPORT] ✗ JSON write failed: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            log_info!("[EXPORT] ✗ JSON serialize failed: {}", e);
                        }
                    }
                    
                    // Export to CSV
                    let csv_content = format!(
                        "Metric,Value,Unit\ncurrent,{:.2},µs\nmax,{:.2},µs\naverage,{:.2},µs\np99,{:.2},µs\np99.9,{:.2},µs\ntotal_spikes,{},count\ntotal_smis,{},count\nsmi_correlated_spikes,{},count\nactive_governor,{},\ngovernor_hz,{},MHz\n",
                        metrics.current_us,
                        metrics.max_us,
                        metrics.avg_us,
                        metrics.p99_us,
                        metrics.p99_9_us,
                        metrics.total_spikes,
                        metrics.total_smis,
                        metrics.spikes_correlated_to_smi,
                        metrics.active_governor,
                        metrics.governor_hz
                    );
                    
                    match std::fs::write(&csv_filename, csv_content) {
                        Ok(()) => {
                            log_info!("[EXPORT] ✓ CSV: {}", csv_filename);
                        }
                        Err(e) => {
                            log_info!("[EXPORT] ✗ CSV write failed: {}", e);
                        }
                    }
                }
                None => {
                    log_info!("[EXPORT] ✗ No performance metrics available");
                }
            }
        }
    });
}

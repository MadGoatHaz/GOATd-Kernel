//! Phase 3.2 - Performance Scoring and Personality Analysis Engine
//!
//! This module implements the GOAT Score calculation for kernel performance using the
//! standardized 7-metric Performance Spectrum model.
//!
//! ## Architecture
//! - **7-Metric Spectrum**: Latency (27%), Consistency (18%), Jitter (15%), Throughput (10%),
//!   CPU Efficiency (10%), Thermal (10%), SMI Resilience (10%)
//! - **GOAT Score**: Weighted aggregate (0-1000) from normalized metrics
//! - **Personality Analysis**: Derives personality type based on metric strengths
//! - **Balanced Override**: Detects versatile kernels with no dominant weakness

use crate::system::performance::{BenchmarkMetrics, PerformanceMetrics};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Personality profile for a kernel configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalityType {
    /// Optimized for gaming: Low latency, responsiveness, low jitter
    Gaming,
    /// Real-time performance: Ultra-precise, consistent, micro-latency focused
    RealTime,
    /// Workstation: Balanced, thermal efficient, sustainable load handling
    Workstation,
    /// High-throughput: Optimized for syscall performance and parallelism
    Throughput,
    /// Balanced: All-around versatile, no dominant weakness
    Balanced,
    /// Server: Optimized for stability, task agility, and efficiency
    Server,
}

impl fmt::Display for PersonalityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersonalityType::Gaming => write!(f, "Gaming"),
            PersonalityType::RealTime => write!(f, "Real-Time"),
            PersonalityType::Workstation => write!(f, "Workstation"),
            PersonalityType::Throughput => write!(f, "Throughput"),
            PersonalityType::Balanced => write!(f, "Balanced"),
            PersonalityType::Server => write!(f, "Server"),
        }
    }
}

impl PersonalityType {
    /// Get a brief description of this personality
    pub fn description(&self) -> &'static str {
        match self {
            PersonalityType::Gaming => {
                "Optimized for fast, responsive gaming and interactive workloads"
            }
            PersonalityType::RealTime => {
                "Ultra-precise real-time performance with micro-latency focus"
            }
            PersonalityType::Workstation => {
                "Balanced performance suitable for creative and development work"
            }
            PersonalityType::Throughput => {
                "Optimized for high syscall throughput and parallel workloads"
            }
            PersonalityType::Balanced => {
                "Versatile all-around performance with no dominant weakness"
            }
            PersonalityType::Server => {
                "Optimized for stable task scheduling and long-term efficiency"
            }
        }
    }

    /// Get a short symbol/emoji representation
    pub fn symbol(&self) -> &'static str {
        match self {
            PersonalityType::Gaming => "🎮",
            PersonalityType::RealTime => "⚡",
            PersonalityType::Workstation => "💼",
            PersonalityType::Throughput => "🚀",
            PersonalityType::Balanced => "⚖️",
            PersonalityType::Server => "🖥️",
        }
    }
}

/// The GOAT Score and Personality Analysis Result
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScoringResult {
    /// GOAT Score (0-1000): Weighted aggregate of 7 metrics
    pub goat_score: u16,
    /// Primary personality type
    pub personality: PersonalityType,
    /// Primary strength metric
    pub primary_strength: String,
    /// Secondary strength metric
    pub secondary_strength: String,
    /// Areas for improvement
    pub improvement_area: String,
    /// Human-readable brief (2-3 sentences)
    pub brief: String,
    /// Balanced Override flag: true if metrics are well-rounded
    pub is_balanced_override: bool,
    /// Percentage above average (0-100 range)
    pub specialization_index: f32,
}

/// Performance Scorer: Transforms raw metrics into GOAT Score and Personality
pub struct PerformanceScorer {
    /// Reference benchmarks for normalization (best-case values)
    pub reference_benchmarks: ReferenceBenchmarks,
}

/// Reference benchmarks for metric normalization
#[derive(Clone, Debug)]
pub struct ReferenceBenchmarks {
    /// Best (lowest) P99 latency in microseconds
    pub p99_latency_us: f32,
    /// Best (lowest) P99.9 latency in microseconds
    pub p99_9_latency_us: f32,
    /// Best (lowest) micro-jitter P99.99 in microseconds
    pub micro_jitter_p99_99_us: f32,
    /// Best (lowest) context-switch RTT in microseconds
    pub context_switch_rtt_us: f32,
    /// Best (highest) syscall throughput in calls/sec
    pub syscall_throughput_per_sec: f32,
    /// Best (lowest) task wakeup latency in microseconds
    pub task_wakeup_latency_us: f32,
    /// Maximum acceptable core temperature in Celsius
    pub max_core_temp_c: f32,
    /// Cold temperature baseline in Celsius (for efficiency scaling)
    pub cold_temp_c: f32,
}

impl Default for ReferenceBenchmarks {
    fn default() -> Self {
        ReferenceBenchmarks {
            p99_latency_us: 50.0,                    // 50µs P99 baseline (very responsive)
            p99_9_latency_us: 100.0,                 // 100µs P99.9 baseline
            micro_jitter_p99_99_us: 1000.0, // 1000µs (1ms) jitter baseline (Excellent threshold, aligned with 0-10ms gauge range)
            context_switch_rtt_us: 150.0,   // 150µs context-switch baseline
            syscall_throughput_per_sec: 1_000_000.0, // 1M syscalls/sec baseline
            task_wakeup_latency_us: 100.0,  // 100µs task wakeup baseline
            max_core_temp_c: 80.0,          // 80°C as max acceptable
            cold_temp_c: 40.0,              // 40°C as thermal baseline
        }
    }
}

impl PerformanceScorer {
    /// Create a new scorer with default reference benchmarks
    pub fn new() -> Self {
        PerformanceScorer {
            reference_benchmarks: ReferenceBenchmarks::default(),
        }
    }

    /// Create a new scorer with custom reference benchmarks
    pub fn with_references(references: ReferenceBenchmarks) -> Self {
        PerformanceScorer {
            reference_benchmarks: references,
        }
    }

    /// Score a PerformanceMetrics instance (Phase 2 collector output)
    ///
    /// TRUSTWORTHY CALIBRATION: Applies noise floor fairness offset
    /// If raw latency is below detected hardware noise floor, prevents critical penalization
    pub fn score_metrics(&self, metrics: &PerformanceMetrics) -> ScoringResult {
        // Build synthetic BenchmarkMetrics if missing
        let benchmark_metrics = metrics.benchmark_metrics.clone().unwrap_or_default();
        self.score_benchmark_metrics(&benchmark_metrics, metrics)
    }

    /// Score BenchmarkMetrics directly using 7-metric Performance Spectrum
    ///
    /// TRUSTWORTHY CALIBRATION: Applies noise floor fairness to latency scoring
    /// If latency is detected to be within hardware noise floor, applies fairness offset
    pub fn score_benchmark_metrics(
        &self,
        benchmark: &BenchmarkMetrics,
        raw_metrics: &PerformanceMetrics,
    ) -> ScoringResult {
        // Normalize 7 metrics to 0-100 scale
        // FAIRNESS: Apply noise floor offset if latency is below detected hardware noise
        let latency_score = self
            .normalize_responsiveness_with_fairness(raw_metrics.p99_us, raw_metrics.noise_floor_us);
        let consistency_score =
            self.normalize_consistency_cv(raw_metrics.p99_us, raw_metrics.rolling_consistency_us);
        let jitter_score =
            self.normalize_micro_precision(benchmark.micro_jitter.as_ref().map(|m| m.p99_99_us));
        let throughput_score = self.normalize_syscall_performance(
            benchmark
                .syscall_saturation
                .as_ref()
                .map(|m| m.calls_per_second),
        );
        let efficiency_score = self.normalize_context_efficiency(
            benchmark.context_switch_rtt.as_ref().map(|m| m.avg_rtt_us),
        );
        let thermal_score = self.normalize_thermal_efficiency(&raw_metrics.core_temperatures);
        let smi_score = self.normalize_smi_resistance(
            raw_metrics.total_spikes,
            raw_metrics.spikes_correlated_to_smi,
        );

        // Calculate GOAT Score using 7-metric weights
        // Latency (27%), Consistency (18%), Jitter (15%), Throughput (10%),
        // Efficiency (10%), Thermal (10%), SMI Res (10%)
        let weighted_score = (latency_score * 0.27)
            + (consistency_score * 0.18)
            + (jitter_score * 0.15)
            + (throughput_score * 0.10)
            + (efficiency_score * 0.10)
            + (thermal_score * 0.10)
            + (smi_score * 0.10);

        // weighted_score is already on 0-100 scale, multiply by 10.0 to get 0-1000
        let goat_score = ((weighted_score * 10.0).min(1000.0)) as u16;

        // Determine personality based on dominant metrics
        let metrics_vec = vec![
            ("Latency", latency_score),
            ("Consistency", consistency_score),
            ("Jitter", jitter_score),
            ("Throughput", throughput_score),
            ("Efficiency", efficiency_score),
            ("Thermal", thermal_score),
            ("SMI-Resilience", smi_score),
        ];

        let (primary_name, primary_score) = metrics_vec
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let mut sorted_metrics = metrics_vec.clone();
        sorted_metrics.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (secondary_name, secondary_score) = sorted_metrics
            .get(1)
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let (improvement_name, improvement_score) = sorted_metrics
            .last()
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let primary_strength = format!("{}: {:.1}/100", primary_name, primary_score);
        let secondary_strength = format!("{}: {:.1}/100", secondary_name, secondary_score);
        let improvement_area = format!("{}: {:.1}/100", improvement_name, improvement_score);

        // Determine personality type
        // weighted_score is already on 0-100 scale (weighted average of normalized metrics)
        let avg_score = weighted_score;
        let is_balanced = (primary_score - avg_score).abs() < 10.0;
        let personality = if is_balanced {
            PersonalityType::Balanced
        } else {
            self.classify_personality_from_metrics(primary_name)
        };

        // Generate brief
        let brief = self.generate_brief_from_metrics(&personality, primary_name, goat_score);

        // Calculate specialization index
        let deviation = (primary_score - avg_score).max(0.0);
        let specialization_index = (deviation / avg_score * 100.0).clamp(0.0, 100.0);

        ScoringResult {
            goat_score,
            personality,
            primary_strength,
            secondary_strength,
            improvement_area,
            brief,
            is_balanced_override: is_balanced,
            specialization_index,
        }
    }

    /// Normalize P99 latency with Trustworthy Calibration fairness offset
    ///
    /// TRUSTWORTHY CALIBRATION: If raw latency is below detected hardware noise floor,
    /// prevents critical red-zone penalization. Hardware noise (SMIs) should not impact score.
    ///
    /// **Fairness Logic**:
    /// - If latency <= noise_floor_us: Apply fairness by boosting to neutral (50.0)
    /// - If latency > noise_floor_us: Use standard responsiveness normalization
    ///
    /// This ensures hardware noise doesn't unfairly penalize kernel performance.
    pub fn normalize_responsiveness_with_fairness(&self, p99_us: f32, noise_floor_us: f32) -> f32 {
        // FAIRNESS: If detected latency is within hardware noise floor, apply fairness offset
        if noise_floor_us > 0.0 && p99_us <= noise_floor_us {
            eprintln!("[SCORING] [FAIRNESS] Latency {:.1}µs <= noise_floor {:.1}µs: Applying fairness offset (neutral 50.0 instead of critical red)",
                p99_us, noise_floor_us);
            return 50.0; // Return neutral score instead of critical red
        }

        // Otherwise, use standard normalization
        self.normalize_responsiveness(p99_us)
    }

    /// Normalize P99 latency to 0-100 responsiveness score
    ///
    /// **3-Segment Piecewise Linear Normalization (0-10,000µs range)**:
    /// - 0-100µs: 100.0 down to 60.0 (sub-microsecond precision region)
    /// - 100-1000µs: 60.0 down to 40.0 (microsecond region)
    /// - 1000-10000µs: 40.0 down to 0.0 (millisecond region)
    ///
    /// This multi-scale approach preserves sub-microsecond visibility while
    /// accommodating the full 0-10ms range without clamping.
    /// Values above 10000µs (10ms) are clamped to 0.0.
    pub fn normalize_responsiveness(&self, p99_us: f32) -> f32 {
        let clamped = p99_us.max(0.0).min(10000.0);

        if clamped <= 100.0 {
            // 0-100µs: 100.0 down to 60.0 (high-precision sub-µs region)
            100.0 - ((clamped / 100.0) * 40.0)
        } else if clamped <= 1000.0 {
            // 100-1000µs: 60.0 down to 40.0 (microsecond region)
            let progress = (clamped - 100.0) / 900.0;
            60.0 - (progress * 20.0)
        } else {
            // 1000-10000µs: 40.0 down to 0.0 (millisecond region)
            let progress = (clamped - 1000.0) / 9000.0;
            40.0 - (progress * 40.0)
        }
        .max(0.0)
    }

    /// Normalize Consistency using Coefficient of Variation (CV)
    ///
    /// **Laboratory Grade Calibration**:
    /// - CV <= 5% (std_dev / mean <= 0.05): 100 score (Perfect Laboratory Grade)
    /// - CV >= 30% (std_dev / mean >= 0.30): 0 score (Poor Frame Pacing)
    ///
    /// **Linear Formula**: `100.0 * (1.0 - (CV - 0.05) / 0.25).clamp(0.001, 1.0)`
    ///
    /// This measures relative scheduling purity independent of baseline latency.
    /// High CV indicates frame pacing / micro-stutter issues even with low average latency.
    pub fn normalize_consistency_cv(&self, mean_latency_us: f32, std_dev_us: f32) -> f32 {
        if mean_latency_us <= 0.0 || std_dev_us < 0.0 {
            return 50.0; // Default to middle if data invalid
        }

        // Calculate Coefficient of Variation
        let cv = std_dev_us / mean_latency_us;

        // Linear normalization: 1.0 - (CV - 0.05) / 0.25, clamped to [0.001, 1.0]
        let normalized = (1.0 - ((cv - 0.05) / 0.25)).max(0.001).min(1.0);

        // Convert to 0-100 scale
        normalized * 100.0
    }

    /// Normalize P99.9 latency to 0-100 consistency score (legacy method)
    ///
    /// **Deprecated**: Use `normalize_consistency_cv()` instead for Laboratory Grade calibration.
    /// Kept for backward compatibility with existing scoring logic.
    pub fn normalize_consistency(&self, p99_9_us: f32) -> f32 {
        self.normalize_lower_is_better(p99_9_us, self.reference_benchmarks.p99_9_latency_us, 1000.0)
    }

    /// Normalize absolute peak jitter to 0-100 micro-precision score
    ///
    /// **Aligned Jitter Normalization (0-10,000µs range)**:
    /// - Green: 0-1,000µs (100 score) - Excellent scheduling
    /// - Yellow: 1,000-5,000µs (50 score) - Good scheduling
    /// - Red: 5,000-10,000µs (0 score) - Poor scheduling
    ///
    /// Piecewise linear: Converts 0-10,000µs range to 100-0 score
    fn normalize_micro_precision(&self, p99_99_us: Option<f32>) -> f32 {
        match p99_99_us {
            Some(val) => {
                // Align with 0-10,000µs gauge range
                let clamped = val.min(10000.0);
                if clamped <= 1000.0 {
                    // 0-1000µs: 100-60 score (excellent region)
                    100.0 - ((clamped / 1000.0) * 40.0)
                } else if clamped <= 5000.0 {
                    // 1000-5000µs: 60-20 score (good region)
                    60.0 - (((clamped - 1000.0) / 4000.0) * 40.0)
                } else {
                    // 5000-10000µs: 20-0 score (poor region)
                    20.0 - (((clamped - 5000.0) / 5000.0) * 20.0)
                }
            }
            None => 50.0, // Default if not measured
        }
    }

    /// Normalize context-switch RTT to 0-100 context efficiency score
    ///
    /// **RECALIBRATED LABORATORY-GRADE NORMALIZATION (0.5µs - 50µs range)**:
    /// Expanded range reflects real-world hardware diversity and typical performance:
    /// - 0.5µs: 100 pts (Perfect - ultra-low kernel overhead)
    /// - 5.0µs: 80 pts (Excellent - typical high-performance tuned kernel)
    /// - 15.0µs: 50 pts (Good/Baseline - reasonable production kernel, typical real-world)
    /// - 30.0µs: 20 pts (Acceptable - marginal but workable overhead)
    /// - 50.0µs: 0 pts (Floor - unacceptable overhead)
    /// At 8.0µs: score ≈ 70pts (Good) - consistent with real-world clean systems
    ///
    /// Piecewise linear: Representative of real-world cross-core RTT measurements
    /// Uses Median (not P99) for statistically representative scoring
    fn normalize_context_efficiency(&self, rtt_us: Option<f32>) -> f32 {
        match rtt_us {
            Some(val) => {
                // EXPANDED RANGE: 0.5µs (Perfect) to 50µs (Floor)
                let clamped = val.max(0.5).min(50.0);

                if clamped <= 0.5 {
                    // Perfect ultra-low overhead: 0.5µs = 100 score
                    100.0
                } else if clamped <= 5.0 {
                    // Excellent region: 0.5-5µs maps to 100-80 score
                    let progress = (clamped - 0.5) / 4.5;
                    100.0 - (progress * 20.0)
                } else if clamped <= 15.0 {
                    // Good/Baseline region: 5-15µs maps to 80-50 score
                    // At 8.0µs: progress = (8.0-5.0)/10.0 = 0.3, score = 80 - (0.3*30) = 71pts
                    let progress = (clamped - 5.0) / 10.0;
                    80.0 - (progress * 30.0)
                } else if clamped <= 30.0 {
                    // Acceptable region: 15-30µs maps to 50-20 score
                    let progress = (clamped - 15.0) / 15.0;
                    50.0 - (progress * 30.0)
                } else {
                    // Poor region: 30-50µs maps to 20-0 score
                    let progress = (clamped - 30.0) / 20.0;
                    20.0 - (progress * 20.0)
                }
            }
            None => 50.0,
        }
    }

    /// Normalize syscall throughput to 0-100 performance score
    fn normalize_syscall_performance(&self, throughput: Option<u64>) -> f32 {
        match throughput {
            Some(val) => {
                let val_f32 = val as f32;
                self.normalize_higher_is_better(
                    val_f32,
                    self.reference_benchmarks.syscall_throughput_per_sec,
                    100_000.0, // Worst case: 100k/sec
                )
            }
            None => 50.0,
        }
    }

    /// Normalize task wakeup latency to 0-100 task agility score
    fn normalize_task_agility(&self, latency_us: Option<f32>) -> f32 {
        match latency_us {
            Some(val) => self.normalize_lower_is_better(
                val,
                self.reference_benchmarks.task_wakeup_latency_us,
                500.0,
            ),
            None => 50.0,
        }
    }

    /// Normalize thermal data to 0-100 thermal efficiency score
    ///
    /// **Tiered Thermal Normalization (0-85°C+ range)**:
    /// - 0.0-60.0°C: 100.0 score (Green Zone - optimal)
    /// - 60.0-85.0°C: Linear drop 100.0 -> 50.0 (Yellow Zone - acceptable)
    /// - Above 85.0°C: Rapid drop 50.0 -> 0.0 (Red Zone - critical)
    ///
    /// Piecewise linear: Generous green zone until 60°C, then accelerating penalty above 85°C
    pub fn normalize_thermal_efficiency(&self, core_temps: &[f32]) -> f32 {
        if core_temps.is_empty() {
            return 50.0; // Default if no temperature data
        }

        let max_temp = core_temps.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Score based on max temperature (lower is better)
        // Tiered piecewise linear normalization
        if max_temp <= 60.0 {
            100.0 // Green zone: 0-60°C (optimal operating range)
        } else if max_temp <= 85.0 {
            // Yellow zone: 60-85°C maps to 100-50 score (linear drop)
            100.0 - (((max_temp - 60.0) / 25.0) * 50.0)
        } else {
            // Red zone: Above 85°C maps to 50-0 score (rapid drop)
            let progress = ((max_temp - 85.0) / 15.0).min(1.0);
            50.0 - (progress * 50.0)
        }
    }

    /// Normalize SMI spike correlation to 0-100 resistance score
    pub fn normalize_smi_resistance(&self, total_spikes: u64, smi_correlated: u64) -> f32 {
        if total_spikes == 0 {
            return 100.0; // No spikes = perfect resistance
        }

        let smi_ratio = (smi_correlated as f32) / (total_spikes as f32);
        // Lower SMI correlation is better
        100.0 * (1.0 - smi_ratio.min(1.0))
    }

    /// Helper: Normalize where lower values are better (latencies, etc.)
    fn normalize_lower_is_better(&self, actual: f32, reference: f32, worst_case: f32) -> f32 {
        // Defensive: Handle NaN and Infinity
        if !actual.is_finite() {
            if actual.is_nan() {
                return 50.0; // NaN → default to middle score
            } else if actual.is_infinite() && actual.is_sign_positive() {
                return 0.0; // +Infinity → worst case
            } else {
                return 100.0; // -Infinity → best case
            }
        }

        if actual <= reference {
            100.0 // Better than reference
        } else if actual >= worst_case {
            0.0 // Worse than worst case
        } else {
            let range = worst_case - reference;
            let position = actual - reference;
            100.0 - ((position / range) * 100.0)
        }
    }

    /// Helper: Normalize where higher values are better (throughput, etc.)
    fn normalize_higher_is_better(&self, actual: f32, reference: f32, worst_case: f32) -> f32 {
        // Defensive: Handle NaN and Infinity
        if !actual.is_finite() {
            if actual.is_nan() {
                return 50.0; // NaN → default to middle score
            } else if actual.is_infinite() && actual.is_sign_positive() {
                return 100.0; // +Infinity → best case (infinite throughput)
            } else {
                return 0.0; // -Infinity → worst case (negative throughput)
            }
        }

        if actual >= reference {
            100.0 // Better than reference
        } else if actual <= worst_case {
            0.0 // Worse than worst case
        } else {
            let range = reference - worst_case;
            let position = actual - worst_case;
            (position / range) * 100.0
        }
    }

    /// Classify personality from dominant metric
    fn classify_personality_from_metrics(&self, primary_metric: &str) -> PersonalityType {
        match primary_metric {
            "Latency" => PersonalityType::Gaming,
            "Jitter" => PersonalityType::RealTime,
            "Throughput" => PersonalityType::Throughput,
            "Thermal" => PersonalityType::Workstation,
            "Efficiency" => PersonalityType::Server,
            _ => PersonalityType::Balanced,
        }
    }

    /// Generate brief from 7-metric spectrum
    fn generate_brief_from_metrics(
        &self,
        personality: &PersonalityType,
        primary_metric: &str,
        goat_score: u16,
    ) -> String {
        let score_descriptor = match goat_score {
            850..=1000 => "exceptional",
            750..=849 => "outstanding",
            650..=749 => "excellent",
            550..=649 => "very good",
            450..=549 => "good",
            350..=449 => "solid",
            250..=349 => "fair",
            _ => "needs improvement",
        };

        format!(
            "{} personality profile ({}) delivers {} performance overall. Strongest in {}.",
            personality.symbol(),
            personality,
            score_descriptor,
            primary_metric
        )
    }
}

impl Default for PerformanceScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_personality_display() {
        assert_eq!(PersonalityType::Gaming.to_string(), "Gaming");
        assert_eq!(PersonalityType::RealTime.to_string(), "Real-Time");
        assert_eq!(PersonalityType::Balanced.to_string(), "Balanced");
    }

    #[test]
    fn test_personality_symbol() {
        assert_eq!(PersonalityType::Gaming.symbol(), "🎮");
        assert_eq!(PersonalityType::RealTime.symbol(), "⚡");
        assert_eq!(PersonalityType::Balanced.symbol(), "⚖️");
    }

    #[test]
    fn test_scorer_creation() {
        let scorer = PerformanceScorer::new();
        assert_eq!(scorer.reference_benchmarks.p99_latency_us, 50.0);
    }

    #[test]
    fn test_normalize_responsiveness() {
        let scorer = PerformanceScorer::new();

        // Segment 1: 0-100µs
        // At 0µs (best case): 100 points
        let score = scorer.normalize_responsiveness(0.0);
        assert_eq!(score, 100.0);

        // At 50µs (mid-segment 1): 80 points
        let score = scorer.normalize_responsiveness(50.0);
        assert_eq!(score, 80.0);

        // At 100µs (boundary): 60 points
        let score = scorer.normalize_responsiveness(100.0);
        assert_eq!(score, 60.0);

        // Segment 2: 100-1000µs
        // At 550µs (mid-segment 2): 50 points (60 - (450/900)*20 = 60 - 10 = 50)
        let score = scorer.normalize_responsiveness(550.0);
        assert!(score > 49.0 && score < 51.0); // Allow small floating point error

        // At 1000µs (boundary): 40 points
        let score = scorer.normalize_responsiveness(1000.0);
        assert_eq!(score, 40.0);

        // Segment 3: 1000-10000µs
        // At 5500µs (mid-segment 3): 20 points (40 - (4500/9000)*40 = 40 - 20 = 20)
        let score = scorer.normalize_responsiveness(5500.0);
        assert_eq!(score, 20.0);

        // At 10000µs (worst case): 0 points
        let score = scorer.normalize_responsiveness(10000.0);
        assert_eq!(score, 0.0);

        // Above ceiling: 0 points
        let score = scorer.normalize_responsiveness(15000.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_thermal_efficiency_normalization() {
        let scorer = PerformanceScorer::new();

        // Cold temperature
        let cold_temps = vec![30.0, 35.0];
        let score = scorer.normalize_thermal_efficiency(&cold_temps);
        assert_eq!(score, 100.0);

        // Warm temperature
        let warm_temps = vec![60.0, 65.0];
        let score = scorer.normalize_thermal_efficiency(&warm_temps);
        assert!(score > 10.0 && score < 100.0);

        // Very hot (throttling)
        let hot_temps = vec![90.0, 95.0];
        let score = scorer.normalize_thermal_efficiency(&hot_temps);
        assert!(score < 20.0);
    }

    #[test]
    fn test_smi_resistance() {
        let scorer = PerformanceScorer::new();

        // No spikes = perfect
        let score = scorer.normalize_smi_resistance(0, 0);
        assert_eq!(score, 100.0);

        // 50% SMI-correlated
        let score = scorer.normalize_smi_resistance(100, 50);
        assert_eq!(score, 50.0);
    }
}

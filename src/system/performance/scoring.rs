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

use serde::{Deserialize, Serialize};
use std::fmt;
use crate::system::performance::{BenchmarkMetrics, PerformanceMetrics};


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
            PersonalityType::Gaming => "Optimized for fast, responsive gaming and interactive workloads",
            PersonalityType::RealTime => "Ultra-precise real-time performance with micro-latency focus",
            PersonalityType::Workstation => "Balanced performance suitable for creative and development work",
            PersonalityType::Throughput => "Optimized for high syscall throughput and parallel workloads",
            PersonalityType::Balanced => "Versatile all-around performance with no dominant weakness",
            PersonalityType::Server => "Optimized for stable task scheduling and long-term efficiency",
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
            p99_latency_us: 50.0,           // 50µs P99 baseline (very responsive)
            p99_9_latency_us: 100.0,        // 100µs P99.9 baseline
            micro_jitter_p99_99_us: 200.0,  // 200µs P99.99 baseline (ultra-precise)
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
    pub fn score_metrics(&self, metrics: &PerformanceMetrics) -> ScoringResult {
        // Build synthetic BenchmarkMetrics if missing
        let benchmark_metrics = metrics.benchmark_metrics.clone().unwrap_or_default();
        self.score_benchmark_metrics(&benchmark_metrics, metrics)
    }

    /// Score BenchmarkMetrics directly using 7-metric Performance Spectrum
    pub fn score_benchmark_metrics(
        &self,
        benchmark: &BenchmarkMetrics,
        raw_metrics: &PerformanceMetrics,
    ) -> ScoringResult {
        // Normalize 7 metrics to 0-100 scale
        let latency_score = self.normalize_responsiveness(raw_metrics.p99_us);
        let consistency_score = self.normalize_consistency_cv(
            raw_metrics.p99_us,
            raw_metrics.rolling_consistency_us,
        );
        let jitter_score = self.normalize_micro_precision(
            benchmark.micro_jitter.as_ref().map(|m| m.p99_99_us),
        );
        let throughput_score = self.normalize_syscall_performance(
            benchmark.syscall_saturation.as_ref().map(|m| m.calls_per_second),
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
        let weighted_score =
            (latency_score * 0.27) +
            (consistency_score * 0.18) +
            (jitter_score * 0.15) +
            (throughput_score * 0.10) +
            (efficiency_score * 0.10) +
            (thermal_score * 0.10) +
            (smi_score * 0.10);
        
        let goat_score = ((weighted_score * 1000.0).min(1000.0)) as u16;

        // Determine personality based on dominant metrics
        let metrics_vec = vec![
            ("Latency", latency_score),
            ("Consistency", consistency_score),
            ("Jitter", jitter_score),
            ("Throughput", throughput_score),
            ("Efficiency", efficiency_score),
            ("Thermal", thermal_score),
            ("SMI-Res", smi_score),
        ];
        
        let (primary_name, primary_score) = metrics_vec.iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let mut sorted_metrics = metrics_vec.clone();
        sorted_metrics.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let (secondary_name, secondary_score) = sorted_metrics.get(1)
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let (improvement_name, improvement_score) = sorted_metrics.last()
            .map(|(n, s)| (*n, *s))
            .unwrap_or(("Balanced", 50.0));

        let primary_strength = format!("{}: {:.1}/100", primary_name, primary_score);
        let secondary_strength = format!("{}: {:.1}/100", secondary_name, secondary_score);
        let improvement_area = format!("{}: {:.1}/100", improvement_name, improvement_score);

        // Determine personality type
        let avg_score = weighted_score;
        let is_balanced = (primary_score - avg_score / 10.0).abs() < 10.0;
        let personality = if is_balanced {
            PersonalityType::Balanced
        } else {
            self.classify_personality_from_metrics(primary_name)
        };

        // Generate brief
        let brief = self.generate_brief_from_metrics(&personality, primary_name, goat_score);

        // Calculate specialization index
        let deviation = (primary_score - avg_score / 10.0).max(0.0);
        let specialization_index = (deviation / (avg_score / 10.0) * 100.0).clamp(0.0, 100.0);

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

    /// Normalize P99 latency to 0-100 responsiveness score
    pub fn normalize_responsiveness(&self, p99_us: f32) -> f32 {
        self.normalize_lower_is_better(p99_us, self.reference_benchmarks.p99_latency_us, 500.0)
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

    /// Normalize P99.99 micro-jitter to 0-100 micro-precision score
    fn normalize_micro_precision(&self, p99_99_us: Option<f32>) -> f32 {
        match p99_99_us {
            Some(val) => {
                self.normalize_lower_is_better(val, self.reference_benchmarks.micro_jitter_p99_99_us, 1000.0)
            }
            None => 50.0, // Default if not measured
        }
    }

    /// Normalize context-switch RTT to 0-100 context efficiency score
    fn normalize_context_efficiency(&self, rtt_us: Option<f32>) -> f32 {
        match rtt_us {
            Some(val) => {
                self.normalize_lower_is_better(val, self.reference_benchmarks.context_switch_rtt_us, 500.0)
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
            Some(val) => {
                self.normalize_lower_is_better(val, self.reference_benchmarks.task_wakeup_latency_us, 500.0)
            }
            None => 50.0,
        }
    }

    /// Normalize thermal data to 0-100 thermal efficiency score
    pub fn normalize_thermal_efficiency(&self, core_temps: &[f32]) -> f32 {
        if core_temps.is_empty() {
            return 50.0; // Default if no temperature data
        }

        let max_temp = core_temps.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Score based on max temperature (lower is better)
        if max_temp <= self.reference_benchmarks.cold_temp_c {
            100.0 // Cold, very efficient
        } else if max_temp >= self.reference_benchmarks.max_core_temp_c {
            10.0 // Thermal throttling risk
        } else {
            let range = self.reference_benchmarks.max_core_temp_c - self.reference_benchmarks.cold_temp_c;
            let position = max_temp - self.reference_benchmarks.cold_temp_c;
            100.0 - ((position / range) * 90.0)
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
    fn generate_brief_from_metrics(&self, personality: &PersonalityType, primary_metric: &str, goat_score: u16) -> String {
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
        
        // At reference: 100 points
        let score = scorer.normalize_responsiveness(50.0);
        assert_eq!(score, 100.0);
        
        // Worse than reference but better than worst case
        let score = scorer.normalize_responsiveness(200.0);
        assert!(score > 0.0 && score < 100.0);
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

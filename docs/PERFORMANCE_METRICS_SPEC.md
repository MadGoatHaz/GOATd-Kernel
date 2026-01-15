# Performance Metrics Specification (The 7 Pillars)

This document defines the metrics used in the GOATd Kernel Performance Spectrum, explaining their significance, derivation, and optimal vs. poor thresholds.

## KernBench Professional Tiers (V2 Calibration)

The UI has been recalibrated to professional "KernBench" standards for critical latency detection:

### P99.9 Latency KPI Gauge (Micro-Stutter Detection)
- **Scale**: 0 - 200µs
- **Green Zone** (Good): < 20µs (Imperceptible microstutters)
- **Yellow Zone** (Warning): 20-100µs (Approaching jitter threshold)
- **Red Zone** (Bad): > 100µs (Micro-stutter visible to users)
- **Explanation**: P99.9 >100µs causes perceptible frame drops in gaming and audio. This threshold detects when 1 in 1000 samples exceeds the stutter boundary.

### Max Latency KPI Gauge (Peak Performance)
- **Scale**: 0 - 1000µs
- **Green Zone** (Good): < 50µs (Excellent responsiveness)
- **Yellow Zone** (Warning): 50-500µs (Acceptable but degraded)
- **Red Zone** (Bad): > 500µs (Poor performance tier)
- **Explanation**: Max latency >500µs indicates systemic performance degradation or interrupt contention.

### Consistency Metric (Laboratory Grade CV % Scale)
- **Unit**: Coefficient of Variation (CV %) - Standard Deviation expressed as % of mean
- **Scale**: 2% - 30% (Laboratory Grade to Poor)
- **Optimal**: < 2% CV (Laboratory Grade - imperceptible variance)
- **Poor**: ≥ 30% CV (Critical - frame pacing failure)
- **Explanation**: Measures relative scheduling stability independent of baseline latency. Laboratory Grade (< 2% CV) represents "silent" kernels with microsecond-level precision suitable for gaming and audio. Display shows only standard deviation in µs for clarity.
- **Calculation**: `CV = (σ / μ) * 100` from rolling 1000-sample window, then normalized via Laboratory Grade formula


## Rolling 1000-Sample P99 Methodology

To allow performance scores to recover while maintaining diagnostic accuracy, all core metrics (Latency, Throughput, Efficiency) now use a **rolling 1000-sample window** to calculate the **99th percentile (P99)**. This approach:

- **Dampens spike outliers**: A single spike no longer permanently caps the score
- **Preserves diagnostics**: P99 accurately reflects typical system behavior
- **Enables recovery**: Scores improve as performance improves over time
- **Maintains consistency**: Exactly 1000 samples ensures reproducible calculations

The P99 is calculated as the value at index 990 (99th percentile of 1000 samples) after sorting.

## 1. Latency (P99 Response Time) - High-End Rig Calibration
- **What**: The 99th percentile latency from a rolling 1000-sample window.
- **Why**: Indicates typical "worst-case" responsiveness without being stuck on permanent spikes.
- **How**: Latency samples collected continuously, window maintained via FIFO buffer, P99 extracted via sorting.
- **High-End Calibration** (9800X3D/265K rig standards):
    - **Optimal**: **10µs P99** (The "Holy Grail" for ultra-responsive systems)
    - **Poor**: **500µs P99** (Objectively High, indicating systemic degradation)
    - **Normalization**: `(1.0 - (rolling_p99 - 10.0) / 490.0).clamp(0.001, 1.0)`
- **Unit**: Microseconds (µs)

## 2. Throughput (Syscalls Saturation)
- **What**: The P99 number of system calls executed per second from a rolling 1000-sample window.
- **Why**: Measures the typical execution capacity without being affected by temporary slow operations.
- **How**: Syscall throughput samples aggregated every measurement interval, P99 extracted from rolling window.
- **Thresholds**:
    - **Optimal**: 1.0M+ Ops/s P99
    - **Poor**: < 100k Ops/s P99
- **Unit**: Operations per second (Ops/s or k/s)

## 3. Jitter (Clamped Relative Jitter - Purity of Scheduling)
- **What**: The relative variability of latency samples, expressed as a normalized coefficient of variation (std_dev / mean).
- **Why**: Measures the predictability and "purity" of scheduling independent of baseline latency. Critical for audio, gaming, and real-time tasks.
- **How**: Calculated from the last 100 latency samples using the clamped relative jitter formula.
- **Clamped Relative Jitter Formula**:
    ```
    Scoring = (1.0 - ((std_dev / mean) - 0.05) / 0.25).clamp(0.001, 1.0)
    ```
- **Calibration** (Normalized Benchmark standard):
    - **Optimal**: 5% Relative Noise (1.0 score) - Perfect scheduling purity
    - **Poor**: 30% Relative Noise (0.001 score) - Highly inconsistent scheduling
- **Why Relative?**: Accounts for baseline latency variance across different hardware (low-latency systems naturally have lower noise, but may have higher % variation)
- **Display**: Raw standard deviation in microseconds (e.g., "12.5µs")
- **Unit**: Percentage Relative Noise (5-30%), displayed as Microseconds (µs)

## 4. CPU Efficiency (Context-Switch Overhead)
- **What**: The P99 overhead time for a thread context switch from a rolling 1000-sample window.
- **Why**: Indicates the typical scheduler overhead. Lower overhead means more CPU cycles go to actual work.
- **How**: Context-switch overhead measured by a pipe-based benchmark, P99 extracted from rolling window.
- **Thresholds**:
    - **Optimal**: < 1.0µs overhead
    - **Poor**: > 100.0µs overhead
- **Unit**: Microseconds (µs)

## 5. Thermal (Heat Stability)
- **What**: Average core temperature across all logical processors.
- **Why**: High heat leads to thermal throttling, which causes unpredictable performance drops.
- **How**: Read from `/sys/class/thermal` or `hwmon`.
- **Thresholds**:
    - **Optimal**: < 40°C
    - **Poor**: > 90°C
- **Unit**: Degrees Celsius (°C)

## 6. Consistency (Coefficient of Variation - Laboratory Grade CV %)
- **What**: The Coefficient of Variation (CV%) = (Standard Deviation / Mean) * 100% from the rolling 1000-sample latency window.
- **Why**: Measures *relative* scheduling purity independent of baseline latency. High CV indicates variability proportional to the latency itself, which causes perceptible micro-stutter and frame pacing issues.
- **How**:
    ```
    CV% = (σ / μ) * 100
    Score = (1.0 - (CV - 0.05) / 0.25).clamp(0.001, 1.0)
    ```
    Calculated from rolling 1000-sample window using mean (μ) and standard deviation (σ).

### Consistency Rating Scale (CV %)

| CV % Range | Grade | Score | Interpretation | Frame Pacing Impact |
|-----------|-------|-------|-----------------|-------------------|
| **< 2%** | **Laboratory Grade** | **1.0** | Silent kernels; microsecond-level precision | Zero perceptible stutter; imperceptible variance |
| 2% - 7% | Optimal | 0.8-1.0 | Excellent scheduling stability | Imperceptible micro-stutter |
| 7% - 15% | Good/Stable | 0.4-0.8 | Acceptable for most workloads | Minimal perceptible latency variance |
| 15% - 30% | Moderate/Noisy | 0.001-0.4 | Scheduling inconsistency visible | Noticeable micro-stutter in gaming/audio |
| **≥ 30%** | **Poor/Critical** | **0.001** | High variance; frame pacing failure | Severe frame drops; visible stuttering |

- **Professional Calibration** (Frame Pacing / Statistical Process Control):
    - **Laboratory Grade Floor**: 2% CV (< 0.02) → Identifies "Silent" kernels with imperceptible variance
    - **Optimal Threshold**: 5% CV (0.05) → Gold Standard (1.0 score)
    - **Danger Zone**: 30% CV (0.30) → Poor score (0.001); Frame pacing/micro-stutter begins
    - **Normalized Score Linear Formula**: `1.0 - ((CV - 0.05) / 0.25)` clamped to [0.001, 1.0]

- **Display Unit**: Millisecond (µs) - shown as "X.Xµs" in Performance Spectrum (standard deviation σ, not CV%)
- **Critical Insight**: A "fast" 10µs-average latency with 50% CV (5µs variability) will score 0.001 (Poor) and cause frame pacing issues, even though the absolute value is low. Conversely, a 50µs latency with 4% CV will score 1.0 (Laboratory Grade) due to exceptional stability.
- **Note**: This is complementary to Jitter (Metric 3), which measures *relative* scheduling purity from recent samples. Consistency measures long-term stability via rolling 1000-sample window.

### Why High CV (> 30%) Causes Frame Pacing Problems

When Coefficient of Variation exceeds 30%, the scheduler exhibits erratic behavior:

1. **Scheduler Fairness Breakdown**: Task wakeup times vary wildly, causing some threads to be delayed while others race. This creates visible timing artifacts.
2. **GPU/Display Pipeline Misalignment**: In gaming (60 FPS = 16.67ms frames), variance of ±5µs is imperceptible, but ±500µs variance (5% CV at 10µs baseline) causes frame drops and micro-stutter.
3. **Real-Time Task Degradation**: Audio streaming (1ms latency tolerance), MIDI (5ms tolerance), and interactive input require CV < 15% to stay reliable.
4. **Thermal/Frequency Throttling Signal**: High CV often indicates CPU frequency scaling, thermal throttling, or interrupt contention—all recoverable but require tuning.

**Formula Interpretation**:
- At CV = 5%: Score = 1.0 (Optimal).
- At CV = 15%: Score = (1.0 - (0.15 - 0.05) / 0.25) = (1.0 - 0.4) = 0.6 (Good).
- At CV = 30%: Score = (1.0 - (0.30 - 0.05) / 0.25) = (1.0 - 1.0) = 0.001 (Poor/critical failure).

## 7. SMI Resilience (Interrupt Mitigation)
- **What**: The ability of the system to avoid or mitigate System Management Interrupts (SMIs).
- **Why**: SMIs are invisible to the OS and cause massive latency spikes.
- **How**: Correlating performance spikes with hardware SMI counters (e.g., MSR 0x34).
- **Thresholds**:
    - **Optimal**: 0 SMIs detected
    - **Poor**: 10+ SMIs detected
- **Unit**: Correlation Ratio or Count (Detected/Total)

## Jitter vs. Consistency Clarification

### Jitter (Metric 3) - Clamped Relative Jitter (Purity of Scheduling)

The **Jitter** metric is the **Clamped Relative Jitter** calculated from recent latency samples:

- **Calculation**: Relative standard deviation (std_dev / mean) of the most recent 100 latency samples
- **Formula**: `(1.0 - ((relative_jitter - 0.05) / 0.25)).clamp(0.001, 1.0)`
- **What it measures**: The *purity and predictability of the scheduler* independent of baseline latency
- **Interpretation**:
  - Low relative jitter (5% noise) = 1.0 score = Perfect scheduling purity (Holy Grail)
  - High relative jitter (30% noise) = 0.001 score = Poor, inconsistent scheduling
- **Use Case**: Critical for comparing scheduling consistency across different hardware tiers and workloads
- **Why relative?**: Normalizes across systems with different baseline latencies (10µs vs 100µs rigs)

### Consistency (Metric 5) - Absolute Standard Deviation (µs)

The **Consistency** metric measures **absolute variability** in microseconds:

- **Calculation**: Standard deviation of the rolling 1000-sample latency window
- **What it measures**: The *absolute* latency variability in real microseconds
- **Interpretation**:
  - Low standard deviation (< 5µs) = Highly consistent baseline
  - High standard deviation (> 50µs) = Variable baseline performance
- **Use Case**: Direct measure of how stable latencies are in real-world units

### Key Difference

| Metric | Jitter | Consistency |
|--------|--------|-------------|
| **Formula** | Relative (std_dev / mean) | Absolute (std_dev in µs) |
| **Measurement** | Purity of scheduling | Absolute latency stability |
| **Scale** | 5-30% relative noise | 5-50µs absolute noise |
| **Best for** | Cross-hardware comparison | Real-world performance |
| **Example** | 10µs baseline with 0.5µs SD = 5% (1.0 score) | Same system = 0.5µs consistency |

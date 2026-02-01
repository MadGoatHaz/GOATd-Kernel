# Performance Metrics Specification

The GOATd Kernel uses a 7-metric spectrum to evaluate kernel performance, providing a comprehensive "GOAT Score" (0-1000) and a personality categorization for each build. This specification aligns with the implementation in [`src/system/performance/scoring.rs`](src/system/performance/scoring.rs).

## 1. The 7-Metric Spectrum

Each metric is normalized against reference benchmarks to ensure consistent scoring across different hardware environments.

| Metric | Measurement | Reference Baseline | Description |
| :--- | :--- | :--- | :--- |
| **Responsiveness (P99)** | P99 Latency (µs) | 50.0 µs | Time to respond to user input/interrupts. |
| **Consistency (P99.9)** | P99.9 Latency (µs) | 100.0 µs | Tail latency stability under load. |
| **Micro-Jitter (P99.99)**| P99.99 Latency (µs) | 200.0 µs | Ultra-fine grain timing precision. |
| **Task Wakeup** | Wakeup Latency (µs) | 15.0 µs | Time for a task to begin execution after wakeup. |
| **Context Switching** | RTT (µs) | 2.5 µs | Round-trip time for thread context swaps. |
| **Throughput** | Syscalls/sec | System Dependent | Maximum system call execution rate. |
| **Efficiency** | Thermal Delta (°C) | Max Core Temp | Performance per thermal watt (thermal headroom). |

## 2. Trustworthy Calibration Logic

To maintain fairness and accuracy, the GOATd Kernel employs **Trustworthy Calibration**. This logic addresses the "Hardware Noise" problem, where System Management Interrupts (SMIs) or hardware-level background processes can artificially inflate latency measurements.

### Fairness Logic:
- **Noise Floor Detection:** The system detects the hardware's intrinsic noise floor.
- **Calibration Offset:** If raw latency is below the detected hardware noise floor, the system prevents critical "red-zone" penalization.
- **Fairness Application:** 
  - `latency <= noise_floor_us`: Boost to a neutral score (50.0).
  - `latency > noise_floor_us`: Standard normalization applied.

This ensures that hardware-level interference does not unfairly penalize a kernel's performance score.

## 3. Personality Profiles

Based on the 7-metric spectrum, the system assigns a `PersonalityType`:

- **Gaming:** Optimized for low latency, high responsiveness, and minimal jitter.
- **RealTime:** Ultra-precise, consistent, micro-latency focused.
- **Workstation:** Balanced, thermal efficient, sustainable load handling.
- **Throughput:** Optimized for syscall performance and high parallelism.
- **Balanced:** All-around versatile, no dominant weakness.

## 4. Solo-Developer Persona

As a solo developer, this specification provides a transparent, data-driven framework for evaluating kernel performance. Instead of relying on "feel," we use standardized metrics and trustworthy calibration to ensure every optimization is backed by verifiable performance gains.

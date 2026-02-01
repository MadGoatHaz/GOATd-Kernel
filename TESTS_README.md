# GOATd Kernel Test Suite

## Overview
This document provides a transparent and honest assessment of the current testing infrastructure for the GOATd Kernel project. Our testing strategy combines high-precision unit tests with complex integration scenarios to ensure system stability, performance reliability, and toolchain integrity.

## Testing Frameworks
The project leverages the Rust ecosystem's native testing capabilities augmented by industry-standard asynchronous runtimes:
- **Rust Native Testing:** Used for deterministic logic, configuration parsing, and utility functions.
- **Tokio Test Runtime:** Powering asynchronous integration tests, timeout diagnostics, and multi-threaded orchestration scenarios.
- **Mocking & Simulation:** Extensive use of mock environment variables and file system structures to simulate kernel build environments without requiring root privileges.

## Coverage Metrics & Module Status
Current coverage is categorized by functional domain, reflecting the project's evolution as of v0.2.1.

| Module | Status | Validation Depth |
| :--- | :--- | :--- |
| **Kernel Orchestrator** | ![Validated](https://img.shields.io/badge/Status-Validated-green) | High: Async phase transitions, timeouts, and state persistence. |
| **Toolchain (LLVM)** | ![Validated](https://img.shields.io/badge/Status-Validated-green) | High: Explicit verification of Clang/LLVM components and caching. |
| **Configuration** | ![Validated](https://img.shields.io/badge/Status-Validated-green) | High: Profile defaults, override logic, and whitelist/modprobed localmodconfig. |
| **UI Synchronization** | ![In Development](https://img.shields.io/badge/Status-In_Development-yellow) | Medium: State scaling and cross-thread event synchronization. |
| **Performance Metrics** | ![In Development](https://img.shields.io/badge/Status-In_Development-yellow) | Medium: Nanosecond precision validation and stressor isolation. |
| **Hardware Detection** | ![Stabilizing](https://img.shields.io/badge/Status-Stabilizing-blue) | Medium: CPU/GPU feature detection and boot parameter validation. |

## High-Value Integration Tests
We maintain several critical test suites that safeguard the "Golden Path" of kernel development:

### 1. Real Kernel Build Simulation
- **File:** `tests/real_kernel_build_integration.rs`
- **Focus:** Validates the entire build pipeline lifecycle, including log capture, timeout handling, and gaming profile application. It ensures the orchestrator can handle real-world build durations and failures.

### 2. LLVM Toolchain Verification
- **File:** `src/system/verification.rs`
- **Focus:** Implements the `verify_llvm_toolchain` suite which ensures all required LLVM/Clang binaries (ld.lld, llvm-ar, etc.) are present and functional before a build commences. Includes internal caching to prevent redundant execution.

### 3. Performance Battle Tests
- **File:** `tests/performance_battle_tests.rs`
- **Focus:** Stress-testing the performance data collector to ensure nanosecond precision and isolation from system jitter (e.g., SMI correlation reliability).

## Running the Suites

### Standard Validation
For general development verification:
```bash
cargo test
```

### Official Build Verification
To run the comprehensive verification script used in the release pipeline:
```bash
./scripts/verify.sh
```

## Continuous Improvement
The suite is under active expansion. Current priorities include:
- Enhancing UI scaling precision across varied DPPI environments.
- Expanding hardware detection coverage for legacy BIOS/MBR systems.
- Formalizing the performance baseline calibration for diverse kernel profiles.

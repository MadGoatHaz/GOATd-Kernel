# GOATd Kernel Test Suite

This document provides an inventory of the test infrastructure for the GOATd Kernel project, including unit tests, integration tests, and specialized performance benchmarks.

## Execution Instructions

To run the standard test suite:

```bash
cargo test
```

To run a specific test file:

```bash
cargo test --test <test_name>
```

To run performance benchmarks:

```bash
cargo run --bin efficiency_test
cargo run --bin latency_test
```

## Test Inventory

### Integration Tests (`tests/`)

These tests verify the interaction between different components of the system.

- **`chunk_3_header_discovery_sync.rs`**: Verifies header discovery and synchronization logic.
- **`chunk_4_alpm_hook_verification.rs`**: Validates ALPM hook installation and detection.
- **`comprehensive_feature_realization.rs`**: Top-level feature verification.
- **`config_tests.rs`**: Validates configuration management and persistence.
- **`dynamic_versioning_test.rs`**: Ensures kernel versioning logic is correct.
- **`forensic_diagnostic.rs`**: Tests system diagnostic and forensic capabilities.
- **`git_tests.rs`**: Verifies Git operations for kernel source management.
- **`hardware_tests.rs`**: Validates hardware detection (CPU, GPU, RAM, etc.).
- **`integration_tests.rs`**: General integration scenarios.
- **`lifecycle_pipe_integration.rs`**: Tests the end-to-end build lifecycle.
- **`logging_integration_test.rs`**: Verifies the logging system.
- **`modprobed_localmodconfig_validation.rs`**: Validates `modprobed-db` integration.
- **`mpl_integration_test.rs`**: Tests the Master Profile Layer logic.
- **`performance_baseline_calibration.rs`**: Calibrates performance metrics.
- **`performance_battle_tests.rs`**: Stress tests for the performance monitor.
- **`phase_1_infrastructure_test.rs`**: Validates core project infrastructure.
- **`phase_2_collector_test.rs`**: Tests data collection mechanisms.
- **`phase_3_scoring_audit.rs`**: Audits the scoring algorithm.
- **`profile_pipeline_validation.rs`**: Validates the profile application pipeline.
- **`real_kernel_build_integration.rs`**: Simulates/Verifies actual kernel build steps.
- **`ui_scaling_tests.rs`**: Verifies UI responsiveness and scaling.
- **`ui_sync_tests.rs`**: Ensures UI state remains synchronized with the backend.

### Unit Tests (Internal)

Located within the `src/` directory, these test individual modules in isolation.

- **`src/lib.rs`**: Core library exports and versioning.
- **`src/config/`**: Comprehensive tests for exclusions, whitelists, and profile finalization.
- **`src/hardware/`**: Unit tests for individual hardware component detection.
- **`src/kernel/patcher/tests.rs`**: Specialized tests for the kernel patching and rebranding engine.
- **`src/system/performance/`**: Extensive tests for benchmarks, stressors, and diagnostics.

### Performance Binaries (`src/bin/`)

- **`efficiency_test.rs`**: Measures system efficiency under load.
- **`latency_test.rs`**: Specialized latency measurement tool.

## Test Results

Last full suite run: **SYSTEM GREEN** (2026-02-01)
- **Unit Tests**: Passed (456 tests)
- **Integration Tests**: Passed (22 tests)
- **Doc Tests**: Passed (26 passed, 23 ignored)

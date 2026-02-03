# GOATd Kernel Test Suite

This document provides an inventory of the tests within the GOATd Kernel project and instructions for their execution.

## Test Inventory

The test suite is divided into unit tests (within `src/`) and integration tests (within `tests/`).

### Integration Tests (`master/tests/`)

*   **`chunk_3_header_discovery_sync.rs`**: Validates kernel header discovery and synchronization logic.
*   **`chunk_4_alpm_hook_verification.rs`**: Verifies ALPM hook structures and post-install system integrity checks.
*   **`comprehensive_feature_realization.rs`**: Ensures performance features are correctly applied across different profiles (Gaming, Server).
*   **`config_tests.rs`**: Validates configuration loading, saving, and validation logic.
*   **`dynamic_versioning_test.rs`**: Tests the dynamic versioning system for kernel builds.
*   **`forensic_diagnostic.rs`**: Tests raw latency data collection and forensic analysis capabilities.
*   **`git_tests.rs`**: Validates Git operations for kernel source management.
*   **`hardware_tests.rs`**: Verifies hardware detection logic (CPU, GPU, RAM, etc.).
*   **`integration_tests.rs`**: Comprehensive end-to-end testing of the orchestrator and build phases.
*   **`lifecycle_pipe_integration.rs`**: Tests the performance monitoring lifecycle state transitions.
*   **`logging_integration_test.rs`**: Verifies integration with the logging system.
*   **`logging_robustness_test.rs`**: Tests logging under stress and high-frequency events.
*   **`modprobed_localmodconfig_validation.rs`**: Validates `modprobed-db` integration and `localmodconfig` generation.
*   **`mpl_integration_test.rs`**: Tests the Monitor-Profile-Loop (MPL) architecture.
*   **`performance_baseline_calibration.rs`**: Calibrates the performance monitoring baseline overhead.
*   **`performance_battle_tests.rs`**: Stress tests the performance monitoring system under heavy load.
*   **`performance_monitoring_lifecycle_test.rs`**: Detailed testing of performance monitoring states and configs.
*   **`phase_1_infrastructure_test.rs`**: Tests the underlying infrastructure for performance monitoring.
*   **`phase_2_collector_test.rs`**: Tests data collection mechanisms for performance metrics.
*   **`phase_3_scoring_audit.rs`**: Audits the scoring logic for performance diagnostics.
*   **`phase_3_scoring_demonstration.rs`**: Demonstrates the scoring system with sample data.
*   **`phase_3_variant_aware_rebranding.rs`**: Validates rebranding logic based on kernel variants.
*   **`profile_pipeline_validation.rs`**: Validates the end-to-end profile application pipeline.
*   **`real_kernel_build_integration.rs`**: Integration test simulating a real kernel build process.
*   **`stressor_diagnostic_tests.rs`**: Validates the implementation details of system stressors.
*   **`stressor_integration_tests.rs`**: Tests the integration of stressors with the performance monitor.
*   **`ui_scaling_tests.rs`**: Validates UI behavior and state updates across different scales.
*   **`ui_sync_tests.rs`**: Extensive testing of UI state synchronization with the underlying controller.

### Unit Tests

Unit tests are located within the `src/` directory alongside the implementation code. They cover:
*   Configuration management (`config/`)
*   Hardware detection (`hardware/`)
*   Kernel management and patching (`kernel/`)
*   Orchestration logic (`orchestrator/`)
*   System performance monitoring (`system/performance/`)
*   UI state and logic (`ui/`)

## Execution Instructions

To run the full test suite, navigate to the `master/` directory and use `cargo`:

```bash
cd master
cargo test
```

### Running Specific Tests

To run a specific integration test file:

```bash
cargo test --test <test_file_name>
```
Example: `cargo test --test hardware_tests`

To run a specific test function:

```bash
cargo test <test_function_name>
```

### Doc Tests

Documentation tests can be run using:

```bash
cargo test --doc
```

## Hygiene Requirements

*   All tests must use the `goatdkernel` crate name for imports.
*   Tests should avoid external side effects or clean up after themselves (e.g., using `tempfile`).
*   Mocking should be used for hardware-dependent logic where possible.

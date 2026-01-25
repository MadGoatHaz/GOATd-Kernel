# GOATd Kernel Test Suite Inventory

This document provides a comprehensive inventory of the tests available in the GOATd Kernel project, their purpose, and instructions for execution.

## Execution Instructions

Run the full test suite:
```bash
cargo test
```

Run specific integration tests:
```bash
cargo test --test <test_name>
```

Example:
```bash
cargo test --test dynamic_versioning_test
```

## Unit Tests

Unit tests are located within the source files in `src/` (usually in a `mod tests` block). They cover core logic for:
- **Config**: Exclusions, Finalization, Loading, Modprobed, Profiles, Whitelist.
- **Hardware**: GPU detection, RAM, Storage.
- **Kernel**: Audit, Git, LTO, Manager, Parser, Patcher, PKGBUILD, Validator.
- **System**: Health, SCX, Verification.
- **UI**: App, Controller, Scaling, Threading.

## Integration Tests (`tests/`)

| Test File | Description |
|-----------|-------------|
| `comprehensive_feature_realization.rs` | Verifies full feature set integration. |
| `config_tests.rs` | Comprehensive configuration management tests. |
| `dynamic_versioning_test.rs` | Verifies "latest" version resolution and fallback hierarchy (Poll → Cache → Local → Baseline). |
| `forensic_diagnostic.rs` | System diagnostic and forensic data collection tests. |
| `git_tests.rs` | Git operations, version validation, and source management. |
| `hardware_tests.rs` | Full hardware detection and validation suite. |
| `integration_tests.rs` | Orchestrator phase transitions and state management. |
| `lifecycle_pipe_integration.rs` | End-to-end lifecycle testing from scan to install. |
| `logging_integration_test.rs` | Log collection, rotation, and session management. |
| `logging_robustness_test.rs` | Concurrent logging, high volume, and directory creation safety. |
| `modprobed_localmodconfig_validation.rs` | modprobed-db integration and localmodconfig simulation. |
| `mpl_integration_test.rs` | Modular Patcher Layer (MPL) sourcing and injection tests. |
| `performance_baseline_calibration.rs` | Baseline metrics for performance comparison. |
| `performance_battle_tests.rs` | Stressor isolation, SMI correlation, and precision timing. |
| `performance_monitoring_lifecycle_test.rs` | Monitoring state machine, ring buffer, and summary capture. |
| `phase_1_infrastructure_test.rs` | Cgroup, watchdog, and PID exclusion logic. |
| `phase_2_collector_test.rs` | Syscall, Jitter, and Wakeup data collectors. |
| `phase_3_scoring_audit.rs` | Scoring mathematics, personality generation, and thermal efficiency. |
| `phase_3_scoring_demonstration.rs` | Personality phenotype demonstrations (Gaming, Throughput, etc.). |
| `phase_3_variant_aware_rebranding.rs` | Variant detection (Zen, Hardened, etc.) and PKGBUILD rebranding logic. |
| `profile_pipeline_validation.rs` | Profile definition, optimization levels, and Clang/LTO enforcement. |
| `real_kernel_build_integration.rs` | Live simulation of kernel builds with async timeouts and log capture. |
| `stressor_diagnostic_tests.rs` | Stressor worker isolation and CPU affinity. |
| `stressor_integration_tests.rs` | Multi-stressor coordination and resource cleanup. |
| `ui_scaling_tests.rs` | DPI awareness, font scaling, and window width responsiveness. |
| `ui_sync_tests.rs` | UI state persistence and synchronization across profile changes. |

## Maintenance & Hygiene

- **Path Resolution**: Tests use relative paths where possible or standard `/tmp` directories for transient artifacts.
- **Dynamic Naming**: Tests are updated to handle variant-aware naming (e.g., `linux-tkg`, `linux-zen`).
- **Clean Assertions**: Junk assertions and obsolete logic have been removed to ensure tests reflect the current project state.

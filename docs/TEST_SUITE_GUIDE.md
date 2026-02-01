# GOATd Kernel Test Suite Guide

This guide provides an overview of the GOATd Kernel test architecture, descriptions of existing tests, and instructions for running and extending the suite.

## Test Architecture

A multi-tiered testing strategy ensures reliability across the kernel-building and performance-monitoring pipelines.

### Unit Tests
Unit tests are located within the [`src/`](src/) directory, typically in `tests` modules at the bottom of each file. They focus on individual functions and logic isolation.
- **Coverage**: Config parsing, hardware detection logic, state transitions, etc.

### Integration Tests
Integration tests are located in the [`tests/`](tests/) directory. They verify the interaction between multiple modules and external system components.
- **Tools**: `#[tokio::test]` for asynchronous orchestration.
- **Focus**: End-to-end build pipelines, UI state synchronization, and performance stressor isolation.

---

## High-Value Integration Tests

| File | Purpose |
|------|---------|
| [`tests/real_kernel_build_integration.rs`](tests/real_kernel_build_integration.rs) | **Critical Path**: Exercises the `AsyncOrchestrator` build pipeline, including timeout handling, log diagnostics, and dry-run injection. |
| [`tests/lifecycle_pipe_integration.rs`](tests/lifecycle_pipe_integration.rs) | **System Integrity**: Verifies the complete install/uninstall lifecycle with full log capture via `LogCollector`. |
| [`tests/comprehensive_feature_realization.rs`](tests/comprehensive_feature_realization.rs) | **Feature Parity**: Verifies that performance features (LTO, MGLRU, Polly, BORE) are correctly applied to `.config` and `PKGBUILD`. |
| [`tests/performance_battle_tests.rs`](tests/performance_battle_tests.rs) | **Precision Verification**: Lab-grade verification of nanosecond precision, SMI correlation, and stressor isolation. |
| [`tests/mpl_integration_test.rs`](tests/mpl_integration_test.rs) | **Architecture Validation**: Ensures Multi-Path Loading (MPL) logic works across different mount points. |
| [`tests/ui_sync_tests.rs`](tests/ui_sync_tests.rs) | **UX Reliability**: Validates Egui UI properties synchronize with Rust `AppState` to prevent startup state mismatches. |
| [`tests/phase_1_infrastructure_test.rs`](tests/phase_1_infrastructure_test.rs) | **Modularization**: Validates Phase 1 (Preparation) infrastructure in isolation. |

---

## Running the Test Suite

### Standard Execution
```bash
cargo test
```

### Targeted Testing
To run a specific test file:
```bash
cargo test --test real_kernel_build_integration
```

To run a specific test within a file:
```bash
cargo test --test performance_battle_tests test_nanosecond_precision
```

### Dry-Run Build Testing
Test the build pipeline without long compilation times:
```bash
GOATD_DRY_RUN_HOOK=1 cargo test --test real_kernel_build_integration
```

---

## Troubleshooting

### SMI Detection (MSR Access)
- *Failure*: `test_smi_correlation_reliability` fails to read MSR.
- *Fix*: Ensure the `msr` kernel module is loaded (`sudo modprobe msr`) and the test runner has read access to `/dev/cpu/*/msr`.

### Timeout in Build Integration
- *Failure*: `test_async_orchestrator_timeout` fails unexpectedly.
- *Fix*: Check if the environment is extremely resource-constrained, causing the "Preparation" phase to exceed the 5s mock timeout.

### Missing Kernel Artifacts
- *Failure*: `tests/lifecycle_pipe_integration.rs` skips tests.
- *Requirement*: This test expects a built `.pkg.tar.zst` in the workspace. Provide a mock artifact or run a build first.

---

## Guidelines for Adding New Tests

1. **Feature Realization**: When adding a new Kconfig option or build-time optimization, add a verification case to [`tests/comprehensive_feature_realization.rs`](tests/comprehensive_feature_realization.rs).
2. **UI Impact**: If the feature has a toggle in the UI, add a synchronization test to [`tests/ui_sync_tests.rs`](tests/ui_sync_tests.rs).
3. **Log Visibility**: Use `LogCollector` and include diagnostic markers (e.g., `[FEATURE-NAME]`).
4. **Modularity**: New complex features should include a standalone phase test (e.g., `tests/phase_X_...`).

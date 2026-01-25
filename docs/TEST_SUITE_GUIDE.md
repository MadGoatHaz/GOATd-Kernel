# GOATd Kernel Test Suite Guide

This guide provides an overview of the GOATd Kernel test architecture, descriptions of existing tests, and instructions for running and extending the suite.

## Test Architecture

GOATd Kernel employs a multi-tiered testing strategy to ensure reliability across its complex kernel-building and performance-monitoring pipelines.

### Unit Tests
Unit tests are located within the [`src/`](src/) directory, typically in `tests` modules at the bottom of each file. They focus on individual functions and logic isolation.
- **Tools**: Standard `#[test]` attribute.
- **Coverage**: Config parsing, hardware detection logic, state transitions, etc.

### Integration Tests
Integration tests are located in the [`tests/`](tests/) directory. They verify the interaction between multiple modules and external system components (like the filesystem or kernel build tools).
- **Tools**: `#[tokio::test]` for asynchronous orchestration.
- **Focus**: End-to-end build pipelines, UI state synchronization, and performance stressor isolation.

---

## Integration Test Inventory

| File | Purpose |
|------|---------|
| [`tests/ui_sync_tests.rs`](tests/ui_sync_tests.rs) | Validates Egui UI properties synchronize with Rust `AppState`. Prevents startup checkbox bugs. |
| [`tests/performance_battle_tests.rs`](tests/performance_battle_tests.rs) | Lab-grade verification of nanosecond precision, SMI correlation, and stressor isolation. |
| [`tests/comprehensive_feature_realization.rs`](tests/comprehensive_feature_realization.rs) | Verifies that ALL performance features (LTO, MGLRU, Polly, BORE) are correctly applied to `.config` and `PKGBUILD`. |
| [`tests/lifecycle_pipe_integration.rs`](tests/lifecycle_pipe_integration.rs) | Verifies the complete install/uninstall lifecycle with full log capture. |
| [`tests/real_kernel_build_integration.rs`](tests/real_kernel_build_integration.rs) | Tests the `AsyncOrchestrator` build pipeline, including timeout handling and log diagnostics. |
| [`tests/stressor_integration_tests.rs`](tests/stressor_integration_tests.rs) | Exercises `StressorManager` for CPU, Memory, and Scheduler stressors, ensuring graceful cleanup. |
| [`tests/config_tests.rs`](tests/config_tests.rs) | Logic validation for configuration profiles and manual overrides. |
| [`tests/hardware_tests.rs`](tests/hardware_tests.rs) | Integration with system hardware detection (CPU, RAM, GPU, Storage). |

---

## Running the Test Suite

### Running Everything
```bash
cargo test
```

### Running Specific Integration Tests
To run a specific test file:
```bash
cargo test --test ui_sync_tests
```

To run a specific test within a file:
```bash
cargo test --test performance_battle_tests test_nanosecond_precision
```

### Performance Test Considerations
Some tests in [`tests/performance_battle_tests.rs`](tests/performance_battle_tests.rs) require specific environment conditions:
- **Core Isolation**: Tests for stressor leakage assume affinity works. If running in a VM with limited cores, these might report higher variance.
- **Permissions**: Tests involving MSR (Model Specific Registers) for SMI detection require root or specific capabilities.

---

## Troubleshooting

### common environment-specific failures
1. **SMI Detection (MSR Access)**:
   - *Failure*: `test_smi_correlation_reliability` or similar fails to read MSR.
   - *Fix*: Ensure the `msr` kernel module is loaded (`sudo modprobe msr`) and the test runner has read access to `/dev/cpu/*/msr`.

2. **Timeout in Build Integration**:
   - *Failure*: `test_async_orchestrator_timeout` fails unexpectedly.
   - *Fix*: Check if the test environment is extremely resource-constrained, causing the "Preparation" phase to exceed the 5s mock timeout before it even hits the expected failure point.

3. **Missing Kernel Artifacts**:
   - *Failure*: `tests/lifecycle_pipe_integration.rs` skips tests.
   - *Requirement*: This test expects a built `.pkg.tar.zst` in the workspace. Run a build first or provide a mock artifact.

---

## UI Synchronization Pattern

GOATd uses a "Canonical Truth" pattern for UI synchronization:
1. **Authoritative State**: [`src/config/mod.rs`](src/config/mod.rs) (`AppState`) is the source of truth.
2. **Controller Mediation**: [`src/ui/controller.rs`](src/ui/controller.rs) (`AppController`) manages all state transitions.
3. **Synchronous Sync**: On startup, `apply_current_profile_defaults()` is called *before* the Egui event loop starts to ensure checkboxes match the loaded profile immediately.

Reference test: [`tests/ui_sync_tests.rs`](tests/ui_sync_tests.rs)

---

## Guidelines for Adding New Tests

1. **Feature Realization**: When adding a new Kconfig option or build-time optimization, add a verification case to [`tests/comprehensive_feature_realization.rs`](tests/comprehensive_feature_realization.rs).
2. **UI Impact**: If the feature has a toggle in the UI, add a synchronization test to [`tests/ui_sync_tests.rs`](tests/ui_sync_tests.rs).
3. **Log Visibility**: Ensure new asynchronous operations use the `LogCollector` and include diagnostic markers (e.g., `[FEATURE-NAME]`).
4. **Clickable References**: Always use clickable file references in documentation to maintain developer velocity.

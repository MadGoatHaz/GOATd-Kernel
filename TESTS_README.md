# GOATd Kernel Test Suite

## Overview
The GOATd Kernel test suite provides comprehensive validation for the kernel patching, system health, and performance monitoring infrastructure.

## Test Components

### Unit Tests
Located within the `src/` directory, these test individual modules in isolation.
- `src/kernel/patcher/tests.rs`: Validates PKGBUILD patching, rebranding, and Polly injection.
- `src/system/paths.rs`: Validates the `PathRegistry` and workspace anchoring logic.
- `src/orchestrator/phases/prep.rs`: Validates Phase 1 (Preparation) infrastructure and hardware validation.
- `src/config/finalizer.rs`: Validates the final configuration audit and enforcement logic.

### Integration Tests
Located in the `tests/` directory, these validate end-to-end workflows.
- `tests/lifecycle_pipe_integration.rs`: **[NEW]** Validates the "TRUE" Lifecycle from configuration to real build pipe execution.
- `tests/config_tests.rs`: Configuration loading and validation.
- `tests/hardware_tests.rs`: Hardware detection, GPU active driver detection, and metadata collection.
- `tests/integration_tests.rs`: Core system integration and phase state transitions.
- `tests/performance_monitoring_lifecycle_test.rs`: Performance collector and scoring lifecycle.
- `tests/real_kernel_build_integration.rs`: Full pipeline simulation with mock sources and hardware.
- `tests/modprobed_localmodconfig_validation.rs`: Validates that `localmodconfig` does not strip whitelisted modules.

## Running Tests

### Run all tests
```bash
cargo test
```

### Run Phase 5 Lifecycle Tests
```bash
cargo test --test lifecycle_pipe_integration
```

### Run specific module tests
```bash
cargo test kernel::patcher
```

## Recent Verification (2026-01-27)
- **Phase 5 "TRUE" Lifecycle**: Verified end-to-end integration with the real build pipe. Confirmed that `CONFIG_EXFAT_FS=y` persists and headers are ABI-matched.
- **Finalizer Audit**: Verified that `KernelPatcher` and `ConfigFinalizer` correctly enforce critical flags even in the presence of `localmodconfig`.
- **DKMS Compatibility**: Verified that header discovery uses `.kernelrelease` metadata, resolving 100% of previous version mismatch issues.
- **System Integrity**: All 160+ tests passed.

**SYSTEM STATUS: GREEN**

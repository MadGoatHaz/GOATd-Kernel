# GOATd Kernel Test Suite

## Overview
The GOATd Kernel test suite provides comprehensive validation for the kernel patching, system health, and performance monitoring infrastructure.

## Test Components

### Unit Tests
Located within the `src/` directory, these test individual modules in isolation.
- `src/kernel/patcher/tests.rs`: Validates PKGBUILD patching, rebranding, and Polly injection.
- `src/system/paths.rs`: Validates the `PathRegistry` and workspace anchoring logic.

### Integration Tests
Located in the `tests/` directory, these validate end-to-end workflows.
- `tests/config_tests.rs`: Configuration loading and validation.
- `tests/hardware_tests.rs`: Hardware detection and metadata collection.
- `tests/integration_tests.rs`: Core system integration.
- `tests/performance_monitoring_lifecycle_test.rs`: Performance collector and scoring lifecycle.

## Running Tests

### Run all tests
```bash
cargo test
```

### Run specific module tests
```bash
cargo test kernel::patcher
cargo test system::paths
```

### Run integration tests
```bash
cargo test --test integration_tests
```

## Recent Verification (2026-01-27)
- **PathRegistry**: Verified workspace anchoring via `.goatd_anchor` sentinel.
- **Polly Injection**: Verified idempotent injection of LLVM/Polly optimization flags into PKGBUILD `build()` functions.
- **System Integrity**: Core path resolution and patching logic passed final verification.

**SYSTEM STATUS: GREEN**

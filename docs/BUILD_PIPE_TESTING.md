# Build Pipeline & Testing

This document details the modular build pipeline and the testing strategies used to ensure reliability during kernel compilation and deployment.

## Build Pipeline Phases

The build process is managed by [`src/orchestrator/executor.rs`](src/orchestrator/executor.rs) and is divided into several logical phases to ensure safety and reproducibility.

### 1. Audit Phase (Preparation)
- **Validation**: Performs hardware capability checks (CPU features, RAM capacity) and validates the target workspace.
- **Environment Setup**: Ensures the build directory is initialized and the `.goatd_anchor` file is present to prevent out-of-bounds filesystem operations.
- **Reference**: `validate_hardware`, `prepare_kernel_build`.

### 2. Config Phase (Resolution)
- **Version Resolution**: Resolves "latest" version strings to concrete kernel versions via Git or local source inspection.
- **Parity Check**: Verifies SHA256 checksums between templates and the workspace to detect stale or corrupted PKGBUILDs.
- **Reference**: `resolve_kernel_version`, `validate_kernel_config`.

### 3. Build Phase (Execution)
- **Compilation**: Executes the actual kernel build using the configured toolchain (LLVM/Clang by default).
- **Real-time Monitoring**: Captures stdout/stderr via callbacks for UI integration and log collection.
- **Timeout Protection**: Wraps the build process in a configurable timeout to prevent indefinite hangs.
- **Reference**: `execute_kernel_build`.

### 4. Export Phase (Finalization)
- **Kernelrelease Discovery**: Discover the exact `kernelrelease` string from the build artifacts.
- **Cross-Mount Propagation**: Propagates the `.kernelrelease` file across the workspace, including parent directories and subdirectories, to ensure consistency across different mount points or container boundaries.
- **Reference**: `propagate_kernelrelease`, `discover_kernelrelease_robust`.

## Dry-Run Mode & Hooks

To facilitate testing without the cost of a full kernel compilation, the executor supports a "Dry-Run" mode triggered by environment variables.

- **`GOATD_DRY_RUN_HOOK`**: When set, the executor halts immediately before the expensive compilation step. It dumps the resolved configuration and environment state to stderr for verification.
- **Usage**:
  ```bash
  GOATD_DRY_RUN_HOOK=1 cargo test --test real_kernel_build_integration
  ```

## Testing Strategy

- **Unit Tests**: Found within source files (e.g., `src/kernel/patcher/tests.rs`) for localized logic.
- **Integration Tests**: Comprehensive end-to-end flows located in the `tests/` directory.
- **Mocking**: Hardware and filesystem interactions are abstracted to allow testing on non-target environments.

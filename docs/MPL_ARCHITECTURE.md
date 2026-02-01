# Metadata Persistence Layer (MPL) Architecture

The Metadata Persistence Layer (MPL) is the GOATd Kernel's mechanism for ensuring metadata consistency across the build process, from Rust orchestrator state to shell-based PKGBUILD environments.

## Overview

The MPL serves as the "bridge" between the Rust-based orchestrator and the Arch Linux build system. It ensures that critical information (like kernel versions, profile names, and build IDs) is consistently handled even when execution moves between different languages and environments.

## The `.goatd_metadata` File

The core of the MPL is the `.goatd_metadata` file, located at the workspace root.

### Format
The file uses a shell-compatible format, allowing it to be directly sourced by `bash` or other shells while remaining parsable by Rust.

**Example Content:**
```bash
GOATD_BUILD_ID="550e8400-e29b-41d4-a716-446655440000"
GOATD_KERNELRELEASE="6.19.0-goatd-gaming"
GOATD_KERNEL_VERSION="6.19.0"
GOATD_PROFILE="Gaming"
GOATD_VARIANT="linux"
GOATD_LTO_LEVEL="full"
GOATD_WORKSPACE_ROOT="/home/user/GOATd"
```

### Integrity Checks
- **Atomic Writes:** The system uses temporary files and atomic renames to prevent corruption during updates.
- **Sourcing Validation:** The patcher injects checks into PKGBUILDs to ensure the file exists and is sourced before proceeding with the build.
- **Parsing Robustness:** `MPLMetadata::from_shell_format` in [`src/models.rs`](src/models.rs) handles value extraction with fallback defaults.

## Interaction with `src/orchestrator/state.rs`

The `OrchestrationState` in [`src/orchestrator/state.rs`](src/orchestrator/state.rs) tracks the live build status. The MPL interacts with this state at key lifecycle points:

1.  **Initialization:** At the start of a build, the orchestrator generates the initial metadata and writes the `.goatd_metadata` file.
2.  **Patcher Injection:** The patcher ([`src/kernel/patcher/mod.rs`](src/kernel/patcher/mod.rs)) reads the workspace root from the state and injects instructions into the PKGBUILD to source the MPL file.
3.  **Kernelrelease Propagation:** After the kernel build completes, the orchestrator extracts the final `KERNELRELEASE` from the build artifacts and updates the MPL file.
4.  **Verification:** The verification subsystem uses the MPL as the definitive source of truth to confirm that the installed kernel matches the orchestrated build.

## Key Components

- **[`MPLMetadata`](src/models.rs):** The Rust structure representing the metadata.
- **`executor::update_mpl`:** The function in [`src/orchestrator/executor.rs`](src/orchestrator/executor.rs) that handles post-build metadata updates.
- **`verification::verify`:** Uses MPL to validate build integrity.

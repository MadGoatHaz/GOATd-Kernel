# GOATd Kernel Test Suite Documentation

## LLVM/Clang Toolchain Enforcement
The GOATd Kernel project enforces the use of the LLVM/Clang toolchain for all kernel builds to ensure performance, security, and modern feature support (such as ThinLTO and CFI).

*   **Enforcement Status:** **ACTIVE**.
*   **Requirements:**
    *   `CC=clang`
    *   `CXX=clang++`
    *   `LD=ld.lld`
    *   `LLVM=1` (passed to Kbuild)
    *   LLVM utilities: `llvm-ar`, `llvm-nm`, `llvm-strip`, `llvm-objcopy`, `llvm-readelf`.
*   **Verification:**
    *   `src/system/verification.rs` contain the logic for `verify_llvm_toolchain()`.
    *   Tests in `tests/real_kernel_build_integration.rs` verify that the configuration correctly sets `force_clang` and `full_llvm_mode`.
    *   `src/kernel/patcher/tests.rs` ensures that audits fail if `CONFIG_CC_IS_GCC` is detected.

## UI Repaint Throttling & OOM Mitigation
To prevent GPU command buffer saturation and potential Out-Of-Memory (OOM) errors, especially during high-volume event logging (like during a kernel build), the UI implements several throttling strategies.

*   **Timed Repaint Throttling:** When `is_building` is true, the UI shifts from event-driven repaints to a fixed 60Hz interval (16ms) using `ctx.request_repaint_after()`.
*   **Hysteresis Scaling:** Window scaling updates are throttled using a 0.05 delta threshold and timing guards to prevent feedback loops.
*   **Performance Data Throttling:** Metrics collection and percentile calculations are throttled (e.g., every 100ms-200ms) to reduce CPU overhead.

## Concurrent Versioning Throttling & HashSet/Guard Logic
To prevent redundant and overlapping network requests for kernel version updates, a multi-layered throttling strategy is employed.

*   **HashSet In-Flight Tracking:** A `HashSet<String>` (`version_poll_active`) tracks currently active polling requests by variant name.
*   **Time-Based Guard:** A 30-second throttle window is enforced via `last_version_poll` (HashMap of `Instant`) before a refresh can be triggered.
*   **Atomic Marking:** Variants are added to the active set *before* the asynchronous task is spawned, ensuring no race conditions allow duplicate spawns for the same variant.

## Test Inventory & Structure
The project maintains a comprehensive suite of integration and unit tests located in the `tests/` directory.

### Core Kernel Refactor Tests (Phase 4 Verification)
*   [`tests/chunk_3_header_discovery_sync.rs`](tests/chunk_3_header_discovery_sync.rs): Validates header discovery synchronization between the manager and patcher. Ensures deterministic naming schemas (`{variant}-headers-{version}-{release}`) and GOATd-aware pivoting. Verified via direct symlink inspection and path resolution checks.
*   [`tests/phase_3_variant_aware_rebranding.rs`](tests/phase_3_variant_aware_rebranding.rs): Verifies PKGBUILD transformation logic across all 6 supported kernel variants (linux, lts, hardened, zen, mainline, tkg).
*   [`tests/integration_tests.rs`](tests/integration_tests.rs): Full orchestration flow tests, including state persistence and phase transitions.

### Other Significant Test Suites
*   `tests/config_tests.rs`: Validates hardware-specific exclusions, profiles, and modprobed-db parsing.
*   `tests/performance_battle_tests.rs`: Stress tests for the performance scoring and monitoring subsystem.
*   `tests/git_tests.rs`: Verifies kernel source acquisition and patch application logic.
*   `tests/ui_sync_tests.rs`: Ensures UI-to-Backend state synchronization and threading safety.

## Test Execution
Run the full suite using:
```bash
cargo test
```

### Targeted Execution
To run specific tests related to the Kernel Header Management Refactor:
```bash
cargo test --test chunk_3_header_discovery_sync
cargo test --test phase_3_variant_aware_rebranding
cargo test --test integration_tests
```

Note: Some performance-sensitive tests or those requiring specific hardware access may be `#[ignored]` by default. Use `-- --ignored` to run them.

## PKGBUILD Version Synchronization logic (pkgver-pkgrel)
The GOATd Kernel patcher ensures that the PKGBUILD `pkgver` and `pkgrel` variables are perfectly synchronized with the actual kernel release version (e.g., from `.kernelrelease`). This prevents path mismatches where headers are installed to a directory that doesn't match the expected `uname -r` format.

*   **Logic:**
    *   Splits the resolved version at the last hyphen to separate `pkgver` and `pkgrel`.
    *   Sanitizes `pkgver` by replacing internal hyphens with dots (e.g., `6.19-rc6` becomes `6.19.rc6`).
    *   Ensures `pkgrel` is always present, defaulting to `1` if not found.
*   **Verification:**
    *   `src/kernel/patcher/pkgbuild.rs` implements `synchronize_pkgbuild_version`.
    *   `src/kernel/patcher/tests.rs` contains unit tests for PKGBUILD rebranding and version extraction.

## Kernel Header Management Verification
Specific verification strategies are employed to ensure the integrity of the new header management system:

*   **Deterministic Matching**: Tests verify that header packages follow the `${pkgbase}-headers-${pkgver}-${pkgrel}` schema exactly.
*   **Makefile Parsing Fallback**: The system includes logic to fall back to parsing the kernel `Makefile` (VERSION, PATCHLEVEL, SUBLEVEL, EXTRAVERSION) if standard discovery fails, ensuring synchronization even with non-standard sources.
*   **Path Alignment**: Verification that headers are installed to `/usr/src/${pkgbase}-${pkgver}-${pkgrel}` and that this path matches the `uname -r` produced by the built kernel.

## Robust regex extraction for shell variables
The system uses high-precision regular expressions to extract and modify shell variables in PKGBUILD files, supporting various quoting styles (unquoted, single-quoted, double-quoted) and trailing comments.

*   **Supported Variables:** `pkgname`, `pkgbase`, `pkgver`, `pkgrel`, `source`.
*   **Regex Pattern:** `VAR\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s#]+))`
*   **Safety Features:**
    *   Regex patterns use multiline mode (`(?m)`) and anchored starts (`^`) to avoid false positives.
    *   Repairs verify the extracted values against expected results after modification.

## Recent Test Cycle Verification (2026-01-30)
The full test suite was executed successfully following the Kernel Header Management Refactor.

*   **Status:** **SYSTEM GREEN**
*   **Key Fixes:**
    *   Synchronized `KernelConfig` initialization in `tests/integration_tests.rs` with new `pkgbase` field.
    *   Fixed type mismatch in `tests/chunk_3_header_discovery_sync.rs` for GOATd pivot parsing.
*   **Infrastructure Verified:**
    *   **Config System:** Validated exclusions, profiles, and modprobed-db integration.
    *   **Patcher Logic:** Rebranding and version synchronization logic confirmed.
    *   **Performance Phase:** Scoring mathematics and personality-aware calibrations verified.
    *   **UI Sync:** State persistence and scaling logic confirmed across resolution scenarios.

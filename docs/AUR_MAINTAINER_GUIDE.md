# AUR Maintainer & PKGBUILD Transformation Guide

This document outlines the logic used by GOATd Kernel to transform standard Arch Linux PKGBUILDs into branded GOATd variants. This is primarily handled by the Rust-based patcher.

## PKGBUILD Transformation Logic

The core transformation logic resides in [`src/kernel/patcher/pkgbuild.rs`](src/kernel/patcher/pkgbuild.rs). It automates the conversion of a standard kernel package into a optimized GOATd variant.

### 1. `pkgbase` Rebranding

The patcher identifies the base kernel variant and applies GOATd branding.

- **Detection**: Uses `PKGBASE_REGEX` to extract the core identity (e.g., `linux`, `linux-zen`).
- **Scheme**: `linux-{variant}-goatd-{profile}`.
- **Examples**:
    - `pkgbase='linux'` + `gaming` profile -> `pkgbase='linux-goatd-gaming'`
    - `pkgbase='linux-zen'` + `server` profile -> `pkgbase='linux-zen-goatd-server'`

### 2. Idempotency & Branding Lock

To prevent double-branding (e.g., `linux-goatd-gaming-goatd-gaming`), the patcher implements a **Branding Lock**.
- It scans for the `-goatd-` string in `pkgname` or `package_` functions.
- If detected, it skips the rebranding phase.

### 3. Variant Agnosticism

The patcher is designed to be "variant-agnostic." It can handle:
- Standard Arch repositories (Core/Extra).
- AUR variants (via source URL detection in `validate_source_repair`).
- External Git repositories.

### 4. Technical Implementation Detail: `src/kernel/patcher/pkgbuild.rs`

Key functions:
- `detect_kernel_variant()`: Extracts the base name from `pkgbase`.
- `patch_pkgbuild_for_rebranding()`: Coordinates the renaming of `pkgbase`, `pkgname`, and `package_*` functions.
- `synchronize_pkgbuild_version()`: Updates `pkgver` and `pkgrel` to match the resolved source version.

## Maintainer Workflow (Solo Developer)

As the sole maintainer, the goal is high automation.
1. **Source Discovery**: The orchestrator finds the latest kernel version or tracks a specific tag.
2. **Cloning**: `src/kernel/git.rs` handles cloning the PKGBUILD source.
3. **Patching**: 
    - `patch_pkgbuild_for_rebranding` applies name changes.
    - `inject_rust_rmeta_fix` ensures compatibility with Rust-enabled kernels.
    - `inject_nvidia_dkms_shim_into_headers_package` handles hardware-specific fixes.
4. **Validation**: The `verify.sh` script or internal validation logic ensures the resulting PKGBUILD is valid Arch syntax.

---
*Note: This guide is intended for the maintainer of the GOATd Kernel project to understand how PKGBUILDs are mutated during the build lifecycle.*

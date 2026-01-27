# KBuild Header Repair Blueprint

## Overview
This blueprint addresses the missing root-level `.kernelrelease` file in the kernel headers package. While the kernel image package correctly includes this file at the root, the headers package currently only has it nested within `include/config/`. The "Strict Protocol" used by GOATd repair hooks requires this file at `/usr/src/linux-<version>/.kernelrelease` for reliable version verification and DKMS synchronization.

## 1. Architectural Strategy
We will implement a dual-layer fix:
1.  **Template Update**: Enhance the `package_headers()` template in [`src/kernel/patcher/templates.rs`](src/kernel/patcher/templates.rs) to explicitly create the root `.kernelrelease` file using the derived `_actual_ver` (derived from the actual build artifact).
2.  **Patcher Alignment**: Ensure [`src/kernel/patcher/pkgbuild.rs`](src/kernel/patcher/pkgbuild.rs) correctly identifies the headers package functions and injects the updated logic.

## 2. Technical Specification

### 2.1 Explicit `.kernelrelease` Creation
The headers packaging logic must derive the version string from the build tree's `include/config/kernel.release` (or `make kernelrelease` fallback) and write it to the root of the headers destination.

### 2.2 Unified Template Injection
We will update `get_headers_injection` in [`src/kernel/patcher/templates.rs`](src/kernel/patcher/templates.rs) to include the metadata creation step.

**New Template Snippet:**
```bash
# =====================================================================
# PHASE-E2.1: CORE METADATA INJECTION (.kernelrelease)
# =====================================================================
# CRITICAL: Create root-level .kernelrelease for "Strict Protocol" hooks
# This ensures /usr/src/linux-${_actual_ver}/.kernelrelease exists
if [ -n "${_actual_ver}" ] && [ -d "${_headers_dir}" ]; then
    echo "${_actual_ver}" > "${_headers_dir}/.kernelrelease"
    echo "[PHASE-E2.1] Created root metadata: ${_headers_dir}/.kernelrelease" >&2
fi
```

### 2.3 Resilience & Derivation
The content of `.kernelrelease` must be derived from `_actual_ver`, which is discovered via a 5-tier strategy in the current patcher logic:
1.  Local `.kernelrelease` artifact.
2.  Search in `${srcdir}`.
3.  `GOATD_KERNELRELEASE` environment variable.
4.  Existing `pkgdir` modules.
5.  Fallback to `_kernver` (PKGBUILD variable).

## 3. Implementation Plan

### 3.1 Update `templates.rs`
- Modify `get_headers_injection` to include the `PHASE-E2.1` block.
- Modify `get_module_dir_creation` to ensure the version logic and headers action are consistent with the metadata requirement.

### 3.2 Update `pkgbuild.rs`
- Verify `inject_module_directory_creation` correctly uses the updated templates.
- Ensure the `builddir` calculation in `patch_pkgbuild_for_rebranding` (Phase 21) remains synchronized with the new metadata path.

## 4. Verification Protocol
1.  **Build Validation**: Run a test build and verify the headers package contents.
2.  **Path Check**: Confirm existence of `/usr/src/linux-<version>/.kernelrelease`.
3.  **Content Match**: Ensure `cat /usr/src/linux-<version>/.kernelrelease` matches `uname -r` of the built kernel.
4.  **Hook Test**: Verify that the `MODULE_REPAIR_INSTALL` hook (Phase 15) correctly finds the source directory using the `STRICT PROTOCOL`.

---
*Direct to Orchestrator: The blueprint is ready for implementation review. No Code Mode delegation is required as per the lock.*

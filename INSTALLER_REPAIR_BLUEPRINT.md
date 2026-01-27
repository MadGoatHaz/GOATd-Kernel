# INSTALLER_REPAIR_BLUEPRINT.md

Hello Orchestrator,

This blueprint addresses the header discovery disconnect where 6.18.7 headers are installed but verification is misled by 6.19.0 paths due to loose heuristic branding matches.

## 1. Problem Analysis
The current `discover_kernel_headers` logic in [`src/system/verification.rs`](src/system/verification.rs) uses a "Strategy 0" that prioritizes "GOATd-branded" strings. If a directory like `linux-goatd-mainline` (version 6.19.0) exists while we are running 6.18.7, the branding match triggers before the exact version match, leading to symlink contamination.

## 2. Proposed Fixes

### 2.1. Heuristic Priority Reversal ([`src/system/verification.rs`](src/system/verification.rs))
We will demote branding-based heuristics below version-based heuristics.

**New Priority Order:**
1. **Strategy 1 (Exact Match)**: Check `/usr/src/linux-{kernel_version}`.
2. **Strategy 2 (Base Version Match)**: Check `/usr/src/linux-{base_version}` BUT only if `.kernelrelease` inside matches `kernel_version` exactly.
3. **Strategy 3 (Strict Scan)**: Scan `/usr/src/linux-*` and ONLY accept if `.kernelrelease` matches `kernel_version` exactly.
4. **Strategy 4 (Branding Fallback)**: Only if the above fail, look for GOATd-branded paths, still requiring strict `.kernelrelease` validation.

### 2.2. Atomic Symlink Validation ([`src/system/verification.rs`](src/system/verification.rs))
The `create_kernel_symlinks_fallback` function will be hardened to ensure it NEVER points `build` or `source` to a directory that hasn't passed the strict `.kernelrelease` check.

- If `discover_kernel_headers` returns `None`, the fallback logic must fail loudly rather than attempting a loose match.
- Verification will be added *after* symlink creation to confirm the linked directory's `.kernelrelease` is indeed what the kernel expects.

### 2.3. .install Hook Alignment ([`src/kernel/patcher/templates.rs`](src/kernel/patcher/templates.rs))
The `MODULE_REPAIR_INSTALL` template (Phase 15) currently iterates through `/usr/src/linux-*` and uses `cat` on `.kernelrelease`. We will align this with the Rust logic.

- **Strict Pathing**: Prioritize `/usr/src/linux-$(uname -r)` and `/usr/src/linux-headers-$(uname -r)` before scanning.
- **Validation**: If no directory contains a `.kernelrelease` matching `$(uname -r)`, the hook MUST NOT create or update the symlinks. It should log a "VERSION MISMATCH" error instead of linking to the "nearest" GOATd directory.

## 3. Implementation Plan

### Step 1: Rust Core Hardening
- Modify `discover_kernel_headers` to move branding checks to the end.
- Add a `verify_directory_version(path, expected_version)` helper to centralize `.kernelrelease` reading.

### Step 2: Fallback Logic Hardening
- Update `create_kernel_symlinks_fallback` to use the helper.
- Ensure that if a symlink exists but points to a mismatched version, it is removed and replaced ONLY with a verified match.

### Step 3: Template Synchronization
- Update `MODULE_REPAIR_INSTALL` in `templates.rs` to implement the same "Version First, Branding Last" priority.
- Ensure the bash `for candidate in /usr/src/linux-*` loop immediately skips candidates that don't have a matching `.kernelrelease`.

## 4. Verification Strategy
1. **Mock Test**: Create `/usr/src/linux-6.19.0-goatd` and `/usr/src/linux-6.18.7-arch1`. Run the discovery logic for `6.18.7`. It should ignore the GOATd-branded path and pick the exact version match.
2. **Integration Test**: Run the full verification pipe on a system with "long-named" 6.19.0 headers present while 6.18.7 is running.

---
Orchestrator, please review this blueprint. If approved, I will switch to Code Mode for implementation.

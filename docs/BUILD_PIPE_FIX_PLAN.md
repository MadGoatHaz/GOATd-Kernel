# BUILD PIPE FIX & OPTIMIZATION PLAN

This document outlines a 20-point plan to fix the Polly flag issue, optimize the GOATd Kernel build pipeline, and ensure robust UI/Profile propagation.

## 1. FIX: Polly Flag Injection
**Issue**: Build fails with `Unknown command line argument '-polly'`. This is caused by passing `-polly` directly to `clang` instead of via `-mllvm`.

1.  **Standardize Polly Flag Format**: Update all injection points to use the correct `-mllvm -polly` sequence.
2.  **Verify Quoting in `src/kernel/patcher/env.rs`**: Ensure that when these flags are exported as `KCFLAGS` or `KCPPFLAGS`, they are properly quoted to prevent the shell from splitting them incorrectly.
3.  **Sanitize `KCFLAGS` Propagation**: Ensure `KCFLAGS` doesn't double-inject flags if they are already present in the source `Makefile`.
4.  **Fix Test Validation**: Update `tests/profile_pipeline_validation.rs` to validate the combined `-mllvm -polly` string as a single atomic unit.

## 2. OPTIMIZATION: Build Pipeline Speed & Reliability
5.  **Implement Parallel `KCONFIG_ALLCONFIG` Processing**: Ensure `generate_config_override` in `src/kernel/patcher/kconfig.rs` is called before any `make` commands to leverage native kernel parallel config resolution.
6.  **Optimize Modprobed-db Pathing**: Cache the results of `modprobed-db` discovery in `scripts/setup-caching.sh` to avoid redundant database lookups during every `prepare()` phase.
7.  **Surgical LTO Flag Cleanup**: Refine `src/kernel/lto.rs:remove_icf_flags` to also strip redundant LTO flags that might be injected by both the UI and the base `PKGBUILD`.
8.  **Enable ZSTD Compression Level Tuning**: Allow the build pipe to use `--threads=0` for ZSTD module compression to maximize multi-core utilization.
9.  **Build Tree Hygiene**: Implement aggressive cleanup of `srcdir` in `scripts/rebuild.sh` only for failed builds, while preserving objects for successful partial rebuilds.
10. **Pre-flight Toolchain Check**: Add a check in `scripts/verify.sh` to ensure `clang`, `lld`, and `llvm-polly` are installed and match expected versions (v19+) before starting the build.

## 3. RELIABILITY: Zero Functionality Loss & Regressions
11. **Triple-Lock LTO Enforcement**: Maintain the "Phase 5 Hard Enforcer" in `src/kernel/patcher/kconfig.rs` to prevent `oldconfig` from reverting LTO settings.
12. **NVIDIA ABI Safeguard Cluster**: Ensure `CONFIG_ZONE_DEVICE` and related ABI-critical flags are always injected after any user-level config overrides.
13. **Module Symlink Integrity Hook**: Verify that the `.install` repair hook in `src/kernel/patcher/pkgbuild.rs` correctly handles the `/usr/lib/modules/$(uname -r)/build` symlink for DKMS support.
14. **Cross-Mount Anchor Resolution**: Solidify the `resolve_goatd_root()` logic in `PKGBUILD` to handle builds running on non-standard mount points (e.g., `/mnt/build`).
15. **Localversion Collision Prevention**: Ensure `inject_modular_localversion` always results in a unique string to prevent `depmod` conflicts with stock kernels.

## 4. PROPAGATION: UI & Profile Accuracy
16. **Atomic Profile Mapping**: Ensure `src/config/profiles.rs` maps profiles (Gaming, Workstation, etc.) to a unified `BuildConfiguration` object before reaching the patcher.
17. **MGLRU Mask Enforcement**: Propagate the 0x0007 mask via both `CONFIG_LRU_GEN_ENABLED` and the baked-in `CMDLINE` in `src/kernel/patcher/kconfig.rs`.
18. **Native Optimization Passthrough**: Ensure `-march=native` is correctly extracted from hardware detection in `src/hardware/cpu.rs` and injected into `KCFLAGS`.
19. **Hardening Level UI Feedback**: Map `Minimal`, `Standard`, and `Hardened` levels directly to `mitigations=off/on` and specific `CONFIG_FORTIFY_SOURCE` settings.
20. **Audit Log Verification**: Update `scripts/check_kernel.sh` to perform a "Deep Audit" of the final `.config` against the requested UI settings to signal build success/failure.

---
**Status**: Plan Drafted
**Target File**: `docs/BUILD_PIPE_FIX_PLAN.md`

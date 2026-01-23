# Metadata Persistence Layer (MPL) Architecture

## Executive Summary

This document specifies a revolutionary new system for kernel version propagation that replaces the fragile shell code injection and environment variable fallback approach with a robust **Metadata Persistence Layer (MPL)**.

**Key Innovation**: The MPL uses a standardized, immutable metadata file at a fixed relative path within the workspace root, guaranteeing accessibility across all build phases including fakeroot execution, regardless of mount point boundaries.

---

## 1. Problem Analysis: Current System Limitations

### 1.1 Current Approach (Failing)

The existing system relies on:

1. **5-Level Shell Fallback Strategy** ([`src/kernel/patcher.rs`](src/kernel/patcher.rs:36))
   - `.kernelrelease` file discovery
   - `find` command searches
   - Environment variable fallback (`GOATD_KERNELRELEASE`)
   - Directory listing of `$pkgdir`
   - Hardcoded `_kernver` fallback

2. **Environment Variable Propagation** ([`src/orchestrator/executor.rs`](src/orchestrator/executor.rs:391))
   - `GOATD_KERNELRELEASE` set as placeholder
   - Populated after successful build via [`propagate_kernelrelease_to_workspace()`](src/orchestrator/executor.rs:903)

### 1.2 Failure Modes

| Scenario | Current Behavior | Failure Mode |
|----------|-----------------|--------------|
| External scratch disk | File at `/mnt/Optane/goatd/.kernelrelease` | Fails to find in fakeroot when `$srcdir` differs |
| makepkg `$srcdir` changes | Copy propagation to multiple locations | Race condition between copy and package() execution |
| Nested source layouts | Walk up parent directories | Mount point boundaries break path resolution |
| Permission restrictions | Safe write attempts with logging | Silent failure, falls back to `_kernver` |

### 1.3 Root Cause

**The Problem**: The PKGBUILD "pulls" version information using fragile discovery mechanisms. The system assumes:
- `$PWD` is reliable (false in fakeroot)
- File paths are consistent (false across mounts)
- Shell code can locate files (unreliable with nested layouts)

---

## 2. MPL Architecture: Clean Slate Design

### 2.1 Core Principles

1. **Workspace-Anchor Pattern**: All metadata paths are relative to the workspace root, never the current working directory
2. **Immutability**: Once written, metadata files are never modified—new builds create new metadata
3. **Single Source of Truth**: Exactly one metadata file per build session, deterministically located
4. **Cross-Mount Immunity**: The workspace root is the anchor; mount boundaries are transparent

### 2.2 File Format: `.goatd_metadata`

**Location**: `{workspace_root}/.goatd_metadata`

**Format**: Simple key-value pairs with shell-compatible syntax (can be sourced by PKGBUILD)

```bash
# GOATd Kernel Build Metadata
# Generated: 2026-01-20T08:30:00Z
# Build Session ID: a1b2c3d4-e5f6-7890-abcd-ef1234567890

GOATD_BUILD_ID="a1b2c3d4-e5f6-7890-abcd-ef1234567890"
GOATD_KERNELRELEASE="6.19.0-goatd-gaming"
GOATD_KERNEL_VERSION="6.19.0"
GOATD_PROFILE="gaming"
GOATD_VARIANT="linux"
GOATD_LTO_LEVEL="thin"
GOATD_BUILD_TIMESTAMP="2026-01-20T08:30:00Z"
GOATD_WORKSPACE_ROOT="/mnt/Optane/goatd"
GOATD_SOURCE_DIR="/mnt/Optane/goatd/src/linux-6.19.0"
GOATD_PKGVER="6.19.0"
GOATD_PKGREL="1"
GOATD_PROFILE_SUFFIX="-gaming"
```

### 2.3 Metadata File Schema

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `GOATD_BUILD_ID` | UUID v4 | Unique session identifier | `a1b2c3d4-e5f6-7890-abcd-ef1234567890` |
| `GOATD_KERNELRELEASE` | String | Exact kernel release string | `6.19.0-goatd-gaming` |
| `GOATD_KERNEL_VERSION` | String | Base kernel version | `6.19.0` |
| `GOATD_PROFILE` | String | Build profile name | `gaming` |
| `GOATD_VARIANT` | String | Kernel variant | `linux`, `linux-zen` |
| `GOATD_LTO_LEVEL` | String | LTO configuration | `full`, `thin`, `none` |
| `GOATD_BUILD_TIMESTAMP` | ISO 8601 | Build completion time | `2026-01-20T08:30:00Z` |
| `GOATD_WORKSPACE_ROOT` | Absolute path | Workspace root directory | `/mnt/Optane/goatd` |
| `GOATD_SOURCE_DIR` | Absolute path | Kernel source directory | `/mnt/Optane/goatd/src/linux-6.19.0` |
| `GOATD_PKGVER` | String | Package version | `6.19.0` |
| `GOATD_PKGREL` | String | Package release | `1` |
| `GOATD_PROFILE_SUFFIX` | String | LOCALVERSION suffix | `-gaming` |

---

## 3. Workflow: Rust Orchestrator → MPL → PKGBUILD → Verification

### 3.1 Phase 1: Orchestrator Initializes MPL

**Actor**: Rust Orchestrator ([`src/orchestrator/mod.rs`](src/orchestrator/mod.rs))

**Trigger**: Build session begins

**Action**:
```rust
// Pseudocode for MPL initialization
fn initialize_mpl(workspace: &Path, config: &KernelConfig) -> Result<MPL, Error> {
    let build_id = Uuid::new_v4();
    let timestamp = Utc::now().to_rfc3339();
    
    let metadata = MPLMetadata {
        build_id,
        kernel_release: String::new(),  // Will be populated after build
        kernel_version: config.version.clone(),
        profile: config.profile.clone(),
        variant: config.kernel_variant.clone(),
        lto_level: config.lto_type.to_string(),
        build_timestamp: timestamp,
        workspace_root: workspace.canonicalize()?,
        source_dir: workspace.join("src/linux-6.19.0"),  // Determined dynamically
        pkgver: config.version.clone(),
        pkgrel: "1".to_string(),
        profile_suffix: format!("-goatd-{}", config.profile.to_lowercase()),
    };
    
    // Write initial metadata (kernelrelease will be empty)
    let mpl_path = workspace.join(".goatd_metadata");
    write_mpl_file(&mpl_path, &metadata)?;
    
    Ok(MPL { path: mpl_path, metadata })
}
```

### 3.2 Phase 2: Kernel Build Executes

**Actor**: makepkg / kernel build system

**Behavior**: Unchanged—build proceeds normally

**Note**: The `.goatd_metadata` file is NOT read during build; it's purely for version propagation

### 3.3 Phase 3: Kernelrelease Captured and MPL Updated

**Actor**: Rust Executor ([`src/orchestrator/executor.rs`](src/orchestrator/executor.rs))

**Trigger**: Build completes successfully

**Action**:
```rust
async fn capture_and_update_mpl(
    kernel_path: &Path,
    mpl: &mut MPL,
) -> Result<(), BuildError> {
    // 1. Capture kernelrelease from build tree
    let kernelrelease = capture_kernelrelease(kernel_path)?;
    
    // 2. Update MPL metadata
    mpl.metadata.kernel_release = kernelrelease.clone();
    mpl.metadata.source_dir = kernel_path.to_path_buf();
    mpl.metadata.build_timestamp = Utc::now().to_rfc3339();
    
    // 3. Atomically write updated MPL
    let temp_path = mpl.path.with_extension(".tmp");
    write_mpl_file(&temp_path, &mpl.metadata)?;
    std::fs::rename(&temp_path, &mpl.path)?;  // Atomic replacement
    
    eprintln!("[MPL] Updated with kernelrelease: {}", kernelrelease);
    Ok(())
}
```

### 3.4 Phase 4: PKGBUILD Pulls Version from MPL

**Actor**: PKGBUILD `package()` and `_package_*()` functions

**Mechanism**: Source the MPL file to import all `GOATD_*` variables

**PKGBUILD Integration**:
```bash
# At the start of package() function
# CRITICAL: Source MPL from workspace root using absolute path
if [[ -z "${GOATD_WORKSPACE_ROOT}" ]]; then
    # Fallback: Detect workspace root from PKGBUILD location
    _ws_root="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
else
    _ws_root="${GOATD_WORKSPACE_ROOT}"
fi

# Source MPL to get kernel version
if [[ -f "${_ws_root}/.goatd_metadata" ]]; then
    # shellcheck source=/dev/null
    source "${_ws_root}/.goatd_metadata"
    echo "[MPL] Loaded kernelrelease: ${GOATD_KERNELRELEASE}" >&2
else
    echo "[MPL] ERROR: .goatd_metadata not found at ${_ws_root}" >&2
    echo "[MPL] Falling back to .kernelrelease discovery" >&2
    # Fallback to legacy discovery
fi

# Use GOATD_KERNELRELEASE for module directory creation
_actual_ver="${GOATD_KERNELRELEASE}"
mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"
```

### 3.5 Phase 5: Post-Install Verification

**Actor**: Rust Verification System ([`src/system/verification.rs`](src/system/verification.rs))

**Mechanism**: Compare installed kernel version against MPL metadata

```rust
fn verify_kernel_against_mpl(
    installed_version: &str,
    mpl_path: &Path,
) -> Result<VerificationResult, Error> {
    // 1. Read MPL file
    let mpl = read_mpl_file(mpl_path)?;
    
    // 2. Compare installed vs expected
    if installed_version == mpl.kernel_release {
        Ok(VerificationResult::Match {
            installed: installed_version.to_string(),
            expected: mpl.kernel_release,
            build_id: mpl.build_id,
        })
    } else {
        Ok(VerificationResult::Mismatch {
            installed: installed_version.to_string(),
            expected: mpl.kernel_release,
            build_id: mpl.build_id,
        })
    }
}
```

---

## 4. Cross-Mount Strategy

### 4.1 The Workspace-Anchor Pattern

**Problem**: When workspace is on a different mount (e.g., `/mnt/Optane/goatd`), the PKGBUILD's `$srcdir` may differ from the workspace root.

**Solution**: Always anchor to workspace root, not current directory.

```bash
# WRONG (fragile - relies on PWD)
source "../../.goatd_metadata"  # Fails if cwd changes

# RIGHT (robust - uses absolute path from MPL itself)
source "${GOATD_WORKSPACE_ROOT}/.goatd_metadata"
```

### 4.2 MPL Guarantees

| Guarantee | Mechanism |
|-----------|-----------|
| **Mount Transparency** | Workspace root is the anchor; mount boundaries are irrelevant |
| **Fakeroot Survival** | MPL file is on filesystem, not dependent on environment variables |
| **Atomic Updates** | Write to temp file, rename to replace (prevents partial reads) |
| **Idempotent Reads** | Sourcing a shell file multiple times is safe |

### 4.3 Directory Structure Example

```
/mnt/Optane/goatd/                          ← Workspace Root (MPL anchor)
├── .goatd_metadata                         ← MPL file (single source of truth)
├── src/
│   └── linux-6.19.0/                      ← Kernel source (build location)
│       ├── .config
│       ├── .kernelrelease                 ← Build artifact (transient)
│       └── include/config/kernel.release  ← Kernel's internal version
├── pkgbuilds/
│   └── source/
│       └── PKGBUILD                       ← References ${GOATD_WORKSPACE_ROOT}/.goatd_metadata
└── build/                                  ← makepkg working directory
    └── linux-6.19.0-1-x86_64.pkg.tar.zst
```

---

## 5. Implementation Plan

### 5.1 Rust Side Changes

| Component | Change | Priority |
|-----------|--------|----------|
| `src/orchestrator/executor.rs` | Add `MPLManager` struct for metadata file handling | P0 |
| `src/kernel/patcher.rs` | Remove 5-level fallback shell code, replace with MPL sourcing | P0 |
| `src/system/verification.rs` | Add MPL-based verification logic | P1 |
| `src/models.rs` | Add `MPLMetadata` struct | P1 |

### 5.2 PKGBUILD Changes

| Component | Change | Priority |
|-----------|--------|----------|
| `pkgbuilds/source/PKGBUILD` | Add MPL sourcing at start of `package()` and `_package_*()` | P0 |
| `pkgbuilds/binary/PKGBUILD` | Add MPL sourcing for verification | P1 |

### 5.3 Migration Path

1. **Phase 1**: Deploy MPL alongside existing fallback (dual-write)
2. **Phase 2**: PKGBUILD tries MPL first, falls back to legacy if missing
3. **Phase 3**: Remove legacy fallback, MPL becomes required

---

## 6. Best Practices and Future-Proofing

### 6.1 Invariant Guarantees

1. **Exactly one MPL per workspace**: Prevents version confusion
2. **MPL never deleted mid-session**: Atomic rename ensures consistency
3. **All paths absolute in MPL**: Never relative, always resolvable
4. **MPL is source of truth**: PKGBUILD pulls, never patches

### 6.2 Extensibility

The MPL format supports future fields without breaking changes:

```bash
# Future extension: Add new field
GOATD_SECURE_BOOT_ENABLED="1"

# Old PKGBUILDs ignore unknown fields
# New PKGBUILDs can use new fields
```

### 6.3 Workspace Migration

If user moves workspace from one mount to another:

1. Copy entire workspace directory (MPL travels with it)
2. MPL's `GOATD_WORKSPACE_ROOT` is updated on next build
3. No manual intervention required

---

## 7. Comparison: Old vs New

| Aspect | Old System | New MPL System |
|--------|-----------|----------------|
| **Version Discovery** | 5-level fallback | Single MPL file source |
| **Cross-Mount Support** | Fragile path propagation | Workspace-anchor pattern |
| **Fakeroot Compatibility** | Environment variable fallback | Filesystem-based (works) |
| **Shell Code Complexity** | ~50 lines of fallback logic | 3-line MPL source |
| **Verification** | Manual comparison | MPL-based automatic |
| **Debuggability** | Scattered fallback logs | Single MPL file |
| **Future-Proofing** | Ad-hoc additions | Structured schema |

---

## 8. Files Modified

| File | Change Type | Description |
|------|-------------|-------------|
| `src/orchestrator/executor.rs` | Modify | Add MPLManager, update `capture_and_save_kernelrelease` |
| `src/kernel/patcher.rs` | Modify | Remove 5-level fallback, add MPL sourcing injection |
| `src/system/verification.rs` | Modify | Add MPL-based verification |
| `src/models.rs` | Add | `MPLMetadata` struct |
| `pkgbuilds/source/PKGBUILD` | Modify | Add MPL sourcing at function start |
| `pkgbuilds/binary/PKGBUILD` | Modify | Add MPL sourcing for verification |

---

## 9. Conclusion

The Metadata Persistence Layer revolutionizes version propagation by:

1. **Centralizing** all version metadata in a single, well-defined file
2. **Anchoring** to the workspace root for cross-mount immunity
3. **Simplifying** PKGBUILD logic from 50+ lines of fallback to 3 lines of sourcing
4. **Enabling** robust post-install verification

This architecture future-proofs the project against directory and workspace changes, providing a clean, maintainable foundation for kernel build metadata management.

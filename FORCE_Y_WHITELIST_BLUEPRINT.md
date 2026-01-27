# FORCE_Y_WHITELIST_BLUEPRINT.md

**TO:** Orchestrator
**FROM:** Architect
**SUBJECT:** Transition to "Force-Builtin" (=y) Strategy for Critical Filesystems

## 1. OBJECTIVE
To eliminate the risk of `localmodconfig` and dependency resolution dropping critical filesystem drivers (`exfat`, `fat`, `vfat`) and their NLS requirements when they are not present in the active build environment's `modprobed.db`. This is achieved by forcing these drivers to become part of the core `vmlinux` binary (`=y`) instead of optional modules (`=m`).

## 2. DRIVER MAPPING & FORCE-Y TARGETS
All entries below MUST be transitioned from `=m` to `=y` in both Rust logic and Bash templates.

### 2.1 Filesystem Drivers
- `CONFIG_FAT_FS=y`
- `CONFIG_VFAT_FS=y`
- `CONFIG_EXFAT_FS=y`

### 2.2 Mandatory NLS Core (Dependencies)
- `CONFIG_NLS_UTF8=y`
- `CONFIG_NLS_ISO8859_1=y`
- `CONFIG_NLS_CP437=y`
- `CONFIG_NLS_ASCII=y`

## 3. ARCHITECTURAL CHANGES

### 3.1 Rust Logic Updates (`src/config/whitelist.rs`)
The `ESSENTIAL_DRIVERS` list remains the canonical source of truth for *what* is protected, but the `PHASE_G2_ENFORCER` logic must be updated to treat Filesystem/NLS entries differently from others (like USB or Storage which may remain as modules).

### 3.2 Template Updates (`src/kernel/patcher/templates.rs`)

#### 3.2.1 `WHITELIST_INJECTION` (Lines 935-1018)
Updated to inject `=y` for the critical filesystem block:
```bash
# Primary filesystems (MANDATORY BUILT-IN)
CONFIG_EXT4_FS=y
CONFIG_BTRFS_FS=y

# Additional filesystems (FORCED BUILT-IN)
CONFIG_FAT_FS=y
CONFIG_VFAT_FS=y
CONFIG_EXFAT_FS=y
CONFIG_EXFAT_DEFAULT_IOCHARSET="utf8"

# NLS support (FORCED BUILT-IN)
CONFIG_NLS_ASCII=y
CONFIG_NLS_CP437=y
CONFIG_NLS_UTF8=y
CONFIG_NLS_ISO8859_1=y
```

#### 3.2.2 `RESTORE_GOATD_WHITELIST_FUNCTION` (Lines 105-133)
Update the restoration logic to handle both `=m` and `=y` targets.
- **Improved Pattern**: `^CONFIG_...(=[ym]$| is not set)`
- **Enforcement logic**: Use a lookup table or naming convention to decide if a module gets `m` or `y`.
- **Proposed Logic**: If module is in `FORCE_Y_LIST` (exfat, vfat, fat, nls_*), use `=y`. Otherwise `=m`.

#### 3.2.3 `PHASE_G2_ENFORCER` (Lines 699-820)
- Update STEP 3.5 (Whitelist restoration) to specifically re-inject `=y` for filesystem drivers even if they were detected as modules (or not at all) by `localmodconfig`.
- This ensures that after `olddefconfig` runs, these drivers are locked into the core binary.

## 4. IMMUTABLE CORE ENFORCEMENT
The `PHASE_G2_ENFORCER` will now act as a "Built-in Enforcer" for the following:
1. `exfat`, `fat`, `vfat`
2. `nls_utf8`, `nls_iso8859_1`, `nls_cp437`, `nls_ascii`

Any attempt by `localmodconfig` to set these to `m` or comment them out will be overruled in the final `.config` before the `make` command is issued.

## 5. VERIFICATION PROTOCOL
Post-build forensics must verify:
1. `grep "CONFIG_EXFAT_FS=y" .config` returns success.
2. `grep "CONFIG_EXFAT_FS=m" .config` returns failure.
3. `vmlinux` binary size slightly increases (expected).
4. `modprobed.db` absence does not impact these drivers.

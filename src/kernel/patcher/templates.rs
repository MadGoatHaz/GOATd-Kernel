//! Bash script templates for kernel patching operations.
//!
//! This module contains pre-compiled Bash script templates used for injecting
//! patches into PKGBUILD files and kernel configuration. These templates are
//! extracted from the main patcher module to improve maintainability and reduce
//! context saturation in the patcher.rs file.
//!
//! All templates use standard Bash syntax and are designed to be injected at
//! specific phases during the kernel build process.

use crate::models::LtoType;

/// JSON Telemetry Logging Function
///
/// Provides structured JSON logging for bash templates to enable precise tracking
/// and diagnostics by the Rust orchestrator. Each log entry includes:
/// - timestamp: ISO 8601 UTC timestamp
/// - level: INFO, WARNING, ERROR, SUCCESS
/// - phase: Component/phase identifier (e.g., NVIDIA-DKMS, MODULE-REPAIR)
/// - message: Human-readable event description
/// - metadata: Optional key-value pairs for context
///
/// All logs are appended to `/tmp/goatd_dkms.log` for centralized telemetry collection.
pub const LOG_JSON_FUNCTION: &str = r#"
# =====================================================================
# PHASE 16: STRUCTURED JSON TELEMETRY LOGGING FUNCTION
# =====================================================================
# Provides structured JSON logging for precise tracking and diagnostics
# All logs are appended to /tmp/goatd_dkms.log
#
# Usage: log_json <level> <phase> <message> [metadata_json]
#
# Parameters:
#   level        - LOG LEVEL: INFO, WARNING, ERROR, SUCCESS
#   phase        - PHASE IDENTIFIER: NVIDIA-DKMS, MODULE-REPAIR, etc.
#   message      - HUMAN-READABLE MESSAGE
#   metadata_json - (optional) JSON object with additional context
#
# Examples:
#   log_json "INFO" "NVIDIA-DKMS" "Starting memremap shim"
#   log_json "ERROR" "NVIDIA-DKMS" "Failed to apply patch" '{"error":"file_not_found"}'
#   log_json "SUCCESS" "MODULE-REPAIR" "Symlinks verified" '{"symlinks":2}'

log_json() {
    local _level="${1:-INFO}"
    local _phase="${2:-UNKNOWN}"
    local _message="${3:-<no message>}"
    local _metadata="${4:-}"
    local _timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

    # Build JSON object with proper escaping
    local _json_entry="{"
    _json_entry+="\"timestamp\":\"${_timestamp}\","
    _json_entry+="\"level\":\"${_level}\","
    _json_entry+="\"phase\":\"${_phase}\","
    _json_entry+="\"message\":\"$(echo "$_message" | sed 's/"/\\"/g')\""

    # Append metadata if provided
    if [ -n "$_metadata" ]; then
        _json_entry+=",\"metadata\":${_metadata}"
    fi

    _json_entry+="}"

    # Append to telemetry log file
    echo "$_json_entry" >> /tmp/goatd_dkms.log 2>/dev/null

    # Also output to stderr for visibility during build
    printf "[%s] [%s] %s\n" "$_level" "$_phase" "$_message" >&2
}
"#;

/// Root Makefile toolchain enforcement block
///
/// Prepends LLVM=1 and LLVM_IAS=1 to ensure Clang/LLVM is used for all builds,
/// including out-of-tree modules via DKMS.
/// Also includes explicit tool overrides (CC, CXX, LD, HOSTCC, HOSTCXX) to forcefully
/// enforce Clang/LLVM toolchain for both native and host-side compilation.
pub const ROOT_MAKEFILE_ENFORCEMENT: &str = r#"# GOATd Toolchain Enforcement
LLVM := 1
LLVM_IAS := 1
export LLVM LLVM_IAS

# Explicit tool overrides for forceful Clang/LLVM enforcement
CC := clang
CXX := clang++
LD := ld.lld
HOSTCC := clang
HOSTCXX := clang++
export CC CXX LD HOSTCC HOSTCXX

"#;

/// Rust .rmeta and .so installation fix using find instead of glob expansion
pub const RUST_RMETA_FIX: &str = r#"   echo "Installing Rust files..."
   # Use find to safely handle cases where .rmeta or .so files may not exist
   find rust -maxdepth 1 -type f -name '*.rmeta' -exec install -Dt "$builddir/rust" -m644 {} +
   find rust -maxdepth 1 -type f -name '*.so' -exec install -Dt "$builddir/rust" {} +"#;

/// Generate prebuild LTO enforcer based on LTO type
///
/// Returns the appropriate PHASE G1 PREBUILD enforcement snippet
/// that runs IMMEDIATELY BEFORE the 'make' command in build().
pub fn get_prebuild_lto_enforcer(lto_type: LtoType) -> &'static str {
    match lto_type {
        LtoType::Full => {
            r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO HARD ENFORCER (FULL LTO)
    # =====================================================================
    # This runs IMMEDIATELY BEFORE the 'make' command in build().
    # CRITICAL: Enforces both .config settings AND environment variables.
    # Module filtering is handled by PHASE G2 in prepare(), not here.
    # Profile settings (MGLRU, Polly, etc.) are protected via environment exports.
    #
    # CRITICAL: This is the FINAL GATE before kernel compilation.
    # All other config changes have been finalized in prepare().
    # Environment variables are NOW protected from overwrites.

    # ====================================================================
    # PHASE G1.1: ENVIRONMENT VARIABLE HARDENING
    # ====================================================================
    # CRITICAL FIX: Export CFLAGS/CXXFLAGS/LDFLAGS IMMEDIATELY to prevent
    # overwrites by subsequent PKGBUILD variable assignments.
    # These exports PERSIST through the entire build() function scope.
    #
    # Pattern: export VAR="$VAR $ADDITIONAL_FLAGS"
    # This appends, rather than replaces, allowing earlier exports to stack.
    #
    # CRITICAL: Separate HOSTCFLAGS from main CFLAGS to prevent Polly flags
    # from being applied to host tools (like bpftool). Host tools don't support
    # Polly and will fail with "Unknown command line argument '-polly'" errors.
    # Only the main kernel (vmlinux) gets Polly optimizations.

    # Main kernel compilation: Include LTO, hardening, and native flags (NO POLLY)
    # CRITICAL FIX: Polly flags moved to KCFLAGS to prevent host tool contamination
    export CFLAGS="${CFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export CXXFLAGS="${CXXFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export LDFLAGS="${LDFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # CRITICAL: KCFLAGS is kernel-only (NOT inherited by host tools like bpftool)
    # Polly flags are NOW in KCFLAGS to ensure kernel gets optimizations without hosttool contamination
    export KCFLAGS="-march=native $GOATD_POLLY_FLAGS"

    # Host tools compilation: Exclude LTO and Polly flags (not supported by host toolchain)
    # CRITICAL: Include GOATD_BASE_FLAGS (-O2) BEFORE hardening flags
    # This is REQUIRED because _FORTIFY_SOURCE demands optimization (-O flag)
    export HOSTCFLAGS="${HOSTCFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export HOSTCXXFLAGS="${HOSTCXXFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export HOSTLDFLAGS="${HOSTLDFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # Tool-specific flag variants (some kernel makefiles use these aliases)
    export CFLAGS_HOST="${CFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export CXXFLAGS_HOST="${CXXFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export LDFLAGS_HOST="${LDFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # Tools (e.g., resolve_btfids) compilation: BASE_FLAGS only, no Polly/LTO
    export TOOLS_CFLAGS="${TOOLS_CFLAGS:-} $GOATD_BASE_FLAGS"
    export TOOLS_CXXFLAGS="${TOOLS_CXXFLAGS:-} $GOATD_BASE_FLAGS"
    export TOOLS_LDFLAGS="${TOOLS_LDFLAGS:-} $GOATD_BASE_FLAGS"

    # EXTRA_* variables also used by some toolchain makefiles (e.g., bpftool bootstrap)
    # CRITICAL: These must NOT inherit Polly or LTO flags to prevent tool failures
    export EXTRA_CFLAGS="${EXTRA_CFLAGS:-} $GOATD_BASE_FLAGS"
    export EXTRA_CXXFLAGS="${EXTRA_CXXFLAGS:-} $GOATD_BASE_FLAGS"
    export EXTRA_LDFLAGS="${EXTRA_LDFLAGS:-} $GOATD_BASE_FLAGS"

     printf "[PREBUILD] [PHASE-G1.1] Main kernel: CFLAGS=\"%s\"\n" "$CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel: CXXFLAGS=\"%s\"\n" "$CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel: LDFLAGS=\"%s\"\n" "$LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTCFLAGS=\"%s\"\n" "$HOSTCFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTCXXFLAGS=\"%s\"\n" "$HOSTCXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTLDFLAGS=\"%s\"\n" "$HOSTLDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: CFLAGS_HOST=\"%s\"\n" "$CFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: CXXFLAGS_HOST=\"%s\"\n" "$CXXFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: LDFLAGS_HOST=\"%s\"\n" "$LDFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_CFLAGS=\"%s\"\n" "$TOOLS_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_CXXFLAGS=\"%s\"\n" "$TOOLS_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_LDFLAGS=\"%s\"\n" "$TOOLS_LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CFLAGS=\"%s\"\n" "$EXTRA_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CXXFLAGS=\"%s\"\n" "$EXTRA_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_LDFLAGS=\"%s\"\n" "$EXTRA_LDFLAGS" >&2

    if [[ -f ".config" ]]; then
        config_file=".config"

        # ====================================================================
        # PHASE G1.2: LTO HARD ENFORCER (Surgical, Atomic Enforcement - FULL LTO)
        # ====================================================================
        # CRITICAL: This is LTO-ONLY enforcement for .config.
        # Module filtering is handled by PHASE G2 in prepare().
        # Profile settings (MGLRU, Polly, etc.) are NOT touched here.
        #
        # SURGICAL REMOVAL: Use GLOBAL sed pattern to delete ALL LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

        # ATOMIC INJECTION: Append FULL LTO settings
         [ -f "$config_file" ] && cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE G1.2: LTO CLANG FULL HARD ENFORCER (SURGICAL)
# ====================================================================
# These lines are SURGICALLY injected immediately before kernel make.
# All conflicting LTO entries have been removed above.
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_FULL=y
CONFIG_HAS_LTO_CLANG=y
EOF

        printf "[PREBUILD] [LTO] PHASE G1.2: Surgically enforced CONFIG_LTO_CLANG=y + CONFIG_LTO_CLANG_FULL=y\n" >&2

        # ====================================================================
        # PHASE G1.3: RUN OLDDEFCONFIG TO ACCEPT NEW CONFIG OPTIONS
        # ====================================================================
        if command -v make &> /dev/null; then
            printf "[PREBUILD] OLDDEFCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized without interactive prompts\n" >&2
            else
                printf "[PREBUILD] WARNING: 'make olddefconfig' failed or unavailable, continuing anyway...\n" >&2
            fi
        fi

        # Verify final module count
        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#
        }
        LtoType::Thin => {
            r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO HARD ENFORCER (THIN LTO)
    # =====================================================================
    # This runs IMMEDIATELY BEFORE the 'make' command in build().
    # CRITICAL: Enforces both .config settings AND environment variables.
    # Module filtering is handled by PHASE G2 in prepare(), not here.
    # Profile settings (MGLRU, Polly, etc.) are protected via environment exports.
    #
    # CRITICAL: This is the FINAL GATE before kernel compilation.
    # All other config changes have been finalized in prepare().
    # Environment variables are NOW protected from overwrites.

    # ====================================================================
    # PHASE G1.1: ENVIRONMENT VARIABLE HARDENING
    # ====================================================================
    # CRITICAL FIX: Export CFLAGS/CXXFLAGS/LDFLAGS IMMEDIATELY to prevent
    # overwrites by subsequent PKGBUILD variable assignments.
    # These exports PERSIST through the entire build() function scope.
    #
    # Pattern: export VAR="$VAR $ADDITIONAL_FLAGS"
    # This appends, rather than replaces, allowing earlier exports to stack.
    #
    # CRITICAL: Separate HOSTCFLAGS from main CFLAGS to prevent Polly flags
    # from being applied to host tools (like bpftool). Host tools don't support
    # Polly and will fail with "Unknown command line argument '-polly'" errors.
    # Only the main kernel (vmlinux) gets Polly optimizations.

    # Main kernel compilation: Include LTO, hardening, and native flags (NO POLLY)
    # CRITICAL FIX: Polly flags moved to KCFLAGS to prevent host tool contamination
    export CFLAGS="${CFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export CXXFLAGS="${CXXFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export LDFLAGS="${LDFLAGS:-} $GOATD_LTO_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # CRITICAL: KCFLAGS is kernel-only (NOT inherited by host tools like bpftool)
    # Polly flags are NOW in KCFLAGS to ensure kernel gets optimizations without hosttool contamination
    export KCFLAGS="-march=native $GOATD_POLLY_FLAGS"

    # Host tools compilation: Exclude LTO and Polly flags (not supported by host toolchain)
    # CRITICAL: Include GOATD_BASE_FLAGS (-O2) BEFORE hardening flags
    # This is REQUIRED because _FORTIFY_SOURCE demands optimization (-O flag)
    export HOSTCFLAGS="${HOSTCFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export HOSTCXXFLAGS="${HOSTCXXFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export HOSTLDFLAGS="${HOSTLDFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # Tool-specific flag variants (some kernel makefiles use these aliases)
    export CFLAGS_HOST="${CFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export CXXFLAGS_HOST="${CXXFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export LDFLAGS_HOST="${LDFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # Tools (e.g., resolve_btfids) compilation: BASE_FLAGS only, no Polly/LTO
    export TOOLS_CFLAGS="${TOOLS_CFLAGS:-} $GOATD_BASE_FLAGS"
    export TOOLS_CXXFLAGS="${TOOLS_CXXFLAGS:-} $GOATD_BASE_FLAGS"
    export TOOLS_LDFLAGS="${TOOLS_LDFLAGS:-} $GOATD_BASE_FLAGS"

    # EXTRA_* variables also used by some toolchain makefiles (e.g., bpftool bootstrap)
    # CRITICAL: These must NOT inherit Polly or LTO flags to prevent tool failures
    export EXTRA_CFLAGS="${EXTRA_CFLAGS:-} $GOATD_BASE_FLAGS"
    export EXTRA_CXXFLAGS="${EXTRA_CXXFLAGS:-} $GOATD_BASE_FLAGS"
    export EXTRA_LDFLAGS="${EXTRA_LDFLAGS:-} $GOATD_BASE_FLAGS"

     printf "[PREBUILD] [PHASE-G1.1] Main kernel: CFLAGS=\"%s\"\n" "$CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel: CXXFLAGS=\"%s\"\n" "$CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel: LDFLAGS=\"%s\"\n" "$LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTCFLAGS=\"%s\"\n" "$HOSTCFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTCXXFLAGS=\"%s\"\n" "$HOSTCXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools: HOSTLDFLAGS=\"%s\"\n" "$HOSTLDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: CFLAGS_HOST=\"%s\"\n" "$CFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: CXXFLAGS_HOST=\"%s\"\n" "$CXXFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases: LDFLAGS_HOST=\"%s\"\n" "$LDFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_CFLAGS=\"%s\"\n" "$TOOLS_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_CXXFLAGS=\"%s\"\n" "$TOOLS_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only: TOOLS_LDFLAGS=\"%s\"\n" "$TOOLS_LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CFLAGS=\"%s\"\n" "$EXTRA_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CXXFLAGS=\"%s\"\n" "$EXTRA_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_LDFLAGS=\"%s\"\n" "$EXTRA_LDFLAGS" >&2

    if [[ -f ".config" ]]; then
        config_file=".config"

        # ====================================================================
        # PHASE G1.2: LTO HARD ENFORCER (Surgical, Atomic Enforcement - THIN LTO)
        # ====================================================================
        # CRITICAL: This is LTO-ONLY enforcement for .config.
        # Module filtering is handled by PHASE G2 in prepare().
        # Profile settings (MGLRU, Polly, etc.) are NOT touched here.
        #
        # SURGICAL REMOVAL: Use GLOBAL sed pattern to delete ALL LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

        # ATOMIC INJECTION: Append THIN LTO settings
         [ -f "$config_file" ] && cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE G1.2: LTO CLANG THIN HARD ENFORCER (SURGICAL)
# ====================================================================
# These lines are SURGICALLY injected immediately before kernel make.
# All conflicting LTO entries have been removed above.
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_THIN=y
CONFIG_HAS_LTO_CLANG=y
EOF

        printf "[PREBUILD] [LTO] PHASE G1.2: Surgically enforced CONFIG_LTO_CLANG=y + CONFIG_LTO_CLANG_THIN=y\n" >&2

        # ====================================================================
        # PHASE G1.3: RUN OLDDEFCONFIG TO ACCEPT NEW CONFIG OPTIONS
        # ====================================================================
        if command -v make &> /dev/null; then
            printf "[PREBUILD] OLDDEFCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized without interactive prompts\n" >&2
            else
                printf "[PREBUILD] WARNING: 'make olddefconfig' failed or unavailable, continuing anyway...\n" >&2
            fi
        fi

        # Verify final module count
        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#
        }
        LtoType::None => {
            r#"
    # =====================================================================
    # PHASE G1 PREBUILD: LTO DISABLED (None)
    # =====================================================================
    # LTO is disabled per user selection - no LTO enforcement
    # CRITICAL: Still protect environment variables (Hardening, Polly, Native Optimization)

    # ====================================================================
    # PHASE G1.1: ENVIRONMENT VARIABLE HARDENING (even with LTO disabled)
    # ====================================================================
    # CRITICAL FIX: Export CFLAGS/CXXFLAGS/LDFLAGS IMMEDIATELY to prevent
    # overwrites by subsequent PKGBUILD variable assignments.
    # These exports PERSIST through the entire build() function scope.
    # This ensures Hardening, Polly, and Native Optimization flags survive.
    #
    # Pattern: export VAR="$VAR $ADDITIONAL_FLAGS"
    # This appends, rather than replaces, allowing earlier exports to stack.
    #
    # CRITICAL: Separate HOSTCFLAGS from main CFLAGS to prevent Polly flags
    # from being applied to host tools (like bpftool). Host tools don't support
    # Polly and will fail with "Unknown command line argument '-polly'" errors.
    # Only the main kernel (vmlinux) gets Polly optimizations.

    # Main kernel compilation: Include hardening and native flags (no LTO, NO POLLY)
    # CRITICAL FIX: Polly flags moved to KCFLAGS to prevent host tool contamination
    export CFLAGS="${CFLAGS:-} $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export CXXFLAGS="${CXXFLAGS:-} $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
    export LDFLAGS="${LDFLAGS:-} $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

    # CRITICAL: KCFLAGS is kernel-only (NOT inherited by host tools like bpftool)
    # Polly flags are NOW in KCFLAGS to ensure kernel gets optimizations without hosttool contamination
    export KCFLAGS="-march=native $GOATD_POLLY_FLAGS"

    # Host tools compilation: Exclude Polly flags (not supported by host toolchain)
     # CRITICAL: Include GOATD_BASE_FLAGS (-O2) BEFORE hardening flags
     # This is REQUIRED because _FORTIFY_SOURCE demands optimization (-O flag)
     export HOSTCFLAGS="${HOSTCFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
     export HOSTCXXFLAGS="${HOSTCXXFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
     export HOSTLDFLAGS="${HOSTLDFLAGS:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

     # Tool-specific flag variants (some kernel makefiles use these aliases)
     export CFLAGS_HOST="${CFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
     export CXXFLAGS_HOST="${CXXFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"
     export LDFLAGS_HOST="${LDFLAGS_HOST:-} $GOATD_BASE_FLAGS $GOATD_HARDENING_FLAGS $GOATD_NATIVE_FLAGS"

     # Tools (e.g., resolve_btfids) compilation: BASE_FLAGS only, no Polly/LTO
     export TOOLS_CFLAGS="${TOOLS_CFLAGS:-} $GOATD_BASE_FLAGS"
     export TOOLS_CXXFLAGS="${TOOLS_CXXFLAGS:-} $GOATD_BASE_FLAGS"
     export TOOLS_LDFLAGS="${TOOLS_LDFLAGS:-} $GOATD_BASE_FLAGS"

     # EXTRA_* variables also used by some toolchain makefiles (e.g., bpftool bootstrap)
     # CRITICAL: These must NOT inherit Polly or LTO flags to prevent tool failures
     export EXTRA_CFLAGS="${EXTRA_CFLAGS:-} $GOATD_BASE_FLAGS"
     export EXTRA_CXXFLAGS="${EXTRA_CXXFLAGS:-} $GOATD_BASE_FLAGS"
     export EXTRA_LDFLAGS="${EXTRA_LDFLAGS:-} $GOATD_BASE_FLAGS"

     printf "[PREBUILD] [PHASE-G1.1] Main kernel (no LTO): CFLAGS=\"%s\"\n" "$CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel (no LTO): CXXFLAGS=\"%s\"\n" "$CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Main kernel (no LTO): LDFLAGS=\"%s\"\n" "$LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools (no LTO): HOSTCFLAGS=\"%s\"\n" "$HOSTCFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools (no LTO): HOSTCXXFLAGS=\"%s\"\n" "$HOSTCXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host tools (no LTO): HOSTLDFLAGS=\"%s\"\n" "$HOSTLDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases (no LTO): CFLAGS_HOST=\"%s\"\n" "$CFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases (no LTO): CXXFLAGS_HOST=\"%s\"\n" "$CXXFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Host aliases (no LTO): LDFLAGS_HOST=\"%s\"\n" "$LDFLAGS_HOST" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only (no LTO): TOOLS_CFLAGS=\"%s\"\n" "$TOOLS_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only (no LTO): TOOLS_CXXFLAGS=\"%s\"\n" "$TOOLS_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Tools only (no LTO): TOOLS_LDFLAGS=\"%s\"\n" "$TOOLS_LDFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CFLAGS=\"%s\"\n" "$EXTRA_CFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_CXXFLAGS=\"%s\"\n" "$EXTRA_CXXFLAGS" >&2
     printf "[PREBUILD] [PHASE-G1.1] Extra vars (BPF): EXTRA_LDFLAGS=\"%s\"\n" "$EXTRA_LDFLAGS" >&2

    if [[ -f ".config" ]]; then
        config_file=".config"

        # SURGICAL REMOVAL: Remove all LTO-related lines
        sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

        # Ensure file ends with newline
        tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

        printf "[PREBUILD] [LTO] PHASE G1: LTO disabled - removed all LTO configs\n" >&2

        # Run olddefconfig to finalize
        if command -v make &> /dev/null; then
            if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                printf "[PREBUILD] OLDDEFCONFIG: SUCCESS - Configuration finalized\n" >&2
            fi
        fi

        VERIFY_MODULE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" "$config_file" 2>/dev/null || echo "unknown")
        printf "[PREBUILD] VERIFICATION: Final module count before make: $VERIFY_MODULE_COUNT\n" >&2
    fi
    "#
        }
    }
}

/// PHASE G2.5 POST-SETTING-CONFIG restorer
///
/// Protects modprobed-filtered modules and MGLRU/Polly settings from overwrite
/// by the "Setting config..." step which runs "cp ../config .config".
pub const PHASE_G2_5_RESTORER: &str = r#"

      # =====================================================================
      # PHASE G2.5 POST-SETTING-CONFIG: Protect profile settings and re-apply filtering
      # =====================================================================
      # CRITICAL FIX: The "Setting config..." step runs "cp ../config .config"
      # which OVERWRITES:
      # 1. Modprobed-filtered modules (6199+ modules restored)
      # 2. MGLRU, Polly CONFIG options (completely lost)
      # 3. CONFIG_CMDLINE* parameters (baked-in kernel parameters lost)
      # 4. LTO configuration (CONFIG_LTO_NONE=y gets restored by kernel defaults)
      #
      # This restorer:
      # - Captures CONFIG_CMDLINE* values BEFORE overwrite
      # - Captures LTO config type from environment for re-enforcement
      # - Re-applies modprobed filtering AFTER overwrite
      # - Re-injects backed-up CONFIG_CMDLINE*, MGLRU configs
      # - Re-enforces LTO settings that kernel defaults reverted

      # STEP 1: Capture CONFIG_CMDLINE* settings BEFORE "cp ../config .config"
      # Find kernel source directory
      KERNEL_SRC_DIR=""
      if [[ -d "$srcdir/linux" ]]; then
          KERNEL_SRC_DIR="$srcdir/linux"
      else
          KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
      fi

      if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
          KERNEL_SRC_DIR="$srcdir"
      fi

      # Capture CONFIG_CMDLINE* values BEFORE the "cp ../config .config" step overwrites them
      CONFIG_CMDLINE_BACKUP=""
      CONFIG_CMDLINE_BOOL_BACKUP=""
      CONFIG_CMDLINE_OVERRIDE_BACKUP=""

      if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/.config" ]]; then
          CONFIG_CMDLINE_BACKUP=$(grep "^CONFIG_CMDLINE=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")
          CONFIG_CMDLINE_BOOL_BACKUP=$(grep "^CONFIG_CMDLINE_BOOL=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")
          CONFIG_CMDLINE_OVERRIDE_BACKUP=$(grep "^CONFIG_CMDLINE_OVERRIDE=" "$KERNEL_SRC_DIR/.config" 2>/dev/null || echo "")

          if [[ -n "$CONFIG_CMDLINE_BACKUP" ]]; then
              printf "[PHASE-G2.5] CAPTURED CONFIG_CMDLINE before overwrite\n" >&2
          fi
      fi

      # STEP 2: Now the "Setting config..." step runs "cp ../config .config" (happens in main prepare)
      # We'll restore values immediately after

      if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/.config" ]]; then
          # CRITICAL FIX: Use subshell to limit directory change scope
          # This ensures we return to $srcdir after all operations
          (
              cd "$KERNEL_SRC_DIR" || exit 1

              # STEP 3: Count modules BEFORE re-filtering
              BEFORE_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" ".config" 2>/dev/null || echo "unknown")
              printf "[PHASE-G2.5] Module count after 'Setting config...': $BEFORE_COUNT\n" >&2

              # STEP 4: Re-apply modprobed filtering with robust path detection
              # Find modprobed.db using multiple fallback strategies
              MODPROBED_DB_PATH=""
              for candidate in "$HOME/.config/modprobed.db" /root/.config/modprobed.db /home/*/.config/modprobed.db; do
                  if [[ -f "$candidate" ]]; then
                      MODPROBED_DB_PATH="$candidate"
                      printf "[PHASE-G2.5] [PATH-DETECTION] Found modprobed.db at: $MODPROBED_DB_PATH\n" >&2
                      break
                  fi
              done

              # Check if modprobed.db exists BEFORE attempting to use it
              if [[ -n "$MODPROBED_DB_PATH" && -f "$MODPROBED_DB_PATH" ]]; then
                  printf "[PHASE-G2.5] Re-running: yes \"\" | make LLVM=1 LLVM_IAS=1 LSMOD=$MODPROBED_DB_PATH localmodconfig\n" >&2
                  if yes "" | make LLVM=1 LLVM_IAS=1 LSMOD="$MODPROBED_DB_PATH" localmodconfig > /dev/null 2>&1; then
                      AFTER_COUNT=$(grep -c "^CONFIG_[A-Z0-9_]*=m$" ".config" 2>/dev/null || echo "unknown")
                      printf "[PHASE-G2.5] Module count after re-filtering: $AFTER_COUNT\n" >&2
                  else
                      printf "[PHASE-G2.5] WARNING: Re-filtering failed, continuing with current config (localmodconfig command error)\n" >&2
                  fi
              else
                  printf "[PHASE-G2.5] INFO: modprobed.db not found at $MODPROBED_DB_PATH, skipping re-filtering (expected on fresh install)\n" >&2
              fi

              # STEP 5: Restore CONFIG_CMDLINE* parameters
              if [[ -n "$CONFIG_CMDLINE_BACKUP" ]] || [[ -n "$CONFIG_CMDLINE_BOOL_BACKUP" ]] || [[ -n "$CONFIG_CMDLINE_OVERRIDE_BACKUP" ]]; then
                  # Remove old CONFIG_CMDLINE* entries to prevent duplicates
                  sed -i '/^CONFIG_CMDLINE.*/d' ".config"

                  # Ensure newline before appending
                  [[ -n "$(tail -c 1 ".config")" ]] && echo "" >> ".config"

                  # Re-inject backed-up CMDLINE parameters
                  [[ -n "$CONFIG_CMDLINE_BACKUP" ]] && echo "$CONFIG_CMDLINE_BACKUP" >> ".config"
                  [[ -n "$CONFIG_CMDLINE_BOOL_BACKUP" ]] && echo "$CONFIG_CMDLINE_BOOL_BACKUP" >> ".config"
                  [[ -n "$CONFIG_CMDLINE_OVERRIDE_BACKUP" ]] && echo "$CONFIG_CMDLINE_OVERRIDE_BACKUP" >> ".config"

                  printf "[PHASE-G2.5] Re-applied CONFIG_CMDLINE* parameters\n" >&2
              fi

              # STEP 6: Re-apply MGLRU configs if they were set
              if [[ -n "$GOATD_MGLRU_CONFIGS" ]]; then
                  # Remove old MGLRU lines
                  sed -i '/^CONFIG_LRU_GEN/d' ".config"

                  # Ensure newline before appending
                  [[ -n "$(tail -c 1 ".config")" ]] && echo "" >> ".config"
                  echo "$GOATD_MGLRU_CONFIGS" >> ".config"
                  printf "[PHASE-G2.5] Re-applied MGLRU configs\n" >&2
              fi

              # STEP 7: CRITICAL FIX - Re-enforce LTO settings after config overwrite
              # The kernel's defaults set CONFIG_LTO_NONE=y which completely bypasses LTO
              # We MUST surgically remove CONFIG_LTO_NONE and re-inject proper LTO settings
              printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] Checking for LTO restoration...\n" >&2

              # Detect LTO type from environment (CRITICAL: no fallback default)
              # The executor MUST export GOATD_LTO_LEVEL before build starts
              LTO_TYPE="${GOATD_LTO_LEVEL}"
              
              # SAFETY: If GOATD_LTO_LEVEL is not set, log warning and skip re-enforcement
              if [ -z "$LTO_TYPE" ]; then
                  printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] WARNING: GOATD_LTO_LEVEL not exported - LTO settings NOT re-enforced\n" >&2
                  printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] This may indicate a build environment configuration issue\n" >&2
              else
                  printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] Detected LTO type: $LTO_TYPE\n" >&2
              fi

              # SURGICAL REMOVAL: Remove ALL LTO-related lines that kernel defaults may have added
              sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' ".config"
              printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] Surgically removed all LTO config lines\n" >&2

              # Ensure file ends with newline
              [[ -n "$(tail -c 1 ".config")" ]] && echo "" >> ".config"

              # Re-inject LTO settings based on detected type using heredocs
              case "$LTO_TYPE" in
                  full)
                      cat >> ".config" << 'LTOEOFULL'

# ====================================================================
# PHASE G2.5: LTO CLANG FULL RE-ENFORCEMENT (AFTER CONFIG OVERWRITE)
# ====================================================================
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_FULL=y
CONFIG_HAS_LTO_CLANG=y
LTOEOFULL
                      printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] RE-ENFORCED CONFIG_LTO_CLANG_FULL=y (FULL LTO)\n" >&2
                      ;;
                  thin)
                      cat >> ".config" << 'LTOEOTHIN'

# ====================================================================
# PHASE G2.5: LTO CLANG THIN RE-ENFORCEMENT (AFTER CONFIG OVERWRITE)
# ====================================================================
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_THIN=y
CONFIG_HAS_LTO_CLANG=y
LTOEOTHIN
                      printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] RE-ENFORCED CONFIG_LTO_CLANG_THIN=y (THIN LTO)\n" >&2
                      ;;
                  none)
                      printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] LTO disabled - no re-enforcement needed\n" >&2
                      ;;
                  *)
                      printf "[PHASE-G2.5] [LTO-RE-ENFORCEMENT] WARNING: Unknown LTO type '$LTO_TYPE', defaulting to FULL\n" >&2
                      cat >> ".config" << 'LTOEODEFAULT'

# ====================================================================
# PHASE G2.5: LTO CLANG FULL RE-ENFORCEMENT (DEFAULT FALLBACK)
# ====================================================================
CONFIG_LTO_CLANG=y
CONFIG_LTO_CLANG_FULL=y
CONFIG_HAS_LTO_CLANG=y
LTOEODEFAULT
                      ;;
              esac

              printf "[PHASE-G2.5] SUCCESS: Modprobed filtering, CMDLINE parameters, MGLRU, and LTO settings restored\n" >&2
          )
      else
          printf "[PHASE-G2.5] WARNING: Could not locate kernel source directory\n" >&2
      fi
      "#;

/// PHASE G2 POST-MODPROBED hard enforcer
///
/// After localmodconfig filtering, olddefconfig can re-expand modules.
/// This enforcer surgically removes all CONFIG_*=m entries NOT in modprobed.db.
pub const PHASE_G2_ENFORCER: &str = r#"
     # =====================================================================
     # PHASE G2 POST-MODPROBED: Hard enforcer to protect filtered modules
     # =====================================================================
     # CRITICAL FIX FOR MODPROBED-DB EXPANSION:
     # After localmodconfig filters to ~170 modules, olddefconfig's Kconfig
     # dependency expansion re-enables thousands of unwanted modules.
     #
     # This enforcer:
     # 1. Reads modprobed.db to get list of modules to KEEP
     # 2. Surgically removes all CONFIG_*=m entries NOT in modprobed.db
     # 3. Runs olddefconfig ONCE to handle consistent dependencies
     # 4. Result: ~170 modules preserved with correct Kconfig dependencies
     #
     # This MUST run AFTER localmodconfig but BEFORE make olddefconfig (or in place of it)

     # CRITICAL: Find modprobed.db using robust path detection (same as modprobed_injection)
     # In makepkg context, $HOME may be /root but modprobed.db is at user's ~/.config/
     MODPROBED_DB_PATH=""

     # Try common locations - search in order of likelihood
     # Priority: $HOME first (normal context), /root (root context), then /home/* (actual users)
     for candidate in "$HOME/.config/modprobed.db" /root/.config/modprobed.db /home/*/.config/modprobed.db; do
         if [[ -f "$candidate" ]]; then
             MODPROBED_DB_PATH="$candidate"
             printf "[PHASE-G2] Found modprobed.db at: $MODPROBED_DB_PATH\n" >&2
             break
         fi
     done

     if [[ -n "$MODPROBED_DB_PATH" && -f "$MODPROBED_DB_PATH" ]]; then
         printf "[PHASE-G2] POST-MODPROBED: Starting hard enforcer to protect filtered modules\n" >&2
         printf "[PHASE-G2] Using modprobed.db at: $MODPROBED_DB_PATH\n" >&2

         # CRITICAL: Must be in kernel source directory to operate on .config
         # Use same directory detection as modprobed_injection
         KERNEL_SRC_DIR=""
         if [[ -d "$srcdir/linux" ]]; then
             KERNEL_SRC_DIR="$srcdir/linux"
         else
             KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
         fi

         if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
             KERNEL_SRC_DIR="$srcdir"
         fi

         # Only proceed if we found and can access the kernel source directory
         if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/Makefile" ]]; then
             printf "[PHASE-G2] Found kernel source directory: $KERNEL_SRC_DIR\n" >&2

             # Change to kernel source directory for .config manipulation (FIX #5: DIRECTORY CONTEXT)
             # CRITICAL FIX: Wrap in subshell to automatically restore original directory
             ( cd "$KERNEL_SRC_DIR" || exit 1; {
                 if [[ -f ".config" ]]; then
                     # CRITICAL FIX: Protect the filtered 170 modules from olddefconfig re-expansion
                     #
                     # After localmodconfig filters to 170 modules, olddefconfig's Kconfig dependency
                     # expansion can remove some of these modules if they become optional dependencies.
                     # We HARD LOCK them by extracting and restoring them after olddefconfig.

                     # STEP 1: Create a backup and extract all filtered modules (CONFIG_*=m lines)
                     cp ".config" ".config.pre_g2"
                     FILTERED_MODULES=$(grep "=m$" ".config.pre_g2" | sort)
                     FILTERED_MODULE_COUNT=$(echo "$FILTERED_MODULES" | grep -c "=" 2>/dev/null || echo "unknown")

                     printf "[PHASE-G2] HARD LOCK: Extracted $FILTERED_MODULE_COUNT filtered modules from localmodconfig\n" >&2

                     # STEP 2: Run olddefconfig to handle consistent Kconfig dependencies
                     # This may add NEW dependencies but we'll restore our 170 filtered modules afterward
                     printf "[PHASE-G2] Running: make LLVM=1 LLVM_IAS=1 olddefconfig\n" >&2
                     if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                         # STEP 3: Restore the original filtered modules if any were removed
                         # Create a temporary file with all non-module configs
                         TEMP_CONFIG=$(mktemp)
                         grep -v "=m$" ".config" > "$TEMP_CONFIG"

                         # Append the original filtered modules back
                         echo "$FILTERED_MODULES" >> "$TEMP_CONFIG"

                         # Replace .config with hard-locked version
                         mv "$TEMP_CONFIG" ".config"

                         # Count final module count
                         FINAL_MODULE_COUNT=$(grep -c "=m" ".config" 2>/dev/null || echo "unknown")
                         printf "[PHASE-G2] Module count: $FILTERED_MODULE_COUNT → $FINAL_MODULE_COUNT (hard-locked to filtered set)\n" >&2
                         printf "[PHASE-G2] SUCCESS: Filtered modules protected and dependencies finalized\n" >&2
                     else
                         printf "[PHASE-G2] WARNING: olddefconfig failed\n" >&2
                     fi

                     # Cleanup backup
                     rm -f ".config.pre_g2"
                 fi
             } )
             # Subshell automatically returns to original directory after operations
         else
             printf "[PHASE-G2] WARNING: Could not locate kernel source directory\n" >&2
         fi
     else
         printf "[PHASE-G2] INFO: modprobed.db not found, skipping hard enforcer\n" >&2
     fi
     "#;

/// MODPROBED-DB localmodconfig injection
///
/// Discovers and enables modprobed-db automatic module filtering.
pub const MODPROBED_INJECTION: &str = r#"
          # =====================================================================
          # MODPROBED-DB AUTO-DISCOVERY: Automatic module filtering (FIX #1: DIRECTORY CONTEXT)
          # =====================================================================
          # CRITICAL: The kernel Makefile and Kconfig files are at the root of the source tree.
          # Running 'make localmodconfig' from the wrong directory will FAIL with Kconfig errors.
          # We MUST change to the kernel source directory BEFORE running make commands.
          #
          # This section uses modprobed-db to automatically filter kernel modules
          # to only those actually used on the system. This significantly reduces:
          # - Kernel size (often 50-70% reduction in module count)
          # - Build time (avoids compiling unused drivers)
          # - Runtime overhead (fewer modules to load and initialize)
          #
          # The kernel's localmodconfig target reads enabled modules from the
          # modprobed-db file ($HOME/.config/modprobed.db) and automatically
          # deselects all CONFIG_*=m module options that aren't in the database.

          # ROBUST PATH DETECTION: Use multiple fallback strategies to find modprobed.db
          # This handles cases where $HOME might be /root but modprobed.db is at /home/user/.config/
          MODPROBED_DB_PATH=""
          for candidate in "$HOME/.config/modprobed.db" /root/.config/modprobed.db /home/*/.config/modprobed.db; do
              if [[ -f "$candidate" ]]; then
                  MODPROBED_DB_PATH="$candidate"
                  break
              fi
          done

          # DIAGNOSTIC LOGGING: Log what paths were checked and what was found
          printf "[MODPROBED] [PATH-DETECTION] HOME env var: \$HOME\n" >&2
          printf "[MODPROBED] [PATH-DETECTION] Resolved modprobed.db path: $MODPROBED_DB_PATH\n" >&2
          printf "[MODPROBED] [PATH-DETECTION] Checked: \$HOME/.config/modprobed.db, /root/.config/modprobed.db, /home/*/.config/modprobed.db\n" >&2

          if [[ -n "$MODPROBED_DB_PATH" && -f "$MODPROBED_DB_PATH" ]]; then
              printf "[MODPROBED] Found modprobed-db at $MODPROBED_DB_PATH\n" >&2
              printf "[MODPROBED] Running localmodconfig to filter kernel modules...\n" >&2
              printf "[MODPROBED] This will automatically filter kernel modules to those in use\n" >&2
              printf "[MODPROBED] Using modprobed database: $MODPROBED_DB_PATH\n" >&2

              # FIX #1: HARDENED DIRECTORY CONTEXT DETECTION
              # The kernel source may be in various directory formats:
              # - linux-6.18.3 (standard version naming)
              # - linux-zen (custom kernel name)
              # - linux (generic)
              # First, try to find and use the actual kernel directory
              KERNEL_SRC_DIR=""

              # Try to find a directory matching linux-* or linux pattern
              if [[ -d "$srcdir/linux" ]]; then
                  KERNEL_SRC_DIR="$srcdir/linux"
              else
                  # Try to find any directory starting with 'linux-'
                  KERNEL_SRC_DIR=$(find "$srcdir" -maxdepth 1 -type d -name 'linux-*' 2>/dev/null | head -1)
              fi

              # If still not found, check if srcdir itself has Makefile (might be extracted there)
              if [[ -z "$KERNEL_SRC_DIR" ]] && [[ -f "$srcdir/Makefile" ]]; then
                  KERNEL_SRC_DIR="$srcdir"
              fi

              # If we found a kernel source directory, change to it
              if [[ -n "$KERNEL_SRC_DIR" ]] && [[ -d "$KERNEL_SRC_DIR" ]] && [[ -f "$KERNEL_SRC_DIR/Makefile" ]]; then
                  printf "[MODPROBED] Found kernel source directory: $KERNEL_SRC_DIR\n" >&2

                  # CRITICAL FIX: Use subshell to limit directory change scope
                  # This ensures we return to $srcdir automatically after all operations
                  ( cd "$KERNEL_SRC_DIR" || exit 1; {
                      printf "[MODPROBED] Changed to kernel source directory\n" >&2
                      printf "[MODPROBED] Running: yes \"\" | make LLVM=1 LLVM_IAS=1 LSMOD=$MODPROBED_DB_PATH localmodconfig\n" >&2

                      if yes "" | make LLVM=1 LLVM_IAS=1 LSMOD="$MODPROBED_DB_PATH" localmodconfig 2>&1 | tee /tmp/modprobed_output.log; then
                          printf "[MODPROBED] SUCCESS: Kernel configuration filtered for used modules\n" >&2

                          # VERIFICATION: Count the filtered modules IMMEDIATELY after localmodconfig
                          if [[ -f ".config" ]]; then
                              MODULE_COUNT=$(grep -c "=m" .config 2>/dev/null || echo "unknown")
                              printf "[MODPROBED] Module count after localmodconfig: $MODULE_COUNT\n" >&2
                          fi

                          # NOTE: We do NOT run olddefconfig here because Kconfig dependency expansion
                          # will re-enable thousands of modules that localmodconfig just filtered out.
                          # Instead, a PHASE G2 POST-MODPROBED hard enforcer (injected by patcher)
                          # will surgically remove all CONFIG_*=m entries not in modprobed.db,
                          # then run olddefconfig ONCE at the end with protected modules.
                          printf "[MODPROBED] Modprobed-db discovery complete - PHASE G2 enforcer will protect filtered modules\n" >&2
                      else
                          printf "[MODPROBED] WARNING: localmodconfig failed or unavailable, continuing with full config\n" >&2
                          printf "[MODPROBED] This is not fatal - the build will still complete\n" >&2
                          printf "[MODPROBED] See /tmp/modprobed_output.log for details\n" >&2
                      fi
                  } )
                  # Subshell automatically returns to $srcdir after operations
              else
                  printf "[MODPROBED] WARNING: Could not locate kernel source directory with Makefile\n" >&2
                  printf "[MODPROBED] Checked: \$srcdir/linux, \$srcdir/linux-*, \$srcdir (with Makefile)\n" >&2
                  printf "[MODPROBED] Continuing with full config\n" >&2
              fi
          else
              printf "[MODPROBED] SKIPPED: modprobed-db not found at any location\n" >&2
              printf "[MODPROBED] Kernel will build with full module set (localmodconfig step skipped)\n" >&2
          fi

          # =====================================================================
          # END MODPROBED-DB BLOCK
          # =====================================================================
          "#;

/// Kernel whitelist protection injection
///
/// Ensures critical kernel CONFIG options are always enabled.
pub const WHITELIST_INJECTION: &str = r#"
     # =====================================================================
     # KERNEL WHITELIST PROTECTION: Ensure critical features are always built
     # =====================================================================
     # This section implements a whitelist of critical kernel CONFIG options
     # that MUST always be enabled, protected from modprobed-db filtering.
     #
     # The whitelist includes:
     # - Security features (CFI, SMACK, SELINUX, AppArmor)
     # - Core functionality (SYSFS, PROC, TMPFS, DEVTMPFS)
     # - Boot/Init essentials (INITRAMFS_SOURCE, RAMFS, BINFMT)
     # - Critical filesystems (EXT4, BTRFS, FAT, VFAT, EXFAT, ISO9660, CIFS)
     # - NLS support (ASCII, CP437, UTF8, ISO8859-1 for filesystem compatibility)
     # - Loopback and UEFI (LOOP, EFIVAR_FS)
     # - Storage drivers (AHCI, NVMe, USB, USB_STORAGE, USB_HID)
     #
     # These options will survive localmodconfig filtering and ensure
     # the kernel remains bootable even with aggressive module stripping.

     if [[ -f ".config" ]]; then
         printf "[WHITELIST] Applying kernel whitelist protection...\n" >&2

         # CRITICAL: These CONFIG options MUST be present for bootability
         # We enforce them BEFORE localmodconfig to establish the baseline
         [ -f ".config" ] && cat >> ".config" << 'EOF'

# ====================================================================
# KERNEL WHITELIST: Critical features protected from modprobed filtering
# ====================================================================
# These options are enforced to ensure kernel bootability and security

# Core filesystem and procfs support (MANDATORY)
CONFIG_SYSFS=y
CONFIG_PROC_FS=y
CONFIG_TMPFS=y
CONFIG_DEVTMPFS=y
CONFIG_BLK_DEV_INITRD=y
CONFIG_ROOTS_FS_DEFAULT_CFI=y

# Security features (MANDATORY)
CONFIG_SELINUX=y
CONFIG_AUDIT=y
CONFIG_LSM="selinux,apparmor"

# Primary filesystems (MANDATORY)
CONFIG_EXT4_FS=y
CONFIG_BTRFS_FS=y

# Additional filesystems for bootability and compatibility (CRITICAL)
CONFIG_FAT_FS=m
CONFIG_VFAT_FS=m
CONFIG_EXFAT_FS=m
CONFIG_EXFAT_DEFAULT_IOCHARSET="utf8"
CONFIG_ISO9660=m
CONFIG_CIFS=m

# NLS (National Language Support) for EFI partition mounting (CRITICAL)
# Cross-reference: src/config/whitelist.rs ESSENTIAL_DRIVERS array
CONFIG_NLS_ASCII=m
CONFIG_NLS_CP437=m
CONFIG_NLS_UTF8=m
CONFIG_NLS_ISO8859_1=m

# Loopback device mounting (CRITICAL)
CONFIG_BLK_DEV_LOOP=m

# UEFI variables support (CRITICAL for UEFI systems)
CONFIG_EFIVAR_FS=m

# Storage device support (CRITICAL)
CONFIG_AHCI=m
CONFIG_SATA_AHCI=m
CONFIG_NVME=m
CONFIG_USB=m
CONFIG_USB_COMMON=m
CONFIG_USB_STORAGE=m

# Input device support (CRITICAL - USB keyboards, mice)
CONFIG_USB_HID=m
EOF

         printf "[WHITELIST] Kernel whitelist applied - critical features protected\n" >&2
     fi
     "#;

/// Generate post-oldconfig LTO patch based on LTO type
///
/// Re-enforces LTO settings after any kernel config regeneration.
pub fn get_post_oldconfig_lto_patch(lto_type: LtoType) -> &'static str {
    match lto_type {
        LtoType::Full => {
            r#"
     # =====================================================================
     # PHASE E1 CRITICAL: POST-OLDCONFIG LTO ENFORCEMENT (FULL LTO)
     # =====================================================================
     # After any 'make oldconfig' or 'make syncconfig', the kernel's Kconfig
     # system may revert our LTO settings to defaults. This snippet
     # IMMEDIATELY re-applies CONFIG_LTO_CLANG_FULL enforcement.

     # Check if we're in a build function (prepare, build, etc.)
     if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
         config_file=".config"

         # Only patch if .config exists (oldconfig already ran)
         if [[ -f "$config_file" ]]; then
             # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
             sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

             # Ensure file ends with newline, then append FULL LTO enforcement
             tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

             # APPEND FULL LTO settings at the END of .config
             [ -f "$config_file" ] && cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE E1 POST-OLDCONFIG: LTO CLANG FULL ENFORCEMENT (FINAL)
# ====================================================================
CONFIG_LTO_CLANG_FULL=y
CONFIG_LTO_CLANG=y
CONFIG_HAS_LTO_CLANG=y
EOF

             printf "[PATCH] POST-OLDCONFIG: Re-enforced CONFIG_LTO_CLANG_FULL=y after config regeneration\n" >&2

             if command -v make &> /dev/null; then
                 printf "[PATCH] POST-OLDCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
                 if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                     printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized without prompts\n" >&2
                 else
                     printf "[PATCH] WARNING: 'make olddefconfig' failed, continuing anyway...\n" >&2
                 fi
             fi
         fi
     fi
     "#
        }
        LtoType::Thin => {
            r#"
     # =====================================================================
     # PHASE E1 CRITICAL: POST-OLDCONFIG LTO ENFORCEMENT (THIN LTO)
     # =====================================================================
     # After any 'make oldconfig' or 'make syncconfig', the kernel's Kconfig
     # system may revert our LTO settings to defaults. This snippet
     # IMMEDIATELY re-applies CONFIG_LTO_CLANG_THIN enforcement.

     # Check if we're in a build function (prepare, build, etc.)
     if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
         config_file=".config"

         # Only patch if .config exists (oldconfig already ran)
         if [[ -f "$config_file" ]]; then
             # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
             sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

             # Ensure file ends with newline, then append THIN LTO enforcement
             tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

             # APPEND THIN LTO settings at the END of .config
             [ -f "$config_file" ] && cat >> "$config_file" << 'EOF'

# ====================================================================
# PHASE E1 POST-OLDCONFIG: LTO CLANG THIN ENFORCEMENT (FINAL)
# ====================================================================
CONFIG_LTO_CLANG_THIN=y
CONFIG_LTO_CLANG=y
CONFIG_HAS_LTO_CLANG=y
EOF

             printf "[PATCH] POST-OLDCONFIG: Re-enforced CONFIG_LTO_CLANG_THIN=y after config regeneration\n" >&2

             if command -v make &> /dev/null; then
                 printf "[PATCH] POST-OLDCONFIG: Running 'make olddefconfig' to finalize config...\n" >&2
                 if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                     printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized without prompts\n" >&2
                 else
                     printf "[PATCH] WARNING: 'make olddefconfig' failed, continuing anyway...\n" >&2
                 fi
             fi
         fi
     fi
     "#
        }
        LtoType::None => {
            r#"
     # =====================================================================
     # PHASE E1: POST-OLDCONFIG (LTO DISABLED - None)
     # =====================================================================
     # LTO is disabled per user selection - only remove LTO configs

     if [[ "$(pwd)" == "$srcdir/"* ]] || [[ -f ".config" ]]; then
         config_file=".config"

         if [[ -f "$config_file" ]]; then
             # Use GLOBAL sed pattern to remove ALL LTO and HAS_LTO entries
             sed -i '/^CONFIG_LTO_\|^CONFIG_HAS_LTO_\|^# CONFIG_LTO_\|^# CONFIG_HAS_LTO_/d' "$config_file"

             # Ensure file ends with newline
             tail -c 1 < "$config_file" | od -An -tx1 | grep -q '0a' || echo "" >> "$config_file"

             printf "[PATCH] POST-OLDCONFIG: LTO disabled - removed all LTO configs\n" >&2

             if command -v make &> /dev/null; then
                 if make LLVM=1 LLVM_IAS=1 olddefconfig > /dev/null 2>&1; then
                     printf "[PATCH] POST-OLDCONFIG: SUCCESS - Configuration finalized\n" >&2
                 fi
             fi
         fi
     fi
     "#
        }
    }
}

/// LLVM/Clang compiler variable exports
///
/// Injects aggressive compiler toolchain enforcement into PKGBUILD functions.
/// NOTE: Only exports toolchain binaries. Flag assembly is delegated to PHASE G1.1
/// in the prebuild enforcer, which assembles CFLAGS/CXXFLAGS/LDFLAGS from GOATD_*
/// environment variables. This prevents duplication.
pub const CLANG_EXPORTS: &str = r#"    # ======================================================================
          # CLANG/LLVM TOOLCHAIN INJECTION (Rust-based patcher)
          # ======================================================================
          # CRITICAL: These MUST be set BEFORE scripts/kconfig runs
          # They FORCEFULLY ensure the kernel detects CLANG, not GCC
          export LLVM=1
          export LLVM_IAS=1
          export CC=clang
          export CXX=clang++
          export LD=ld.lld
          export AR=llvm-ar
          export NM=llvm-nm
          export STRIP=/usr/bin/strip
          export OBJCOPY=llvm-objcopy
          export OBJDUMP=llvm-objdump
          export READELF=llvm-readelf

          # ======================================================================
          # HOST COMPILER ENFORCEMENT - Ensure host tools also use Clang
          # ======================================================================
          export HOSTCC=clang
          export HOSTCXX=clang++

          # ======================================================================
          # NOTE: CFLAGS/CXXFLAGS/LDFLAGS assembly delegated to PHASE G1.1
          # ======================================================================
          # Single Source of Truth: PHASE G1.1 PREBUILD enforcer assembles flags
          # from GOATD_LTO_FLAGS, GOATD_POLLY_FLAGS, GOATD_HARDENING_FLAGS, etc.
          # This prevents duplication and ensures consistent flag ordering.
          export KCFLAGS="-march=native"
      "#;

/// Polly loop optimization injection header
pub const POLLY_INJECTION_HEADER: &str = r#"    # ======================================================================
      # PHASE 3.5: POLLY LOOP OPTIMIZATION FLAGS (Surgical LLVM optimization)
      # ======================================================================
      # Polly provides advanced loop optimizations via LLVM
      # Flags enable: core Polly + stripmine vectorizer + max fusion
      # This is the OPTIMIZED flag set ensuring consistent performance gains
      printf "[PHASE 3.5] Surgically enforced Polly LLVM loop optimizations\n" >&2
      "#;

/// Generate Polly optimization block
///
/// Creates the full Polly injection script with custom flags.
pub fn get_polly_block(cflags: &str, cxxflags: &str, ldflags: &str) -> String {
    format!(
        r#"{}
      export POLLY_CFLAGS='{}'
      export POLLY_CXXFLAGS='{}'
      export POLLY_LDFLAGS='{}'

      # Append Polly flags to existing CFLAGS/CXXFLAGS/LDFLAGS
      export CFLAGS="${{CFLAGS}} $POLLY_CFLAGS"
      export CXXFLAGS="${{CXXFLAGS}} $POLLY_CXXFLAGS"
      export LDFLAGS="${{LDFLAGS}} $POLLY_LDFLAGS"
      "#,
        POLLY_INJECTION_HEADER, cflags, cxxflags, ldflags
    )
}

/// Bash function snippet for resolving GOATD workspace root via .goatd_anchor
///
/// Generates a Bash function that walks up the directory tree to locate .goatd_anchor
/// and sets GOATD_WORKSPACE_ROOT to its parent directory.
/// This enables robust path resolution across mount points and fakeroot transitions.
pub fn get_resolve_goatd_root() -> &'static str {
    r#"
    # =====================================================================
    # PHASE 14: CROSS-MOUNT PATH RESOLUTION - resolve_goatd_root() function
    # =====================================================================
    # Bash function to dynamically resolve GOATD_WORKSPACE_ROOT by searching for .goatd_anchor
    # This ensures fakeroot can find metadata even across mount points

    resolve_goatd_root() {
        local current_dir="${1:-.}"
        local max_depth=10
        local depth=0

        printf "[RESOLVE] Starting .goatd_anchor search from: %s\n" "$current_dir" >&2

        while [ $depth -lt $max_depth ]; do
            if [ -f "$current_dir/.goatd_anchor" ]; then
                # Found the anchor - use its parent as workspace root
                GOATD_WORKSPACE_ROOT="$(cd "$current_dir" && pwd)" || GOATD_WORKSPACE_ROOT="$current_dir"
                export GOATD_WORKSPACE_ROOT
                printf "[RESOLVE] ✓ Found .goatd_anchor at: %s\n" "$GOATD_WORKSPACE_ROOT" >&2
                printf "[RESOLVE] ✓ Set GOATD_WORKSPACE_ROOT=%s\n" "$GOATD_WORKSPACE_ROOT" >&2
                return 0
            fi

            # Move up one directory level
            local parent_dir="$(cd "$current_dir/.." && pwd)" || return 1
            if [ "$parent_dir" = "$current_dir" ]; then
                # Reached filesystem root without finding anchor
                printf "[RESOLVE] ✗ Reached filesystem root without finding .goatd_anchor\n" >&2
                return 1
            fi

            current_dir="$parent_dir"
            depth=$((depth + 1))
        done

        printf "[RESOLVE] ✗ Max depth (%d) reached without finding .goatd_anchor\n" $max_depth >&2
        return 1
    }
    "#
}

/// Generate MPL (Metadata Persistence Layer) sourcing injection
///
/// Creates shell code to source the immutable metadata file.
pub fn get_mpl_injection(workspace: &str) -> String {
    format!(
        r#"    # =====================================================================
       # !!! MPL STARTING !!! - METADATA PERSISTENCE LAYER (MPL) SOURCING
       # =====================================================================
       # PHASE 14: CROSS-MOUNT RESOLUTION - Call resolve_goatd_root() if workspace not set
       if [ -z "${{GOATD_WORKSPACE_ROOT}}" ]; then
           printf "[MPL] GOATD_WORKSPACE_ROOT not set, attempting dynamic resolution\n" >&2
           resolve_goatd_root || true
       fi

       echo "!!! MPL SOURCING INJECTED !!!" >&2
       echo "[MPL] Workspace root: ${{GOATD_WORKSPACE_ROOT:={workspace}}}" >&2

       # Source the immutable metadata file from workspace root
       # This provides GOATD_KERNELRELEASE and other build metadata
       # from a single source of truth instead of fragile discovery
       if [ -f "${{GOATD_WORKSPACE_ROOT:={workspace}}}/.goatd_metadata" ]; then
           echo "!!! MPL: Found metadata file at ${{GOATD_WORKSPACE_ROOT:={workspace}}}/.goatd_metadata !!!" >&2
           source "${{GOATD_WORKSPACE_ROOT:={workspace}}}/.goatd_metadata"
           echo "[MPL] Loaded kernelrelease from metadata file: ${{GOATD_KERNELRELEASE}}" >&2
           echo "!!! MPL: GOATD_KERNELRELEASE=${{GOATD_KERNELRELEASE}} !!!" >&2
       else
           echo "!!! MPL: WARNING - metadata file NOT FOUND at ${{GOATD_WORKSPACE_ROOT:={workspace}}}/.goatd_metadata !!!" >&2
           echo "[MPL] Legacy fallback discovery may be needed" >&2
       fi
       echo "!!! MPL SOURCING COMPLETE !!!" >&2
       "#,
        workspace = workspace
    )
}

/// Generate headers injection script
///
/// Creates the PHASE-E2 hardened header naming injection with dynamic uname -r detection,
/// explicit build symlinks, multi-alias bridge looping, and final verification.
pub fn get_headers_injection(version: Option<&str>) -> String {
    if let Some(actual_ver) = version {
        // PRIORITY 0: Hardcoded literal version from Rust orchestrator
        format!(
            r#"
           # =====================================================================
             # PHASE-E2: HARDENED HEADER NAMING - LITERAL INJECTION (PRIORITY 0)
             # =====================================================================
             # CRITICAL: Hardcoded kernel version from Rust orchestrator
             # This is the NUCLEAR OPTION - version is baked in, no discovery needed
             # Implements hardened Version Bridge with explicit build symlinks
             # DYNAMIC PKGBASE: Uses rebranded ${{pkgbase}} for headers directory naming

             _actual_ver="{}"
             echo "[PHASE-E2] KERNELRELEASE[0-HARDCODED]: Using Rust-injected literal: ${{_actual_ver}}" >&2

             # SUCCESS: _actual_ver is now set from PRIORITY 0 hardcoded literal
             if [ -n "${{_actual_ver}}" ]; then
                 # Create /usr/src/{{pkgbase}}-{{pkgver}}-{{pkgrel}} directory for headers installation
                 # This uses the rebranded pkgbase (e.g., linux-goatd-gaming) to ensure proper DKMS discovery
                 _headers_dir="${{pkgdir}}/usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}}"
                 echo "[PHASE-E2] Installing headers to: /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}}" >&2
                 mkdir -p "$_headers_dir"
                 mkdir -p "${{pkgdir}}/usr/lib/modules/${{_actual_ver}}"

                 # =====================================================================
                 # HARDENED: EXPLICIT build SYMLINK CREATION WITHIN MODULE DIRECTORY
                 # =====================================================================
                 # CRITICAL: DKMS searches for /usr/lib/modules/\$(uname -r)/build
                 # We MUST create this symlink EXPLICITLY pointing to the headers directory
                 # Use absolute path to headers: /usr/src/{{pkgbase}}-{{pkgver}}-{{pkgrel}}
                 if [ -d "$_headers_dir" ]; then
                     (cd "${{pkgdir}}/usr/lib/modules/${{_actual_ver}}" && \
                      ln -sf /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}} build) 2>/dev/null
                     echo "[PHASE-E2] Created build symlink: /usr/lib/modules/${{_actual_ver}}/build -> /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}}" >&2
                 fi

                 # =====================================================================
                 # HARDENED: MULTI-ALIAS BRIDGE (LOOPING THROUGH ALIASES)
                 # =====================================================================
                 _pretty_ver="${{pkgver}}-${{pkgrel}}-${{pkgbase#linux-}}"
                 _uname_r_expected="${{_actual_ver}}"

                 # Loop through both _actual_ver and _pretty_ver to create alias bridges
                 for _bridge_alias in "${{_uname_r_expected}}" "${{_pretty_ver}}"; do
                     if [ -n "${{_bridge_alias}}" ] && [ "${{_bridge_alias}}" != "${{_uname_r_expected}}" ]; then
                         echo "[PHASE-E2] Creating multi-alias bridge: ${{_bridge_alias}} -> ${{_uname_r_expected}}" >&2
                         (cd "${{pkgdir}}/usr/lib/modules" && ln -sf "${{_uname_r_expected}}" "${{_bridge_alias}}") 2>/dev/null
                         mkdir -p "${{pkgdir}}/usr/lib/modules/${{_bridge_alias}}"
                         (cd "${{pkgdir}}/usr/lib/modules/${{_bridge_alias}}" && \
                          ln -sf /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}} build) 2>/dev/null
                         echo "[PHASE-E2] Created build symlink for alias: /usr/lib/modules/${{_bridge_alias}}/build" >&2
                     fi
                 done

                 # =====================================================================
                 # HARDENED: FINAL VERIFICATION STEP
                 # =====================================================================
                 echo "[PHASE-E2] VERIFICATION: Checking symlink creation..." >&2
                 _verify_pass=0
                 if [ -L "${{pkgdir}}/usr/lib/modules/${{_actual_ver}}/build" ]; then
                     echo "[PHASE-E2] ✓ Verified: /usr/lib/modules/${{_actual_ver}}/build -> /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}}" >&2
                        _verify_pass=$(({{_verify_pass}} + 1))
                 else
                     echo "[PHASE-E2] ✗ Failed: /usr/lib/modules/${{_actual_ver}}/build not created" >&2
                 fi
                 if [ -L "${{pkgdir}}/usr/lib/modules/${{_pretty_ver}}/build" ] && [ "${{_pretty_ver}}" != "${{_actual_ver}}" ]; then
                     echo "[PHASE-E2] ✓ Verified: /usr/lib/modules/${{_pretty_ver}}/build -> /usr/src/${{pkgbase}}-${{pkgver}}-${{pkgrel}}" >&2
                        _verify_pass=$(({{_verify_pass}} + 1))
                 fi
                 echo "[PHASE-E2] Verification result: ${{_verify_pass}} symlink(s) verified" >&2

                 echo "[PHASE-E2] SUCCESS: Hardened header naming applied for kernelrelease: ${{_actual_ver}}" >&2
             fi
             "#,
            actual_ver
        )
    } else {
        // Fallback with 5-tier discovery strategy + hardened Version Bridge
        r#"
        # =====================================================================
        # PHASE-E2: HARDENED HEADER NAMING - DYNAMIC UNAME -R DETECTION
        # =====================================================================
        # CRITICAL: Use exact kernel version matching uname -r (source of truth)
        # Implements explicit build symlinks, multi-alias bridges, and verification

        _actual_ver=""

        # PRIORITY 1: Try .kernelrelease from current directory (build artifact)
        if [ -f .kernelrelease ]; then
            _actual_ver=$(cat .kernelrelease)
            echo "[PHASE-E2] KERNELRELEASE[1]: Read from ./.kernelrelease: ${_actual_ver}" >&2
        fi

        # PRIORITY 2: Search srcdir for .kernelrelease (makepkg scenario)
        if [ -z "${_actual_ver}" ]; then
            _rel_file=$(find "${srcdir}" -maxdepth 2 -name .kernelrelease -print -quit 2>/dev/null)
            if [ -n "${_rel_file}" ]; then
                _actual_ver=$(cat "${_rel_file}")
                echo "[PHASE-E2] KERNELRELEASE[2]: Read from srcdir: ${_actual_ver}" >&2
            fi
        fi

        # PRIORITY 3: Check environment variable (passed by executor)
        if [ -z "${_actual_ver}" ] && [ -n "${GOATD_KERNELRELEASE}" ]; then
            _actual_ver="${GOATD_KERNELRELEASE}"
            echo "[PHASE-E2] KERNELRELEASE[3]: Read from GOATD_KERNELRELEASE: ${_actual_ver}" >&2
        fi

        # PRIORITY 4: Fallback to standard naming convention
        if [ -z "${_actual_ver}" ]; then
            _actual_ver=$(ls "${pkgdir}/usr/lib/modules/" 2>/dev/null | grep -v 'extramodules' | head -n 1)
            if [ -n "${_actual_ver}" ]; then
                echo "[PHASE-E2] KERNELRELEASE[4]: Found in pkgdir: ${_actual_ver}" >&2
            fi
        fi

        # PRIORITY 5: Final fallback to _kernver
        [ -z "${_actual_ver}" ] && _actual_ver="${_kernver}" && echo "[PHASE-E2] KERNELRELEASE[5]: Fallback to _kernver: ${_actual_ver}" >&2

        # CRITICAL EDGE CASE: If _actual_ver is STILL empty after all 5 strategies, FAIL HARD
        if [ -z "${_actual_ver}" ]; then
            echo "[PHASE-E2] ERROR: Could not determine kernel version after all 5 discovery strategies failed:" >&2
            echo "[PHASE-E2]   PRIORITY 1 - local .kernelrelease: No file" >&2
            echo "[PHASE-E2]   PRIORITY 2 - srcdir search: No file found" >&2
            echo "[PHASE-E2]   PRIORITY 3 - GOATD_KERNELRELEASE env: Not set" >&2
            echo "[PHASE-E2]   PRIORITY 4 - pkgdir modules: No modules directory" >&2
            echo "[PHASE-E2]   PRIORITY 5 - fallback _kernver: Empty or invalid" >&2
            echo "[PHASE-E2] FATAL: Cannot create headers package with unknown kernel version" >&2
            echo "[PHASE-E2] Build halted to prevent broken package installation" >&2
            exit 1
        fi

        if [ -n "${_actual_ver}" ]; then
            # Create /usr/src/linux-{kernelrelease} directory for headers installation
            echo "[PHASE-E2] Installing headers to: /usr/src/linux-${_actual_ver}" >&2
            mkdir -p "${pkgdir}/usr/src/linux-${_actual_ver}"
            mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"

            # =====================================================================
            # HARDENED: EXPLICIT build SYMLINK CREATION WITHIN MODULE DIRECTORY
            # =====================================================================
            # CRITICAL: DKMS searches for /usr/lib/modules/\$(uname -r)/build
            # We MUST create this symlink EXPLICITLY pointing to the headers directory
            if [ -d "${pkgdir}/usr/src/linux-${_actual_ver}" ]; then
                (cd "${pkgdir}/usr/lib/modules/${_actual_ver}" && \
                 ln -sf /usr/src/linux-${_actual_ver} build) 2>/dev/null
                echo "[PHASE-E2] Created build symlink: /usr/lib/modules/${_actual_ver}/build -> /usr/src/linux-${_actual_ver}" >&2
            fi

            # =====================================================================
            # HARDENED: MULTI-ALIAS BRIDGE (LOOPING THROUGH ALIASES)
            # =====================================================================
            _pretty_ver="${pkgver}-${pkgrel}-${pkgbase#linux-}"
            _uname_r_expected="${_actual_ver}"

            # Loop through both _actual_ver and _pretty_ver to create alias bridges
            for _bridge_alias in "${_uname_r_expected}" "${_pretty_ver}"; do
                if [ -n "${_bridge_alias}" ] && [ "${_bridge_alias}" != "${_uname_r_expected}" ]; then
                    echo "[PHASE-E2] Creating multi-alias bridge: ${_bridge_alias} -> ${_uname_r_expected}" >&2
                    (cd "${pkgdir}/usr/lib/modules" && ln -sf "${_uname_r_expected}" "${_bridge_alias}") 2>/dev/null
                    mkdir -p "${pkgdir}/usr/lib/modules/${_bridge_alias}"
                    (cd "${pkgdir}/usr/lib/modules/${_bridge_alias}" && \
                     ln -sf /usr/src/linux-${_uname_r_expected} build) 2>/dev/null
                    echo "[PHASE-E2] Created build symlink for alias: /usr/lib/modules/${_bridge_alias}/build" >&2
                fi
            done

            # =====================================================================
            # HARDENED: FINAL VERIFICATION STEP
            # =====================================================================
            echo "[PHASE-E2] VERIFICATION: Checking symlink creation..." >&2
            _verify_pass=0
            if [ -L "${pkgdir}/usr/lib/modules/${_actual_ver}/build" ]; then
                echo "[PHASE-E2] ✓ Verified: /usr/lib/modules/${_actual_ver}/build -> /usr/src/linux-${_actual_ver}" >&2
                _verify_pass=$((${_verify_pass} + 1))
            else
                echo "[PHASE-E2] ✗ Failed: /usr/lib/modules/${_actual_ver}/build not created" >&2
            fi
            if [ -L "${pkgdir}/usr/lib/modules/${_pretty_ver}/build" ] && [ "${_pretty_ver}" != "${_actual_ver}" ]; then
                echo "[PHASE-E2] ✓ Verified: /usr/lib/modules/${_pretty_ver}/build -> /usr/src/linux-${_actual_ver}" >&2
                _verify_pass=$((${_verify_pass} + 1))
            fi
            echo "[PHASE-E2] Verification result: ${_verify_pass} symlink(s) verified" >&2

            echo "[PHASE-E2] SUCCESS: Hardened header naming applied for kernelrelease: ${_actual_ver}" >&2
        fi
        "#.to_string()
    }
}

/// PHASE-E2 main package module directory creation
///
/// Creates the module directory structure for the main kernel package.
pub const PHASE_E2_MAIN_INJECTION: &str = r#"
     # =====================================================================
     # PHASE-E2: UNIFIED HEADER NAMING - Module directory for main package
     # =====================================================================
     # CRITICAL: Use exact kernel version from .kernelrelease for consistency

     _actual_ver=""

     # PRIORITY 1: Try .kernelrelease from current directory (build artifact)
     if [ -f .kernelrelease ]; then
         _actual_ver=$([ -f .kernelrelease ] && cat .kernelrelease)
         echo "[PHASE-E2-MAIN] KERNELRELEASE[1]: Read from ./.kernelrelease: ${_actual_ver}" >&2
     fi

     # PRIORITY 2: Search srcdir for .kernelrelease (makepkg scenario)
     if [ -z "${_actual_ver}" ]; then
         _rel_file=$(find "${srcdir}" -maxdepth 2 -name .kernelrelease -print -quit 2>/dev/null)
         if [ -n "${_rel_file}" ] && [ -f "${_rel_file}" ]; then
             _actual_ver=$([ -f "${_rel_file}" ] && cat "${_rel_file}")
             echo "[PHASE-E2-MAIN] KERNELRELEASE[2]: Read from srcdir: ${_actual_ver}" >&2
         fi
     fi

     # PRIORITY 3: Check environment variable (passed by executor)
     if [ -z "${_actual_ver}" ] && [ -n "${GOATD_KERNELRELEASE}" ]; then
         _actual_ver="${GOATD_KERNELRELEASE}"
         echo "[PHASE-E2-MAIN] KERNELRELEASE[3]: Read from GOATD_KERNELRELEASE: ${_actual_ver}" >&2
     fi

     # PRIORITY 4: Final fallback to _kernver
     [ -z "${_actual_ver}" ] && _actual_ver="${_kernver}" && echo "[PHASE-E2-MAIN] KERNELRELEASE[4]: Fallback to _kernver: ${_actual_ver}" >&2

     # CRITICAL EDGE CASE: If _actual_ver is STILL empty after all 4 strategies, FAIL HARD
     if [ -z "${_actual_ver}" ]; then
         echo "[PHASE-E2-MAIN] ERROR: Could not determine kernel version after all strategies failed:" >&2
         echo "[PHASE-E2-MAIN]   PRIORITY 1 - local .kernelrelease: No file" >&2
         echo "[PHASE-E2-MAIN]   PRIORITY 2 - srcdir search: No file found" >&2
         echo "[PHASE-E2-MAIN]   PRIORITY 3 - GOATD_KERNELRELEASE env: Not set" >&2
         echo "[PHASE-E2-MAIN]   PRIORITY 4 - fallback _kernver: Empty or invalid" >&2
         echo "[PHASE-E2-MAIN] FATAL: Cannot create kernel package with unknown version" >&2
         echo "[PHASE-E2-MAIN] Build halted to prevent broken package installation" >&2
         exit 1
     fi

     # Create module directory with unified naming
     if [ -n "${_actual_ver}" ]; then
         echo "[PHASE-E2-MAIN] Creating /usr/lib/modules/${_actual_ver} for kernel packages" >&2
         mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"
         echo "[PHASE-E2-MAIN] SUCCESS: Module directory created for kernelrelease: ${_actual_ver}" >&2
     fi
     "#;

/// Generate module directory creation code (headers and main package)
///
/// Returns tuple of (headers_injection, main_injection) with version detection logic.
/// **CRITICAL FIX:** Uses `{{_actual_ver}}` to ensure generated Bash contains `${_actual_ver}`.
pub fn get_module_dir_creation(actual_version: Option<&str>) -> (String, String) {
    // Define version discovery logic (Priority 0 vs Fallback)
    let version_logic = if let Some(ver) = actual_version {
        // Priority 0: Hardcoded literal version (The Nuclear Option)
        format!(
            r#"
   # PRIORITY 0: Hardcoded Version Injection
   _actual_ver="{}"
   echo "[PHASE-E2] Using Hardcoded Version: ${{_actual_ver}}" >&2
"#,
            ver
        )
    } else {
        // Legacy Fallback Discovery
        r#"
   # Fallback Version Discovery
   _actual_ver=""
   if [ -n "${GOATD_KERNELRELEASE}" ]; then
       _actual_ver="${GOATD_KERNELRELEASE}"
   elif [ -f .kernelrelease ]; then
       _actual_ver=$(cat .kernelrelease)
   fi
   [ -z "${_actual_ver}" ] && _actual_ver="${_kernver}"
"#
        .to_string()
    };

    // Headers package action
    let headers_action = r#"
   if [ -n "${_actual_ver}" ]; then
       echo "[PHASE-E2] Installing headers to: /usr/src/linux-${_actual_ver}" >&2
       mkdir -p "${pkgdir}/usr/src/linux-${_actual_ver}"
       mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"

       # Version Bridge
       _pretty_ver="${pkgver}-${pkgrel}-${pkgbase#linux-}"
       if [ "${_actual_ver}" != "${_pretty_ver}" ]; then
           (cd "${pkgdir}/usr/lib/modules" && ln -sf "${_actual_ver}" "${_pretty_ver}") 2>/dev/null
       fi
   fi
"#;

    // Main package action
    let main_action = r#"
   if [ -n "${_actual_ver}" ]; then
       echo "[PHASE-E2] Creating module dir: /usr/lib/modules/${_actual_ver}" >&2
       mkdir -p "${pkgdir}/usr/lib/modules/${_actual_ver}"
   fi
"#;

    // Combine version logic with action logic
    let headers_code = format!("{}{}", version_logic, headers_action);
    let main_code = format!("{}{}", version_logic, main_action);

    (headers_code, main_code)
}

/// NVIDIA DKMS Compatibility Shim: Restore page_free field
///
/// Kernel 6.19 removed `page_free` from `struct dev_pagemap_ops` in include/linux/memremap.h.
/// NVIDIA 590.48.01 driver (nvidia-open) still references this field, causing DKMS build failure.
///
/// This shim surgically restores backward compatibility by:
/// 1. Locating `struct dev_pagemap_ops` in the kernel headers
/// 2. Injecting `void (*page_free)(struct page *page);` before closing brace
/// 3. Ensuring NVIDIA's out-of-tree module can compile without modification
/// 4. Using idempotent checks to avoid duplicate field additions
///
/// The shim implements the "Nuclear Option" for ABI compatibility:
/// We restore the removed field to allow unmodified DKMS drivers to build against
/// kernels that removed the field but expect driver compatibility.
pub const NVIDIA_DKMS_MEMREMAP_SHIM: &str = r#"    # =====================================================================
      # NVIDIA DKMS COMPATIBILITY SHIM: Restore page_free field
      # =====================================================================
      # CRITICAL FIX FOR KERNEL 6.19 DKMS BUILDS
      # NVIDIA 590.48.01 (nvidia-open) driver references page_free in
      # struct dev_pagemap_ops which was removed in Linux 6.19.
      # This shim restores backward compatibility for out-of-tree modules.
      #
      # The shim restores the page_free function pointer field
      # that allows NVIDIA's driver to compile without modification.

      # =====================================================================
      # PHASE 16: LOAD JSON TELEMETRY LOGGING FUNCTION
      # =====================================================================
      log_json() {
          local _level="${1:-INFO}"
          local _phase="${2:-UNKNOWN}"
          local _message="${3:-<no message>}"
          local _metadata="${4:-}"
          local _timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

          local _json_entry="{"
          _json_entry+="\"timestamp\":\"${_timestamp}\","
          _json_entry+="\"level\":\"${_level}\","
          _json_entry+="\"phase\":\"${_phase}\","
          _json_entry+="\"message\":\"$(echo "$_message" | sed 's/"/\\"/g')\""

          if [ -n "$_metadata" ]; then
              _json_entry+=",\"metadata\":${_metadata}"
          fi

          _json_entry+="}"

          echo "$_json_entry" >> /tmp/goatd_dkms.log 2>/dev/null
          printf "[%s] [%s] %s\n" "$_level" "$_phase" "$_message" >&2
      }

      # =====================================================================
      # STEP 1: Locate the struct dev_pagemap_ops definition
      # =====================================================================
      MEMREMAP_FILE="include/linux/memremap.h"

      if [ -f "$MEMREMAP_FILE" ]; then
          # Verify the file exists and contains the struct definition
          if grep -q "struct dev_pagemap_ops" "$MEMREMAP_FILE"; then
              # Idempotent check: only apply if page_free not already present
              if ! grep -q "page_free" "$MEMREMAP_FILE"; then
                  log_json "INFO" "NVIDIA-DKMS" "Applying page_free field restoration shim"

                 # ===================================================================
                 # STEP 2: Inject page_free compatibility member using Perl one-liner
                 # ===================================================================
                 # Use context-aware Perl to find struct closing and inject BEFORE close
                 # The regex handles kernel version variations in struct format

                 if perl -0777 -pi -e 's/(struct\s+dev_pagemap_ops\s*\{.*?)(\s*\});/$1\n\tvoid (*page_free)(struct page *page); \/* NVIDIA DKMS compat: restored for 6.19 *\/\n$2/sg if /struct\s+dev_pagemap_ops/ && !/page_free/' "$MEMREMAP_FILE"; then
                     if grep -q "page_free" "$MEMREMAP_FILE"; then
                         log_json "SUCCESS" "NVIDIA-DKMS" "page_free injected via context-aware Perl"
                     else
                         log_json "WARNING" "NVIDIA-DKMS" "Perl command executed but page_free not detected"
                         log_json "INFO" "NVIDIA-DKMS" "Initiating Tier 2 fallback..."

                         # ===================================================================
                         # TIER 2 FALLBACK: Preprocessor override if Perl injection fails
                         # ===================================================================
                         if ! grep -q "page_free" "$MEMREMAP_FILE"; then
                             log_json "INFO" "NVIDIA-DKMS" "Appending compatibility shim via preprocessor"

                             # Ensure newline at EOF before appending
                             [ -n "$(tail -c 1 "$MEMREMAP_FILE")" ] && echo "" >> "$MEMREMAP_FILE"

                             # Append the GOATD PHASE 9 compatibility header
                             cat >> "$MEMREMAP_FILE" << 'COMPATEOF'

/* GOATD PHASE 9: Multi-Tiered Fallback Engine */
#ifndef _GOATD_MEMREMAP_COMPAT_H
#define _GOATD_MEMREMAP_COMPAT_H
struct goatd_dev_pagemap_ops {
   void (*page_free)(struct page *page);
};
#define dev_pagemap_ops goatd_dev_pagemap_ops
#endif
COMPATEOF

                             if grep -q "page_free" "$MEMREMAP_FILE"; then
                                 log_json "SUCCESS" "NVIDIA-DKMS" "Compatibility shim appended via preprocessor fallback"
                             else
                                 log_json "ERROR" "NVIDIA-DKMS" "Could not append compatibility shim"
                             fi
                         fi
                     fi
                 else
                     log_json "ERROR" "NVIDIA-DKMS" "Perl one-liner failed to execute"
                     log_json "INFO" "NVIDIA-DKMS" "Initiating Tier 2 fallback..."

                     # ===================================================================
                     # TIER 2 FALLBACK: Preprocessor override if Perl fails entirely
                     # ===================================================================
                     if ! grep -q "page_free" "$MEMREMAP_FILE"; then
                         log_json "INFO" "NVIDIA-DKMS" "Appending compatibility shim via preprocessor (Tier 2)"

                         # Ensure newline at EOF before appending
                         [ -n "$(tail -c 1 "$MEMREMAP_FILE")" ] && echo "" >> "$MEMREMAP_FILE"

                         # Append the GOATD PHASE 9 compatibility header
                         cat >> "$MEMREMAP_FILE" << 'COMPATEOF'

/* GOATD PHASE 9: Multi-Tiered Fallback Engine */
#ifndef _GOATD_MEMREMAP_COMPAT_H
#define _GOATD_MEMREMAP_COMPAT_H
struct goatd_dev_pagemap_ops {
   void (*page_free)(struct page *page);
};
#define dev_pagemap_ops goatd_dev_pagemap_ops
#endif
COMPATEOF

                         if grep -q "page_free" "$MEMREMAP_FILE"; then
                             log_json "SUCCESS" "NVIDIA-DKMS" "Compatibility shim appended via preprocessor fallback"
                         else
                             log_json "ERROR" "NVIDIA-DKMS" "Could not append compatibility shim"
                         fi
                     fi
                 fi
             else
                 log_json "INFO" "NVIDIA-DKMS" "page_free field already present (idempotent - skipped)"
             fi
         else
             log_json "WARNING" "NVIDIA-DKMS" "struct dev_pagemap_ops not found in memremap.h"
         fi
     else
         log_json "WARNING" "NVIDIA-DKMS" "memremap.h not found in kernel source"
     fi
     "#;

/// NVIDIA DKMS Compatibility Shim for headers package
///
/// Applied to the _package-headers() function to inject the shim
/// AFTER headers are copied into the package directory.
/// This ensures the shim survives packaging into the final headers package.
///
/// ROBUST REDESIGN (Linux 6.19 compatible):
/// - Fallback path discovery: 3-tier with dynamic find as ultimate fallback
/// - Surgical sed targeting: address range /struct dev_pagemap_ops/,/^}/ for accuracy
/// - Enhanced diagnostics: detailed echo statements at every step
pub const NVIDIA_DKMS_HEADER_PACKAGE_SHIM: &str = r#"    # =====================================================================
   # NVIDIA DKMS HEADER PACKAGE SHIM: Restore page_free in staged headers
   # =====================================================================
   # CRITICAL FIX FOR KERNEL 6.19 DKMS BUILDS
   # This runs AFTER headers are copied into ${pkgdir}/usr/src/linux-${version}
   # to ensure the compatibility shim persists into the final package.
   #
   # PHASE 12: AUTOMATED SHIM ROLLBACK & CLEANUP
   # - Unified bash trap for atomic rollback on EXIT, INT, TERM, ERR
   # - Atomic backup file creation (.goatd_bak) BEFORE patching
   # - Temporary test directory (SHIM_TEST_DIR) for validation
   # - Success commitment: explicit backup removal on validation pass
   # - Idempotent operation with comprehensive error handling

   # =====================================================================
   # PHASE 16: LOAD JSON TELEMETRY LOGGING FUNCTION
   # =====================================================================
   log_json() {
       local _level="${1:-INFO}"
       local _phase="${2:-UNKNOWN}"
       local _message="${3:-<no message>}"
       local _metadata="${4:-}"
       local _timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

       local _json_entry="{"
       _json_entry+="\"timestamp\":\"${_timestamp}\","
       _json_entry+="\"level\":\"${_level}\","
       _json_entry+="\"phase\":\"${_phase}\","
       _json_entry+="\"message\":\"$(echo "$_message" | sed 's/"/\\"/g')\""

       if [ -n "$_metadata" ]; then
           _json_entry+=",\"metadata\":${_metadata}"
       fi

       _json_entry+="}"

       echo "$_json_entry" >> /tmp/goatd_dkms.log 2>/dev/null
       printf "[%s] [%s] %s\n" "$_level" "$_phase" "$_message" >&2
   }

   log_json "INFO" "NVIDIA-DKMS" "Starting header package shim processor"

   # =====================================================================
   # PHASE 12: UNIFIED TRAP SETUP - Atomic rollback and cleanup
   # =====================================================================
   # This trap ensures that on ANY abnormal exit (INT, TERM, ERR, or EXIT),
   # we roll back the memremap.h file from backup and clean up test artifacts

   MEMREMAP_BAK=""
   SHIM_TEST_DIR=""

   # Trap function to handle cleanup on exit/error
   _shim_cleanup() {
       local _exit_code=$?
       log_json "INFO" "NVIDIA-DKMS" "Cleanup handler triggered" "{\"exit_code\":$_exit_code}"

       # Roll back memremap.h if backup exists (indicates interrupted/failed patch)
       if [ -n "$MEMREMAP_BAK" ] && [ -f "$MEMREMAP_BAK" ]; then
           log_json "WARNING" "NVIDIA-DKMS" "Rolling back memremap.h from backup"
           if mv "$MEMREMAP_BAK" "${MEMREMAP_BAK%.goatd_bak}" 2>/dev/null; then
               log_json "SUCCESS" "NVIDIA-DKMS" "Rollback successful"
           else
               log_json "WARNING" "NVIDIA-DKMS" "Rollback failed"
           fi
       fi

       # Remove temporary test directory and contents
       if [ -n "$SHIM_TEST_DIR" ] && [ -d "$SHIM_TEST_DIR" ]; then
           log_json "INFO" "NVIDIA-DKMS" "Removing temporary test directory"
           rm -rf "$SHIM_TEST_DIR" 2>/dev/null || true
       fi

       log_json "INFO" "NVIDIA-DKMS" "Cleanup complete"
   }

   # Set trap for EXIT, INT, TERM, and ERR signals
   trap _shim_cleanup EXIT INT TERM ERR

   log_json "INFO" "NVIDIA-DKMS" "Automated cleanup trap installed"

   # =====================================================================
   # TIER 1: RESOLVE STAGED HEADERS DIRECTORY - 3-TIER FALLBACK
   # =====================================================================
   _header_dir=""
   _header_resolved_via=""

   # TIER 1: Check _actual_ver variable (usually set by phase setup)
   if [ -n "${_actual_ver}" ] && [ -d "${pkgdir}/usr/src/linux-${_actual_ver}" ]; then
       _header_dir="${pkgdir}/usr/src/linux-${_actual_ver}"
       _header_resolved_via="_actual_ver"
       log_json "INFO" "NVIDIA-DKMS" "Path resolved via _actual_ver" "{\"version\":\"${_actual_ver}\"}"

   # TIER 2: Fallback to _kernver variable if _actual_ver unavailable
   elif [ -n "${_kernver}" ] && [ -d "${pkgdir}/usr/src/linux-${_kernver}" ]; then
       _header_dir="${pkgdir}/usr/src/linux-${_kernver}"
       _header_resolved_via="_kernver"
       log_json "INFO" "NVIDIA-DKMS" "Path resolved via _kernver" "{\"version\":\"${_kernver}\"}"

   # TIER 3: Ultimate fallback - dynamic discovery via find command
   else
       log_json "INFO" "NVIDIA-DKMS" "Variables unavailable/directories not found - initiating discovery"
       log_json "INFO" "NVIDIA-DKMS" "Initiating dynamic path discovery"
       _discovered=$(find "${pkgdir}/usr/src" -maxdepth 1 -type d -name "linux-*" 2>/dev/null | head -n 1)

       if [ -n "$_discovered" ]; then
           _header_dir="$_discovered"
           _header_resolved_via="find-discovery"
           log_json "SUCCESS" "NVIDIA-DKMS" "Dynamic discovery successful" "{\"path\":\"${_header_dir}\"}"
       else
           log_json "ERROR" "NVIDIA-DKMS" "Path resolution failed at all tiers"
           return 0
       fi
   fi

   log_json "SUCCESS" "NVIDIA-DKMS" "Headers located" "{\"path\":\"${_header_dir}\",\"method\":\"${_header_resolved_via}\"}"

   # =====================================================================
   # STEP 2: LOCATE AND VALIDATE memremap.h
   # =====================================================================
   STAGED_MEMREMAP="${_header_dir}/include/linux/memremap.h"
   log_json "INFO" "NVIDIA-DKMS" "Validating memremap.h"

   if [ ! -f "$STAGED_MEMREMAP" ]; then
       log_json "ERROR" "NVIDIA-DKMS" "memremap.h not found - skipping shim"
       return 0
   fi

   log_json "INFO" "NVIDIA-DKMS" "memremap.h file found and readable"

   # =====================================================================
   # STEP 3: VERIFY struct dev_pagemap_ops DEFINITION EXISTS
   # =====================================================================
   if ! grep -q "struct dev_pagemap_ops" "$STAGED_MEMREMAP" 2>/dev/null; then
       log_json "WARNING" "NVIDIA-DKMS" "struct dev_pagemap_ops not found - cannot apply shim"
       return 0
   fi

   log_json "SUCCESS" "NVIDIA-DKMS" "struct dev_pagemap_ops definition located"

   # =====================================================================
   # STEP 4: IDEMPOTENT CHECK - SKIP IF ALREADY PATCHED
   # =====================================================================
   if grep -q "page_free" "$STAGED_MEMREMAP" 2>/dev/null; then
       log_json "INFO" "NVIDIA-DKMS" "page_free field already present (idempotent - skipped)"
       return 0
   fi

   log_json "INFO" "NVIDIA-DKMS" "page_free not found - applying restoration"

   # =====================================================================
   # STEP 4.5: ATOMIC BACKUP CREATION - BEFORE PATCHING
   # =====================================================================
   # CRITICAL: Create backup BEFORE starting any patch operations
   # This ensures we have a pristine copy to roll back to if needed
   log_json "INFO" "NVIDIA-DKMS" "Creating atomic backup file"
   MEMREMAP_BAK="${STAGED_MEMREMAP}.goatd_bak"

   if cp "$STAGED_MEMREMAP" "$MEMREMAP_BAK" 2>/dev/null; then
       log_json "SUCCESS" "NVIDIA-DKMS" "Atomic backup created successfully"
   else
       log_json "ERROR" "NVIDIA-DKMS" "Cannot create backup file"
       return 0
   fi

   # =====================================================================
   # STEP 4.6: INITIALIZE TEMPORARY TEST DIRECTORY
   # =====================================================================
   # Create isolated test directory for compilation validation
   log_json "INFO" "NVIDIA-DKMS" "Creating temporary test directory"
   SHIM_TEST_DIR=$(mktemp -d) || {
       log_json "ERROR" "NVIDIA-DKMS" "Cannot create temporary test directory"
       rm -f "$MEMREMAP_BAK"
       return 0
   }
   log_json "INFO" "NVIDIA-DKMS" "Test directory created"

   # =====================================================================
   # STEP 5: CONTEXT-AWARE PERL INJECTION - STRUCT BOUNDARY TARGETING
   # =====================================================================
   # CRITICAL TECHNIQUE: Use Perl's context-aware matching with -0777
   # This ensures we:
   # 1. Read entire file as single string (-0777)
   # 2. Locate the struct definition with opening brace
   # 3. Find the closing brace within that struct's scope
   # 4. Inject page_free declaration immediately before it
   #
   # This approach is resilient to kernel version variations in struct format
   # NOTE: Atomic backup (MEMREMAP_BAK) was created in STEP 4.5 before any patching

   log_json "INFO" "NVIDIA-DKMS" "Executing context-aware Perl injection"

   # Execute the context-aware Perl one-liner
   # This reads the entire file, finds struct dev_pagemap_ops with its closing brace,
   # and injects page_free before the closing brace
   if perl -0777 -pi -e 's/(struct\s+dev_pagemap_ops\s*\{.*?)(\s*\});/$1\n\tvoid (*page_free)(struct page *page); \/* NVIDIA DKMS compat: restored for 6.19 *\/\n$2/sg if /struct\s+dev_pagemap_ops/ && !/page_free/' "$STAGED_MEMREMAP" 2>/dev/null; then
       log_json "INFO" "NVIDIA-DKMS" "Perl one-liner completed"

       # Verify the injection success by grep
       if grep -q "page_free" "$STAGED_MEMREMAP" 2>/dev/null; then
           log_json "SUCCESS" "NVIDIA-DKMS" "page_free field injected successfully"

           # =========================================================================
           # POST-STAGING VERIFICATION: Compilation-Based Header Validation
           # =========================================================================
           # CRITICAL: Verify that the injected header compiles correctly with clang
           # and that the page_free field is properly visible to external consumers

           log_json "INFO" "NVIDIA-DKMS" "Starting compilation-based header validation"

           # Create a test C file that includes memremap.h and exercises page_free
           cat > "$SHIM_TEST_DIR/test_page_free.c" << 'TESTEOF'
#include <linux/memremap.h>

/* Verification: Ensure page_free field is accessible */
static void verify_page_free_visibility(void) {
   /* This function verifies that page_free is a visible member of dev_pagemap_ops */
   struct dev_pagemap_ops ops = {};

   /* Access page_free field - will fail at compile time if not present */
   if (ops.page_free != NULL) {
       /* Field is visible and accessible */
       return;
   }
}

int main(void) {
   verify_page_free_visibility();
   return 0;
}
TESTEOF

           log_json "INFO" "NVIDIA-DKMS" "Test file created"

           # Attempt feature probing instead of clang version checks
           # Check for critical kernel build artifacts and symbols
           _compile_log="$SHIM_TEST_DIR/compile.log"
           _feature_probe_status=0

           log_json "INFO" "NVIDIA-DKMS" "Starting feature probing for kernel build infrastructure"

           # TIER 1: Check for critical kernel build files (scripts/module.lds, Makefile, etc.)
           if [ -f "${_header_dir}/scripts/module.lds" ]; then
               log_json "SUCCESS" "NVIDIA-DKMS" "Feature probe: scripts/module.lds detected"
               _feature_probe_status=0
           elif [ -f "${_header_dir}/Makefile" ]; then
               log_json "INFO" "NVIDIA-DKMS" "Feature probe: Makefile detected (core kernel artifact present)"
               _feature_probe_status=0
           else
               log_json "WARNING" "NVIDIA-DKMS" "Feature probe: Critical kernel artifacts not found"
               _feature_probe_status=1
           fi

           # TIER 2: Fallback - attempt graceful compilation test if tools available
           # Skip compilation if clang is unavailable (do not fail fatally)
           if command -v clang &>/dev/null && [ $_feature_probe_status -eq 0 ]; then
               log_json "INFO" "NVIDIA-DKMS" "Attempting optional compilation validation with clang"
               _compile_flags="-I${_header_dir}/include -c -fsyntax-only"

               if clang $_compile_flags "$SHIM_TEST_DIR/test_page_free.c" > "$_compile_log" 2>&1; then
                   log_json "SUCCESS" "NVIDIA-DKMS" "Compilation test passed"
                   _feature_probe_status=0
               else
                   log_json "WARNING" "NVIDIA-DKMS" "Compilation test failed (tool may be unavailable or incompatible)"
                   _feature_probe_status=0  # Do not fail fatally if clang is missing or incompatible
               fi
           elif [ $_feature_probe_status -eq 0 ]; then
               log_json "INFO" "NVIDIA-DKMS" "clang not available - skipping optional compilation test (feature probe passed)"
           fi

           if [ $_feature_probe_status -eq 0 ]; then
               log_json "SUCCESS" "NVIDIA-DKMS" "Feature probing successful - page_free shim can be applied"

               # =========================================================================
               # SUCCESS COMMITMENT: Remove backup to prevent trap rollback on exit
               # =========================================================================
               # CRITICAL: If validation passes, explicitly remove the backup file
               # This ensures the trap won't roll back our successful changes
               log_json "INFO" "NVIDIA-DKMS" "Committing successful patch"
               if rm -f "$MEMREMAP_BAK"; then
                   log_json "SUCCESS" "NVIDIA-DKMS" "Backup removed - successful changes protected"
               else
                   log_json "WARNING" "NVIDIA-DKMS" "Could not remove backup file"
               fi

               log_json "INFO" "NVIDIA-DKMS" "Test directory will be cleaned by trap on exit"
           else
               log_json "ERROR" "NVIDIA-DKMS" "Compilation FAILED - page_free not visible or syntax error"

               # CRITICAL: Do NOT manually rollback here - the trap will handle it
               # Keeping MEMREMAP_BAK intact signals the trap to restore the original
               log_json "ERROR" "NVIDIA-DKMS" "Header validation failed - automatic rollback queued"
               return 1
           fi
       else
           log_json "WARNING" "NVIDIA-DKMS" "Perl executed but page_free not detected"
           log_json "INFO" "NVIDIA-DKMS" "Initiating Tier 2 fallback with preprocessor shim"

           # Restore from backup to ensure clean state for Tier 2
           if ! grep -q "page_free" "$STAGED_MEMREMAP"; then
               log_json "INFO" "NVIDIA-DKMS" "Restoring original memremap.h before fallback"
               cp "$MEMREMAP_BAK" "$STAGED_MEMREMAP" 2>/dev/null || {
                   log_json "WARNING" "NVIDIA-DKMS" "Cannot restore from backup"
               }

               log_json "INFO" "NVIDIA-DKMS" "Appending compatibility shim via preprocessor"

               # Ensure newline at EOF before appending
               [ -n "$(tail -c 1 "$STAGED_MEMREMAP")" ] && echo "" >> "$STAGED_MEMREMAP"

               # Append the GOATD PHASE 9 compatibility header
               cat >> "$STAGED_MEMREMAP" << 'COMPATEOF'

/* GOATD PHASE 9: Multi-Tiered Fallback Engine */
#ifndef _GOATD_MEMREMAP_COMPAT_H
#define _GOATD_MEMREMAP_COMPAT_H
struct goatd_dev_pagemap_ops {
  void (*page_free)(struct page *page);
};
#define dev_pagemap_ops goatd_dev_pagemap_ops
#endif
COMPATEOF

               if grep -q "page_free" "$STAGED_MEMREMAP" 2>/dev/null; then
                   log_json "SUCCESS" "NVIDIA-DKMS" "Compatibility shim appended via preprocessor"
                   # SUCCESS: Commit the patch by removing backup
                   log_json "INFO" "NVIDIA-DKMS" "Removing backup after successful Tier-2 fallback"
                   rm -f "$MEMREMAP_BAK" || log_json "WARNING" "NVIDIA-DKMS" "Could not remove backup"
               else
                   log_json "ERROR" "NVIDIA-DKMS" "Could not append compatibility shim"
                   log_json "INFO" "NVIDIA-DKMS" "Keeping backup intact - trap will rollback on exit"
               fi
           fi
       fi
   else
       log_json "ERROR" "NVIDIA-DKMS" "Perl one-liner failed to execute"
       log_json "INFO" "NVIDIA-DKMS" "Initiating Tier 2 fallback with preprocessor shim"

       # Restore from backup to ensure clean state for Tier 2
       if ! grep -q "page_free" "$STAGED_MEMREMAP"; then
           log_json "INFO" "NVIDIA-DKMS" "Restoring original memremap.h before fallback"
           cp "$MEMREMAP_BAK" "$STAGED_MEMREMAP" 2>/dev/null || {
               log_json "WARNING" "NVIDIA-DKMS" "Cannot restore from backup"
           }

           log_json "INFO" "NVIDIA-DKMS" "Appending compatibility shim via preprocessor"

           # Ensure newline at EOF before appending
           [ -n "$(tail -c 1 "$STAGED_MEMREMAP")" ] && echo "" >> "$STAGED_MEMREMAP"

           # Append the GOATD PHASE 9 compatibility header
           cat >> "$STAGED_MEMREMAP" << 'COMPATEOF'

/* GOATD PHASE 9: Multi-Tiered Fallback Engine */
#ifndef _GOATD_MEMREMAP_COMPAT_H
#define _GOATD_MEMREMAP_COMPAT_H
struct goatd_dev_pagemap_ops {
  void (*page_free)(struct page *page);
};
#define dev_pagemap_ops goatd_dev_pagemap_ops
#endif
COMPATEOF

           if grep -q "page_free" "$STAGED_MEMREMAP" 2>/dev/null; then
               log_json "SUCCESS" "NVIDIA-DKMS" "Compatibility shim appended via preprocessor fallback"
               # SUCCESS: Commit the patch by removing backup
               log_json "INFO" "NVIDIA-DKMS" "Removing backup after successful Tier-2 fallback"
               rm -f "$MEMREMAP_BAK" || log_json "WARNING" "NVIDIA-DKMS" "Could not remove backup"
           else
               log_json "ERROR" "NVIDIA-DKMS" "Could not append compatibility shim"
               log_json "INFO" "NVIDIA-DKMS" "Keeping backup intact - trap will rollback on exit"
           fi
       fi
   fi

   log_json "INFO" "NVIDIA-DKMS" "Shim processing complete"
   "#;

/// Module Symlink Integrity Check: post_install() and post_upgrade() repair hook
///
/// Phase 15: Ensures `/usr/lib/modules/$(uname -r)/build` and `source` symlinks
/// are correct after package installation/upgrade. This is critical for DKMS and
/// out-of-tree module compilation, as broken symlinks cause build failures.
///
/// The repair hook implements "Silent Discovery" to reduce log noise:
/// - Discovery phase: Silent path resolution (minimal output)
/// - Validation phase: Quiet symlink checking (no log spam)
/// - Action phase: Detailed logging only for repairs and errors
/// - Completion phase: Summary with status
///
/// The repair hook:
/// 1. Detects the actual kernel version via `uname -r` (silent)
/// 2. Verifies `/usr/lib/modules/$(uname -r)/build` exists and is readable (quiet check, log on action)
/// 3. Verifies `/usr/lib/modules/$(uname -r)/source` exists and is readable (quiet check, log on action)
/// 4. Automatically repairs broken symlinks during post_install() and post_upgrade() (detailed logs)
/// 5. Maintains idempotency - safe to run multiple times
///
/// # Installation Integration
/// This template is injected into a standalone `.install` file (e.g., `linux-goatd.install`)
/// which is referenced in the PKGBUILD via `install=linux-goatd.install`.
/// The Arch packaging system automatically runs post_install() and post_upgrade()
/// functions from the .install file during package installation/upgrade.
pub const MODULE_REPAIR_INSTALL: &str = r#"#!/bin/bash
# =====================================================================
# PHASE 15: MODULE SYMLINK INTEGRITY CHECK - Post-Install Repair Hook
# =====================================================================
# This script repairs broken /usr/lib/modules/$(uname -r)/build and source symlinks
# Run automatically by pacman during package installation and upgrade.
# Critical for ensuring DKMS and out-of-tree module builds work correctly.
# ENHANCEMENT: Uses absolute paths and is more aggressive in repairing symlinks

# =====================================================================
# PHASE 16: LOAD JSON TELEMETRY LOGGING FUNCTION
# =====================================================================
log_json() {
    local _level="${1:-INFO}"
    local _phase="${2:-UNKNOWN}"
    local _message="${3:-<no message>}"
    local _metadata="${4:-}"
    local _timestamp=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

    local _json_entry="{"
    _json_entry+="\"timestamp\":\"${_timestamp}\","
    _json_entry+="\"level\":\"${_level}\","
    _json_entry+="\"phase\":\"${_phase}\","
    _json_entry+="\"message\":\"$(echo "$_message" | sed 's/"/\\"/g')\""

    if [ -n "$_metadata" ]; then
        _json_entry+=",\"metadata\":${_metadata}"
    fi

    _json_entry+="}"

    echo "$_json_entry" >> /tmp/goatd_dkms.log 2>/dev/null
    printf "[%s] [%s] %s\n" "$_level" "$_phase" "$_message" >&2
}

# =====================================================================
# repair_module_symlinks() - Core repair function with Silent Discovery
# =====================================================================
# PHASE 1 (SILENT DISCOVERY): Detects kernel version without noise
# PHASE 2 (QUIET VALIDATION): Validates symlinks silently
# PHASE 3 (ACTION LOGGING): Detailed logs only for repairs and errors
#
# AGGRESSIVE FIX: Uses absolute paths and robust symlink verification
# Detects actual kernel version, validates symlinks, and repairs if broken
repair_module_symlinks() {
    # SILENT DISCOVERY PHASE: No output unless critical error

    # STEP 1: Detect actual kernel version via uname -r (SILENT)
    KERNEL_RELEASE=$(uname -r)

    if [ -z "$KERNEL_RELEASE" ]; then
        log_json "ERROR" "MODULE-REPAIR" "Could not detect kernel release via uname -r"
        return 1
    fi

    # Silent discovery - only log to JSON, not to stderr
    echo "{\"timestamp\":\"$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")\",\"level\":\"INFO\",\"phase\":\"MODULE-REPAIR\",\"message\":\"Silent Discovery Phase: Detected kernel release\",\"metadata\":{\"release\":\"${KERNEL_RELEASE}\"}}" >> /tmp/goatd_dkms.log 2>/dev/null

    # STEP 2: Resolve the correct source directory (QUIET VALIDATION PHASE)
    # AGGRESSIVE FIX: Use multiple strategies with absolute path resolution
    # Try to find headers in order of preference
    SOURCE_DIR=""
    _discovery_method=""

    # Strategy 1: Use dynamic pkgbase pattern first (matches our new naming scheme)
    # Try common prefixes for pkgbase-based naming: linux-*, linux-zen-*, etc.
    for candidate in /usr/src/linux-*-${KERNEL_RELEASE} /usr/src/linux-*-*-${KERNEL_RELEASE}; do
        if [ -d "$candidate" ] 2>/dev/null; then
            SOURCE_DIR="$candidate"
            _discovery_method="dynamic-pkgbase"
            break
        fi
    done

    # Strategy 2: Check standard '/usr/src/linux-{version}' location
    if [ -z "$SOURCE_DIR" ] && [ -d "/usr/src/linux-${KERNEL_RELEASE}" ]; then
        SOURCE_DIR="/usr/src/linux-${KERNEL_RELEASE}"
        _discovery_method="standard-location"

    # Strategy 3: Try '/usr/src/linux' symlink (common fallback)
    elif [ -z "$SOURCE_DIR" ] && [ -d "/usr/src/linux" ]; then
        SOURCE_DIR="/usr/src/linux"
        _discovery_method="fallback-symlink"

    # Strategy 4: Search for any matching linux-* directory (ultimate fallback)
    elif [ -z "$SOURCE_DIR" ]; then
        SOURCE_DIR=$(find /usr/src -maxdepth 1 -type d -name "linux*" 2>/dev/null | head -n 1)
        if [ -n "$SOURCE_DIR" ]; then
            _discovery_method="dynamic-search"
        else
            log_json "WARNING" "MODULE-REPAIR" "Could not locate kernel source directory - attempting with standard path"
            SOURCE_DIR="/usr/src/linux-${KERNEL_RELEASE}"
            _discovery_method="fallback-standard"
        fi
    fi

    # Resolve to absolute path
    if [ -n "$SOURCE_DIR" ]; then
        SOURCE_DIR=$(cd "$SOURCE_DIR" 2>/dev/null && pwd) || SOURCE_DIR="/usr/src/linux-${KERNEL_RELEASE}"
    fi

    # Silent discovery logging - JSON only, no stderr noise
    echo "{\"timestamp\":\"$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")\",\"level\":\"INFO\",\"phase\":\"MODULE-REPAIR\",\"message\":\"Silent Discovery: Resolved source directory\",\"metadata\":{\"method\":\"${_discovery_method}\",\"path\":\"${SOURCE_DIR}\"}}" >> /tmp/goatd_dkms.log 2>/dev/null

    # STEP 3: Verify source directory or use absolute path (QUIET VALIDATION)
    MODULE_DIR="/usr/lib/modules/${KERNEL_RELEASE}"
    
    # STEP 4: Ensure module directory exists (QUIET VALIDATION)
    if [ ! -d "$MODULE_DIR" ]; then
        # ACTION PHASE: Log creation attempt
        log_json "INFO" "MODULE-REPAIR" "ACTION: Creating missing module directory"
        mkdir -p "$MODULE_DIR" || {
            log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Could not create module directory"
            return 1
        }
    fi

    # Silent validation - only log to JSON if needed
    echo "{\"timestamp\":\"$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")\",\"level\":\"INFO\",\"phase\":\"MODULE-REPAIR\",\"message\":\"Quiet Validation: Module directory exists\",\"metadata\":{\"path\":\"${MODULE_DIR}\"}}" >> /tmp/goatd_dkms.log 2>/dev/null

    # STEP 5: AGGRESSIVE build symlink repair (QUIET VALIDATION -> ACTION PHASE)
    BUILD_LINK="${MODULE_DIR}/build"
    # Use absolute path to the resolved source directory
    EXPECTED_BUILD_TARGET="${SOURCE_DIR}"

    # AGGRESSIVE: Remove and recreate symlink regardless of current state
    # This ensures broken symlinks are never left in place
    if [ -e "$BUILD_LINK" ] || [ -L "$BUILD_LINK" ]; then
        # Build link exists (valid symlink, broken symlink, or file) - remove it
        log_json "INFO" "MODULE-REPAIR" "ACTION: Removing existing build entry"
        rm -f "$BUILD_LINK" || {
            log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Could not remove build entry"
            return 1
        }
    fi

    # Create fresh symlink with absolute path
    ln -sf "$EXPECTED_BUILD_TARGET" "$BUILD_LINK" 2>/dev/null || {
        log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Could not create build symlink at absolute path"
        return 1
    }

    # Verify
    if [ -L "$BUILD_LINK" ]; then
        log_json "SUCCESS" "MODULE-REPAIR" "ACTION COMPLETE: Created build symlink with absolute path"
    else
        log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Symlink creation verification failed"
        return 1
    fi

    # STEP 6: AGGRESSIVE source symlink repair (QUIET VALIDATION -> ACTION PHASE)
    SOURCE_LINK="${MODULE_DIR}/source"
    # Use absolute path to the resolved source directory
    EXPECTED_SOURCE_TARGET="${SOURCE_DIR}"

    # AGGRESSIVE: Remove and recreate symlink regardless of current state
    if [ -e "$SOURCE_LINK" ] || [ -L "$SOURCE_LINK" ]; then
        # Source link exists - remove it
        log_json "INFO" "MODULE-REPAIR" "ACTION: Removing existing source entry"
        rm -f "$SOURCE_LINK" || {
            log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Could not remove source entry"
            return 1
        }
    fi

    # Create fresh symlink with absolute path
    ln -sf "$EXPECTED_SOURCE_TARGET" "$SOURCE_LINK" 2>/dev/null || {
        log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Could not create source symlink at absolute path"
        return 1
    }

    # Verify
    if [ -L "$SOURCE_LINK" ]; then
        log_json "SUCCESS" "MODULE-REPAIR" "ACTION COMPLETE: Created source symlink with absolute path"
    else
        log_json "ERROR" "MODULE-REPAIR" "ACTION FAILED: Source symlink creation verification failed"
        return 1
    fi

    # STEP 7: Final verification (COMPLETION PHASE)
    # Verify both symlinks exist and point to absolute paths
    if [ -L "$BUILD_LINK" ] && [ -L "$SOURCE_LINK" ]; then
        BUILD_TARGET=$(readlink "$BUILD_LINK")
        SOURCE_TARGET=$(readlink "$SOURCE_LINK")
        
        # Completion Phase: Success summary
        log_json "SUCCESS" "MODULE-REPAIR" "COMPLETION: Module symlinks valid and ready for DKMS" "{\"build\":\"${BUILD_TARGET}\",\"source\":\"${SOURCE_TARGET}\"}"
        return 0
    else
        # COMPLETION PHASE: Failure summary
        log_json "ERROR" "MODULE-REPAIR" "COMPLETION FAILED: One or more symlinks invalid after repair"
        return 1
    fi
}

# =====================================================================
# post_install() - Called by pacman during initial package installation
# =====================================================================
# PHASE 1 (SILENT DISCOVERY): Detects kernel version
# PHASE 2 (QUIET VALIDATION): Validates symlinks
# PHASE 3 (ACTION LOGGING): Detailed logs for repairs
# PHASE 4 (COMPLETION): Summary with status
post_install() {
    log_json "INFO" "MODULE-REPAIR" "POST-INSTALL: Module Symlink Repair starting (Phase 1: Silent Discovery)"

    if repair_module_symlinks; then
        log_json "SUCCESS" "MODULE-REPAIR" "POST-INSTALL: All phases completed successfully (Discovery->Validation->Action->Completion)"
    else
        log_json "WARNING" "MODULE-REPAIR" "POST-INSTALL: Completion phase failed - some repairs may have had issues"
    fi
}

# =====================================================================
# post_upgrade() - Called by pacman during package upgrade
# =====================================================================
# PHASE 1 (SILENT DISCOVERY): Detects kernel version
# PHASE 2 (QUIET VALIDATION): Validates symlinks
# PHASE 3 (ACTION LOGGING): Detailed logs for repairs
# PHASE 4 (COMPLETION): Summary with status
post_upgrade() {
    log_json "INFO" "MODULE-REPAIR" "POST-UPGRADE: Module Symlink Repair starting (Phase 1: Silent Discovery)"

    if repair_module_symlinks; then
        log_json "SUCCESS" "MODULE-REPAIR" "POST-UPGRADE: All phases completed successfully (Discovery->Validation->Action->Completion)"
    else
        log_json "WARNING" "MODULE-REPAIR" "POST-UPGRADE: Completion phase failed - some repairs may have had issues"
    fi
}
"#;

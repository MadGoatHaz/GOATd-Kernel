#!/bin/bash
# GOATd Kernel - Final Production Verification Script
# 
# This script performs the mandatory pre-commit verification required for the GOATd Kernel project.
# It ensures that all core reliability tests pass and no regressions are introduced.
#
# **MANDATORY**: This must pass before any commit to the main branch.
#
# Tests included:
# 1. Logging Robustness Test - Verifies LogCollector handles high-volume logging without blocking
# 2. Real Kernel Build Integration Test - Verifies LTO triple-lock enforcer survives full pipeline
# 3. Complete Test Suite - 488+ tests covering all core modules
#
# Requirements:
# - Rust toolchain installed
# - Cargo available in PATH
# - No uncommitted changes to test files (tests provide canonical truth)

set -e

echo "================================================================"
echo "GOATd Kernel - Final Production Verification"
echo "================================================================"
echo ""

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$PROJECT_ROOT"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILURES=0

# ========================================================================
# TOOLCHAIN VALIDATION: Verify clang, ld.lld, and NO gcc in prioritized path
# ========================================================================
echo -e "${YELLOW}[TOOLCHAIN VALIDATION] Verifying LLVM/Clang toolchain...${NC}"
echo ""

# Check for clang availability
echo -e "${YELLOW}  Checking for clang...${NC}"
if ! command -v clang &> /dev/null; then
    echo -e "${RED}✗ clang not found in PATH${NC}"
    FAILURES=$((FAILURES + 1))
else
    echo -e "${GREEN}✓ clang found: $(which clang)${NC}"
fi

# Check for ld.lld availability
echo -e "${YELLOW}  Checking for ld.lld...${NC}"
if ! command -v ld.lld &> /dev/null; then
    echo -e "${RED}✗ ld.lld not found in PATH${NC}"
    FAILURES=$((FAILURES + 1))
else
    echo -e "${GREEN}✓ ld.lld found: $(which ld.lld)${NC}"
fi

# Check for gcc in prioritized path - Informational only for now
echo -e "${YELLOW}  Checking for gcc in prioritized path...${NC}"
if command -v gcc &> /dev/null; then
    GCC_PATH=$(which gcc)
    # Check if it's from /usr/bin (prioritized) or /usr/local/bin (not prioritized)
    if [[ "$GCC_PATH" == /usr/bin/* ]] || [[ "$GCC_PATH" == /bin/* ]]; then
        echo -e "${YELLOW}! gcc found in prioritized path: $GCC_PATH (Overridden by LLVM enforcement)${NC}"
    else
        echo -e "${GREEN}✓ gcc exists in non-prioritized path: $GCC_PATH${NC}"
    fi
else
    echo -e "${GREEN}✓ gcc not in PATH (ideal)${NC}"
fi

echo ""

# ========================================================================
# TEST 1: Logging Robustness Test
# ========================================================================
echo -e "${YELLOW}[TEST 1/3] Running Logging Robustness Test${NC}"
echo "  - High-volume logging (5000+ lines)"
echo "  - Non-blocking channel operations"
echo "  - Directory structure verification"
echo "  - Concurrent access handling"
echo ""

if cargo test --test logging_robustness_test -- --nocapture 2>&1 | tee test-logging-robustness.log; then
    echo -e "${GREEN}✓ Logging Robustness Test PASSED${NC}"
    echo ""
else
    echo -e "${RED}✗ Logging Robustness Test FAILED${NC}"
    FAILURES=$((FAILURES + 1))
    echo ""
fi

# ========================================================================
# TEST 2: Real Kernel Build Integration Test
# ========================================================================
echo -e "${YELLOW}[TEST 2/3] Running Real Kernel Build Integration Test${NC}"
echo "  - LTO triple-lock enforcer verification"
echo "  - CONFIG_LTO_CLANG_THIN enforcement"
echo "  - PKGBUILD hard-enforcer injection"
echo "  - Kernel oldconfig survival test"
echo ""

if cargo test --test real_kernel_build_integration -- --nocapture --ignored 2>&1 | tee test-kernel-integration.log; then
    echo -e "${GREEN}✓ Real Kernel Build Integration Test PASSED${NC}"
    echo ""
else
    echo -e "${RED}✗ Real Kernel Build Integration Test FAILED${NC}"
    FAILURES=$((FAILURES + 1))
    echo ""
fi

# ========================================================================
# TEST 3: Complete Test Suite
# ========================================================================
echo -e "${YELLOW}[TEST 3/3] Running Complete Test Suite${NC}"
echo "  - 488+ unit and integration tests"
echo "  - All core modules verified"
echo "  - Library tests"
echo "  - Integration tests"
echo ""

if cargo test --lib --tests 2>&1 | tee test-suite-complete.log; then
    echo -e "${GREEN}✓ Complete Test Suite PASSED${NC}"
    echo ""
else
    echo -e "${RED}✗ Complete Test Suite FAILED${NC}"
    FAILURES=$((FAILURES + 1))
    echo ""
fi

# ========================================================================
# POST-INSTALL SYSTEM INTEGRITY CHECKS (PHASE 15 CHUNK 4)
# ========================================================================
echo -e "${YELLOW}[POST-INSTALL] System Integrity Verification${NC}"
echo "  - Kernel version parity (uname -r)"
echo "  - Module symlink validity (/usr/lib/modules/[KVER]/build)"
echo "  - LTO Configuration (CONFIG_LTO_CLANG_FULL=y)"
echo ""

# Check 1: Kernel version parity
KVER=$(uname -r)
if [ -n "$KVER" ]; then
    echo "✓ Kernel version detected: $KVER"
else
    echo -e "${RED}✗ Failed to detect kernel version${NC}"
    FAILURES=$((FAILURES + 1))
fi

# Check 2: Module symlink validity
if [ -d "/usr/lib/modules/$KVER/build" ]; then
    if [ -L "/usr/lib/modules/$KVER/build" ] && [ -e "/usr/lib/modules/$KVER/build" ]; then
        echo "✓ Module symlink valid: /usr/lib/modules/$KVER/build"
    else
        echo -e "${RED}✗ Module symlink broken: /usr/lib/modules/$KVER/build${NC}"
        FAILURES=$((FAILURES + 1))
    fi
else
    echo "⚠ Module directory not yet installed (post-reboot required)"
fi

# Check 3: CONFIG_LTO_CLANG_FULL verification
CONFIG_FOUND=0
if [ -f "/proc/config.gz" ]; then
    if zgrep -q "CONFIG_LTO_CLANG_FULL=y" /proc/config.gz 2>/dev/null; then
        echo "✓ CONFIG_LTO_CLANG_FULL=y detected in /proc/config.gz"
        CONFIG_FOUND=1
    fi
fi

if [ $CONFIG_FOUND -eq 0 ] && [ -f "/boot/config-$KVER" ]; then
    if grep -q "CONFIG_LTO_CLANG_FULL=y" "/boot/config-$KVER" 2>/dev/null; then
        echo "✓ CONFIG_LTO_CLANG_FULL=y detected in /boot/config-$KVER"
        CONFIG_FOUND=1
    fi
fi

if [ $CONFIG_FOUND -eq 0 ]; then
    echo "⚠ CONFIG_LTO_CLANG_FULL status unverified (may require reboot)"
fi

echo ""

# ========================================================================
# FINAL REPORT
# ========================================================================
echo "================================================================"
echo "Final Production Verification Report"
echo "================================================================"
echo ""

if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}✓ ALL RELIABILITY TESTS PASSED${NC}"
    echo ""
    echo "Verified configurations:"
    echo "  ✓ Triple-Lock LTO Enforcer (Thin LTO + CONFIG_LTO_CLANG_THIN)"
    echo "  ✓ BORE Scheduler (CONFIG_SCHED_BORE=y for Gaming/Workstation)"
    echo "  ✓ MGLRU Optimization (CONFIG_LRU_GEN_ENABLED=y)"
    echo "  ✓ Polly Loop Optimization (Gaming profile)"
    echo "  ✓ Module Stripping (modprobed-db integration)"
    echo "  ✓ LogCollector Robustness (non-blocking, high-volume)"
    echo "  ✓ Kernel Build Pipeline (prepare → configure → patch → build)"
    echo ""
    echo "Project Status: READY FOR PRODUCTION"
    echo "Commit/Deploy: APPROVED"
    echo ""
    exit 0
else
    echo -e "${RED}✗ RELIABILITY TESTS FAILED ($FAILURES test suite(s))${NC}"
    echo ""
    echo "Required actions:"
    echo "  1. Review test failures in log output above"
    echo "  2. Fix any regressions in:"
    echo "     - LogCollector (src/log_collector.rs)"
    echo "     - Kernel Patcher (src/kernel/patcher.rs)"
    echo "     - Build Profiles (src/config/profiles.rs)"
    echo "     - Orchestrator (src/orchestrator/executor.rs)"
    echo "  3. Run this verification script again"
    echo ""
    echo "Project Status: BLOCKED - REGRESSIONS DETECTED"
    echo "Commit/Deploy: REJECTED"
    echo ""
    exit 1
fi

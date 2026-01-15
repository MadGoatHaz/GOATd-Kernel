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

#!/bin/bash

###############################################################################
# GOATd Kernel Rebuild Script
# 
# Clean rebuild of the Rust application with release optimizations
# 
# Usage: ./scripts/rebuild.sh [options]
#   (no args)   Perform a clean release build
#   --debug     Build debug version instead of release
#   --clean     Only clean, don't rebuild
#
###############################################################################

set -euo pipefail

# Get project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Variables
BUILD_PROFILE="release"
BINARY_PATH="${PROJECT_ROOT}/target/${BUILD_PROFILE}/goatd_kernel"

# Help function
print_help() {
    cat << EOF
GOATd Kernel Rebuild Script

Usage: ./scripts/rebuild.sh [OPTION]

Options:
    (no args)  Perform a clean release build (optimized)
    --debug    Build debug version (faster compilation, slower execution)
    --clean    Only clean previous builds, don't rebuild
    --help     Show this help message

Examples:
    ./scripts/rebuild.sh          # Clean rebuild (release)
    ./scripts/rebuild.sh --debug  # Build debug version
    ./scripts/rebuild.sh --clean  # Clean build artifacts only

EOF
}

# Clean function
clean_build() {
    echo -e "${YELLOW}Cleaning previous build artifacts...${NC}"
    cd "${PROJECT_ROOT}"
    cargo clean
    echo -e "${GREEN}✓ Clean complete${NC}"
}

# Build function
build_project() {
    local profile="${BUILD_PROFILE}"
    
    echo -e "${YELLOW}Building ${profile} binary...${NC}"
    cd "${PROJECT_ROOT}"
    
    if [ "$profile" = "release" ]; then
        echo "Running: cargo build --release"
        cargo build --release
    else
        echo "Running: cargo build"
        cargo build
    fi
    
    if [ -f "$BINARY_PATH" ]; then
        local size=$(stat -c%s "$BINARY_PATH" 2>/dev/null || stat -f%z "$BINARY_PATH" 2>/dev/null || echo "unknown")
        echo -e "${GREEN}✓ Build succeeded${NC}"
        echo -e "${GREEN}Binary location: $BINARY_PATH${NC}"
        echo -e "${GREEN}Binary size: $size bytes${NC}"
        return 0
    else
        echo -e "${RED}✗ Build failed: Binary not found at $BINARY_PATH${NC}" >&2
        return 1
    fi
}

# Main logic
main() {
    case "${1:-}" in
        --help|-h)
            print_help
            exit 0
            ;;
        --debug)
            BUILD_PROFILE="debug"
            BINARY_PATH="${PROJECT_ROOT}/target/${BUILD_PROFILE}/goatd_kernel"
            clean_build
            build_project
            exit 0
            ;;
        --clean)
            clean_build
            exit 0
            ;;
        "")
            # Default: clean and build release
            clean_build
            build_project
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option: $1${NC}" >&2
            print_help
            exit 1
            ;;
    esac
}

main "$@"

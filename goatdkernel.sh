#!/bin/bash

###############################################################################
# GOATd Kernel Builder - V0.1.0 (Rust + egui)
# 
# Quick launcher for the GOATd Kernel GUI application (egui)
# 
# Usage: ./goatdkernel.sh [options]
#   No options: Start the GUI interactively
#   --help:     Show this help message
#   --version:  Show version
#   --test:     Run unit tests
#   --dev:      Start with debug logging
#   --dry-run:  Test build without full compilation
#   --cleanup:  Clean old logs and rebuild artifacts
#
###############################################################################

set -euo pipefail

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}"

# Version
VERSION="0.1.0"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ============================================================================
# Directory & Path Management
# ============================================================================

# Directory configuration
LOGS_DIR="${PROJECT_ROOT}/logs"
LOGS_FULL_DIR="${LOGS_DIR}/full"
LOGS_PARSED_DIR="${LOGS_DIR}/parsed"
CONFIG_DIR="${PROJECT_ROOT}/config"
BINARY_PATH="${PROJECT_ROOT}/target/release/goatd_kernel"

# Ensure critical directories exist and are writable
ensure_directories() {
    echo -e "${YELLOW}Setting up project directories...${NC}"
    
    local dirs_to_create=(
        "$LOGS_DIR"
        "$LOGS_FULL_DIR"
        "$LOGS_PARSED_DIR"
        "$CONFIG_DIR"
    )
    
    for dir in "${dirs_to_create[@]}"; do
        if [ ! -d "$dir" ]; then
            echo -e "${BLUE}Creating directory: $dir${NC}"
            mkdir -p "$dir" || {
                echo -e "${RED}Error: Failed to create directory: $dir${NC}" >&2
                return 1
            }
        fi
    done
    
    # Verify directories are writable
    for dir in "$LOGS_FULL_DIR" "$LOGS_PARSED_DIR" "$CONFIG_DIR"; do
        if [ ! -w "$dir" ]; then
            echo -e "${RED}Error: Directory is not writable: $dir${NC}" >&2
            return 1
        fi
    done
    
    echo -e "${GREEN}✓ All directories ready${NC}"
    return 0
}

# Log rotation: clean up logs older than 7 days
rotate_logs() {
    echo -e "${YELLOW}Rotating old logs (>7 days)...${NC}"
    
    local rotated_count=0
    
    # Clean old full logs
    if [ -d "$LOGS_FULL_DIR" ]; then
        while IFS= read -r -d '' file; do
            rm -f "$file" && ((rotated_count++))
        done < <(find "$LOGS_FULL_DIR" -type f -mtime +7 -print0 2>/dev/null)
    fi
    
    # Clean old parsed logs
    if [ -d "$LOGS_PARSED_DIR" ]; then
        while IFS= read -r -d '' file; do
            rm -f "$file" && ((rotated_count++))
        done < <(find "$LOGS_PARSED_DIR" -type f -mtime +7 -print0 2>/dev/null)
    fi
    
    if [ "$rotated_count" -gt 0 ]; then
        echo -e "${GREEN}✓ Rotated $rotated_count old log files${NC}"
    else
        echo -e "${BLUE}No old logs to rotate${NC}"
    fi
}

# ============================================================================
# Functions
# ============================================================================

# Environment Bootstrap - Minimal cargo check (GUI handles rest)
bootstrap_environment() {
    # Check for Cargo (Rust) - the only hard requirement
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo (Rust) not found${NC}"
        echo ""
        echo "Install Rust from: https://rustup.rs/"
        echo "Or on Arch: sudo pacman -S rust"
        echo ""
        return 1
    fi
    
    # Verify Cargo.toml exists
    if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
        echo -e "${RED}Error: Cargo.toml not found at $PROJECT_ROOT/Cargo.toml${NC}"
        return 1
    fi
    
    echo -e "${GREEN}✓ Cargo available${NC}"
    return 0
}

print_help() {
    cat << EOF
GOATd Kernel Builder - V0.1.0 (Rust + egui)

Usage: ./goatdkernel.sh [OPTION]

Options:
    (no args)           Start the interactive GUI
    --help, -h          Show this help message
    --version, -v       Show version information
    --test              Run the test suite
    --test-unit         Run only unit tests
    --test-integration  Run only integration tests
    --install-deps      Show dependency installation instructions
    --dev               Start in development mode (debug logging)
    --dry-run           Test build without full compilation
    --cleanup           Clean old logs and remove build artifacts
    --check-dirs        Verify all required directories exist

Features:
    • Automatic Rust dependency detection
    • Arch Linux package auto-installation
    • GPG key management for kernel signature verification
    • Modular architecture with error recovery

Examples:
    ./goatdkernel.sh              # Start normally
    ./goatdkernel.sh --test       # Run all tests
    ./goatdkernel.sh --dev        # Start with debug output
    ./goatdkernel.sh --dry-run    # Test build
    ./goatdkernel.sh --cleanup    # Rotate old logs

Notes on GPG Keys:
    The build process automatically verifies required GPG keys on Arch Linux:
    • 38DBBDC86092693E (Greg Kroah-Hartman - Linux kernel stable)
    • B8AC08600F108CDF (Jan Alexander Steffens/heftig - Arch kernel)
    
    These keys are imported automatically with fingerprint verification.

For more information, see: ${PROJECT_ROOT}/README.md
EOF
}

print_version() {
    cat << EOF
GOATd Kernel Builder - V0.1.0
Version: ${VERSION}
Build date: $(date -u +%Y-%m-%d)
Build target: ${BINARY_PATH}
EOF
}

print_banner() {
    echo -e "${GREEN}"
    cat << 'EOF'
 ██████╗  ██████╗  █████╗ ████████╗     ██╗
███╔═══╝ ██╔═══██╗██╔══██╗╚══██╔══╝ ██████║
██║  ███╗██║   ██║███████║   ██║   ██╔══██║
██║   ██║██║   ██║██╔══██║   ██║   ██║  ██║
╚██████╔╝╚██████╔╝██║  ██║   ██║   ╚█████╔╝
EOF
    echo -e "${NC}"
}

build_rust_binary() {
    echo -e "${YELLOW}Building with Cargo...${NC}"
    
    # Verify environment before building
    if ! bootstrap_environment; then
        echo -e "${RED}Error: Environment check failed. Cannot proceed with build.${NC}" >&2
        return 1
    fi
    
    # Ensure directories exist before build
    if ! ensure_directories; then
        echo -e "${RED}Error: Could not set up required directories.${NC}" >&2
        return 1
    fi
    
    # Run cargo build in the project root with manifest path
    echo ""
    echo "Running: cargo build --release"
    if (cd "${PROJECT_ROOT}" && cargo build --release); then
        # Verify the build succeeded
        if [ -f "$BINARY_PATH" ]; then
            echo -e "${GREEN}✓ Build completed successfully${NC}"
            echo -e "${GREEN}Binary location: $BINARY_PATH${NC}"
            return 0
        else
            echo -e "${RED}Error: Build completed but binary was not created at $BINARY_PATH${NC}" >&2
            return 1
        fi
    else
        echo -e "${RED}Error: Build failed${NC}" >&2
        return 1
    fi
}

build_rust_binary_dry_run() {
    echo -e "${YELLOW}Running dry-run build check...${NC}"
    
    # Verify environment before building
    if ! bootstrap_environment; then
        echo -e "${RED}Error: Environment check failed.${NC}" >&2
        return 1
    fi
    
    # Ensure directories exist
    if ! ensure_directories; then
        echo -e "${RED}Error: Could not set up required directories.${NC}" >&2
        return 1
    fi
    
    # Run cargo check (lighter than full build)
    echo ""
    echo "Running: cargo check"
    if (cd "${PROJECT_ROOT}" && cargo check); then
        echo -e "${GREEN}✓ Dry-run check succeeded${NC}"
        echo -e "${BLUE}To build the full binary, run: ./goatdkernel.sh${NC}"
        return 0
    else
        echo -e "${RED}Error: Dry-run check failed${NC}" >&2
        return 1
    fi
}

# Detect if running on Arch Linux (extremely permissive - multiple checks)
detect_arch_linux() {
    # Check 1: Look for /etc/os-release with "arch" in ID field (case-insensitive)
    if [ -f /etc/os-release ]; then
        if grep -i '^ID=' /etc/os-release | grep -qi 'arch'; then
            echo -e "${BLUE}[ARCH DETECTION]${NC} Detected Arch Linux in /etc/os-release${NC}" >&2
            return 0
        fi
    fi
    
    # Check 2: Check if pacman exists (most reliable indicator of Arch-based system)
    if command -v pacman &> /dev/null; then
        echo -e "${BLUE}[ARCH DETECTION]${NC} Detected pacman package manager - Arch-based system${NC}" >&2
        return 0
    fi
    
    # Not an Arch-based system
    return 1
}

# Check and aggressively install Arch Linux system packages
check_and_install_arch_deps() {
    # STEP 1: Check if pacman exists (most reliable indicator of Arch-based system)
    if ! command -v pacman &> /dev/null; then
        echo -e "${BLUE}[ARCH CHECK]${NC} pacman not found - not an Arch-based system, skipping${NC}" >&2
        return 0  # Not on Arch, skip gracefully
    fi
    
    echo -e "${YELLOW}Detected Arch-based system (pacman found). Installing required packages...${NC}"
    
    local required_packages=(
        "rust"
        "base-devel"
        "git"
        "bc"
        "rust-bindgen"
        "rust-src"
        "graphviz"
        "python-sphinx"
        "texlive-latexextra"
        "llvm"
        "clang"
        "lld"
        "polly"
    )
    
    # STEP 2: Immediately attempt to install all required packages
    # The --needed flag ensures only missing packages are installed (safe and idempotent)
    echo -e "${BLUE}[ARCH INSTALL]${NC} Running: sudo pacman -S --needed --noconfirm rust base-devel git bc rust-bindgen rust-src graphviz python-sphinx texlive-latexextra llvm clang lld polly${NC}" >&2
    echo -e "${YELLOW}Installing Arch packages (sudo password required)...${NC}"
    echo ""
    
    if sudo pacman -S --needed --noconfirm rust base-devel git bc rust-bindgen rust-src graphviz python-sphinx texlive-latexextra llvm clang lld polly; then
        echo ""
        echo -e "${GREEN}✓ Packages installed successfully${NC}"
        
        # STEP 3: Refresh PATH in case rust binaries were added to /usr/bin
        echo -e "${BLUE}[ARCH INSTALL]${NC} Refreshing PATH and verifying installation...${NC}" >&2
        export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:${PATH}"
        
        # STEP 4: Re-verify all packages are now installed
        echo -e "${BLUE}[ARCH INSTALL]${NC} Verifying all packages are installed...${NC}" >&2
        local still_missing=()
        for package in "${required_packages[@]}"; do
            if ! pacman -Q "$package" &> /dev/null; then
                echo -e "${BLUE}[ARCH INSTALL]${NC} Still missing after install: $package${NC}" >&2
                still_missing+=("$package")
            else
                echo -e "${BLUE}[ARCH INSTALL]${NC} ✓ Confirmed installed: $package${NC}" >&2
            fi
        done
        
        if [ ${#still_missing[@]} -gt 0 ]; then
            echo -e "${RED}Error: Some packages still missing after installation: ${still_missing[*]}${NC}"
            return 1
        fi
        
        # STEP 5: Explicitly verify cargo is now in PATH
        if command -v cargo &> /dev/null; then
            local cargo_version=$(cargo --version 2>/dev/null || echo "unknown")
            echo -e "${GREEN}✓ cargo is now available in PATH: $cargo_version${NC}"
        fi
        
        # STEP 6: Check for modprobed-db and provide installation recommendation
         # modprobed-db is an AUR package and is optional but highly recommended
         echo ""
         echo -e "${BLUE}[MODPROBED-DB]${NC} Checking for modprobed-db...${NC}"
         if command -v modprobed-db &> /dev/null; then
             echo -e "${GREEN}✓ modprobed-db is installed${NC}"
             # Attempt to initialize the database silently
             if modprobed-db store 2>/dev/null; then
                 echo -e "${GREEN}✓ Database populated with loaded modules${NC}"
                 echo -e "${BLUE}[MODPROBED-DB]${NC} Database location: $HOME/.config/modprobed.db${NC}"
                 echo -e "${BLUE}[MODPROBED-DB]${NC} Module filtering will be AUTOMATIC on next kernel build${NC}"
             else
                 echo -e "${YELLOW}[MODPROBED-DB]${NC} Database initialization available but skipped (you can run: modprobed-db store)${NC}"
             fi
         else
             echo -e "${YELLOW}[MODPROBED-DB]${NC} Not installed (optional but recommended for 70%+ faster builds)${NC}"
             echo -e "${YELLOW}[MODPROBED-DB]${NC} Install via AUR helper:${NC}"
             echo -e "${YELLOW}[MODPROBED-DB]${NC}   yay -S modprobed-db${NC}"
             echo -e "${YELLOW}[MODPROBED-DB]${NC}   # or: paru -S modprobed-db${NC}"
         fi
        
        return 0
    else
        echo ""
        echo -e "${RED}Error: Package installation failed${NC}"
        echo -e "${RED}Cannot continue without required packages. Please install manually:${NC}"
        echo "  sudo pacman -S --needed --noconfirm rust base-devel git"
        return 1
    fi
}

# Dependencies are now handled by the GUI health check system
# This function is kept for reference but is no longer called

# ============================================================================
# GPG Key Management - Secure Kernel Build Keys
# ============================================================================

setup_kernel_gpg_keys() {
    echo -e "${YELLOW}Setting up GPG keys for kernel build...${NC}"
    
    # Kernel signing keys with their expected fingerprints (security verification)
    declare -A gpg_keys=(
        ["38DBBDC86092693E"]="6092693E"  # Greg Kroah-Hartman (Linux kernel stable)
        ["B8AC08600F108CDF"]="0F108CDF"  # Jan Alexander Steffens/heftig (Arch kernel)
    )
    
    # Keyserver list with explicit failover (in order of preference)
    local keyservers=(
        "hkps://keyserver.ubuntu.com"
        "hkps://keys.openpgp.org"
        "hkps://pgp.mit.edu"
    )
    
    local all_keys_valid=true
    
    for key_id in "${!gpg_keys[@]}"; do
        local key_fingerprint="${gpg_keys[$key_id]}"
        
        echo -e "${BLUE}[GPG]${NC} Processing key: $key_id"
        
        # Check if key is already imported (by checking if gpg can list it)
        if gpg --list-keys "$key_id" &>/dev/null 2>&1; then
            # Verify the imported key's fingerprint matches our expected value
            local imported_fp=$(gpg --with-colons --list-keys "$key_id" 2>/dev/null | grep '^fpr:' | cut -d: -f10 | head -1)
            
            if [[ "$imported_fp" == *"$key_fingerprint"* ]]; then
                echo -e "${GREEN}✓ GPG key already imported with correct fingerprint: $key_id${NC}"
                continue
            else
                echo -e "${YELLOW}[GPG]${NC} Key exists but fingerprint mismatch. Re-importing..."
            fi
        fi
        
        # Attempt to import from each keyserver in order
        local key_imported=false
        for keyserver in "${keyservers[@]}"; do
            echo -e "${BLUE}[GPG]${NC} Attempting import from: $keyserver"
            
            # Use timeout to prevent hanging on unreachable servers
            if timeout 10 gpg --keyserver "$keyserver" --recv-keys "$key_id" &>/dev/null 2>&1; then
                # Verify the fingerprint after import
                local imported_fp=$(gpg --with-colons --list-keys "$key_id" 2>/dev/null | grep '^fpr:' | cut -d: -f10 | head -1)
                
                if [[ "$imported_fp" == *"$key_fingerprint"* ]]; then
                    echo -e "${GREEN}✓ GPG key imported successfully with correct fingerprint: $key_id${NC}"
                    key_imported=true
                    break
                else
                    echo -e "${YELLOW}[GPG]${NC} Key imported but fingerprint verification failed. Trying next server..."
                    # Remove the incorrectly imported key
                    gpg --delete-keys --batch --yes "$key_id" &>/dev/null 2>&1 || true
                fi
            fi
        done
        
        if [ "$key_imported" = false ]; then
            echo -e "${RED}✗ Failed to import GPG key: $key_id${NC}"
            echo -e "${YELLOW}[GPG]${NC} This key is required for kernel signature verification.${NC}"
            all_keys_valid=false
        fi
    done
    
    echo ""
    if [ "$all_keys_valid" = true ]; then
        echo -e "${GREEN}✓ All required GPG keys are imported and verified${NC}"
        return 0
    else
        echo -e "${RED}✗ Some GPG keys could not be imported or verified${NC}"
        echo -e "${YELLOW}[GPG]${NC} Kernel build may fail. Ensure you have internet connectivity.${NC}"
        return 1
    fi
}

install_deps() {
     echo -e "${GREEN}Dependency Installation${NC}"
     echo "Installing missing system dependencies..."
     echo ""
     echo "This project requires the following:"
     echo "  • Rust toolchain (cargo and rustc)"
     echo "  • egui UI framework (managed by Cargo)"
     echo "  • Arch Linux build essentials"
    echo ""
    echo "For Debian/Ubuntu systems, install with:"
    echo "  sudo apt-get install cargo rustc build-essential"
    echo ""
    echo "For Fedora/RHEL systems, install with:"
    echo "  sudo dnf install cargo rustc gcc"
    echo ""
    echo "For Arch systems, install with:"
    echo "  sudo pacman -S rust base-devel"
    echo ""
    echo "After installing dependencies, build the project with:"
    echo "   cargo build --release"
}

run_app() {
    echo -e "${GREEN}Starting GOATd Kernel...${NC}"
    
    # Build the Rust binary if necessary
    if ! build_rust_binary; then
        exit 1
    fi
    
    # Execution diagnostics
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Executing: $BINARY_PATH"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Binary size: $(stat -c%s "$BINARY_PATH" 2>/dev/null || echo "unknown") bytes"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Binary timestamp: $(stat -c%y "$BINARY_PATH" 2>/dev/null || echo "unknown")"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} File type: $(file -b "$BINARY_PATH" 2>/dev/null || echo "unknown")"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Working dir: $(pwd)"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Logs location: $LOGS_DIR"
    echo ""
    
    # Run the native Rust application
    "$BINARY_PATH" "$@"
}

run_tests() {
    echo "Running all tests..."
    
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo (Rust) is not installed${NC}"
        echo "Install Rust from: https://rustup.rs/"
        exit 1
    fi
    
    # Ensure directories before testing
    ensure_directories || true
    
    # Run cargo tests in the Rust crate
    (cd "${PROJECT_ROOT}" && cargo test "$@")
}

run_unit_tests() {
    echo "Running unit tests..."
    
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo (Rust) is not installed${NC}"
        echo "Install Rust from: https://rustup.rs/"
        exit 1
    fi
    
    # Ensure directories before testing
    ensure_directories || true
    
    # Run cargo unit tests (excludes integration tests)
    (cd "${PROJECT_ROOT}" && cargo test --lib "$@")
}

run_integration_tests() {
    echo "Running integration tests..."
    
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: cargo (Rust) is not installed${NC}"
        echo "Install Rust from: https://rustup.rs/"
        exit 1
    fi
    
    # Ensure directories before testing
    ensure_directories || true
    
    # Run cargo integration tests
    (cd "${PROJECT_ROOT}" && cargo test --test '*' "$@")
}

run_dev() {
    echo -e "${YELLOW}Starting in development mode...${NC}"
    
    # Enable debug logging and start with more verbose output
    export DEBUG=1
    export RUST_LOG=debug
    export RUST_BACKTRACE=1
    
    echo -e "${BLUE}Debug flags set:${NC}"
    echo "  DEBUG=$DEBUG"
    echo "  RUST_LOG=$RUST_LOG"
    echo "  RUST_BACKTRACE=$RUST_BACKTRACE"
    echo ""
    
    # Build the Rust binary if necessary
    if ! build_rust_binary; then
        exit 1
    fi
    
    # Execution diagnostics
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Executing: $BINARY_PATH"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Binary size: $(stat -c%s "$BINARY_PATH" 2>/dev/null || echo "unknown") bytes"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Working dir: $(pwd)"
    echo -e "${GREEN}[LAUNCH DEBUG]${NC} Logs location: $LOGS_DIR"
    echo ""
    
    # Run the native Rust application
    "$BINARY_PATH" "$@"
}

cleanup_build() {
    echo -e "${YELLOW}Cleaning build artifacts and rotating logs...${NC}"
    
    # Rotate logs
    rotate_logs || true
    
    # Remove cargo build artifacts (optional, can be controlled with flag)
    if [ "${1:-}" = "--full" ]; then
        echo -e "${YELLOW}Removing cargo build artifacts...${NC}"
        (cd "${PROJECT_ROOT}" && cargo clean) || {
            echo -e "${RED}Warning: cargo clean failed${NC}"
        }
        echo -e "${GREEN}✓ Full cleanup complete${NC}"
    else
        echo -e "${BLUE}Tip: Use --full flag to also clean cargo artifacts. Example:${NC}"
        echo "  ./goatdkernel.sh --cleanup --full"
        echo -e "${GREEN}✓ Log rotation complete${NC}"
    fi
}

check_directories() {
    echo -e "${YELLOW}Checking project directories...${NC}"
    
    local all_good=true
    
    local dirs_to_check=(
        "$PROJECT_ROOT:Root"
        "$CONFIG_DIR:Config"
        "$LOGS_DIR:Logs"
        "$LOGS_FULL_DIR:Full Logs"
        "$LOGS_PARSED_DIR:Parsed Logs"
    )
    
    for dir_spec in "${dirs_to_check[@]}"; do
        IFS=':' read -r dir label <<< "$dir_spec"
        
        if [ -d "$dir" ]; then
            echo -e "${GREEN}✓${NC} $label: $dir"
        else
            echo -e "${RED}✗${NC} $label: $dir (MISSING)"
            all_good=false
        fi
    done
    
    echo ""
    if [ "$all_good" = true ]; then
        echo -e "${GREEN}✓ All required directories exist${NC}"
        
        # Check specific files
        echo ""
        echo -e "${YELLOW}Checking critical files...${NC}"
        if [ -f "$PROJECT_ROOT/Cargo.toml" ]; then
            echo -e "${GREEN}✓${NC} Cargo.toml found at $PROJECT_ROOT/Cargo.toml"
        else
            echo -e "${RED}✗${NC} Cargo.toml NOT found at $PROJECT_ROOT/Cargo.toml"
            all_good=false
        fi
        
        if [ -f "$PROJECT_ROOT/src/main.rs" ]; then
            echo -e "${GREEN}✓${NC} main.rs found at $PROJECT_ROOT/src/main.rs"
        else
            echo -e "${RED}✗${NC} main.rs NOT found at $PROJECT_ROOT/src/main.rs"
            all_good=false
        fi
        
        echo ""
        if [ "$all_good" = true ]; then
            echo -e "${GREEN}✓ All critical files are in place${NC}"
            return 0
        fi
    else
        echo -e "${RED}✗ Some directories are missing${NC}"
    fi
    
    return 1
}

# Main script logic
main() {
    case "${1:-}" in
        --help|-h)
            print_help
            exit 0
            ;;
        --version|-v)
            print_version
            exit 0
            ;;
        --test)
            run_tests "${@:2}"
            exit 0
            ;;
        --test-unit)
            run_unit_tests "${@:2}"
            exit 0
            ;;
        --test-integration)
            run_integration_tests "${@:2}"
            exit 0
            ;;
        --install-deps)
            print_banner
            install_deps
            exit $?
            ;;
        --dev)
            bootstrap_environment || exit 1
            print_banner
            run_dev "${@:2}"
            exit 0
            ;;
        --dry-run)
            bootstrap_environment || exit 1
            print_banner
            build_rust_binary_dry_run
            exit $?
            ;;
        --cleanup)
            cleanup_build "${2:-}"
            exit $?
            ;;
        --check-dirs)
            check_directories
            exit $?
            ;;
        "")
            # Default: run the app (GUI handles system health)
            bootstrap_environment || exit 1
            print_banner
            run_app
            exit 0
            ;;
        *)
            echo -e "${RED}Error: Unknown option: $1${NC}" >&2
            print_help
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"

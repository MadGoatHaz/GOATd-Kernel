#!/bin/bash
# GOATd Kernel Builder - Installation Script
# Version: 2.1.0
# Installs GOATd Kernel Builder (Rust/egui) on Arch Linux systems

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
VERSION="2.1.0"
PACKAGE_NAME="goatd-kernel"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_DIR="${INSTALL_DIR:-$PROJECT_DIR}"
BINARY_NAME="goatd_kernel"
BINARY_PATH="$INSTALL_DIR/target/release/$BINARY_NAME"

# Functions
print_header() {
    echo -e "${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

# Check if running as root (needed for some operations)
check_root() {
    if [[ $EUID -ne 0 ]]; then
        print_warning "Not running as root. Some features may require elevated privileges."
        print_info "You can run with sudo if needed: sudo bash scripts/install.sh"
    fi
}

# Check for required dependencies
check_dependencies() {
    print_header "Checking Dependencies"
    
    local missing=()
    
    # Check Rust toolchain
    if ! command -v cargo &> /dev/null; then
        missing+=("cargo")
    else
        rustc_version=$(rustc --version 2>&1 | awk '{print $2}')
        print_success "Rust $rustc_version found"
    fi
    
    # Check git
    if ! command -v git &> /dev/null; then
        missing+=("git")
    else
        git_version=$(git --version | awk '{print $3}')
        print_success "Git $git_version found"
    fi
    
    # Check pacman (Arch Linux)
    if ! command -v pacman &> /dev/null; then
        print_error "This installer is designed for Arch Linux and Arch-based distributions"
        exit 1
    fi
    
    # Check for sudo
    if ! command -v sudo &> /dev/null; then
        missing+=("sudo")
    else
        print_success "sudo found"
    fi
    
    # Report missing dependencies
    if [ ${#missing[@]} -gt 0 ]; then
        print_error "Missing required packages: ${missing[*]}"
        print_info "Install Rust from: https://rustup.rs/"
        print_info "For Arch, install with: sudo pacman -S rustup git base-devel"
        exit 1
    fi
    
    print_success "All required dependencies found"
}

# Install system dependencies
install_system_deps() {
    print_header "Installing System Dependencies"
    
    local packages=(
        "base-devel"
        "rust"
    )
    
    print_info "Installing packages: ${packages[*]}"
    
    if command -v pacman &> /dev/null; then
        if sudo pacman -S --noconfirm "${packages[@]}"; then
            print_success "System dependencies installed"
        else
            print_error "Failed to install system dependencies"
            exit 1
        fi
    fi
}

# Build Rust binary
build_binary() {
    print_header "Building Rust Binary"
    
    if [ ! -f "$INSTALL_DIR/Cargo.toml" ]; then
        print_error "Cargo.toml not found in $INSTALL_DIR"
        exit 1
    fi
    
    print_info "Building $PACKAGE_NAME from $INSTALL_DIR"
    
    if (cd "$INSTALL_DIR" && cargo build --release); then
        print_success "Rust binary built successfully"
    else
        print_error "Failed to build Rust binary"
        exit 1
    fi
}

# Install binary to system
install_binary() {
    print_header "Installing Binary"
    
    if [ ! -f "$BINARY_PATH" ]; then
        print_error "Binary not found at $BINARY_PATH"
        print_info "Did the build succeed?"
        exit 1
    fi
    
    # Copy binary to /usr/local/bin
    if sudo cp "$BINARY_PATH" "/usr/local/bin/$BINARY_NAME"; then
        print_success "Binary installed to /usr/local/bin/$BINARY_NAME"
    else
        print_error "Failed to install binary"
        exit 1
    fi
    
    # Make executable
    if sudo chmod +x "/usr/local/bin/$BINARY_NAME"; then
        print_success "Binary permissions set correctly"
    fi
}

# Verify installation
verify_installation() {
    print_header "Verifying Installation"
    
    # Check if binary exists in PATH
    if command -v "$BINARY_NAME" &> /dev/null; then
        print_success "Binary found in PATH"
    else
        print_warning "Binary not found in PATH (may need to reload shell)"
    fi
    
    # Check version
    if /usr/local/bin/$BINARY_NAME --version &> /dev/null; then
        version=$(/usr/local/bin/$BINARY_NAME --version)
        print_success "Version: $version"
    else
        print_warning "Could not verify version"
    fi
}

# Create necessary directories
setup_directories() {
    print_header "Setting Up Directories"
    
    local dirs=(
        ~/.config/goatd
        ~/.config/goatd/builds
        ~/.config/goatd/configs
    )
    
    for dir in "${dirs[@]}"; do
        if mkdir -p "$dir"; then
            print_success "Created $dir"
        else
            print_warning "Could not create $dir"
        fi
    done
}

# Print next steps
print_next_steps() {
    print_header "Installation Complete"
    
    echo ""
    print_success "GOATd Kernel Builder has been installed successfully!"
    echo ""
    echo "Next steps:"
    echo "  1. Start building your kernel:"
    echo "     /usr/local/bin/$BINARY_NAME"
    echo ""
    echo "  2. Or use the wrapper script from the project:"
    echo "     $INSTALL_DIR/goatdkernel.sh"
    echo ""
    echo "Documentation:"
    echo "  - README: $INSTALL_DIR/README.md"
    echo "  - Architecture: $INSTALL_DIR/ARCHITECTURAL_BLUEPRINT_V2.md"
    echo ""
    echo "Get help:"
    echo "  $BINARY_NAME --help"
    echo "  $BINARY_NAME --version"
    echo ""
}

# Main installation flow
main() {
    print_header "GOATd Kernel Builder v${VERSION} Installer"
    
    # Check if running on Arch Linux
    if [ ! -f /etc/os-release ]; then
        print_error "Cannot detect OS"
        exit 1
    fi
    
    # Source OS info
    source /etc/os-release
    
    # Check if Arch or Arch-based
    case "$ID_LIKE" in
        *arch*)
            print_success "Detected $PRETTY_NAME"
            ;;
        *)
            print_warning "Detected $PRETTY_NAME (not officially supported, may work anyway)"
            ;;
    esac
    
    echo ""
    check_root
    echo ""
    check_dependencies
    echo ""
    
    # Offer to install system dependencies
    read -p "Install system dependencies? (y/n) " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        install_system_deps
        echo ""
    fi
    
    # Build Rust binary
    build_binary
    echo ""
    
    # Install binary to system
    install_binary
    echo ""
    
    # Setup directories
    setup_directories
    echo ""
    
    # Verify installation
    verify_installation
    echo ""
    
    # Print next steps
    print_next_steps
}

# Run main function
main "$@"

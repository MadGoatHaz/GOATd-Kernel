#!/bin/bash
#
# GOATd Kernel Release & AUR Push Automation
# 
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.2.0
#
# This script automates:
# 1. Version bumping in Cargo.toml
# 2. Git tagging and pushing to GitHub
# 3. Waiting for GitHub Actions binary release
# 4. Automatically updating AUR packages (goatdkernel + goatdkernel-bin)
# 5. Generating .SRCINFO and pushing to AUR

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
AUR_BASE_DIR="${AUR_BASE_DIR:-$HOME/aur}"
AUR_SOURCE_DIR="${AUR_BASE_DIR}/goatdkernel"
AUR_BINARY_DIR="${AUR_BASE_DIR}/goatdkernel-bin"
GITHUB_REPO="MadGoatHaz/GOATd-Kernel"
GITHUB_API="https://api.github.com/repos/${GITHUB_REPO}"

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[⚠]${NC} $*"
}

log_error() {
    echo -e "${RED}[✗]${NC} $*" >&2
}

die() {
    log_error "$*"
    exit 1
}

check_requirements() {
    log_info "Checking requirements..."
    
    local missing=""
    for cmd in git sed curl makepkg; do
        if ! command -v "$cmd" &> /dev/null; then
            missing="$missing $cmd"
        fi
    done
    
    if [ -n "$missing" ]; then
        die "Missing required commands:$missing"
    fi
    
    log_success "All requirements met"
}

validate_version() {
    local version="$1"
    if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        die "Invalid version format: $version (expected: X.Y.Z)"
    fi
}

check_git_status() {
    log_info "Checking Git status..."
    
    if [ -n "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
        die "Working directory has uncommitted changes. Please commit or stash them first."
    fi
    
    log_success "Working directory is clean"
}

check_aur_repos() {
    log_info "Checking AUR repository clones..."
    
    if [ ! -d "$AUR_SOURCE_DIR/.git" ]; then
        log_warn "Source AUR repo not found at $AUR_SOURCE_DIR"
        log_info "Please clone: git clone ssh://aur@aur.archlinux.org/goatdkernel.git $AUR_SOURCE_DIR"
        return 1
    fi
    
    if [ ! -d "$AUR_BINARY_DIR/.git" ]; then
        log_warn "Binary AUR repo not found at $AUR_BINARY_DIR"
        log_info "Please clone: git clone ssh://aur@aur.archlinux.org/goatdkernel-bin.git $AUR_BINARY_DIR"
        return 1
    fi
    
    log_success "Both AUR repositories found"
    return 0
}

update_cargo_toml() {
    local version="$1"
    log_info "Updating Cargo.toml version to $version..."
    
    sed -i "s/^version = \"[^\"]*\"/version = \"$version\"/" "$REPO_ROOT/Cargo.toml"
    
    log_success "Cargo.toml updated"
}

git_tag_and_push() {
    local version="$1"
    log_info "Creating Git tag v$version..."
    
    cd "$REPO_ROOT"
    
    # Check if tag already exists
    if git rev-parse "v$version" >/dev/null 2>&1; then
        die "Tag v$version already exists"
    fi
    
    git tag -a "v$version" -m "Release v$version"
    log_success "Tag created"
    
    log_info "Pushing tag to GitHub..."
    git push origin "v$version"
    log_success "Tag pushed to GitHub"
}

wait_for_release() {
    local version="$1"
    log_info "Waiting for GitHub Actions to complete binary build..."
    log_info "Release URL: https://github.com/$GITHUB_REPO/releases/tag/v$version"
    echo ""
    
    local max_attempts=120  # 10 minutes with 5-second intervals
    local attempt=0
    
    while [ $attempt -lt $max_attempts ]; do
        # Check if release exists
        local release_response=$(curl -s -H "Accept: application/vnd.github.v3+json" \
            "$GITHUB_API/releases/tags/v$version" || echo "{}")
        
        if echo "$release_response" | grep -q '"assets"'; then
            local asset_count=$(echo "$release_response" | grep -c '"name"' || echo 0)
            if [ "$asset_count" -ge 2 ]; then
                log_success "GitHub release completed with artifacts"
                return 0
            fi
        fi
        
        attempt=$((attempt + 1))
        if [ $((attempt % 12)) -eq 0 ]; then
            log_info "Still waiting... (attempt $attempt/120)"
        fi
        sleep 5
    done
    
    log_warn "Timeout waiting for GitHub release (10 minutes)"
    log_warn "The script will continue, but please verify the release is available before pushing to AUR"
    echo -e "${YELLOW}Visit: https://github.com/$GITHUB_REPO/releases/tag/v$version${NC}"
    
    # Ask user to confirm
    read -p "Press Enter to continue, or Ctrl+C to abort: "
}

fetch_sha256() {
    local version="$1"
    log_info "Fetching SHA256 from GitHub Release..."
    
    local sha256_url="https://github.com/$GITHUB_REPO/releases/download/v$version/goatdkernel-$version-x86_64.tar.gz.sha256"
    local sha256=$(curl -s "$sha256_url" | awk '{print $1}' || echo "")
    
    if [ -z "$sha256" ]; then
        die "Failed to fetch SHA256 from GitHub Release"
    fi
    
    echo "$sha256"
    log_success "SHA256: $sha256"
}

update_aur_source() {
    local version="$1"
    log_info "Updating source AUR package..."
    
    cd "$AUR_SOURCE_DIR"
    
    # Check for uncommitted changes
    if [ -n "$(git status --porcelain)" ]; then
        log_warn "Source AUR repo has uncommitted changes, stashing..."
        git stash
    fi
    
    # Ensure we're on main/master and up to date
    git pull --rebase origin main 2>/dev/null || git pull --rebase origin master 2>/dev/null || true
    
    # Copy PKGBUILD template
    cp "$REPO_ROOT/pkgbuilds/source/PKGBUILD" ./
    
    # Update version
    sed -i "s/^pkgver=.*/pkgver=$version/" PKGBUILD
    
    # Generate .SRCINFO
    makepkg --printsrcinfo > .SRCINFO
    
    # Commit
    git add PKGBUILD .SRCINFO
    git commit -m "Update to version $version" || log_warn "Nothing to commit in source package"
    
    # Push
    git push origin main 2>/dev/null || git push origin master 2>/dev/null || true
    
    log_success "Source AUR package updated"
}

update_aur_binary() {
    local version="$1"
    local sha256="$2"
    log_info "Updating binary AUR package..."
    
    cd "$AUR_BINARY_DIR"
    
    # Check for uncommitted changes
    if [ -n "$(git status --porcelain)" ]; then
        log_warn "Binary AUR repo has uncommitted changes, stashing..."
        git stash
    fi
    
    # Ensure we're on main/master and up to date
    git pull --rebase origin main 2>/dev/null || git pull --rebase origin master 2>/dev/null || true
    
    # Copy PKGBUILD template
    cp "$REPO_ROOT/pkgbuilds/binary/PKGBUILD" ./
    
    # Update version
    sed -i "s/^pkgver=.*/pkgver=$version/" PKGBUILD
    
    # Update SHA256 (handle array format with 'SKIP')
    # Replace the sha256sums line with the actual hash and 'SKIP'
    sed -i "s/sha256sums=.*/sha256sums=('$sha256' 'SKIP')/" PKGBUILD
    
    # Generate .SRCINFO
    makepkg --printsrcinfo > .SRCINFO
    
    # Commit
    git add PKGBUILD .SRCINFO
    git commit -m "Update to version $version" || log_warn "Nothing to commit in binary package"
    
    # Push
    git push origin main 2>/dev/null || git push origin master 2>/dev/null || true
    
    log_success "Binary AUR package updated"
}

print_summary() {
    local version="$1"
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}Release v$version Complete!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Completed tasks:"
    echo "  ✓ Updated Cargo.toml"
    echo "  ✓ Created and pushed Git tag v$version"
    echo "  ✓ Triggered GitHub Actions binary build"
    echo "  ✓ Updated source AUR package (goatdkernel)"
    echo "  ✓ Updated binary AUR package (goatdkernel-bin)"
    echo "  ✓ Generated and pushed .SRCINFO files"
    echo ""
    echo "Next steps:"
    echo "  1. Monitor GitHub Actions: https://github.com/$GITHUB_REPO/actions"
    echo "  2. Check AUR pages:"
    echo "     - https://aur.archlinux.org/packages/goatdkernel"
    echo "     - https://aur.archlinux.org/packages/goatdkernel-bin"
    echo ""
}

# Main execution
main() {
    if [ $# -ne 1 ]; then
        echo "Usage: $0 <version>"
        echo "Example: $0 0.2.0"
        exit 1
    fi
    
    local version="$1"
    
    log_info "GOATd Kernel Release & AUR Push Automation"
    log_info "Version: $version"
    echo ""
    
    # Pre-flight checks
    check_requirements
    validate_version "$version"
    check_git_status
    
    if ! check_aur_repos; then
        die "AUR repositories not properly set up. Please clone them first."
    fi
    
    echo ""
    log_info "Starting release process..."
    echo ""
    
    # Update version in repo
    update_cargo_toml "$version"
    
    # Commit version bump
    cd "$REPO_ROOT"
    git add Cargo.toml Cargo.lock >/dev/null 2>&1 || true
    git commit -m "Bump version to $version" || log_warn "No changes to commit for version bump"
    
    # Tag and push
    git_tag_and_push "$version"
    
    # Wait for GitHub Actions
    wait_for_release "$version"
    
    # Fetch SHA256
    local sha256=$(fetch_sha256 "$version")
    
    # Update AUR packages
    update_aur_source "$version"
    update_aur_binary "$version" "$sha256"
    
    # Print summary
    print_summary "$version"
}

main "$@"

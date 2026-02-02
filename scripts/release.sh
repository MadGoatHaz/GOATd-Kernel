#!/bin/bash
#
# GOATd Kernel GitHub Release Automation
# 
# Usage: ./scripts/release.sh
# 
# This script automates GitHub releases with interactive prompts and safety checks:
# 1. Prompts for version number
# 2. Checks GitHub for existing release (with conflict protection)
# 3. Detects uncommitted changes and offers to commit them
# 4. Bumps version in Cargo.toml
# 5. Commits and pushes code changes
# 6. Creates Git tag and pushes to GitHub
# 7. Creates GitHub Release
# 8. Builds release binary and creates tarball
# 9. Uploads tarball to GitHub Release

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Path Configuration:
# This script is located at master/scripts/release.sh
# REPO_ROOT is set to the master/ directory (where .git lives)
# This ensures all git operations and path references are within the master/ repository
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Verify REPO_ROOT is the git repository root (master/)
if [ ! -d "$REPO_ROOT/.git" ]; then
    echo -e "${RED}[✗]${NC} REPO_ROOT does not contain .git directory: $REPO_ROOT" >&2
    echo -e "${RED}[✗]${NC} This script must be run from within the master/ git repository." >&2
    exit 1
fi

GITHUB_REPO="MadGoatHaz/GOATd-Kernel"

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

log_prompt() {
    echo -e "${CYAN}[?]${NC} $*"
}

die() {
    log_error "$*"
    exit 1
}

check_requirements() {
    log_info "Checking requirements..."
    
    # Check for git
    if ! command -v git &> /dev/null; then
        die "Missing required command: git. Please install Git first."
    fi
    
    # Check for gh
    if ! command -v gh &> /dev/null; then
        die "Missing required command: gh (GitHub CLI). Please install GitHub CLI first."
    fi
    
    # Check for cargo
    if ! command -v cargo &> /dev/null; then
        die "Missing required command: cargo. Please install Rust/Cargo first."
    fi
    
    # Check gh authentication (with timeout to prevent hanging)
    log_info "Checking GitHub CLI authentication (timeout: 10s)..."
    if ! timeout 10 gh auth status &>/dev/null; then
        die "GitHub CLI (gh) is not authenticated or auth check timed out. Please run 'gh auth login' first."
    fi
    
    log_success "All requirements met (git, gh, cargo)"
}

validate_version() {
    local version="$1"
    if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        die "Invalid version format: $version (expected: X.Y.Z, e.g., 0.2.1)"
    fi
}

# Interactive commit function for pre-release changes
interactive_commit_prompt() {
    local version="$1"
    
    log_info "Checking for uncommitted changes in $REPO_ROOT..."
    
    # Check for uncommitted changes using git -C for repository targeting
    if [ -z "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
        log_success "Working directory is clean - no uncommitted changes found"
        return 0
    fi
    
    # Display the uncommitted changes
    log_warn "Uncommitted changes detected:"
    echo ""
    git -C "$REPO_ROOT" status --short
    echo ""
    
    # Prompt user for action
    log_prompt "Found uncommitted changes. Would you like to commit them to GitHub master before releasing? (y/N): "
    local response
    read -r response < /dev/tty || die "Failed to read user input"
    
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        log_info "Skipping automatic commit of uncommitted changes"
        log_info "Continuing with release process..."
        return 0
    fi
    
    # User wants to commit - prompt for commit message
    local default_message="chore: pre-release preparations for v${version}"
    echo ""
    log_prompt "Enter commit message (default: \"${default_message}\"): "
    local user_message
    read -r user_message < /dev/tty || die "Failed to read user input"
    
    local commit_message="${user_message:-$default_message}"
    
    log_info "Staging all changes..."
    git -C "$REPO_ROOT" add -A
    
    log_info "Committing changes with message: \"$commit_message\""
    git -C "$REPO_ROOT" commit -m "$commit_message"
    
    log_success "Changes committed locally"
    
    # Push verification - ensure changes are pushed to origin master
    log_info "Pushing commit to origin master..."
    git -C "$REPO_ROOT" push origin master || die "Failed to push commit to origin master"
    
    log_success "Changes pushed to origin master"
}

check_git_status() {
    log_info "Checking Git status..."
    
    # Note: Uncommitted changes are now handled by interactive_commit_prompt
    # This function now primarily checks branch status
    
    # Check if we're on main/master branch
    local branch
    branch=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)
    if [[ "$branch" != "main" && "$branch" != "master" ]]; then
        log_warn "Not on main/master branch (currently on: $branch)"
        log_prompt "Continue anyway? (y/N): "
        local response
        read -r response < /dev/tty || die "Failed to read user input"
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            die "Aborted by user"
        fi
    fi
    
    log_success "Git repository check complete"
}

check_github_release() {
    local version="$1"
    
    log_info "Checking GitHub for existing release v$version..."
    
    if gh release view "v$version" --repo "$GITHUB_REPO" &>/dev/null; then
        return 0  # Release exists
    else
        return 1  # Release does not exist
    fi
}

handle_existing_release() {
    local version="$1"
    
    log_warn "Version v$version already exists on GitHub."
    log_prompt "Replace it? This will delete the existing release and tag. (y/N): "
    local response
    read -r response < /dev/tty || die "Failed to read user input"
    
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        log_info "Aborting release. Use a different version number."
        exit 0
    fi
    
    log_info "Deleting existing GitHub release v$version..."
    gh release delete "v$version" --repo "$GITHUB_REPO" --yes || log_warn "Failed to delete release (may not exist)"
    
    log_info "Deleting local tag v$version..."
    git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || true
    
    log_info "Deleting remote tag v$version..."
    git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || true
    
    log_success "Existing release and tag cleaned up"
}

prompt_version() {
    # Use stderr for prompt to ensure it's visible even in command substitution
    echo -e "${CYAN}[?]${NC} Enter version number (format: X.Y.Z, e.g., 0.2.1): " >&2
    
    # Read from /dev/tty to ensure we get user input even in subshells
    local version
    if ! read -r version < /dev/tty; then
        die "Failed to read version input"
    fi
    
    # Trim whitespace
    version=$(echo "$version" | xargs)
    
    if [ -z "$version" ]; then
        die "Version cannot be empty"
    fi
    
    validate_version "$version"
    echo "$version"
}

update_cargo_toml() {
    local version="$1"
    log_info "Updating Cargo.toml version to $version..."
    
    if [ ! -f "$REPO_ROOT/Cargo.toml" ]; then
        die "Cargo.toml not found at $REPO_ROOT/Cargo.toml"
    fi
    
    sed -i "s/^version = \"[^\"]*\"/version = \"$version\"/" "$REPO_ROOT/Cargo.toml"
    
    log_success "Cargo.toml updated"
}

commit_version_bump() {
    local version="$1"
    
    # Use -C flag for all git operations to ensure we're in the correct repository
    
    # Check if there are changes to commit
    if [ -z "$(git -C "$REPO_ROOT" status --porcelain)" ]; then
        log_warn "No changes to commit for version bump"
        return 0
    fi
    
    log_info "Staging Cargo.toml changes..."
    git -C "$REPO_ROOT" add Cargo.toml Cargo.lock 2>/dev/null || git -C "$REPO_ROOT" add Cargo.toml
    
    log_info "Committing version bump to v$version..."
    git -C "$REPO_ROOT" commit -m "chore(release): bump version to v$version"
    
    log_success "Version bump committed"
}

push_code() {
    log_info "Pushing code to GitHub..."
    
    git -C "$REPO_ROOT" push origin master || die "Failed to push code to origin master"
    
    log_success "Code pushed to GitHub"
}

git_tag_and_push() {
    local version="$1"
    
    log_info "Creating Git tag v$version..."
    
    # Create annotated tag
    git -C "$REPO_ROOT" tag -a "v$version" -m "Release v$version"
    
    log_info "Pushing tag to GitHub..."
    git -C "$REPO_ROOT" push origin "v$version"
    
    log_success "Tag v$version created and pushed"
}

build_release_binary() {
    log_info "Building release binary..."
    
    cd "$REPO_ROOT"
    
    # Build in release mode
    cargo build --release || die "Failed to build release binary"
    
    log_success "Release binary built"
}

create_tarball() {
    local version="$1"
    
    log_info "Creating release tarball..."
    
    cd "$REPO_ROOT"
    
    local binary_name="goatdkernel"
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    
    # Check if binary exists
    if [ ! -f "target/release/$binary_name" ]; then
        die "Binary not found at target/release/$binary_name"
    fi
    
    # Create tarball with binary and assets
    tar -czf "$tarball_name" \
        -C target/release "$binary_name" \
        -C "$REPO_ROOT" assets/ LICENSE README.md \
        2>/dev/null || tar -czf "$tarball_name" -C target/release "$binary_name"
    
    # Generate SHA256 checksum
    sha256sum "$tarball_name" > "${tarball_name}.sha256"
    
    log_success "Tarball created: $tarball_name"
    log_success "SHA256 checksum created"
}

create_github_release() {
    local version="$1"
    
    log_info "Creating GitHub release..."
    
    # Create release in draft mode initially
    gh release create "v$version" \
        --repo "$GITHUB_REPO" \
        --title "v$version" \
        --notes "Release v$version" \
        --draft
    
    log_success "GitHub release created (draft)"
}

upload_release_assets() {
    local version="$1"
    
    log_info "Uploading release assets..."
    
    cd "$REPO_ROOT"
    
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    local sha256_file="${tarball_name}.sha256"
    
    if [ ! -f "$tarball_name" ]; then
        die "Tarball not found: $tarball_name"
    fi
    
    # Upload tarball
    gh release upload "v$version" "$tarball_name" --repo "$GITHUB_REPO"
    log_success "Tarball uploaded"
    
    # Upload SHA256 checksum
    if [ -f "$sha256_file" ]; then
        gh release upload "v$version" "$sha256_file" --repo "$GITHUB_REPO"
        log_success "SHA256 checksum uploaded"
    fi
}

publish_release() {
    local version="$1"
    
    log_info "Publishing release (removing draft status)..."
    
    gh release edit "v$version" --repo "$GITHUB_REPO" --draft=false
    
    log_success "Release published"
}

cleanup_local_files() {
    local version="$1"
    
    log_info "Cleaning up local files..."
    
    cd "$REPO_ROOT"
    
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    local sha256_file="${tarball_name}.sha256"
    
    rm -f "$tarball_name" "$sha256_file"
    
    log_success "Local files cleaned up"
}

print_summary() {
    local version="$1"
    echo ""
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  🎉 GitHub Release v$version Complete!${NC}"
    echo -e "${GREEN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Completed tasks:"
    echo "  ✓ Pre-release changes committed (if any)"
    echo "  ✓ Version bumped in Cargo.toml"
    echo "  ✓ Changes committed and pushed to GitHub"
    echo "  ✓ Git tag v$version created and pushed"
    echo "  ✓ Release binary built"
    echo "  ✓ Tarball and SHA256 checksum created"
    echo "  ✓ GitHub Release created with assets"
    echo ""
    echo -e "Release URL: ${CYAN}https://github.com/$GITHUB_REPO/releases/tag/v$version${NC}"
    echo ""
    log_warn "Note: AUR packages must be updated separately using the maintenance scripts."
    echo ""
}

confirm_release() {
    local version="$1"
    local current_branch
    current_branch=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)
    
    echo ""
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  Release Summary${NC}"
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "Version:     v$version"
    echo "Repository:  $GITHUB_REPO"
    echo "Branch:      $current_branch"
    echo ""
    log_prompt "Proceed with release? (y/N): "
    local response
    read -r response < /dev/tty || die "Failed to read user input"
    
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        die "Release aborted by user"
    fi
}

# Main execution
main() {
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  GOATd Kernel GitHub Release Automation${NC}"
    echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}"
    echo ""
    
    # Step 1: Check requirements (git and gh explicitly checked)
    check_requirements
    
    # Step 2: Prompt for version early so we can use it in commit messages
    # CRITICAL FIX: Declare 'local' separately to avoid stdout capture issues
    # with command substitution that hides the prompt from the user
    local version
    version=$(prompt_version)
    echo ""
    log_info "Preparing release for version: v$version"
    echo ""
    
    # Step 3: Interactive commit prompt for any uncommitted changes
    interactive_commit_prompt "$version"
    
    # Step 4: Check git status (branch validation)
    check_git_status
    
    # Step 5: Check for existing release (conflict protection)
    if check_github_release "$version"; then
        handle_existing_release "$version"
    else
        log_info "No existing release found for v$version - proceeding with new release"
    fi
    
    # Step 6: Confirm release
    confirm_release "$version"
    
    echo ""
    log_info "Starting release process..."
    echo ""
    
    # Step 7: Update version in Cargo.toml
    update_cargo_toml "$version"
    
    # Step 8: Commit and push version bump
    commit_version_bump "$version"
    push_code
    
    # Step 9: Create and push Git tag
    git_tag_and_push "$version"
    
    # Step 10: Build release binary
    build_release_binary
    
    # Step 11: Create tarball
    create_tarball "$version"
    
    # Step 12: Create GitHub release
    create_github_release "$version"
    
    # Step 13: Upload assets
    upload_release_assets "$version"
    
    # Step 14: Publish release (remove draft status)
    publish_release "$version"
    
    # Step 15: Cleanup local files
    cleanup_local_files "$version"
    
    # Step 16: Print summary
    print_summary "$version"
}

main "$@"

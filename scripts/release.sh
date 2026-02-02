#!/bin/bash
#
# GOATd Kernel GitHub Release Automation
# 
# Usage: ./scripts/release.sh
# 
# This script automates GitHub releases with interactive prompts and safety checks:
# 1. Prompts for version number
# 2. Unified pre-flight safety check (GitHub release + Git tags)
# 3. Detects uncommitted changes and offers to commit them
# 4. Bumps version in Cargo.toml
# 5. Commits and pushes code changes
# 6. Creates Git tag and pushes to GitHub (with tag conflict handling)
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

# AUTO_APPROVE mode for automated testing
# Set AUTO_APPROVE=yes to automatically answer "y" to all prompts
read_input() {
    local default_value="${1:-}"
    
    # If AUTO_APPROVE is set, return "y" for yes/no prompts
    if [[ "${AUTO_APPROVE:-}" == "yes" ]]; then
        echo "y"
        return 0
    fi
    
    # Otherwise, try to read from /dev/tty or stdin
    local response
    if [ -e /dev/tty ] && [ -r /dev/tty ]; then
        if ! read -r response < /dev/tty; then
            return 1
        fi
    else
        if ! read -r response; then
            return 1
        fi
    fi
    
    echo "$response"
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
    response=$(read_input) || die "Failed to read user input"
    
    
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
    user_message=$(read_input) || die "Failed to read user input"
    
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
        response=$(read_input) || die "Failed to read user input"
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            die "Aborted by user"
        fi
    fi
    
    log_success "Git repository check complete"
}

# ============================================================================
# UNIFIED PRE-FLIGHT SAFETY CHECKS
# ============================================================================

# Check if GitHub release exists
check_github_release() {
    local version="$1"
    
    if gh release view "v$version" --repo "$GITHUB_REPO" &>/dev/null; then
        return 0  # Release exists
    else
        return 1  # Release does not exist
    fi
}

# Check if Git tag exists locally
_check_git_tag_local() {
    local version="$1"
    git -C "$REPO_ROOT" tag -l "v$version" | grep -q "v$version"
}

# Check if Git tag exists remotely
_check_git_tag_remote() {
    local version="$1"
    git -C "$REPO_ROOT" ls-remote --tags origin "refs/tags/v$version" 2>/dev/null | grep -q "v$version"
}

# Check tag existence and return status
check_git_tag_exists() {
    local version="$1"
    local tag_local=false
    local tag_remote=false
    
    if _check_git_tag_local "$version"; then
        tag_local=true
    fi
    
    if _check_git_tag_remote "$version"; then
        tag_remote=true
    fi
    
    if [[ "$tag_local" == true && "$tag_remote" == true ]]; then
        echo "both"
    elif [[ "$tag_local" == true ]]; then
        echo "local"
    elif [[ "$tag_remote" == true ]]; then
        echo "remote"
    else
        echo "none"
    fi
}

# Unified pre-flight safety check
preflight_safety_check() {
    local version="$1"
    local has_conflicts=false
    local github_exists=false
    local tag_status
    
    log_info "Running pre-flight safety checks for v$version..."
    echo ""
    
    # Check 1: GitHub Release
    echo -e "  ${BLUE}Checking GitHub release...${NC}"
    if check_github_release "$version"; then
        echo -e "  ${YELLOW}  ⚠ GitHub release v$version exists${NC}"
        github_exists=true
        has_conflicts=true
    else
        echo -e "  ${GREEN}  ✓ No GitHub release found${NC}"
    fi
    
    # Check 2: Git Tags
    echo -e "  ${BLUE}Checking Git tags...${NC}"
    tag_status=$(check_git_tag_exists "$version")
    case "$tag_status" in
        "both")
            echo -e "  ${YELLOW}  ⚠ Git tag v$version exists locally AND remotely${NC}"
            has_conflicts=true
            ;;
        "local")
            echo -e "  ${YELLOW}  ⚠ Git tag v$version exists locally only${NC}"
            has_conflicts=true
            ;;
        "remote")
            echo -e "  ${YELLOW}  ⚠ Git tag v$version exists remotely only${NC}"
            has_conflicts=true
            ;;
        "none")
            echo -e "  ${GREEN}  ✓ No Git tags found${NC}"
            ;;
    esac
    
    echo ""
    
    # Handle conflicts if any
    if [[ "$has_conflicts" == true ]]; then
        log_warn "One or more conflicts detected!"
        echo ""
        
        # Unified prompt for handling all conflicts
        log_prompt "Delete existing release/tag(s) and re-create for this commit? (y/N): "
        local response
        response=$(read_input) || die "Failed to read user input"
        
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            log_info "Aborting release. Use a different version number."
            exit 0
        fi
        
        # Handle GitHub release deletion
        if [[ "$github_exists" == true ]]; then
            log_info "Deleting existing GitHub release v$version..."
            gh release delete "v$version" --repo "$GITHUB_REPO" --yes 2>/dev/null || log_warn "Failed to delete release (may already be gone)"
        fi
        
        # Handle tag deletion based on status
        case "$tag_status" in
            "both"|"local")
                log_info "Deleting local tag v$version..."
                git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || log_warn "Failed to delete local tag"
                ;;&
            "both"|"remote")
                log_info "Deleting remote tag v$version..."
                git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || log_warn "Failed to delete remote tag"
                ;;
        esac
        
        log_success "Existing release and tags cleaned up"
    else
        log_success "Pre-flight checks passed - no conflicts found"
    fi
    
    # Return the tag status for git_tag_and_push to use
    echo "$tag_status"
}

# Legacy function - kept for compatibility but now integrated into preflight
cleanup_conflicting_release_and_tag() {
    local version="$1"
    
    log_warn "Cleaning up conflicting release and tags..."
    
    # Delete GitHub release
    gh release delete "v$version" --repo "$GITHUB_REPO" --yes 2>/dev/null || true
    
    # Delete local tag
    git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || true
    
    # Delete remote tag
    git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || true
    
    log_success "Cleanup complete"
}

prompt_version() {
    # Check if version was provided as command-line argument (for automation)
    if [ $# -ge 1 ] && [ -n "$1" ]; then
        local version="$1"
        validate_version "$version"
        echo "$version"
        return 0
    fi
    
    # Use stderr for prompt to ensure it's visible even in command substitution
    echo -e "${CYAN}[?]${NC} Enter version number (format: X.Y.Z, e.g., 0.2.1): " >&2
    
    # Read from /dev/tty if available, otherwise use stdin
    local version
    if [ -e /dev/tty ] && [ -r /dev/tty ]; then
        if ! read -r version < /dev/tty; then
            die "Failed to read version input from /dev/tty"
        fi
    else
        # Fallback to standard input for non-interactive environments
        if ! read -r version; then
            die "Failed to read version input (hint: provide version as argument: ./release.sh X.Y.Z)"
        fi
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
    local tag_status="${2:-none}"
    
    log_info "Creating Git tag v$version..."
    
    # Double-check tag status if not provided (safety net)
    if [[ "$tag_status" == "none" ]]; then
        tag_status=$(check_git_tag_exists "$version")
    fi
    
    # Handle any remaining tag conflicts (should be none after preflight, but just in case)
    case "$tag_status" in
        "both"|"local")
            log_warn "Local tag v$version still exists after preflight"
            log_info "Attempting to delete local tag..."
            git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || {
                log_error "Failed to delete local tag v$version"
                if [[ "${AUTO_APPROVE:-}" != "yes" ]]; then
                    log_prompt "Delete local tag manually and press Enter to continue, or Ctrl+C to abort..."
                    read_input > /dev/null || true
                fi
            }
            ;;&
        "both"|"remote")
            log_warn "Remote tag v$version still exists after preflight"
            log_info "Attempting to delete remote tag..."
            git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || {
                log_error "Failed to delete remote tag v$version"
                if [[ "${AUTO_APPROVE:-}" != "yes" ]]; then
                    log_prompt "Delete remote tag manually and press Enter to continue, or Ctrl+C to abort..."
                    read_input > /dev/null || true
                fi
            }
            ;;
    esac
    
    # Create annotated tag
    git -C "$REPO_ROOT" tag -a "v$version" -m "Release v$version" || die "Failed to create tag v$version"
    
    log_info "Pushing tag to GitHub..."
    git -C "$REPO_ROOT" push origin "v$version" || die "Failed to push tag v$version"
    
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
    
    local binary_name="goatd_kernel"
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    
    # Check if binary exists
    if [ ! -f "target/release/$binary_name" ]; then
        die "Release binary not found: target/release/$binary_name"
    fi
    
    # Create tarball with binary, wrapper script, and install script
    tar -czf "$tarball_name" \
        -C target/release "$binary_name" \
        -C "$REPO_ROOT" goatdkernel.sh \
        -C "$REPO_ROOT/scripts" install.sh \
        || die "Failed to create tarball"
    
    # Generate SHA256 checksum
    sha256sum "$tarball_name" > "${tarball_name}.sha256"
    
    # Move tarball and checksum to repo root for upload
    mv "$tarball_name" "$REPO_ROOT/"
    mv "${tarball_name}.sha256" "$REPO_ROOT/"
    
    log_success "Tarball created: $tarball_name"
    log_success "SHA256 checksum: ${tarball_name}.sha256"
}

create_github_release() {
    local version="$1"
    
    log_info "Creating GitHub release (draft mode)..."
    
    # Get the current commit message for release notes
    local commit_message
    commit_message=$(git -C "$REPO_ROOT" log -1 --pretty=%B)
    
    # Create release in draft mode first
    gh release create "v$version" \
        --repo "$GITHUB_REPO" \
        --title "v$version" \
        --notes "Release v$version

$commit_message" \
        --draft \
        || die "Failed to create GitHub release"
    
    log_success "GitHub release created (draft)"
}

upload_release_assets() {
    local version="$1"
    
    log_info "Uploading release assets..."
    
    cd "$REPO_ROOT"
    
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    local sha256_file="${tarball_name}.sha256"
    
    # Upload tarball
    if [ -f "$tarball_name" ]; then
        gh release upload "v$version" "$tarball_name" --repo "$GITHUB_REPO"
        log_success "Tarball uploaded"
    fi
    
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
    response=$(read_input) || die "Failed to read user input"
    
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
    version=$(prompt_version "${1:-}")
    echo ""
    log_info "Preparing release for version: v$version"
    echo ""
    
    # Step 3: Interactive commit prompt for any uncommitted changes
    interactive_commit_prompt "$version"
    
    # Step 4: Check git status (branch validation)
    check_git_status
    
    # Step 5: Unified pre-flight safety check (GitHub release + Git tags)
    # This replaces the separate check_github_release and handle_existing_release calls
    local tag_status
    tag_status=$(preflight_safety_check "$version")
    
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
    
    # Step 9: Create and push Git tag (passing tag_status for safety)
    git_tag_and_push "$version" "$tag_status"
    
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

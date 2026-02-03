#!/bin/bash
#
# GOATd Kernel GitHub Release Automation
# 
# Usage: ./scripts/release.sh [VERSION]
# 
# This script automates GitHub releases with interactive prompts and safety checks:
# 1. Prompts for version number (or accepts as argument)
# 2. Unified pre-flight safety check (GitHub release + Git tags)
# 3. Detects uncommitted changes and offers to commit them
# 4. Bumps version in Cargo.toml
# 5. Commits and pushes code changes
# 6. Creates Git tag and pushes to GitHub (with robust tag conflict handling)
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
# This script is located at scripts/release.sh
# REPO_ROOT is set to the ./ directory (where .git lives)
# This ensures all git operations and path references are within the ./ repository
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Verify REPO_ROOT is the git repository root (./)
if [ ! -d "$REPO_ROOT/.git" ]; then
    echo -e "${RED}[✗]${NC} REPO_ROOT does not contain .git directory: $REPO_ROOT" >&2
    echo -e "${RED}[✗]${NC} This script must be run from within the root git repository." >&2
    exit 1
fi

GITHUB_REPO="MadGoatHaz/GOATd-Kernel"

# Global variable for version (set by prompt_version, used throughout)
RELEASE_VERSION=""

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

log_debug() {
    if [[ "${DEBUG_RELEASE:-}" == "1" ]]; then
        echo -e "${BLUE}[DEBUG]${NC} $*"
    fi
}

log_progress() {
    echo -e "${YELLOW}[PROGRESS]${NC} $*"
}

die() {
    log_error "$*"
    exit 1
}

check_requirements() {
    log_info "Checking requirements..."
    log_debug "check_requirements: Starting requirement checks"
    
    # Check for git
    if ! command -v git &> /dev/null; then
        die "Missing required command: git. Please install Git first."
    fi
    log_debug "check_requirements: git found"
    
    # Check for gh
    if ! command -v gh &> /dev/null; then
        die "Missing required command: gh (GitHub CLI). Please install GitHub CLI first."
    fi
    log_debug "check_requirements: gh found"
    
    # Check for cargo
    if ! command -v cargo &> /dev/null; then
        die "Missing required command: cargo. Please install Rust/Cargo first."
    fi
    log_debug "check_requirements: cargo found"
    
    # Check gh authentication
    log_info "Checking GitHub CLI authentication..."
    log_debug "check_requirements: Running gh auth status"
    if ! gh auth status &>/dev/null; then
        die "GitHub CLI (gh) is not authenticated. Please run 'gh auth login' first."
    fi
    log_debug "check_requirements: gh auth status succeeded"
    
    log_success "All requirements met (git, gh, cargo)"
}

validate_version() {
    local version="$1"
    if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        die "Invalid version format: $version (format: X.Y.Z, e.g., 0.2.1)"
    fi
}

# Interactive commit function for pre-release changes
interactive_commit_prompt() {
    local version="$1"
    
    log_info "Checking for uncommitted changes in $REPO_ROOT..."
    log_debug "interactive_commit_prompt: Checking git status"
    
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
    log_prompt "Found uncommitted changes. Commit them before releasing? (y/N): "
    read -p "" response </dev/tty

    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        log_info "Skipping automatic commit of uncommitted changes"
        log_info "Continuing with release process..."
        return 0
    fi
    
    # User wants to commit - prompt for commit message
    local default_message="chore: pre-release preparations for v${version}"
    echo ""
    log_prompt "Enter commit message (default: \"${default_message}\"): "
    read -p "" user_message </dev/tty
    
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
    log_debug "check_git_status: Starting git status checks"
    
    # Note: Uncommitted changes are now handled by interactive_commit_prompt
    # This function now primarily checks branch status
    
    # Check if we're on main/master branch
    local branch
    branch=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD)
    if [[ "$branch" != "main" && "$branch" != "master" ]]; then
        log_warn "Not on main/master branch (currently on: $branch)"
        log_prompt "Continue anyway? (y/N): "
        read -p "" response </dev/tty
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
    
    log_progress "Checking GitHub release existence for v$version (gh release view)"
    if timeout 10 gh release view "v$version" --repo "$GITHUB_REPO" &>/dev/null; then
        log_progress "GitHub release v$version found"
        return 0  # Release exists
    else
        local exit_code=$?
        if [ $exit_code -eq 124 ]; then
            log_warn "GitHub release check timed out after 10 seconds (network may be unreachable)"
        else
            log_progress "GitHub release v$version does not exist"
        fi
        return 1  # Release does not exist
    fi
}

# Check if Git tag exists locally
_check_git_tag_local() {
    local version="$1"
    log_progress "Checking local git tag v$version"
    git -C "$REPO_ROOT" tag -l "v$version" | grep -q "v$version"
}

# Check if Git tag exists remotely
_check_git_tag_remote() {
    local version="$1"
    log_progress "Checking remote git tag v$version (git ls-remote)"
    if ! timeout 10 git -C "$REPO_ROOT" ls-remote --tags origin "refs/tags/v$version" 2>/dev/null | grep -q "v$version"; then
        local exit_code=$?
        if [ $exit_code -eq 124 ]; then
            log_warn "Git ls-remote check timed out after 10 seconds (network may be unreachable)"
        fi
        return 1
    fi
    return 0
}

# Check tag existence and return status
check_git_tag_exists() {
    local version="$1"
    local tag_local=false
    local tag_remote=false
    
    if _check_git_tag_local "$version" 2>/dev/null; then
        tag_local=true
    fi
    
    if _check_git_tag_remote "$version" 2>/dev/null; then
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
    log_debug "preflight_safety_check: Starting safety checks"
    echo ""
    
    # Check 1: GitHub Release
    log_info "Checking GitHub for existing release v$version..."
    log_progress "BEFORE: gh release view (this may hang if GitHub is unreachable)"
    if check_github_release "$version"; then
        log_warn "GitHub release v$version already exists"
        github_exists=true
        has_conflicts=true
    else
        log_success "No GitHub release found for v$version"
    fi
    log_progress "AFTER: gh release view completed"
    
    # Check 2: Git Tags
    log_info "Checking for existing Git tags v$version..."
    log_progress "BEFORE: git ls-remote (this may hang if remote is unreachable)"
    tag_status=$(check_git_tag_exists "$version")
    log_progress "AFTER: git ls-remote completed"
    
    case "$tag_status" in
        "both")
            log_warn "Git tag v$version exists locally AND remotely"
            has_conflicts=true
            ;;
        "local")
            log_warn "Git tag v$version exists locally only"
            has_conflicts=true
            ;;
        "remote")
            log_warn "Git tag v$version exists remotely only"
            has_conflicts=true
            ;;
        "none")
            log_success "No Git tags found for v$version"
            ;;
    esac
    
    echo ""
    
    # Handle conflicts if any
    if [[ "$has_conflicts" == true ]]; then
        log_warn "One or more conflicts detected!"
        echo ""
        
        # Unified prompt for handling all conflicts
        log_prompt "Delete existing release/tag(s) and re-create? (y/N): "
        read -p "" response </dev/tty
        
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
        # Always attempt both local and remote deletion to ensure robust cleanup
        # even if status detection was incomplete or had sync issues
        case "$tag_status" in
            "both"|"local")
                log_info "Deleting local tag v$version..."
                git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || log_warn "Failed to delete local tag (may not exist locally)"
                ;;&
            "both"|"remote")
                log_info "Deleting remote tag v$version..."
                git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || log_warn "Failed to delete remote tag (may not exist remotely)"
                ;;
        esac
        
        # Ensure local tag is always deleted as fallback for "remote"-only case
        # This handles situations where local tag exists but wasn't detected
        if [[ "$tag_status" == "remote" ]]; then
            log_info "Force-deleting local tag v$version (safety fallback)..."
            git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || true
        fi
        
        log_success "Existing release and tags cleaned up"
    else
        log_success "Pre-flight checks passed - no conflicts found"
    fi
    
    # Set global variable for tag status instead of echoing
    TAG_STATUS="$tag_status"
}

# prompt_version sets the global RELEASE_VERSION variable
# No command substitution is used - prevents stdout capture issues
prompt_version() {
    # Check if version was provided as command-line argument (for automation)
    if [ $# -ge 1 ] && [ -n "$1" ]; then
        local version="$1"
        validate_version "$version"
        log_debug "prompt_version: Version provided as argument: $version"
        RELEASE_VERSION="$version"
        return 0
    fi
    
    log_debug "prompt_version: No version argument, prompting user"
    
    # Prompt user for version input - simple read without any redirection
    log_prompt "Enter version number (format: X.Y.Z, e.g., 0.2.1): "
    local version
    read -r version || die "Failed to read version input. Hint: provide version as argument: ./release.sh X.Y.Z"
    
    log_debug "prompt_version: Got version input: '$version'"
    
    # Trim whitespace
    version=$(echo "$version" | xargs)
    
    if [ -z "$version" ]; then
        die "Version cannot be empty"
    fi
    
    validate_version "$version"
    RELEASE_VERSION="$version"
}

update_cargo_toml() {
    local version="$1"
    log_info "Updating Cargo.toml version to $version..."
    log_debug "update_cargo_toml: Updating version in Cargo.toml"
    
    if [ ! -f "$REPO_ROOT/Cargo.toml" ]; then
        die "Cargo.toml not found at $REPO_ROOT/Cargo.toml"
    fi
    
    sed -i "s/^version = \"[^\"]*\"/version = \"$version\"/" "$REPO_ROOT/Cargo.toml"
    
    log_success "Cargo.toml updated"
}

commit_version_bump() {
    local version="$1"
    
    log_debug "commit_version_bump: Starting version bump commit"
    
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
    log_debug "push_code: Pushing to origin master"
    
    git -C "$REPO_ROOT" push origin master || die "Failed to push code to origin master"
    
    log_success "Code pushed to GitHub"
}

git_tag_and_push() {
    local version="$1"
    local tag_status="${2:-none}"
    
    log_info "Creating Git tag v$version..."
    log_debug "git_tag_and_push: Creating tag for version $version"
    
    # Double-check tag status if not provided (safety net)
    if [[ "$tag_status" == "none" ]]; then
        tag_status=$(check_git_tag_exists "$version")
    fi
    
    # Handle any remaining tag conflicts (should be none after preflight, but just in case)
    case "$tag_status" in
        "both"|"local")
            log_warn "Local tag v$version still exists - attempting deletion..."
            git -C "$REPO_ROOT" tag -d "v$version" 2>/dev/null || log_warn "Local tag deletion failed, will use --force to overwrite"
            ;;&
        "both"|"remote")
            log_warn "Remote tag v$version still exists - attempting deletion..."
            git -C "$REPO_ROOT" push origin --delete "v$version" 2>/dev/null || log_warn "Remote tag deletion failed, will force-push new tag"
            ;;
    esac
    
    # Create annotated tag with --force flag to allow overwriting if it still exists locally
    # This handles cases where the deletion in preflight didn't fully complete
    git -C "$REPO_ROOT" tag -a "v$version" -m "Release v$version" --force || die "Failed to create tag v$version"
    
    log_info "Syncing tag cache with remote..."
    # Fetch tags from remote with --force to ensure local cache is current and not stale
    # This prevents "stale info" errors when pushing tags that may have been recreated
    git -C "$REPO_ROOT" fetch --tags --force || log_warn "Failed to fetch tags, proceeding with push attempt"
    
    log_info "Pushing tag to GitHub..."
    # Use --force for tag push (tags are atomic, safer than --force-with-lease)
    # This handles the case where remote tag has been recreated and local tracking is stale
    git -C "$REPO_ROOT" push origin "v$version" --force || die "Failed to push tag v$version"
    
    log_success "Tag v$version created and pushed"
}

build_release_binary() {
    log_info "Building release binary..."
    log_debug "build_release_binary: Starting cargo build --release"
    
    cd "$REPO_ROOT"
    
    # Build in release mode
    cargo build --release || die "Failed to build release binary"
    
    log_success "Release binary built"
}

create_tarball() {
    local version="$1"
    
    log_info "Creating release tarball..."
    log_debug "create_tarball: Creating tarball for version $version"
    
    cd "$REPO_ROOT"
    
    local binary_name="goatdkernel"
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    
    # Check if binary exists
    if [ ! -f "target/release/$binary_name" ]; then
        die "Release binary not found: target/release/$binary_name"
    fi
    
    # Create staging directory structure
    local staging_dir="goatdkernel-${version}-x86_64"
    rm -rf "$staging_dir"
    mkdir -p "$staging_dir/bin"
    
    # Copy binary to staging directory
    cp "target/release/$binary_name" "$staging_dir/bin/" || die "Failed to copy binary to staging directory"
    
    # Create tarball with staged binary, wrapper script, and install script
    tar -czf "$tarball_name" \
        -C . "$staging_dir/bin/$binary_name" \
        -C "$REPO_ROOT" goatdkernel.sh \
        -C "$REPO_ROOT/scripts" install.sh \
        || die "Failed to create tarball"
    
    # Clean up staging directory
    rm -rf "$staging_dir"
    
    # Generate SHA256 checksum
    sha256sum "$tarball_name" > "${tarball_name}.sha256"
    
    # Verify tarball doesn't already exist in root before moving
    if [ -f "$tarball_name" ] && [ "$(pwd)/$tarball_name" != "$REPO_ROOT/$tarball_name" ]; then
        mv "$tarball_name" "$REPO_ROOT/"
        mv "${tarball_name}.sha256" "$REPO_ROOT/"
    fi
    
    log_success "Tarball created: $tarball_name"
    log_success "SHA256 checksum: ${tarball_name}.sha256"
}

create_github_release() {
    local version="$1"
    
    log_info "Creating GitHub release (draft mode)..."
    log_debug "create_github_release: Creating GitHub release for v$version"
    
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
    log_debug "upload_release_assets: Uploading assets for v$version"
    
    cd "$REPO_ROOT"
    
    local tarball_name="goatdkernel-${version}-x86_64.tar.gz"
    local sha256_file="${tarball_name}.sha256"
    local png_file="$REPO_ROOT/assets/goatdkernel.png"
    
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
    
    # Upload PNG icon asset
    if [ -f "$png_file" ]; then
        gh release upload "v$version" "$png_file" --repo "$GITHUB_REPO"
        log_success "PNG icon uploaded"
    else
        log_warn "PNG icon not found at $png_file - skipping icon upload"
    fi
}

publish_release() {
    local version="$1"
    
    log_info "Publishing release (removing draft status)..."
    log_debug "publish_release: Publishing release v$version"
    
    gh release edit "v$version" --repo "$GITHUB_REPO" --draft=false
    
    log_success "Release published"
}

cleanup_local_files() {
    local version="$1"
    
    log_info "Cleaning up local files..."
    log_debug "cleanup_local_files: Removing tarball and checksum files"
    
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
    read -p "" response </dev/tty
    
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
    log_debug "main: Step 1 - Checking requirements"
    check_requirements
    log_debug "main: Requirements check passed"
    
    # Step 2: Prompt for version early so we can use it in commit messages
    log_debug "main: Step 2 - Prompting for version"
    prompt_version "${1:-}"
    local version="$RELEASE_VERSION"
    log_debug "main: Got version: $version"
    echo ""
    log_info "Preparing release for version: v$version"
    echo ""
    
    # Step 3: Interactive commit prompt for any uncommitted changes
    log_debug "main: Step 3 - Interactive commit prompt"
    interactive_commit_prompt "$version"
    
    # Step 4: Check git status (branch validation)
    log_debug "main: Step 4 - Checking git status"
    check_git_status
    
    # Step 5: Unified pre-flight safety check (GitHub release + Git tags)
    log_debug "main: Step 5 - Pre-flight safety checks"
    log_progress "MAIN STEP 5: About to call preflight_safety_check"
    # Call directly without command substitution to allow interactive prompts on TTY
    preflight_safety_check "$version"
    local tag_status="$TAG_STATUS"
    log_progress "MAIN STEP 5: preflight_safety_check completed, tag_status=$tag_status"
    
    # Step 6: Confirm release
    log_debug "main: Step 6 - Confirming release"
    confirm_release "$version"
    
    echo ""
    log_info "Starting release process..."
    echo ""
    
    # Step 7: Update version in Cargo.toml
    log_debug "main: Step 7 - Updating Cargo.toml"
    update_cargo_toml "$version"
    
    # Step 8: Commit and push version bump
    log_debug "main: Step 8 - Committing and pushing version bump"
    commit_version_bump "$version"
    push_code
    
    # Step 9: Create and push Git tag (passing tag_status for safety)
    log_debug "main: Step 9 - Creating git tag"
    git_tag_and_push "$version" "$tag_status"
    
    # Step 10: Build release binary
    log_debug "main: Step 10 - Building release binary"
    build_release_binary
    
    # Step 11: Create tarball
    log_debug "main: Step 11 - Creating tarball"
    create_tarball "$version"
    
    # Step 12: Create GitHub release
    log_debug "main: Step 12 - Creating GitHub release"
    create_github_release "$version"
    
    # Step 13: Upload assets
    log_debug "main: Step 13 - Uploading assets"
    upload_release_assets "$version"
    
    # Step 14: Publish release (remove draft status)
    log_debug "main: Step 14 - Publishing release"
    publish_release "$version"
    
    # Step 15: Invoke AUR maintenance (metadata synchronization - non-blocking)
    log_info "Step 15: Synchronizing AUR metadata..."
    (
        export PKGVER=$(grep "^pkgver=" aur/PKGBUILD | awk -F= '{print $2}' | tr -d "\"'")
        export PKGREL=$(grep "^pkgrel=" aur/PKGBUILD | awk -F= '{print $2}' | tr -d "\"'")
        if command -v python3 &> /dev/null; then
            python3 aur/maintenance_aur.py \
                --version "$PKGVER" \
                --release "$PKGREL" \
                2>&1 | sed 's/^/[AUR] /' || log_warn "AUR maintenance encountered an error (non-blocking)"
        else
            log_warn "python3 not found; skipping AUR metadata synchronization"
        fi
    ) &

    # Step 16: Cleanup local files
    log_debug "main: Step 16 - Cleaning up local files"
    cleanup_local_files "$version"
    
    # Step 17: Print summary
    log_debug "main: Step 17 - Printing summary"
    print_summary "$version"
    
    log_debug "main: Release completed successfully"
}

main "$@"

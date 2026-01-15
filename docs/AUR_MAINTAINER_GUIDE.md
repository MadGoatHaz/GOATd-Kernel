# AUR Maintainer Guide

This guide explains how to maintain and update the GOATd Kernel Builder AUR packages.

## Overview

There are two AUR packages for GOATd Kernel Builder:

- **`goatdkernel`** (source): Builds from Git tag on the user's system
- **`goatdkernel-bin`** (binary): Pre-built binary from GitHub Releases

Both packages are automatically packaged via CI/CD, but manual steps are needed to publish to the AUR.

## Setup: First-Time AUR Account Configuration

### 1. Create AUR Account
- Visit https://aur.archlinux.org/register/
- Create your account with SSH key authentication

### 2. Configure SSH Keys
```bash
# Generate SSH key (or use existing)
ssh-keygen -t rsa -b 4096 -f ~/.ssh/aur

# Add public key to AUR account
cat ~/.ssh/aur.pub
# Copy output and paste into AUR account settings

# Configure SSH for AUR git
cat >> ~/.ssh/config << EOF
Host aur.archlinux.org
    IdentityFile ~/.ssh/aur
    User aur
EOF

chmod 600 ~/.ssh/config
```

### 3. Verify SSH Connection
```bash
ssh aur.archlinux.org
# Should show: Welcome to the Arch Linux User Repository
```

### 4. Clone AUR Repositories
```bash
# Clone both AUR packages locally
mkdir -p ~/aur
git clone ssh://aur@aur.archlinux.org/goatdkernel.git ~/aur/goatdkernel
git clone ssh://aur@aur.archlinux.org/goatdkernel-bin.git ~/aur/goatdkernel-bin
```

## Workflow: One-Command Release & AUR Push

The repository includes an automated release script (`scripts/release.sh`) that handles the entire release workflow with a single command.

### Quick Start: One-Command Release

```bash
# From the GOATd-Kernel repository root
./scripts/release.sh 0.2.0
```

That's it! The script will:

1. **Update version** in `Cargo.toml`
2. **Create Git tag** (`v0.2.0`)
3. **Push tag to GitHub** (triggers GitHub Actions binary build)
4. **Wait for binary release** (monitors GitHub Actions)
5. **Update both AUR packages** (`goatdkernel` and `goatdkernel-bin`)
6. **Generate `.SRCINFO`** for both packages
7. **Push to AUR** automatically

### Manual Verification

While the script handles everything, you can manually verify:

```bash
# Monitor GitHub Actions
open https://github.com/MadGoatHaz/GOATd-Kernel/actions

# Check AUR package pages
open https://aur.archlinux.org/packages/goatdkernel
open https://aur.archlinux.org/packages/goatdkernel-bin
```

## Legacy Workflow: Manual Step-by-Step (Reference)

For reference, here's the manual process that the script automates:

### Step 1: Tag the Release in GitHub

```bash
# In the GOATd-Kernel repo
git tag -a v0.2.0 -m "Release version 0.2.0"
git push origin v0.2.0
```

This triggers the `.github/workflows/release.yml` GitHub Action, which:
1. Builds the Rust binary for x86_64
2. Creates a tarball: `goatdkernel-0.2.0-x86_64.tar.gz`
3. Computes SHA256 checksum
4. Creates a GitHub Release with these artifacts

### Step 2: Update Source PKGBUILD

```bash
# Clone the AUR repo (or update if already cloned)
git clone ssh://aur@aur.archlinux.org/goatdkernel.git ~/aur/goatdkernel
cd ~/aur/goatdkernel

# Copy the source PKGBUILD template
cp ./pkgbuilds/source/PKGBUILD ~/aur/goatdkernel/

# Update version number
# Edit PKGBUILD and change pkgver=0.2.0

# Edit .SRCINFO to update metadata
# Run: makepkg --printsrcinfo > .SRCINFO
cd ~/aur/goatdkernel
makepkg --printsrcinfo > .SRCINFO

# Verify it works locally
makepkg  # Test build

# Commit and push
git add PKGBUILD .SRCINFO
git commit -m "Update to version 0.2.0"
git push
```

### Step 3: Update Binary PKGBUILD

```bash
# Clone the AUR repo (or update if already cloned)
git clone ssh://aur@aur.archlinux.org/goatdkernel-bin.git ~/aur/goatdkernel-bin
cd ~/aur/goatdkernel-bin

# Copy the binary PKGBUILD template
cp ./pkgbuilds/binary/PKGBUILD ~/aur/goatdkernel-bin/

# Update version number and SHA256 hash
# Edit PKGBUILD and change pkgver=0.2.0

# Get the SHA256 from the GitHub Release
# Visit: https://github.com/MadGoatHaz/GOATd-Kernel/releases/tag/v0.2.0
# Copy the value from: goatdkernel-0.2.0-x86_64.tar.gz.sha256
# Or run:
sha256sum=$(curl -s https://github.com/MadGoatHaz/GOATd-Kernel/releases/download/v0.2.0/goatdkernel-0.2.0-x86_64.tar.gz.sha256 | awk '{print $1}')
echo $sha256sum

# Update PKGBUILD with the sha256sum
# Change the sha256sums array line to:
# sha256sums=('ACTUAL_SHA256_HERE' 'SKIP')

# Edit .SRCINFO
cd ~/aur/goatdkernel-bin
makepkg --printsrcinfo > .SRCINFO

# Verify it works locally
makepkg  # Test build

# Commit and push
git add PKGBUILD .SRCINFO
git commit -m "Update to version 0.2.0"
git push
```

## Script Reference: `scripts/release.sh`

The automated release script provides the following features:

### Prerequisites
- AUR repositories cloned locally (default: `~/aur/goatdkernel` and `~/aur/goatdkernel-bin`)
- SSH access to AUR configured
- Clean Git working directory

### Usage
```bash
./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.2.0
```

### Configuration
The script respects the `AUR_BASE_DIR` environment variable:
```bash
# Use custom AUR directory
AUR_BASE_DIR=/custom/path ./scripts/release.sh 0.2.0
```

### What It Does

1. **Validates input**: Checks version format (X.Y.Z)
2. **Pre-flight checks**: Ensures clean repo, AUR clones exist, required tools available
3. **Version bump**: Updates `Cargo.toml` and creates version commit
4. **Git tagging**: Creates annotated tag and pushes to GitHub
5. **GitHub Actions wait**: Polls GitHub API for binary release completion
6. **SHA256 fetch**: Automatically retrieves SHA256 from GitHub Release
7. **AUR sync**: Updates both PKGBUILD files with new version and SHA256
8. **SRCINFO generation**: Runs `makepkg --printsrcinfo` for both packages
9. **AUR push**: Commits and pushes both packages to AUR

### Error Handling

The script includes robust error handling:
- Exits on missing requirements
- Validates version format before proceeding
- Checks for uncommitted changes
- Verifies AUR repositories exist
- Handles git branch name variations (main/master)
- Gracefully handles timeout for GitHub release build

### Troubleshooting Script Issues

**"Missing required commands"**
```bash
# Ensure you have: git, sed, curl, makepkg
sudo pacman -S base-devel
```

**"AUR repositories not properly set up"**
```bash
# Clone the repositories
mkdir -p ~/aur
git clone ssh://aur@aur.archlinux.org/goatdkernel.git ~/aur/goatdkernel
git clone ssh://aur@aur.archlinux.org/goatdkernel-bin.git ~/aur/goatdkernel-bin

# Or set custom directory
export AUR_BASE_DIR=/your/custom/path
./scripts/release.sh 0.2.0
```

**"Working directory has uncommitted changes"**
```bash
# Commit or stash your changes
git commit -am "Your message"
# or
git stash
```

**"SSH Connection Issues"**
- Verify SSH key is added to AUR account
- Check `.ssh/config` has correct settings for aur.archlinux.org
- Test with: `ssh -vv aur.archlinux.org`

## Troubleshooting

### SSH Connection Issues
- Verify SSH key is added to AUR account
- Check `.ssh/config` has correct settings
- Test with: `ssh -vv aur.archlinux.org`

### .SRCINFO Generation Fails
- Ensure you have `pacman` and `base-devel` installed
- Run from within the package directory
- Example: `cd ~/aur/goatdkernel && makepkg --printsrcinfo > .SRCINFO`

### SHA256 Mismatch
- Double-check the GitHub Release URL
- Verify the tarball filename matches
- Recompute locally: `sha256sum goatdkernel-X.X.X-x86_64.tar.gz`

### Build Fails Locally
- Test with `makepkg -si` to install deps
- Review error logs for missing dependencies
- Ensure `pkgver`, `pkgrel`, and deps are correct in PKGBUILD

## Asset Files

The PKGBUILD files reference assets that should be in the repository:

```
assets/
├── goatdkernel.desktop      # Desktop entry file
├── com.goatd.kernel.policy  # Polkit policy file
└── goatdkernel.svg          # Application icon
```

These are included in the source tarball and must be present during both source and binary package builds.

## GitHub Actions: Binary Release Automation

When you push a tag (e.g., `v0.2.0`), GitHub Actions automatically:

1. **Builds the binary** using `cargo build --release --locked`
2. **Creates tarball** named `goatdkernel-0.2.0-x86_64.tar.gz`
3. **Computes SHA256** and saves to `.sha256` file
4. **Creates GitHub Release** with both files attached

You only need to run the release script, which handles the rest!

## Resources

- **AUR Documentation**: https://wiki.archlinux.org/title/Arch_User_Repository
- **PKGBUILD Format**: https://wiki.archlinux.org/title/PKGBUILD
- **AUR Submission Guidelines**: https://wiki.archlinux.org/title/AUR_submission_guidelines
- **GitHub Releases API**: https://docs.github.com/en/rest/releases
- **makepkg Documentation**: https://wiki.archlinux.org/title/Makepkg

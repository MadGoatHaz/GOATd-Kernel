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

## Workflow: Releasing a New Version

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

## Quick Reference: Full Update Script

Here's a bash script to automate the AUR update process:

```bash
#!/bin/bash
set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
REPO_DIR="${2:-.}"

# 1. Get the SHA256
echo "Fetching SHA256 from GitHub Release..."
SHA256=$(curl -s "https://github.com/MadGoatHaz/GOATd-Kernel/releases/download/v${VERSION}/goatdkernel-${VERSION}-x86_64.tar.gz.sha256" | awk '{print $1}')
echo "SHA256: $SHA256"

# 2. Update source package
echo "Updating source package..."
cd ~/aur/goatdkernel
cp "${REPO_DIR}/pkgbuilds/source/PKGBUILD" ./
sed -i "s/pkgver=.*/pkgver=${VERSION}/" PKGBUILD
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Update to version ${VERSION}"
git push
echo "✓ Source package updated"

# 3. Update binary package
echo "Updating binary package..."
cd ~/aur/goatdkernel-bin
cp "${REPO_DIR}/pkgbuilds/binary/PKGBUILD" ./
sed -i "s/pkgver=.*/pkgver=${VERSION}/" PKGBUILD
sed -i "s/'SKIP' 'SKIP'/'${SHA256}' 'SKIP'/" PKGBUILD
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Update to version ${VERSION}"
git push
echo "✓ Binary package updated"

echo "✓ All AUR packages updated to ${VERSION}"
```

Save as `update-aur.sh` and run:
```bash
bash update-aur.sh 0.2.0 /path/to/GOATd-Kernel
```

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

You only need to copy the SHA256 and update the binary PKGBUILD. No manual compilation needed!

## Resources

- **AUR Documentation**: https://wiki.archlinux.org/title/Arch_User_Repository
- **PKGBUILD Format**: https://wiki.archlinux.org/title/PKGBUILD
- **AUR Submission Guidelines**: https://wiki.archlinux.org/title/AUR_submission_guidelines
- **GitHub Releases API**: https://docs.github.com/en/rest/releases

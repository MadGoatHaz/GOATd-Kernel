# AUR Maintenance Integration Documentation

## Overview
The AUR maintenance pipeline integrates `aur/maintenance_aur.py` into `./scripts/release.sh` (Step 15) to automatically synchronize package metadata (version, pkgrel, sha256) to the AUR repository.

## Components

### 1. maintenance_aur.py
- **Location:** `aur/maintenance_aur.py`
- **Purpose:** Fetch latest GitHub release info, update PKGBUILD/SRCINFO with correct version/checksum
- **Entry:** Command-line arguments: `--version`, `--release`
- **Authentication:** Optional `GITHUB_TOKEN` environment variable
- **Non-blocking:** Errors logged but don't fail release workflow

### 2. PKGBUILD Configuration
- **Location:** `aur/PKGBUILD`
- **Key Variables:** pkgver=0.2.3, pkgrel=1
- **Source Mapping:** Local filename uses ${pkgrel}, remote URL uses only ${pkgver}
- **SHA256:** Maintained by maintenance_aur.py

### 3. release.sh Integration
- **Location:** `./scripts/release.sh`, Step 15
- **Trigger:** After GitHub release published, before cleanup
- **Exports:** PKGVER, PKGREL from aur/PKGBUILD
- **Invocation:** `python3 aur/maintenance_aur.py --version $PKGVER --release $PKGREL`
- **Error Handling:** Non-blocking (log_warn if fails)

## Version Strategy
- **GitHub Tag Format:** v${pkgver} (e.g., v0.2.3)
- **GitHub Asset Name:** goatdkernel-${pkgver}-x86_64.tar.gz
- **Local AUR Filename:** goatdkernel-${pkgver}-${pkgrel}-x86_64.tar.gz
- **pkgrel:** Always extracted from PKGBUILD, never from tag

## Development Workflow
1. Make code changes and tag release: `v0.2.3`
2. Run `./scripts/release.sh` (builds, uploads, and triggers AUR sync)
3. Step 15 automatically:
   - Extracts version from aur/PKGBUILD
   - Calls maintenance_aur.py to update metadata
   - Re-generates .SRCINFO
4. Manual AUR push: `cd aur && git push` (when ready)

## Testing
- See `./TESTS_README.md` for AUR integration test references

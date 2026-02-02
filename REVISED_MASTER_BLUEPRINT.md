# REVISED Master Blueprint: AUR Decoupling & Manual Flow

## 1. Overview
This blueprint outlines the decoupling of the general release process (`release.sh`) from the AUR maintenance process (`maintenance_aur.py`). The goal is to allow the user to handle AUR pushes manually to accommodate password-protected SSH keys and prevent terminal hangs caused by automated background processes.

## 2. Decoupling Strategy

### 2.1 Revert `release.sh` Integration
- **Action**: Remove any code blocks in [`master/scripts/release.sh`](master/scripts/release.sh) that invoke `python3 aur/maintenance_aur.py`.
- **Reason**: The user requires these to be separate steps.
- **Verification**: `release.sh` should complete its local tasks (tagging, tarball creation, SHA256 generation) and exit without spawning AUR tasks.

### 2.2 Re-architecting `maintenance_aur.py`
- **Action**: Disable automatic `git push` and `git commit` within the script's logic.
- **New Flow**:
    1. Parse Version (from `Cargo.toml` or CLI arg).
    2. Download/Locate Tarball.
    3. Calculate SHA256.
    4. Update `PKGBUILD`.
    5. Run `makepkg --printsrcinfo > .SRCINFO`.
    6. **Terminal Output**: Print a clearly formatted block of shell commands for the user to copy/paste.

## 3. Manual Instruction Block (Proposal)
The script will output the following upon successful preparation:
```bash
AUR Preparation Complete. Run the following to finish the update:

cd aur
git add PKGBUILD .SRCINFO
git commit -m "Update to [VERSION]"
git push
```

## 4. Addressing the Terminal Hang
- **Cause Analysis**: Likely caused by `release.sh` waiting for a backgrounded process or a piped command in `maintenance_aur.py` that hit a password prompt (which isn't visible in some terminal configurations/background modes).
- **Fix**: 
    - Full removal of background calls in `release.sh`.
    - Ensure `maintenance_aur.py` uses `subprocess.run` with `check=True` for preparation steps but stays strictly local.
    - No interactive shells or backgrounding.

## 5. SemVer Synchronization
- Both scripts must use the same regex for version validation:
  `^([0-9]+)\.([0-9]+)\.([0-9]+)(-([0-9]+))?$`
- `release.sh` will handle the 3-part version for local tagging.
- `maintenance_aur.py` will handle the 3-part version for AUR `pkgver`.

## 6. Implementation Steps for Code Mode
1. **Edit** [`master/scripts/release.sh`](master/scripts/release.sh):
   - Locate and delete the `maintenance_aur.py` execution block.
2. **Edit** [`aur/maintenance_aur.py`](aur/maintenance_aur.py):
   - Comment out or delete `git_commit` and `git_push` function calls.
   - Implement `print_manual_instructions()` function.
   - Verify `subprocess` calls don't hang by setting appropriate timeouts or ensuring they are non-interactive.
3. **Validation**:
   - Run `release.sh` -> Confirm it finishes without calling AUR.
   - Run `maintenance_aur.py` -> Confirm it updates files and prints manual commands.

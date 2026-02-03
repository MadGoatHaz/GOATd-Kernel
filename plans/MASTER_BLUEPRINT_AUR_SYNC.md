# Master Blueprint: AUR Makepkg Failure Resolution

This blueprint outlines the strategy to resolve the `makepkg` failure caused by package name discrepancies and tarball structure mismatches between the build system and the AUR package definition.

## 1. Problem Analysis

- **Binary Name Discrepancy**: `Cargo.toml` and `release.sh` produce `goatd_kernel`, but `PKGBUILD` expects `goatdkernel`.
- **Tarball Structure Discrepancy**: `release.sh` creates a flat tarball with various files at the root, while `PKGBUILD` expects a nested hierarchy: `goatdkernel-${pkgver}-x86_64/bin/goatdkernel`.
- **Naming Inconsistency**: The project refers to itself variously as `GOATd-Kernel`, `goatd_kernel`, and `goatdkernel`.

## 2. Strategic Solution

We will standardize on `goatdkernel` (no underscore) for the public-facing binary and package name, while maintaining the Rust crate's internal name if necessary, though ideally, even the binary name in `Cargo.toml` should align.

### Action A: Align Binaries in `Cargo.toml`
Update the `[[bin]]` section in `./Cargo.toml` to produce a binary named `goatdkernel`. This ensures that `cargo build --release` naturally outputs the expected file.

### Action B: Standardize `release.sh` Staging
Modify `./scripts/release.sh` to create a structured staging directory before archiving.
- Create `staging/goatdkernel-${version}-x86_64/bin/`
- Move the compiled binary into the `bin/` subfolder.
- Include other scripts (e.g., `goatdkernel.sh`, `install.sh`) in the staging area if they are part of the distributed package.
- Archive the entire staging directory so the tarball contains the expected prefix.

### Action C: Update `PKGBUILD` Logic
Align `aur/PKGBUILD` to point to the correct files within the extracted tarball structure.
- Ensure `install` commands use the correct paths relative to `$srcdir`.
- Verify the `source` array naming matches the output of `release.sh`.

## 3. Implementation Steps

### Step 1: Rust Configuration (`./Cargo.toml`)
- Change `[[bin]] name` from `goatd_kernel` to `goatdkernel`.
- (Optional) Change `[package] name` to `goatdkernel` for consistency.

### Step 2: Build Script Refinement (`./scripts/release.sh`)
- Define a staging variable: `STAGING_DIR="staging/goatdkernel-${version}-x86_64"`.
- Implement `mkdir -p "$STAGING_DIR/bin"`.
- Copy binary: `cp "target/release/goatdkernel" "$STAGING_DIR/bin/"`.
- Update the `tar` command to archive from the `staging` root.

### Step 3: Package Definition (`aur/PKGBUILD`)
- Verify `package()` function uses the structured path: `"$srcdir/goatdkernel-${pkgver}-x86_64/bin/goatdkernel"`.
- Ensure no underscores are used in binary references.

## 4. Verification Plan (No `read_file` required)

To verify the fix without reading the files directly, the following CLI-based checks will be performed:

1.  **Binary Name Check**:
    `grep "name = \"goatdkernel\"" ./Cargo.toml`
2.  **Tarball Structure Check**:
    After running a mock-release or observing the build, run:
    `tar -tf goatdkernel-*.tar.gz | grep "bin/goatdkernel"`
    This confirms the hierarchy exists.
3.  **Local Makepkg Simulation**:
    Run `makepkg -p aur/PKGBUILD` (or equivalent check) to verify the `package()` function can find the files.
4.  **Symbol Search**:
    `grep -r "goatd_kernel" ./scripts/release.sh` to ensure no stale references remain in the release logic.

## 5. Summary of Changes

| Component | Change | Reason |
| :--- | :--- | :--- |
| `Cargo.toml` | Binary name `goatd_kernel` -> `goatdkernel` | Consistency with AUR |
| `release.sh` | Add staging directory hierarchy | Match `PKGBUILD` expectations |
| `PKGBUILD` | Align `install` paths | Resolve "No such file" error |

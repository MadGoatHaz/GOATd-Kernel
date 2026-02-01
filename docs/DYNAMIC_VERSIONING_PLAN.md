# Dynamic Versioning Plan (`v0.2.1`)

GOATd Kernel uses a dynamic versioning strategy to ensure that kernel builds are uniquely identifiable by their profile, date, and source lineage.

## Versioning Strategy

The current overarching project version is `v0.2.1`. However, the kernels produced match the upstream Linux versioning with GOATd-specific injections.

### 1. Kernel Version String Injection

The versioning logic injects metadata into the kernel's local version.

- **Profile Injection**: The active profile name (e.g., `gaming`, `efficiency`) is appended to the version.
- **Date/Build ID**: Build timestamps or sequence counters are used to distinguish between different builds of the same kernel version.
- **Scheme**: `pkgver-pkgrel-goatd-{profile}`.

### 2. RC (Release Candidate) Tag Handling

Handling of upstream pre-releases (RCs) is critical for early testers. This is managed in [`src/kernel/git.rs`](src/kernel/git.rs).

- **Lower Precedence**: RC versions are internally treated as having lower precedence than the final releases (e.g., `6.13-rc7` < `6.13.0`).
- **Tag Extraction**: The `libgit2` wrapper extracts tags like `v6.13-rc7`.
- **Normalization**: The system normalizes these tags to ensure compatibility with Arch Linux's `pkgver` requirements (e.g., replacing hyphens with dots if necessary).

### 3. Source Version Validation

Before building, the system validates that the source code matches the expected version.
- **`validate_source_version`**: Located in `src/kernel/git.rs`, this function reads the `PKGBUILD` and compares the `pkgver` against the intended target.
- **`synchronize_pkgbuild_version`**: If the source version is correct but the `PKGBUILD` metadata is stale, [`src/kernel/patcher/pkgbuild.rs`](src/kernel/patcher/pkgbuild.rs) updates the file in-place.

## Workflow

1. **Discovery**: Orchestrator identifies target version (e.g., `6.18.3` or `6.19-rc6`).
2. **Tag Verification**: `src/kernel/git.rs` confirms the tag exists in the upstream repo.
3. **Localversion Patching**: The patcher modifies the `Kconfig` or `PKGBUILD` variable to include the GOATd suffix.
4. **Final String**: The resulting kernel reports its version via `uname -r` as something like `6.18.3-arch1-1-goatd-gaming`.

---
*This versioning plan ensures that users can always identify exactly which GOATd profile they are running and whether it is a stable or RC-based build.*

/// Kernel Manager module: High-level package management logic
///
/// This module handles:
/// - Listing installed kernel packages
/// - Scanning workspace for built kernel artifacts
/// - Deleting built kernel files with robustness improvements
/// - Parsing kernel package filenames
///
/// All operations use strict filtering to ensure only actual kernel packages are involved.
use std::path::PathBuf;

// Import logging macros
use crate::log_info;

/// Represents a kernel package (installed or built)
#[derive(Clone, Debug)]
pub struct KernelPackage {
    pub name: String,
    pub version: String,
    pub is_goatd: bool,
    pub path: Option<PathBuf>,
}

impl KernelPackage {
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.version)
    }
}

/// Kernel Artifact Registry: Safely correlates kernel, headers, and docs
///
/// Ensures that when installing a kernel package, the corresponding headers and docs
/// are correctly identified and bundled together. This prevents version mismatches
/// where kernel, headers, and docs from different builds could be installed.
///
/// # Example
/// ```
/// use std::path::PathBuf;
/// let registry_result = goatd_kernel::kernel::manager::KernelArtifactRegistry::new(
///     PathBuf::from("/path/to/linux-6.18.3-x86_64.pkg.tar.zst"),
///     "linux".to_string(),
///     "6.18.3-arch1-1".to_string(),
/// );
/// ```
#[derive(Clone, Debug)]
pub struct KernelArtifactRegistry {
    /// Main kernel package path
    pub kernel_path: PathBuf,
    /// Kernel variant (linux, linux-zen, linux-lts, linux-goatd-{profile}, etc.)
    pub kernel_variant: String,
    /// Kernel release version (6.18.3-arch1-1, 6.18.3-arch1-1-goatd-{profile}, etc.)
    pub kernel_release: String,
    /// Headers package path (if found)
    pub headers_path: Option<PathBuf>,
    /// Docs package path (if found)
    pub docs_path: Option<PathBuf>,
    /// Whether all artifacts are from the same build (versions match)
    pub integrity_verified: bool,
}

impl KernelArtifactRegistry {
    /// Create a new registry and validate artifact correlation
    ///
    /// # Arguments
    /// * `kernel_path` - Absolute path to the main kernel package
    /// * `kernel_variant` - Kernel variant (extracted from filename)
    /// * `kernel_release` - Kernel release version (internal "Source of Truth")
    ///
    /// # Returns
    /// A new registry with all related artifacts collected and verified using heuristic matching
    ///
    /// # Hyper-Heuristic Discovery Strategy (Chunk 3)
    /// This method uses intelligent fallback logic to find headers/docs:
    /// 1. First tries primary pattern: {variant}{suffix}-{kernel_release}
    /// 2. Then tries filename fallback: {variant}{suffix}-{filename_version}
    /// 3. Then tries alternate prefixes with both version formats
    /// 4. Then tries fuzzy component matching
    /// 5. Finally tries variant prefix matching if needed
    pub fn new(
        kernel_path: PathBuf,
        kernel_variant: String,
        kernel_release: String,
        workspace_path: Option<&std::path::Path>,
    ) -> Result<Self, String> {
        if !kernel_path.exists() {
            return Err(format!(
                "Kernel package not found: {}",
                kernel_path.display()
            ));
        }

        // CRITICAL (Chunk 1): CANONICALIZE kernel_path immediately
        // Ensures absolute path for privileged execution context
        let kernel_path = match kernel_path.canonicalize() {
            Ok(abs_path) => {
                log_info!("[KernelArtifactRegistry] [CANONICALIZE] Resolved kernel_path to absolute: {}", abs_path.display());
                abs_path
            }
            Err(e) => {
                return Err(format!("Failed to canonicalize kernel_path: {}", e));
            }
        };

        let parent_dir = match kernel_path.parent() {
            Some(p) => p,
            None => return Err("Cannot determine parent directory of kernel package".to_string()),
        };

        // Extract filename version as fallback (e.g., "6.19rc6-1" from the .pkg.tar.zst name)
        let filename_version = Self::extract_version_from_filename(&kernel_path);
        log_info!("[KernelArtifactRegistry] Kernel Identity: variant='{}', internal_version='{}', filename_version={:?}",
            kernel_variant, kernel_release, filename_version);

        // Extract core identity for diagnostic logging
        let variant_core = if kernel_variant.starts_with("linux-") {
            &kernel_variant[6..]
        } else {
            &kernel_variant
        };
        let version_core = Self::extract_version_core(&kernel_release);

        // STRATEGY 0 (Highest Priority): Search workspace_path if provided
        let mut headers_path = None;
        let mut docs_path = None;

        if let Some(workspace_root) = workspace_path {
            log_info!("[KernelArtifactRegistry] [STRATEGY-0] Searching workspace: {}", workspace_root.display());
            
            // Search for headers and docs in workspace variant subdirectories
            if let Ok(entries) = std::fs::read_dir(workspace_root) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if metadata.is_dir() {
                            let entry_path = entry.path();
                            let entry_name = entry_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            
                            // Look for headers and docs in variant subdirectories
                            if entry_name.contains("-headers-") {
                                log_info!("[KernelArtifactRegistry] [STRATEGY-0] Found candidate headers at: {}", entry_path.display());
                                // Validate this is a kernel headers package
                                if entry_path.join("include/linux").exists() {
                                    headers_path = Some(entry_path.clone());
                                    log_info!("[KernelArtifactRegistry] [STRATEGY-0] ✓ Verified headers with include/linux: {}", entry_path.display());
                                }
                            } else if entry_name.contains("-docs-") {
                                log_info!("[KernelArtifactRegistry] [STRATEGY-0] Found candidate docs at: {}", entry_path.display());
                                if entry_path.join("usr/share/doc").exists() {
                                    docs_path = Some(entry_path.clone());
                                    log_info!("[KernelArtifactRegistry] [STRATEGY-0] ✓ Verified docs with usr/share/doc: {}", entry_path.display());
                                }
                            }
                        }
                    }
                }
            }
        }

        // STRATEGY 1: Fall back to proximity-based search in the same directory
        if headers_path.is_none() {
            log_info!("[KernelArtifactRegistry] [STRATEGY-1] Proximity search in: {}", parent_dir.display());
            headers_path = Self::find_related_artifact(
                parent_dir,
                &kernel_variant,
                &kernel_release,
                filename_version.as_deref(),
                "-headers",
            )?;
        }

        // STRATEGY 1: Fall back to proximity-based search for docs
        if docs_path.is_none() {
            log_info!("[KernelArtifactRegistry] [STRATEGY-1] Proximity search for docs in: {}", parent_dir.display());
            docs_path = Self::find_related_artifact(
                parent_dir,
                &kernel_variant,
                &kernel_release,
                filename_version.as_deref(),
                "-docs",
            )?;
        }

        // Mandatory Guard (Chunk 4): Enhanced diagnostic logging with attempted permutations
        if headers_path.is_none() {
            log_info!("[KernelArtifactRegistry] ⚠️⚠️⚠️ CRITICAL WARNING: Headers NOT FOUND ⚠️⚠️⚠️");
            log_info!("[KernelArtifactRegistry] Attempted Permutations:");
            log_info!(
                "[KernelArtifactRegistry]   1. {}-headers-{} (primary version match)",
                kernel_variant,
                kernel_release
            );
            if let Some(ref fb_ver) = filename_version {
                log_info!(
                    "[KernelArtifactRegistry]   2. {}-headers-{} (filename version fallback)",
                    kernel_variant,
                    fb_ver
                );
            }
            if !variant_core.is_empty() && variant_core != kernel_variant {
                log_info!("[KernelArtifactRegistry]   3. linux-headers-{}-{} (alternate prefix - primary)", variant_core, kernel_release);
                if let Some(ref fb_ver) = filename_version {
                    log_info!("[KernelArtifactRegistry]   4. linux-headers-{}-{} (alternate prefix - filename)", variant_core, fb_ver);
                }
            }
            log_info!("[KernelArtifactRegistry]   5. Fuzzy match: contains '{}' AND 'headers' AND '{}' (component-based)",
                variant_core, version_core);
            log_info!(
                "[KernelArtifactRegistry]   6. {}-headers-* (final fallback - any version)",
                kernel_variant
            );
            log_info!("[KernelArtifactRegistry] Headers are MANDATORY for DKMS module builds!");
            log_info!("[KernelArtifactRegistry] This installation will result in Partial Success (no out-of-tree modules)");
            log_info!("[KernelArtifactRegistry] ⚠️⚠️⚠️ DKMS will fail to build out-of-tree modules without headers ⚠️⚠️⚠️");
        }

        // Phase 10: Deep validation - Verify tarball content if headers found
        let mut integrity_verified = headers_path.is_some();

        if let Some(ref headers) = headers_path {
            // Create a temporary registry instance for calling verify_tarball_content
            let temp_registry = KernelArtifactRegistry {
                kernel_path: kernel_path.clone(),
                kernel_variant: kernel_variant.clone(),
                kernel_release: kernel_release.clone(),
                headers_path: None,
                docs_path: None,
                integrity_verified: false,
            };

            // Define critical files to check in headers tarball
            let critical_files = ["include/linux/memremap.h"];

            match temp_registry.verify_tarball_content(headers, &critical_files) {
                Ok(true) => {
                    log_info!("[KernelArtifactRegistry] ✓ Deep validation PASSED: All critical files found in headers tarball");
                    integrity_verified = true;
                }
                Ok(false) => {
                    log_info!("[KernelArtifactRegistry] ✗ Deep validation FAILED: Critical files missing from headers tarball");
                    integrity_verified = false;
                }
                Err(e) => {
                    log_info!("[KernelArtifactRegistry] ⚠️ Deep validation ERROR: Failed to verify tarball content: {}", e);
                    integrity_verified = false;
                }
            }
        } else {
            log_info!(
                "[KernelArtifactRegistry] Skipping deep validation: Headers tarball not found"
            );
        }

        Ok(KernelArtifactRegistry {
            kernel_path,
            kernel_variant,
            kernel_release,
            headers_path,
            docs_path,
            integrity_verified,
        })
    }

    /// Verify tarball content by peeking inside and checking for critical files
    ///
    /// # Arguments
    /// * `path` - Path to the tarball (.pkg.tar.zst file)
    /// * `critical_files` - List of critical files to check for (e.g., ["include/linux/memremap.h"])
    ///
    /// # Returns
    /// `Ok(true)` if all critical files are found, `Ok(false)` if any are missing, `Err` on command failure
    ///
    /// # Implementation Details
    /// - Uses `tar -tf` to list tarball contents without extracting
    /// - Normalizes paths by stripping leading './' and '/'
    /// - Logs detailed results for diagnostics
    fn verify_tarball_content(
        &self,
        path: &PathBuf,
        critical_files: &[&str],
    ) -> Result<bool, String> {
        use std::process::Command;

        // Run tar -tf to list tarball contents
        // Use raw bytes to capture accurate path information
        let output = Command::new("tar")
            .args(&["-tf"])
            .arg(path) // Pass path as OsStr to preserve UTF-8 safety
            .output()
            .map_err(|e| format!("Failed to run tar -tf on tarball: {}", e))?;

        if !output.status.success() {
            // Use lossy conversion ONLY for logging
            return Err(format!(
                "tar -tf failed for tarball {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Use lossy conversion to handle any non-UTF8 paths in tarball gracefully
        let tarball_contents = String::from_utf8_lossy(&output.stdout);
        let mut all_files_found = true;

        // Check each critical file
        for critical_file in critical_files {
            // Normalize the critical file path (remove leading ./ or /)
            let normalized_critical = critical_file
                .trim_start_matches("./")
                .trim_start_matches("/");

            // Check if any line in tarball contents matches this critical file
            // Use suffix matching to handle Arch's deep directory structure (usr/lib/modules/*/build/...)
            let found = tarball_contents.lines().any(|line| {
                let normalized_line = line.trim_start_matches("./").trim_start_matches("/");

                // Exact match
                if normalized_line == normalized_critical {
                    return true;
                }

                // Directory prefix match (e.g., "include/linux/memremap.h/" is a directory)
                if normalized_line.starts_with(&format!("{}/", normalized_critical)) {
                    return true;
                }

                // Suffix match for deeply nested paths
                // E.g., "usr/lib/modules/6.18/build/include/linux/memremap.h" ends with "include/linux/memremap.h"
                if normalized_line.ends_with(normalized_critical) {
                    // Verify it's a proper path boundary (preceded by / or at start)
                    let prefix_len = normalized_line
                        .len()
                        .saturating_sub(normalized_critical.len());
                    if prefix_len == 0 {
                        return true; // Already checked above, but included for clarity
                    }
                    let preceding_char = normalized_line.chars().nth(prefix_len - 1);
                    if preceding_char == Some('/') {
                        return true;
                    }
                }

                false
            });

            if found {
                log_info!(
                    "[KernelArtifactRegistry] ✓ Critical file found in tarball: {} (including deeply nested paths)",
                    critical_file
                );
            } else {
                log_info!(
                    "[KernelArtifactRegistry] ✗ CRITICAL FILE MISSING from tarball: {}",
                    critical_file
                );
                all_files_found = false;
            }
        }

        Ok(all_files_found)
    }

    /// Find a related artifact (headers or docs) using hyper-heuristic permutation discovery
    /// with dynamic identity builder support for `linux-{variant}-goatd-{profile}` scheme
    ///
    /// # Arguments
    /// * `parent_dir` - Directory to search in
    /// * `kernel_variant` - Kernel variant (possibly with GOATd branding) to match
    ///   - Examples: "linux", "linux-zen", "linux-zen-goatd-gaming", "linux-goatd-workstation"
    /// * `kernel_release` - Primary kernel version (internal "Source of Truth")
    /// * `filename_version` - Secondary version from package filename (fallback)
    /// * `suffix` - "-headers" or "-docs"
    ///
    /// # Hyper-Heuristic Permutation Strategy with Dynamic Identity Support
    /// Tries multiple naming permutations in order of likelihood, automatically handling
    /// the dynamic `linux-{variant}-goatd-{profile}` Master Identity scheme by pivoting on `-goatd-`:
    /// 1. **Primary pattern**: {kernel_variant}{suffix}-{kernel_release}
    ///    - Handles: "linux-zen-goatd-gaming-headers-6.18.0"
    /// 2. **Fallback filename pattern**: {kernel_variant}{suffix}-{filename_version}
    /// 3. **Alternate prefix**: linux-{suffix}-{variant_core}-{kernel_release}
    /// 4. **Alternate prefix (filename)**: linux-{suffix}-{variant_core}-{filename_version}
    /// 5. **GOATd-aware alternate prefix**: Pivots on `-goatd-` for flexible matching
    /// 6. **Fuzzy component match**: Contains variant AND suffix AND version_core
    /// 7. **Final fallback**: Any {kernel_variant}{suffix}- with ANY version
    ///
    /// # Returns
    /// Path to the artifact if found, or None if not found
    fn find_related_artifact(
        parent_dir: &std::path::Path,
        kernel_variant: &str,
        kernel_release: &str,
        filename_version: Option<&str>,
        suffix: &str,
    ) -> Result<Option<PathBuf>, String> {
        let entries = std::fs::read_dir(parent_dir)
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        let entries_vec: Vec<_> = entries.flatten().collect();

        // PHASE 20: Dynamic Identity Builder Support with GOATd Pivot
        // Extract core identity from kernel_variant with GOATd branding awareness
        // E.g., "linux-zen-goatd-gaming" -> "zen-goatd-gaming"
        let variant_core = if kernel_variant.starts_with("linux-") {
            &kernel_variant[6..] // Skip "linux-"
        } else {
            kernel_variant
        };

        // NEW: Pivot on -goatd- to extract base variant and profile
        // E.g., "zen-goatd-gaming" -> base="zen", profile=Some("gaming")
        let (base_variant, profile) = if let Some(goatd_pos) = variant_core.find("-goatd-") {
            let base = &variant_core[..goatd_pos];
            let profile_and_rest = &variant_core[goatd_pos + 7..]; // +7 to skip "-goatd-"
            (base, Some(profile_and_rest))
        } else {
            (variant_core, None)
        };

        // Extract version core (base version without release number)
        // E.g., "6.19.0-rc6-1" -> "6.19" or "6.19rc6"
        let version_core = Self::extract_version_core(kernel_release);

        // Build list of permutations to try
        let mut permutations: Vec<(String, &str)> = vec![];

        // Permutation 1: STANDARDIZED IDENTITY STRATEGY - {pkgbase}-{suffix}-{pkgver}-{pkgrel}
        // This matches the specification format: /usr/src/${pkgbase}-${pkgver}-${pkgrel}
        // where headers are installed as {pkgbase}-headers-{pkgver}-{pkgrel}
        // Extract pkgver and pkgrel from kernel_release (split at last hyphen)
        if let Some(last_hyphen_pos) = kernel_release.rfind('-') {
            let pkgver_with_rel = &kernel_release[..last_hyphen_pos];
            let pkgrel = &kernel_release[last_hyphen_pos + 1..];
            // Sanitize pkgver by replacing hyphens with dots (Arch Linux convention)
            let pkgver = pkgver_with_rel.replace('-', ".");
            permutations.push((
                format!("{}{}-{}-{}", kernel_variant, suffix, pkgver, pkgrel),
                "standardized identity (sanitized pkgver)",
            ));
            // Also try without sanitization in case it's already sanitized
            permutations.push((
                format!(
                    "{}{}-{}-{}",
                    kernel_variant, suffix, pkgver_with_rel, pkgrel
                ),
                "standardized identity (unsanitized pkgver)",
            ));
        }

        // Permutation 1.5: Primary pattern: {variant}{suffix}-{kernel_release}
        permutations.push((
            format!("{}{}-{}", kernel_variant, suffix, kernel_release),
            "primary version match",
        ));

        // Permutation 2: Fallback filename pattern: {variant}{suffix}-{filename_version}
        if let Some(fb_ver) = filename_version {
            permutations.push((
                format!("{}{}-{}", kernel_variant, suffix, fb_ver),
                "filename version fallback",
            ));
        }

        // Permutation 3: Alternate prefix: linux-{suffix}-{variant_core}-{kernel_release}
        if variant_core != kernel_variant && !variant_core.is_empty() {
            permutations.push((
                format!("linux{}-{}-{}", suffix, variant_core, kernel_release),
                "alternate prefix (primary version)",
            ));
        }

        // Permutation 4: Alternate prefix with filename version
        if let Some(fb_ver) = filename_version {
            if variant_core != kernel_variant && !variant_core.is_empty() {
                permutations.push((
                    format!("linux{}-{}-{}", suffix, variant_core, fb_ver),
                    "alternate prefix (filename version)",
                ));
            }
        }

        // Permutation 4b: GOATd-aware alternate prefix (PHASE 20)
        // For variants like "linux-zen-goatd-gaming", also try "linux-headers-zen-goatd-gaming-"
        if let Some(prof) = profile {
            if !base_variant.is_empty() {
                permutations.push((
                    format!(
                        "linux{}-{}-goatd-{}-{}",
                        suffix, base_variant, prof, kernel_release
                    ),
                    "GOATd-aware alternate (primary)",
                ));
                if let Some(fb_ver) = filename_version {
                    permutations.push((
                        format!("linux{}-{}-goatd-{}-{}", suffix, base_variant, prof, fb_ver),
                        "GOATd-aware alternate (filename)",
                    ));
                }
            }
        }

        // Try exact permutation matches
        for (pattern, strategy_name) in &permutations {
            for entry in &entries_vec {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(filename) = entry.file_name().to_str() {
                            if filename.starts_with(pattern) && filename.ends_with(".pkg.tar.zst") {
                                log_info!(
                                    "[KernelArtifactRegistry] ✓ Found {} via {}: {}",
                                    suffix,
                                    strategy_name,
                                    filename
                                );
                                // CRITICAL (Chunk 1): CANONICALIZE before returning
                                // Ensures absolute path for privileged execution context
                                let artifact_path = match entry.path().canonicalize() {
                                    Ok(abs_path) => {
                                        log_info!(
                                            "[KernelArtifactRegistry] [CANONICALIZE] Resolved artifact path to absolute: {}",
                                            abs_path.display()
                                        );
                                        abs_path
                                    }
                                    Err(e) => {
                                        return Err(format!(
                                            "Failed to canonicalize artifact path {}: {}",
                                            entry.path().display(),
                                            e
                                        ));
                                    }
                                };
                                return Ok(Some(artifact_path));
                            }
                        }
                    }
                }
            }
        }

        // Permutation 5: Fuzzy component match with GOATd pivot (PHASE 20)
        // Any file containing variant AND suffix AND version core
        // If GOATd profile present, check for base, goatd, AND profile
        if !variant_core.is_empty() && !version_core.is_empty() {
            for entry in &entries_vec {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(filename) = entry.file_name().to_str() {
                            let has_suffix = filename.contains(suffix);
                            let has_version = filename.contains(&version_core);

                            // Check variant components
                            let has_variant = if let Some(prof) = profile {
                                // GOATd variant: check for base, goatd, AND profile
                                filename.contains(base_variant)
                                    && filename.contains("-goatd-")
                                    && filename.contains(prof)
                            } else {
                                // Non-GOATd variant: check for variant_core
                                filename.contains(variant_core)
                            };

                            if has_suffix
                                && has_variant
                                && has_version
                                && filename.ends_with(".pkg.tar.zst")
                            {
                                log_info!("[KernelArtifactRegistry] ✓ Found {} via fuzzy component match (GOATd-aware): {}",
                                    suffix, filename);
                                // CRITICAL (Chunk 1): CANONICALIZE before returning
                                let artifact_path = match entry.path().canonicalize() {
                                    Ok(abs_path) => {
                                        log_info!(
                                            "[KernelArtifactRegistry] [CANONICALIZE] Resolved artifact path to absolute: {}",
                                            abs_path.display()
                                        );
                                        abs_path
                                    }
                                    Err(e) => {
                                        return Err(format!(
                                            "Failed to canonicalize artifact path {}: {}",
                                            entry.path().display(),
                                            e
                                        ));
                                    }
                                };
                                return Ok(Some(artifact_path));
                            }
                        }
                    }
                }
            }
        }

        // Permutation 6: Final fallback - Fuzzy prefix match
        // Accept ANY version as long as variant prefix matches
        for entry in &entries_vec {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    if let Some(filename) = entry.file_name().to_str() {
                        let prefix = format!("{}{}-", kernel_variant, suffix);
                        if filename.starts_with(&prefix) && filename.ends_with(".pkg.tar.zst") {
                            log_info!("[KernelArtifactRegistry] ⚠ Found {} via fuzzy prefix match (version may differ): {}",
                                suffix, filename);
                            // CRITICAL (Chunk 1): CANONICALIZE before returning
                            let artifact_path = match entry.path().canonicalize() {
                                Ok(abs_path) => {
                                    log_info!(
                                        "[KernelArtifactRegistry] [CANONICALIZE] Resolved artifact path to absolute: {}",
                                        abs_path.display()
                                    );
                                    abs_path
                                }
                                Err(e) => {
                                    return Err(format!(
                                        "Failed to canonicalize artifact path {}: {}",
                                        entry.path().display(),
                                        e
                                    ));
                                }
                            };
                            return Ok(Some(artifact_path));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Extract core version from kernel release string (Chunk 2)
    ///
    /// # Examples
    /// - "6.19.0-rc6-arch1-1" -> "6.19"
    /// - "6.19rc6-1" -> "6.19"
    /// - "6.19.3" -> "6.19"
    fn extract_version_core(kernel_release: &str) -> String {
        // Split by '-' and take the first segment
        let first_segment = kernel_release.split('-').next().unwrap_or("");

        // For dotted versions like "6.19.0", take first two components
        let parts: Vec<&str> = first_segment.split('.').collect();
        if parts.len() >= 2 {
            format!("{}.{}", parts[0], parts[1])
        } else if !first_segment.is_empty() {
            first_segment.to_string()
        } else {
            kernel_release.to_string()
        }
    }

    /// Extract version string from kernel package filename
    ///
    /// # Example
    /// `linux-goatd-gaming-6.19rc6-1-x86_64.pkg.tar.zst` -> `Some("6.19rc6-1")`
    ///
    /// # Strategy
    /// Finds the position where the version starts (first dash followed by a digit)
    /// and extracts everything up to the architecture suffix (-x86_64, -i686, etc.)
    fn extract_version_from_filename(kernel_path: &std::path::Path) -> Option<String> {
        if let Some(filename) = kernel_path.file_name() {
            if let Some(filename_str) = filename.to_str() {
                // Remove .pkg.tar.zst suffix
                let base = filename_str.strip_suffix(".pkg.tar.zst")?;

                // Must start with "linux-"
                if !base.starts_with("linux-") {
                    return None;
                }

                let remainder = &base[6..]; // Skip "linux-"

                // Find where version starts: first dash followed by a digit
                let mut version_start_pos = None;
                for (i, ch) in remainder.char_indices() {
                    if ch == '-' {
                        if let Some(next_ch) = remainder[i + 1..].chars().next() {
                            if next_ch.is_ascii_digit() {
                                version_start_pos = Some(i);
                                break;
                            }
                        }
                    }
                }

                if let Some(pos) = version_start_pos {
                    let version_with_arch = &remainder[pos + 1..];

                    // Remove architecture suffix (-x86_64, -i686, -aarch64, -armv7h, -riscv64)
                    let version = version_with_arch
                        .strip_suffix("-x86_64")
                        .or_else(|| version_with_arch.strip_suffix("-i686"))
                        .or_else(|| version_with_arch.strip_suffix("-aarch64"))
                        .or_else(|| version_with_arch.strip_suffix("-armv7h"))
                        .or_else(|| version_with_arch.strip_suffix("-riscv64"))
                        .unwrap_or(version_with_arch);

                    if !version.is_empty() {
                        log_info!(
                            "[KernelArtifactRegistry] Extracted filename version: {} from {}",
                            version,
                            filename_str
                        );
                        return Some(version.to_string());
                    }
                }
            }
        }
        None
    }

    /// Collect all artifact paths in order for pacman installation
    ///
    /// Returns a Vec of PathBuf entries in the order they should be installed:
    /// 1. Main kernel package
    /// 2. Headers (if found and existing)
    /// 3. Docs (if found and existing)
    ///
    /// # Important
    /// Paths are returned as stored during initialization. Callers must handle path
    /// canonicalization (see src/ui/controller.rs for examples using PathBuf::canonicalize()).
    /// This method performs existence checks on all collected paths before returning them.
    /// If a path doesn't exist, it's skipped with a warning log.
    pub fn collect_all_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![];

        // Always include main kernel package (existence was verified in new())
        if self.kernel_path.exists() {
            paths.push(self.kernel_path.clone());
        } else {
            log_info!(
                "[KernelArtifactRegistry] [WARNING] Main kernel package does not exist: {}",
                self.kernel_path.display()
            );
        }

        // Add headers if found and existing
        if let Some(ref headers) = self.headers_path {
            if headers.exists() {
                paths.push(headers.clone());
            } else {
                log_info!("[KernelArtifactRegistry] [WARNING] Headers package path references non-existent file: {}", headers.display());
            }
        }

        // Add docs if found and existing
        if let Some(ref docs) = self.docs_path {
            if docs.exists() {
                paths.push(docs.clone());
            } else {
                log_info!("[KernelArtifactRegistry] [WARNING] Docs package path references non-existent file: {}", docs.display());
            }
        }

        paths
    }

    /// Get a summary of the registry for logging
    pub fn summary(&self) -> String {
        let mut items = vec![self
            .kernel_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()];

        if let Some(ref headers) = self.headers_path {
            if let Some(name) = headers.file_name().and_then(|n| n.to_str()) {
                items.push(name.to_string());
            }
        }

        if let Some(ref docs) = self.docs_path {
            if let Some(name) = docs.file_name().and_then(|n| n.to_str()) {
                items.push(name.to_string());
            }
        }

        format!("[{}]", items.join(", "))
    }
}

use crate::ui::KernelManagerTrait;

/// Default implementation of KernelManagerTrait
pub struct KernelManagerImpl;

impl KernelManagerImpl {
    pub fn new() -> Result<Self, String> {
        Ok(KernelManagerImpl)
    }
}

impl KernelManagerTrait for KernelManagerImpl {
    fn list_installed(&self) -> Vec<KernelPackage> {
        list_installed_kernels()
    }

    fn scan_workspace(&self, path: &str) -> Vec<KernelPackage> {
        scan_workspace_kernels_impl(path)
    }

    fn delete_built_artifact(&self, pkg: &KernelPackage) -> Result<(), String> {
        if let Some(path) = &pkg.path {
            // Get the parent directory where the kernel package is located
            let parent_dir = match path.parent() {
                Some(p) => p,
                None => {
                    return Err("Cannot determine parent directory for kernel package".to_string())
                }
            };

            // Collect all matching files (main, headers, docs) before deletion
            let matching_files =
                collect_matching_kernel_files(parent_dir, &pkg.name, &pkg.version)?;

            if matching_files.is_empty() {
                return Err(format!(
                    "No kernel packages found for {} ({})",
                    pkg.name, pkg.version
                ));
            }

            let mut deleted_count = 0;
            let mut main_deleted = false;
            let mut errors = vec![];

            // Delete each matching file
            for file_path in matching_files {
                let is_main = file_path == *path;
                match std::fs::remove_file(&file_path) {
                    Ok(()) => {
                        // Verify the file is actually gone
                        if file_path.exists() {
                            let err_msg = format!(
                                "File deletion reported success but file still exists: {}",
                                file_path.display()
                            );
                            log_info!("[KernelManager] [ERROR] {}", err_msg);
                            errors.push(err_msg);
                        } else {
                            deleted_count += 1;
                            if is_main {
                                main_deleted = true;
                            }
                            log_info!(
                                "[KernelManager] [SUCCESS] Deleted {} kernel artifact: {}",
                                if is_main { "main" } else { "related" },
                                file_path
                                    .file_name()
                                    .and_then(|f| f.to_str())
                                    .unwrap_or("unknown")
                            );
                        }
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Failed to delete kernel artifact {}: {}",
                            file_path.display(),
                            e
                        );
                        log_info!("[KernelManager] [ERROR] {}", err_msg);
                        errors.push(err_msg);
                    }
                }
            }

            // Success if we deleted the main file
            if main_deleted {
                log_info!(
                    "[KernelManager] [SUMMARY] Kernel deletion complete: deleted {} total artifacts",
                    deleted_count
                );
                Ok(())
            } else if !errors.is_empty() {
                // Main file failed to delete
                Err(format!(
                    "Failed to delete main kernel package: {}",
                    errors.join("; ")
                ))
            } else {
                Err(
                    "Main kernel package was not among the matching files (unexpected state)"
                        .to_string(),
                )
            }
        } else {
            Err("Kernel package has no associated path".to_string())
        }
    }
}

/// List installed kernel packages using pacman
/// Returns a Vec of kernel package strings formatted as "linux (6.18.3)" or "linux (6.18.3-GOATd-Gamer)"
/// Preserves GOATd branding and profile identifiers when present
/// Flexible filtering: accepts any package starting with "linux-" that isn't a known non-kernel type
/// This automatically includes: linux, linux-zen, linux-lts, linux-hardened, linux-mainline, linux-goatd-*, etc.
/// Excludes: firmware, headers, docs, api-headers packages
fn list_installed_kernels() -> Vec<KernelPackage> {
    use std::process::Command;

    let output = match Command::new("pacman").args(&["-Q"]).output() {
        Ok(out) => out,
        Err(e) => {
            log_info!("[KernelManager] Failed to run pacman -Q: {}", e);
            return vec![];
        }
    };

    if !output.status.success() {
        log_info!("[KernelManager] pacman -Q failed");
        return vec![];
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut kernels = vec![];

    for line in output_str.lines() {
        // pacman -Q output format: "package_name version"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let pkg_name = parts[0];
            let version = parts[1];

            // Exclude firmware packages by EXACT match and PREFIX match
            // Catches: linux-firmware (base) AND linux-firmware-* (variants)
            if pkg_name == "linux-firmware" || pkg_name.starts_with("linux-firmware-") {
                continue;
            }

            // Filter out other non-kernel packages using known suffixes
            // These are packages that are related to kernels but not kernels themselves
            let excluded_suffixes = ["-docs", "-headers", "-api-headers"];
            let is_excluded = excluded_suffixes
                .iter()
                .any(|suffix| pkg_name.ends_with(suffix));

            if is_excluded {
                continue;
            }

            // Flexible filter: accept any package in the "linux-*" namespace
            // This includes: linux, linux-zen, linux-lts, linux-hardened, linux-mainline,
            // and any new variants like linux-goatd-gaming, linux-goatd-server, etc.
            let is_linux_kernel = pkg_name == "linux" || pkg_name.starts_with("linux-");

            if is_linux_kernel {
                // Preserve GOATd branding and profiles in the version string
                let version_lower = version.to_lowercase();
                let has_goatd = version_lower.contains("goatd") || pkg_name.contains("goatd");

                kernels.push(KernelPackage {
                    name: pkg_name.to_string(),
                    version: version.to_string(),
                    is_goatd: has_goatd,
                    path: None,
                });

                // Log at debug level to keep terminal clean during scanning
                #[cfg(debug_assertions)]
                log_info!(
                    "[KernelManager] [DEBUG] Found kernel: {} version {} (GOATd: {})",
                    pkg_name,
                    version,
                    has_goatd
                );
            }
        }
    }

    // Only log summary at info level if this is a fresh scan (not every frame)
    if !kernels.is_empty() {
        log_info!(
            "[KernelManager] Kernel scan complete: {} installed kernels found",
            kernels.len()
        );
    }
    kernels
}

/// Scan workspace for built kernel packages (.pkg.tar.zst files)
/// Recursively searches all subdirectories to unlimited depth.
/// Parses filenames to extract clean name and version, filtering out headers/docs
/// Format: "linux-6.18.3-arch1-1-x86_64.pkg.tar.zst" -> "linux (6.18.3-arch1-1)"
/// Gracefully skips directories where permission is denied
fn scan_workspace_kernels_impl(workspace_path: &str) -> Vec<KernelPackage> {
    let workspace_to_scan = if workspace_path.is_empty() {
        "."
    } else {
        workspace_path
    };

    let path = std::path::Path::new(workspace_to_scan);

    if !path.exists() {
        log_info!(
            "[KernelManager] Workspace path does not exist: {}",
            workspace_path
        );
        return vec![];
    }

    let mut kernels = vec![];
    scan_directory_recursive(&path, &mut kernels);

    log_info!(
        "[KernelManager] Found {} built kernels in workspace",
        kernels.len()
    );
    kernels
}

/// Recursively scan a directory for kernel packages
/// Handles deeply nested structures and gracefully skips permission-denied directories
fn scan_directory_recursive(dir: &std::path::Path, kernels: &mut Vec<KernelPackage>) {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                // Skip entries where we can't get metadata
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };

                // Check if it's a file
                if metadata.is_file() {
                    if let Some(filename) = entry.file_name().to_str() {
                        if let Some(pkg) = parse_kernel_package_to_struct(filename, entry.path()) {
                            kernels.push(pkg);
                        }
                    }
                }
                // Recursively descent into subdirectories
                else if metadata.is_dir() {
                    scan_directory_recursive(&entry.path(), kernels);
                }
            }
        }
        Err(e) => {
            // Skip permission denied errors silently, log other errors
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    // Silently skip permission denied
                }
                _ => {
                    log_info!("[KernelManager] Error reading directory: {}", e);
                }
            }
        }
    }
}

/// Parse kernel package filename to extract clean name and version
/// Preserves GOATd branding and profile identifiers when present
/// Filters out headers, docs, firmware packages
fn parse_kernel_package_to_struct(filename: &str, full_path: PathBuf) -> Option<KernelPackage> {
    // Must be a package archive
    if !filename.ends_with(".pkg.tar.zst") {
        return None;
    }

    // Remove the .pkg.tar.zst suffix
    let base = filename.strip_suffix(".pkg.tar.zst")?;

    // Remove the architecture suffix (e.g., -x86_64)
    let parts: Vec<&str> = base.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    let last_part = parts[parts.len() - 1];
    let without_arch = if ["x86_64", "i686", "aarch64", "armv7h", "riscv64"].contains(&last_part) {
        parts[..parts.len() - 1].join("-")
    } else {
        base.to_string()
    };

    // Filter out non-kernel packages first
    // Exclude firmware packages by EXACT match and PREFIX match
    // Catches: linux-firmware (base) AND linux-firmware-* (variants)
    if without_arch == "linux-firmware" || without_arch.starts_with("linux-firmware-") {
        return None;
    }

    // Check for other common non-kernel suffixes and substrings
    if without_arch.contains("-headers")
        || without_arch.contains("-docs")
        || without_arch.contains("-api-headers")
    {
        return None;
    }

    // Match kernel variants: linux, linux-zen, linux-lts, linux-hardened, linux-mainline
    // Handle both GOATd naming schemes (highest priority):
    // 1. Stable: linux-goatd-{profile}-{version}
    // 2. Variant-based: linux-{variant}-goatd-{profile}-{version}
    // Then standard variants (lower priority)
    let kernel_name = if without_arch.contains("-goatd-") {
        // DYNAMIC PROFILE NAMING SCHEME: linux-{variant}-goatd-{profile}-{version}
        // Also handles: linux-goatd-{profile}-{version} (stable variant, any profile after -goatd-)
        // Find the first segment that starts with a digit (version boundary)

        let parts_vec: Vec<&str> = without_arch.split('-').collect();

        // Find where the version starts (first segment with leading digit)
        let mut version_start_idx = parts_vec.len();
        for (idx, part) in parts_vec.iter().enumerate() {
            if part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                version_start_idx = idx;
                break;
            }
        }

        // Everything before version_start_idx is the kernel name
        // e.g., "linux-mainline-goatd-gaming" or "linux-goatd-gaming"
        if version_start_idx > 0 {
            parts_vec[..version_start_idx].join("-")
        } else {
            without_arch.to_string()
        }
    } else if without_arch.starts_with("linux-zen-") {
        "linux-zen".to_string()
    } else if without_arch.starts_with("linux-lts-") {
        "linux-lts".to_string()
    } else if without_arch.starts_with("linux-hardened-") {
        "linux-hardened".to_string()
    } else if without_arch.starts_with("linux-mainline-") {
        "linux-mainline".to_string()
    } else if without_arch.starts_with("linux-") {
        "linux".to_string()
    } else {
        return None;
    };

    // Extract version: everything after the kernel name
    let version_start = kernel_name.len() + 1; // +1 for the '-' separator
    if version_start >= without_arch.len() {
        return None;
    }

    let version = &without_arch[version_start..];

    // Check if this kernel contains GOATd branding
    let version_lower = version.to_lowercase();
    let has_goatd = version_lower.contains("goatd");

    Some(KernelPackage {
        name: kernel_name.to_string(),
        version: version.to_string(),
        is_goatd: has_goatd,
        path: Some(full_path),
    })
}

/// Helper to collect all matching kernel package files in a directory
/// This includes the main kernel package plus any headers and docs packages
///
/// Example: If the main package is "linux-goatd-gaming-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// This will also find:
/// - "linux-goatd-gaming-headers-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - "linux-goatd-gaming-docs-6.18.3-arch1-1-x86_64.pkg.tar.zst"
fn collect_matching_kernel_files(
    dir: &std::path::Path,
    kernel_name: &str,
    version: &str,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut matching_files = vec![];

    // Read the directory
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                if let Some(filename) = entry.file_name().to_str() {
                    // Check if this file matches our kernel pattern
                    if is_matching_related_kernel_package(filename, kernel_name, version) {
                        log_info!(
                            "[KernelManager] [MATCH] Collected kernel artifact: {}",
                            filename
                        );
                        matching_files.push(entry.path());
                    }
                }
            }
        }
    }

    Ok(matching_files)
}

/// Helper to check if a filename matches the kernel package or its related artifacts (headers, docs)
/// Handles both standard and modular naming with dynamic `linux-{variant}-goatd-{profile}` support.
///
/// PHASE 20: Dynamic Identity Support
/// - Standard: "linux-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - GOATd variants: "linux-zen-goatd-gaming-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - GOATd simple: "linux-goatd-workstation-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - Also matches headers and docs variants
///
/// Uses regex-based matching with (-goatd- pivot awareness) to ensure version boundaries
/// are respected and prevent false matches from partial version/variant strings.
fn is_matching_related_kernel_package(filename: &str, kernel_variant: &str, version: &str) -> bool {
    // Must be a package archive
    if !filename.ends_with(".pkg.tar.zst") {
        return false;
    }

    // Remove the .pkg.tar.zst suffix and architecture suffix to normalize
    let base = match filename.strip_suffix(".pkg.tar.zst") {
        Some(b) => b,
        None => return false,
    };

    // Remove architecture suffix (e.g., -x86_64, -i686, -aarch64, -armv7h, -riscv64)
    let parts: Vec<&str> = base.split('-').collect();
    if parts.is_empty() {
        return false;
    }

    let last_part = parts[parts.len() - 1];
    let without_arch = if ["x86_64", "i686", "aarch64", "armv7h", "riscv64"].contains(&last_part) {
        parts[..parts.len() - 1].join("-")
    } else {
        base.to_string()
    };

    // PHASE 20: Extract base variant and profile from kernel_variant if GOATd-branded
    let (base_variant, profile) = if let Some(goatd_pos) = kernel_variant.find("-goatd-") {
        let base = &kernel_variant[..goatd_pos];
        let prof = &kernel_variant[goatd_pos + 7..]; // +7 to skip "-goatd-"
        (base, Some(prof))
    } else {
        (kernel_variant, None)
    };

    // Use regex-based matching to prevent partial over-matching
    // Escape special regex characters in kernel_variant and version
    let variant_escaped = regex::escape(kernel_variant);
    let version_escaped = regex::escape(version);

    // Build patterns with word boundaries to prevent partial matches
    // Pattern: {variant}(-headers|-docs)?-{version}(-\d+)?
    let patterns = [
        format!(r"^{}-{}(?:-\d+)?$", variant_escaped, version_escaped), // Main pattern
        format!(
            r"^{}-headers-{}(?:-\d+)?$",
            variant_escaped, version_escaped
        ), // Headers pattern
        format!(r"^{}-docs-{}(?:-\d+)?$", variant_escaped, version_escaped), // Docs pattern
    ];

    for pattern_str in &patterns {
        if let Ok(regex) = regex::Regex::new(pattern_str) {
            if regex.is_match(&without_arch) {
                return true;
            }
        }
    }

    // PHASE 20: GOATd-aware fallback patterns
    // For GOATd-branded variants, try alternate permutations
    if let Some(prof) = profile {
        let base_escaped = regex::escape(base_variant);
        let prof_escaped = regex::escape(prof);

        // Try "base-goatd-profile" pattern variations with headers/docs
        let goatd_patterns = [
            format!(
                r"^{}-goatd-{}-{}(?:-\d+)?$",
                base_escaped, prof_escaped, version_escaped
            ),
            format!(
                r"^{}-goatd-{}-headers-{}(?:-\d+)?$",
                base_escaped, prof_escaped, version_escaped
            ),
            format!(
                r"^{}-goatd-{}-docs-{}(?:-\d+)?$",
                base_escaped, prof_escaped, version_escaped
            ),
        ];

        for pattern_str in &goatd_patterns {
            if let Ok(regex) = regex::Regex::new(pattern_str) {
                if regex.is_match(&without_arch) {
                    return true;
                }
            }
        }
    }

    false
}

/// Recursively search for a kernel package file and delete it
/// Uses unified recursion instead of nested directory scans
fn delete_kernel_recursive(dir: &std::path::Path, kernel_variant: &str, version: &str) -> bool {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(filename) = entry.file_name().to_str() {
                            if is_matching_kernel_package(filename, kernel_variant, version) {
                                log_info!(
                                    "[KernelManager] [CANDIDATE] Found matching file: {}",
                                    filename
                                );
                                if let Ok(()) = std::fs::remove_file(entry.path()) {
                                    if !entry.path().exists() {
                                        log_info!("[KernelManager] [SUCCESS] Kernel file deleted and verified gone: {}", filename);
                                        return true;
                                    } else {
                                        log_info!("[KernelManager] [WARNING] File deletion reported success but file still exists: {}", filename);
                                    }
                                } else {
                                    log_info!(
                                        "[KernelManager] [ERROR] Failed to delete kernel: {}",
                                        filename
                                    );
                                }
                            }
                        }
                    } else if metadata.is_dir() {
                        // Recursively search subdirectories
                        if delete_kernel_recursive(&entry.path(), kernel_variant, version) {
                            return true;
                        }
                    }
                }
            }
        }
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    // Silently skip permission denied
                }
                _ => {
                    log_info!("[KernelManager] Error reading directory: {}", e);
                }
            }
        }
    }
    false
}

/// Helper to check if a filename matches the kernel package we want to delete
/// Strict criteria: must match variant and version, must NOT be headers/docs/firmware
fn is_matching_kernel_package(filename: &str, kernel_variant: &str, version: &str) -> bool {
    // Must be a package archive
    if !filename.ends_with(".pkg.tar.zst") {
        return false;
    }

    // Must NOT be a firmware package (use EXACT and PREFIX match for robustness)
    // Catches: linux-firmware (base) AND linux-firmware-* (variants)
    if filename == "linux-firmware" || filename.starts_with("linux-firmware") {
        log_info!(
            "[KernelManager] [FILTERED] Excluding firmware package: {}",
            filename
        );
        return false;
    }

    // Must NOT contain other excluded package types
    let excluded = ["-headers", "-docs", "-api-headers"];
    for exclude in &excluded {
        if filename.contains(exclude) {
            log_info!(
                "[KernelManager] [FILTERED] Excluding non-kernel package: {} (contains '{}')",
                filename,
                exclude
            );
            return false;
        }
    }

    // Must start with the kernel variant (e.g., "linux-" or "linux-zen-")
    let expected_prefix = format!("{}-", kernel_variant);
    if !filename.starts_with(&expected_prefix) {
        return false;
    }

    // Must contain the version string
    if !filename.contains(version) {
        return false;
    }

    log_info!(
        "[KernelManager] [MATCH] Package '{}' matches variant='{}', version='{}'",
        filename,
        kernel_variant,
        version
    );
    true
}

/// Safely delete a built kernel file from the workspace with strict matching
/// Returns true if successful, false otherwise
pub fn delete_built_kernel(workspace_path: &str, kernel_display_name: &str) -> bool {
    if workspace_path.is_empty() {
        log_info!("[KernelManager] Cannot delete kernel: workspace path is empty");
        return false;
    }

    // kernel_display_name is formatted as "linux (6.18.3-arch1-1)"
    // Extract the kernel variant and version: "linux" and "6.18.3-arch1-1"
    let parts: Vec<&str> = kernel_display_name.split(' ').collect();
    if parts.len() < 2 {
        log_info!(
            "[KernelManager] Invalid kernel display name format: {}",
            kernel_display_name
        );
        return false;
    }

    let kernel_variant = parts[0]; // "linux", "linux-zen", etc.
    let version_part = parts[1].trim_matches(|c| c == '(' || c == ')'); // "6.18.3-arch1-1"

    log_info!(
        "[KernelManager] Searching for kernel: variant='{}', version='{}'",
        kernel_variant,
        version_part
    );

    let path = PathBuf::from(workspace_path);

    if !path.exists() {
        log_info!(
            "[KernelManager] Workspace path does not exist: {}",
            workspace_path
        );
        return false;
    }

    // Use unified recursive search to find and delete the kernel file
    delete_kernel_recursive(&path, kernel_variant, version_part)
}

/// Helper to find a kernel package file in the workspace
/// Searches recursively for .pkg.tar.zst files matching the kernel variant and version
/// Returns the absolute path to the matching file, or None if not found
pub fn find_kernel_package_file(
    workspace_path: &str,
    kernel_variant: &str,
    version: &str,
) -> Option<PathBuf> {
    if workspace_path.is_empty() {
        log_info!("[KernelManager] Cannot find package: workspace path is empty");
        return None;
    }

    let path = PathBuf::from(workspace_path);

    if !path.exists() {
        log_info!(
            "[KernelManager] Workspace path does not exist: {}",
            workspace_path
        );
        return None;
    }

    // Use unified recursive search
    find_kernel_package_recursive(&path, kernel_variant, version)
}

/// Recursively search for a kernel package file
/// Uses unified recursion instead of nested directory scans
fn find_kernel_package_recursive(
    dir: &std::path::Path,
    kernel_variant: &str,
    version: &str,
) -> Option<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    if metadata.is_file() {
                        if let Some(filename) = entry.file_name().to_str() {
                            if is_matching_kernel_package(filename, kernel_variant, version) {
                                log_info!(
                                    "[KernelManager] [MATCH] Found kernel package: {}",
                                    filename
                                );
                                return Some(entry.path());
                            }
                        }
                    } else if metadata.is_dir() {
                        // Recursively search subdirectories
                        if let Some(found) =
                            find_kernel_package_recursive(&entry.path(), kernel_variant, version)
                        {
                            return Some(found);
                        }
                    }
                }
            }
        }
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    // Silently skip permission denied
                }
                _ => {
                    log_info!("[KernelManager] Error reading directory: {}", e);
                }
            }
        }
    }

    log_info!(
        "[KernelManager] No matching kernel package found for variant='{}', version='{}'",
        kernel_variant,
        version
    );
    None
}

/// DKMS Diagnostic Error Patterns
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DkmsErrorPattern {
    MissingKernelHeaders,
    CompilerMismatch,
    MissingPageFree,
    SymbolNotFound,
    BuildFailure,
}

impl std::fmt::Display for DkmsErrorPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DkmsErrorPattern::MissingKernelHeaders => write!(f, "Missing kernel headers"),
            DkmsErrorPattern::CompilerMismatch => write!(f, "Compiler version mismatch"),
            DkmsErrorPattern::MissingPageFree => write!(f, "Missing page_free member/function"),
            DkmsErrorPattern::SymbolNotFound => write!(f, "Symbol not found"),
            DkmsErrorPattern::BuildFailure => write!(f, "Generic build failure"),
        }
    }
}

/// DKMS Diagnostic Result
#[derive(Clone, Debug)]
pub struct DkmsDiagnostic {
    /// Whether the build succeeded
    pub success: bool,
    /// Detected error patterns
    pub errors: Vec<DkmsErrorPattern>,
    /// Recommendations for resolving issues
    pub recommendations: Vec<String>,
    /// Raw log file path analyzed
    pub log_path: Option<String>,
    /// Last modified timestamp of the log
    pub log_timestamp: Option<String>,
}

impl DkmsDiagnostic {
    /// Create a successful diagnostic result
    pub fn success() -> Self {
        DkmsDiagnostic {
            success: true,
            errors: vec![],
            recommendations: vec![],
            log_path: None,
            log_timestamp: None,
        }
    }

    /// Create a failure diagnostic result with errors
    pub fn failure(errors: Vec<DkmsErrorPattern>, log_path: Option<String>) -> Self {
        let mut diag = DkmsDiagnostic {
            success: false,
            errors,
            recommendations: vec![],
            log_path,
            log_timestamp: None,
        };
        diag.generate_recommendations();
        diag
    }

    /// Generate recommendations based on detected errors
    fn generate_recommendations(&mut self) {
        for error in &self.errors {
            match error {
                DkmsErrorPattern::MissingKernelHeaders => {
                    self.recommendations.push(
                        "Install kernel headers matching your running kernel version".to_string(),
                    );
                    self.recommendations.push(
                        "Run: pacman -S linux-headers (adjust for your kernel variant)".to_string(),
                    );
                }
                DkmsErrorPattern::CompilerMismatch => {
                    self.recommendations.push(
                        "Ensure GCC version used for kernel build matches current GCC".to_string(),
                    );
                    self.recommendations
                        .push("Check /var/log/pacman.log for compiler upgrade history".to_string());
                }
                DkmsErrorPattern::MissingPageFree => {
                    self.recommendations.push(
                        "This out-of-tree module version is incompatible with your kernel"
                            .to_string(),
                    );
                    self.recommendations
                        .push("Update the driver or downgrade kernel version".to_string());
                }
                DkmsErrorPattern::SymbolNotFound => {
                    self.recommendations.push(
                        "Kernel symbol mismatch detected - likely ABI incompatibility".to_string(),
                    );
                    self.recommendations.push(
                        "Rebuild kernel with compatible configuration or update out-of-tree driver"
                            .to_string(),
                    );
                }
                DkmsErrorPattern::BuildFailure => {
                    self.recommendations.push(
                        "Check full build log at the reported path for detailed error".to_string(),
                    );
                    self.recommendations.push(
                        "Ensure all prerequisites are installed (build-essential, linux-headers)"
                            .to_string(),
                    );
                }
            }
        }
    }
}

/// Diagnose DKMS build failures by parsing build logs
///
/// This function locates the latest DKMS build log and parses it for specific
/// error patterns including:
/// - Missing kernel headers
/// - Compiler version mismatch
/// - Missing page_free member or function
/// - Symbol not found errors
/// - Generic build failures
///
/// # Returns
/// A structured `DkmsDiagnostic` result with detected errors and recommendations
pub fn diagnose_dkms_failure() -> DkmsDiagnostic {
    log_info!("[DKMS Diagnostic] Starting DKMS build failure diagnosis");

    // Try to find the latest DKMS build log
    let log_path = match find_latest_dkms_log() {
        Some(path) => {
            log_info!("[DKMS Diagnostic] Found DKMS log at: {}", path);
            path
        }
        None => {
            log_info!("[DKMS Diagnostic] No DKMS build log found");
            return DkmsDiagnostic::failure(vec![DkmsErrorPattern::BuildFailure], None);
        }
    };

    // Get timestamp of the log file
    let log_timestamp = get_file_timestamp(&log_path);

    // Read and parse the log file
    let log_content = match std::fs::read_to_string(&log_path) {
        Ok(content) => content,
        Err(e) => {
            log_info!("[DKMS Diagnostic] Failed to read log file: {}", e);
            return DkmsDiagnostic::failure(vec![DkmsErrorPattern::BuildFailure], Some(log_path));
        }
    };

    // Parse log for error patterns
    let mut detected_errors = vec![];

    // Check for missing kernel headers
    if check_missing_headers(&log_content) {
        detected_errors.push(DkmsErrorPattern::MissingKernelHeaders);
        log_info!("[DKMS Diagnostic] Detected: Missing kernel headers");
    }

    // Check for compiler mismatch
    if check_compiler_mismatch(&log_content) {
        detected_errors.push(DkmsErrorPattern::CompilerMismatch);
        log_info!("[DKMS Diagnostic] Detected: Compiler mismatch");
    }

    // Check for missing page_free member
    if check_missing_page_free(&log_content) {
        detected_errors.push(DkmsErrorPattern::MissingPageFree);
        log_info!("[DKMS Diagnostic] Detected: Missing page_free");
    }

    // Check for symbol not found errors
    if check_symbol_not_found(&log_content) {
        detected_errors.push(DkmsErrorPattern::SymbolNotFound);
        log_info!("[DKMS Diagnostic] Detected: Symbol not found");
    }

    // If no other errors but build failed
    if detected_errors.is_empty() && log_content.to_lowercase().contains("error") {
        detected_errors.push(DkmsErrorPattern::BuildFailure);
        log_info!("[DKMS Diagnostic] Detected: Generic build failure");
    }

    if detected_errors.is_empty() {
        log_info!("[DKMS Diagnostic] DKMS build appears successful");
        return DkmsDiagnostic::success();
    }

    let mut diag = DkmsDiagnostic::failure(detected_errors, Some(log_path));
    diag.log_timestamp = log_timestamp;
    diag
}

/// Find the latest DKMS build log
/// Searches `/var/lib/dkms/*/build/make.log` for vendor-agnostic builds
fn find_latest_dkms_log() -> Option<String> {
    use std::path::PathBuf;
    use std::process::Command;

    // Use find command to locate all out-of-tree module build logs
    let output = Command::new("find")
        .args(&["/var/lib/dkms", "-name", "make.log", "-type", "f"])
        .output()
        .ok()?;

    if !output.status.success() {
        log_info!("[DKMS Diagnostic] find command failed");
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut latest_log = None;
    let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

    for line in output_str.lines() {
        let log_path = PathBuf::from(line.trim());
        if !log_path.exists() {
            continue;
        }

        // Get modification time
        if let Ok(metadata) = std::fs::metadata(&log_path) {
            if let Ok(modified) = metadata.modified() {
                if modified > latest_time {
                    latest_time = modified;
                    latest_log = Some(line.trim().to_string());
                }
            }
        }
    }

    latest_log
}

/// Get formatted timestamp of a file
fn get_file_timestamp(path: &str) -> Option<String> {
    use chrono::{DateTime, Local};

    let metadata = std::fs::metadata(path).ok()?;
    if let Ok(modified) = metadata.modified() {
        let datetime: DateTime<Local> = modified.into();
        return Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string());
    }
    None
}

/// Check for missing kernel headers pattern
fn check_missing_headers(log_content: &str) -> bool {
    let patterns = [
        "fatal error: linux/version.h: No such file or directory",
        "fatal error: linux/kernel.h: No such file or directory",
        "Cannot find the kernel source files",
        "fatal error: generated/uapi/linux/version.h",
        "error: unable to locate the kernel source files",
    ];

    for pattern in &patterns {
        if log_content.to_lowercase().contains(&pattern.to_lowercase()) {
            return true;
        }
    }
    false
}

/// Check for compiler mismatch pattern
fn check_compiler_mismatch(log_content: &str) -> bool {
    let patterns = [
        "sorry, unimplemented",
        "compiler version mismatch",
        "incompatible compiler",
        "gcc version",
    ];

    let lower = log_content.to_lowercase();
    for pattern in &patterns {
        if lower.contains(pattern) {
            // Additional check: must also contain "error" to confirm it's a real error
            if lower.contains("error") {
                return true;
            }
        }
    }
    false
}

/// Check for missing page_free member pattern
fn check_missing_page_free(log_content: &str) -> bool {
    let patterns = [
        "error: 'page_free'",
        "error: 'struct page' has no member named 'page_free'",
        "has no member named 'page_free'",
        "undefined reference to `page_free'",
        "implicit declaration of function 'page_free'",
    ];

    for pattern in &patterns {
        if log_content.contains(pattern) {
            return true;
        }
    }
    false
}

/// Check for symbol not found pattern
fn check_symbol_not_found(log_content: &str) -> bool {
    let patterns = [
        "undefined reference to",
        "symbol is not present",
        "no symbol table",
    ];

    let lower = log_content.to_lowercase();
    let mut count = 0;
    for pattern in &patterns {
        if lower.contains(pattern) {
            count += 1;
        }
    }
    count > 0
}

/// Centralized Path Registry: Workspace Anchoring via `.goatd_anchor`
///
/// This module provides canonical path resolution anchored by a `.goatd_anchor` marker file
/// placed at the workspace root. This eliminates reliance on fragile `parent()` calls and
/// provides absolute, verifiable workspace boundaries.
///
/// The `.goatd_anchor` file serves as a sentinel that marks the canonical workspace root,
/// ensuring all path operations are resolved relative to this verified anchor point.

use std::path::{Path, PathBuf};

/// Centralized registry for workspace-absolute path resolution
///
/// The PathRegistry uses a `.goatd_anchor` marker file to establish an absolute,
/// canonical workspace root. This eliminates fragile `parent()` calls and centralizes
/// path resolution logic.
///
/// # Design Principles
/// 1. **Single Source of Truth**: All path resolution goes through this registry
/// 2. **Anchor-Based Verification**: The `.goatd_anchor` file marks workspace root
/// 3. **Absolute Paths**: All returned paths are canonical (no symlinks, no relative segments)
/// 4. **Early Validation**: Anchor existence is verified immediately on creation
#[derive(Clone, Debug)]
pub struct PathRegistry {
    /// Canonical workspace root (directory containing .goatd_anchor)
    workspace_root: PathBuf,

    /// Resolved kernel source directory
    kernel_source_dir: PathBuf,

    /// Path to the .goatd_anchor file used as the anchor
    anchor_path: PathBuf,
}

impl PathRegistry {
    /// Create a new PathRegistry, anchoring to the `.goatd_anchor` file
    ///
    /// This method searches for `.goatd_anchor` starting from the given path and walking
    /// upward through parent directories until found or the filesystem root is reached.
    ///
    /// # Arguments
    /// * `kernel_source_path` - Path to kernel source directory (may be nested within workspace)
    ///
    /// # Returns
    /// `Ok(PathRegistry)` if `.goatd_anchor` is found and workspace root is verified
    /// `Err(String)` if anchor cannot be found or paths cannot be resolved
    ///
    /// # Example
    /// ```ignore
    /// let registry = PathRegistry::new(PathBuf::from("/workspace/src"))?;
    /// // registry.workspace_root() -> /workspace (location of .goatd_anchor)
    /// // registry.kernel_source_dir() -> /workspace/src
    /// ```
    pub fn new(kernel_source_path: PathBuf) -> Result<Self, String> {
        // Canonicalize the kernel source path to eliminate symlinks
        let canonical_source = kernel_source_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize kernel source path: {}", e))?;

        // Search for .goatd_anchor starting from kernel source, walking up to root
        let workspace_root = Self::find_anchor(&canonical_source)?;

        // Verify anchor path exists
        let anchor_path = workspace_root.join(".goatd_anchor");
        if !anchor_path.exists() {
            return Err(format!(
                "Anchor verification failed: .goatd_anchor does not exist at {:?}",
                anchor_path
            ));
        }

        eprintln!(
            "[PathRegistry] ✓ Anchor found at: {}",
            anchor_path.display()
        );
        eprintln!(
            "[PathRegistry] ✓ Workspace root established: {}",
            workspace_root.display()
        );

        Ok(PathRegistry {
            workspace_root,
            kernel_source_dir: canonical_source,
            anchor_path,
        })
    }

    /// Find the workspace root by searching for `.goatd_anchor`
    ///
    /// Walks upward from the given path through parent directories until:
    /// - `.goatd_anchor` is found (returns workspace root containing it)
    /// - Filesystem root is reached (returns error)
    ///
    /// # Arguments
    /// * `start_path` - Path to start searching from
    ///
    /// # Returns
    /// `Ok(PathBuf)` containing the workspace root directory (parent of `.goatd_anchor`)
    /// `Err(String)` if anchor cannot be found before reaching filesystem root
    fn find_anchor(start_path: &Path) -> Result<PathBuf, String> {
        let mut current = start_path.to_path_buf();

        // Walk upward through parent directories
        loop {
            let anchor = current.join(".goatd_anchor");

            if anchor.exists() {
                eprintln!(
                    "[PathRegistry] Found .goatd_anchor at: {}",
                    anchor.display()
                );
                return Ok(current);
            }

            // Move to parent directory
            if let Some(parent) = current.parent() {
                // Protect against infinite loops at root
                if parent == current {
                    return Err(
                        "Anchor search reached filesystem root: .goatd_anchor not found".to_string()
                    );
                }
                current = parent.to_path_buf();
            } else {
                return Err(
                    "Anchor search reached filesystem root: .goatd_anchor not found".to_string()
                );
            }
        }
    }

    /// Get the canonical workspace root (directory containing `.goatd_anchor`)
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Get the canonical kernel source directory
    pub fn kernel_source_dir(&self) -> &Path {
        &self.kernel_source_dir
    }

    /// Get the PKGBUILD directory (parent of kernel source, typically)
    ///
    /// This is the build root where PKGBUILD resides.
    /// Returns the kernel source directory itself (caller can use parent() if needed,
    /// but this method establishes a canonical build root reference).
    pub fn pkgbuild_dir(&self) -> &Path {
        &self.kernel_source_dir
    }

    /// Get the path to the anchor file itself
    pub fn anchor_path(&self) -> &Path {
        &self.anchor_path
    }

    /// Resolve a relative path against the workspace root
    ///
    /// # Arguments
    /// * `relative_path` - Path relative to workspace root (e.g., "src/kernel")
    ///
    /// # Returns
    /// Absolute path resolved against workspace root
    pub fn resolve_relative(&self, relative_path: &Path) -> PathBuf {
        self.workspace_root.join(relative_path)
    }

    /// Verify that a given path is within workspace boundaries
    ///
    /// # Arguments
    /// * `path` - Path to verify
    ///
    /// # Returns
    /// `true` if path is within workspace root or its children
    /// `false` otherwise
    pub fn is_within_workspace(&self, path: &Path) -> bool {
        if let Ok(canonical) = path.canonicalize() {
            canonical
                .to_string_lossy()
                .starts_with(self.workspace_root.to_string_lossy().as_ref())
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_path_registry_anchoring() {
        let temp = tempdir().expect("Failed to create temp dir");
        let workspace = temp.path().join("workspace");
        let kernel_src = workspace.join("src").join("kernel");
        
        fs::create_dir_all(&kernel_src).expect("Failed to create dirs");
        fs::write(workspace.join(".goatd_anchor"), "").expect("Failed to write anchor");

        let registry = PathRegistry::new(kernel_src.clone()).expect("Failed to create registry");
        
        // Canonicalize expectation because PathRegistry canonicalizes
        let expected_root = workspace.canonicalize().unwrap();
        let expected_kernel_src = kernel_src.canonicalize().unwrap();

        assert_eq!(registry.workspace_root(), expected_root);
        assert_eq!(registry.kernel_source_dir(), expected_kernel_src);
    }

    #[test]
    fn test_path_registry_failure_no_anchor() {
        let temp = tempdir().expect("Failed to create temp dir");
        let kernel_src = temp.path().join("kernel");
        fs::create_dir_all(&kernel_src).expect("Failed to create dirs");

        let result = PathRegistry::new(kernel_src);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}

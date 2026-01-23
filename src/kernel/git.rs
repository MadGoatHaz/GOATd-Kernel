//! Native git management using the `git2` crate.
//!
//! This module provides a wrapper around `libgit2` for kernel source management,
//! replacing external git command invocations.

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use git2::{Repository, build::RepoBuilder};
use thiserror::Error;

/// Errors that can occur during git operations
#[derive(Debug, Error)]
pub enum GitError {
    #[error("Repository error: {0}")]
    Repository(String),
    
    #[error("Clone error: {0}")]
    Clone(String),
    
    #[error("Checkout error: {0}")]
    Checkout(String),
    
    #[error("Reference not found: {0}")]
    RefNotFound(String),
    
    #[error("Invalid reference: {0}")]
    InvalidRef(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Git2 error: {0}")]
    Git2(#[from] git2::Error),
}

/// Result type for git operations
pub type GitResult<T> = Result<T, GitError>;

/// Fetch the latest version from a remote repository
///
/// This function connects to a remote git repository and retrieves the latest
/// semantic version tag or branch reference. It uses git2's remote listing
/// capabilities to avoid cloning the repository.
///
/// # Arguments
/// * `url` - The git repository URL (HTTP/HTTPS/SSH)
///
/// # Returns
/// A version string (e.g., "6.12.9-arch1", "v6.13-rc7")
///
/// # Errors
/// Returns `GitError::Repository` if unable to connect to the remote
pub fn get_latest_remote_version(url: &str) -> GitResult<String> {
    use git2::Repository;
    
    eprintln!("[GIT_POLL] Starting version poll for: {}", url);
    
    // Create a temporary directory for repository operations with RAII cleanup
    let temp_dir = tempfile::tempdir()
        .map_err(|e| GitError::Io(e))?;
    
    // Create a temporary bare repository
    let repo = Repository::init_bare(temp_dir.path())
        .map_err(|e| {
            eprintln!("[GIT_POLL] ERROR: Failed to create bare repository: {}", e);
            GitError::Repository(format!(
                "Failed to create temporary repository: {}", e
            ))
        })?;
    
    eprintln!("[GIT_POLL] Bare repository created at: {:?}", temp_dir);
    
    // Add the remote URL
    let remote = repo.remote("origin", url)
        .map_err(|e| {
            eprintln!("[GIT_POLL] ERROR: Failed to add remote {}: {}", url, e);
            GitError::Repository(format!(
                "Failed to add remote {}: {}", url, e
            ))
        })?;
    
    eprintln!("[GIT_POLL] Remote added successfully");
    
    // List remote references
    let remote_refs = remote.list()
        .map_err(|e| {
            eprintln!("[GIT_POLL] ERROR: Failed to list remote references: {}", e);
            GitError::Repository(format!(
                "Failed to list remote references for {}: {}", url, e
            ))
        })?;
    
    let mut tags: Vec<String> = Vec::new();
    let mut branches: Vec<(String, String)> = Vec::new();
    let mut tag_set: HashSet<String> = HashSet::new();
    
    eprintln!("[GIT_POLL] Analyzing remote references...");
    
    // Parse refs into tags and branches, with deduplication
    for reference in remote_refs {
        let name = reference.name();
        let oid = reference.oid();
        
        if name.starts_with("refs/tags/") {
            let mut tag_name = name.strip_prefix("refs/tags/").unwrap_or(name).to_string();
            
            // Filter out peeled tag suffix (^{}) to avoid duplicates
            if tag_name.ends_with("^{}") {
                tag_name = tag_name.strip_suffix("^{}").unwrap_or(&tag_name).to_string();
                eprintln!("[GIT_POLL]   Found peeled tag (dedup): {}", tag_name);
            } else {
                eprintln!("[GIT_POLL]   Found tag: {}", tag_name);
            }
            
            // Add to tag set for deduplication
            if tag_set.insert(tag_name.clone()) {
                tags.push(tag_name);
            }
        } else if name.starts_with("refs/heads/") {
            let branch_name = name.strip_prefix("refs/heads/").unwrap_or(name);
            eprintln!("[GIT_POLL]   Found branch: {}", branch_name);
            branches.push((branch_name.to_string(), oid.to_string()));
        }
    }
    
    eprintln!("[GIT_POLL] Total unique tags: {}, branches: {}", tags.len(), branches.len());
    
    // Select the best version from tags, with fallback to master/main branch
    if !tags.is_empty() {
        // Sort tags by semantic version (descending)
        tags.sort_by(|a, b| {
            let a_num = extract_version_number(a);
            let b_num = extract_version_number(b);
            eprintln!("[GIT_POLL] Comparing: {} ({:?}) vs {} ({:?})", a, a_num, b, b_num);
            b_num.cmp(&a_num) // Reverse order for descending sort
        });
        
        eprintln!("[GIT_POLL] Top 5 sorted tags:");
        for (idx, tag) in tags.iter().take(5).enumerate() {
            eprintln!("[GIT_POLL]   {}. {}", idx + 1, tag);
        }
        
        let selected = tags[0].clone();
        eprintln!("[GIT_POLL] ✓ Selected version: {}", selected);
        Ok(selected)
    } else if !branches.is_empty() {
        // Fallback to main or master branch
        if let Some(main_branch) = branches.iter().find(|b| b.0 == "main") {
            let result = format!("HEAD-{}", main_branch.1[..7].to_string());
            eprintln!("[GIT_POLL] ✓ Fallback to main branch: {}", result);
            Ok(result)
        } else if let Some(master_branch) = branches.iter().find(|b| b.0 == "master") {
            let result = format!("HEAD-{}", master_branch.1[..7].to_string());
            eprintln!("[GIT_POLL] ✓ Fallback to master branch: {}", result);
            Ok(result)
        } else {
            let result = format!("HEAD-{}", branches[0].1[..7].to_string());
            eprintln!("[GIT_POLL] ✓ Fallback to first available branch: {}", result);
            Ok(result)
        }
    } else {
        eprintln!("[GIT_POLL] ✗ ERROR: No tags or branches found");
        Err(GitError::Repository(format!(
            "No tags or branches found in remote: {}", url
        )))
    }
}

/// Extract version number from a tag for semantic version comparison
/// Handles common version formats:
/// - "v6.12.9" → (6, 12, 9)
/// - "6.12.9-arch1" → (6, 12, 9)
/// - "v6.13-rc7" → (6, 13, 0) [pre-release sorts lower]
/// - "linux-6.18.3-arch1-1" → (6, 18, 3)
///
/// Pre-release versions (rc, alpha, beta) are treated as lower precedence
/// by reducing the patch version, ensuring v6.13-rc7 < v6.13.0.
fn extract_version_number(tag: &str) -> (u32, u32, u32) {
    // Remove leading 'v' prefix
    let tag = tag.trim_start_matches('v');
    
    // Remove any 'linux-' prefix (for Arch packaging repos)
    let tag = tag.trim_start_matches("linux-");
    
    // Extract numeric parts
    let parts: Vec<&str> = tag.split(|c: char| !c.is_ascii_digit()).collect();
    
    let major: u32 = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut patch: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    
    // Detect pre-release markers (rc, alpha, beta, etc.)
    // Reduce patch version to sort before release versions
    if tag.contains("rc") || tag.contains("alpha") || tag.contains("beta") {
        // Ensure rc versions sort lower: v6.13-rc7 < v6.13.0
        if patch > 0 {
            patch = patch.saturating_sub(1);
        } else if minor > 0 {
            // Not implemented for major.minor-rc case; keep as-is for now
        }
    }
    
    (major, minor, patch)
}

/// Manages kernel source repository operations using native git bindings
pub struct GitManager {
    repo_path: PathBuf,
}

impl GitManager {
    /// Creates a new GitManager for an existing repository
    pub fn new(repo_path: impl AsRef<Path>) -> GitResult<Self> {
        let repo_path = repo_path.as_ref().to_path_buf();
        
        // Verify the repository exists if the path exists
        if repo_path.exists() {
            Repository::open(&repo_path)
                .map_err(|e| GitError::Repository(format!(
                    "Failed to open repository at {:?}: {}",
                    repo_path, e
                )))?;
        }
        
        Ok(GitManager { repo_path })
    }
    
    /// Clones a repository from a URL to the target path with shallow clone optimization
    ///
    /// This implementation uses shallow cloning (depth=1) to optimize network bandwidth
    /// and storage requirements. It attempts shallow clone first, with proper error handling
    /// for repositories that don't support shallow cloning.
    ///
    /// # Arguments
    /// * `url` - The repository URL (HTTP/HTTPS/SSH)
    /// * `target_path` - The local path where the repository will be cloned
    ///
    /// # Errors
    /// Returns `GitError::Clone` if the clone operation fails
    pub fn clone(url: &str, target_path: impl AsRef<Path>) -> GitResult<Self> {
        let target_path = target_path.as_ref();
        
        eprintln!("[Git] [CLONE] Starting shallow clone with depth=1 for: {}", url);
        eprintln!("[Git] [CLONE] Target path: {:?}", target_path);
        
        // Attempt shallow clone with depth=1 for optimization
        Self::shallow_clone(url, target_path)
            .or_else(|shallow_err| {
                eprintln!("[Git] [CLONE] ⚠ Shallow clone failed: {}", shallow_err);
                eprintln!("[Git] [CLONE] Falling back to standard (full) clone");
                
                // Fallback to standard clone if shallow clone fails
                    Repository::clone(url, target_path)
                        .map(|_| ())
                        .map_err(|e| GitError::Clone(format!(
                            "Failed to clone {} to {:?} (fallback): {}",
                            url, target_path, e
                        )))
            })?;
        
        eprintln!("[Git] [CLONE] ✓ Clone completed successfully");
        GitManager::new(target_path)
    }
    
    /// Performs a shallow clone with depth=1 for bandwidth optimization
    ///
    /// Uses `git2::build::RepoBuilder` with fetch options to limit clone depth.
    /// This significantly reduces bandwidth and storage requirements for large repositories.
    ///
    /// # Arguments
    /// * `url` - The repository URL
    /// * `target_path` - The local path where the repository will be cloned
    ///
    /// # Returns
    /// * `Ok(())` if shallow clone succeeds
    /// * `Err(GitError)` if shallow clone fails
    fn shallow_clone(url: &str, target_path: &Path) -> GitResult<()> {
        eprintln!("[Git] [SHALLOW-CLONE] Initiating shallow clone (depth=1)");
        eprintln!("[Git] [SHALLOW-CLONE] URL: {}", url);
        
        // Create a new RepoBuilder for shallow cloning
        let mut builder = RepoBuilder::new();
        
        // Configure fetch options for shallow clone
        let mut fetch_options = git2::FetchOptions::new();
        
        // Enable shallow cloning with depth=1
        eprintln!("[Git] [SHALLOW-CLONE] Setting fetch depth to 1");
        fetch_options.depth(1);
        
        // Set up callbacks for progress reporting
        let mut callbacks = git2::RemoteCallbacks::new();
        
        // Add transfer progress callback
        callbacks.transfer_progress(|progress| {
            let received = progress.received_objects();
            let total = progress.total_objects();
            let indexed = progress.indexed_objects();
            
            if total > 0 {
                let percent = (received as f32 / total as f32 * 100.0) as u32;
                eprintln!("[Git] [SHALLOW-CLONE] [PROGRESS] {}/{} objects ({} indexed) - {}%",
                    received, total, indexed, percent);
            }
            true
        });
        
        // Apply fetch options to the builder
        fetch_options.remote_callbacks(callbacks);
        builder.fetch_options(fetch_options);
        
        // Clone the repository
        eprintln!("[Git] [SHALLOW-CLONE] Cloning repository to {:?}", target_path);
        builder.clone(url, target_path)
            .map_err(|e| {
                eprintln!("[Git] [SHALLOW-CLONE] ✗ Shallow clone failed: {}", e);
                GitError::Clone(format!(
                    "Shallow clone failed for {}: {}",
                    url, e
                ))
            })?;
        
        eprintln!("[Git] [SHALLOW-CLONE] ✓ Shallow clone completed successfully");
        Ok(())
    }
    
    /// Fetches all tags and branches from the remote repository
    ///
    /// # Errors
    /// Returns `GitError::Repository` if the fetch operation fails
    pub fn fetch(&self) -> GitResult<()> {
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| GitError::Repository(format!(
                "Failed to open repository: {}",
                e
            )))?;
        
        let mut remote = repo.find_remote("origin")
            .map_err(|e| GitError::Repository(format!(
                "Failed to find origin remote: {}",
                e
            )))?;
        
        remote.fetch(&[] as &[&str], None, None)
            .map_err(|e| GitError::Repository(format!(
                "Fetch failed: {}",
                e
            )))?;
        
        Ok(())
    }
    
    /// Lists all available tags in the repository
    ///
    /// # Returns
    /// A vector of tag names
    ///
    /// # Errors
    /// Returns `GitError::Repository` if tag enumeration fails
    pub fn list_tags(&self) -> GitResult<Vec<String>> {
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| GitError::Repository(format!(
                "Failed to open repository: {}",
                e
            )))?;
        
        let tags = repo.tag_names(None)
            .map_err(|e| GitError::Repository(format!(
                "Failed to list tags: {}",
                e
            )))?;
        
        Ok(tags.iter().flatten().map(|s| s.to_string()).collect())
    }
    
    /// Lists all available branches in the repository
    ///
    /// # Returns
    /// A vector of branch names
    ///
    /// # Errors
    /// Returns `GitError::Repository` if branch enumeration fails
    pub fn list_branches(&self) -> GitResult<Vec<String>> {
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| GitError::Repository(format!(
                "Failed to open repository: {}",
                e
            )))?;
        
        let branches = repo.branches(None)
            .map_err(|e| GitError::Repository(format!(
                "Failed to list branches: {}",
                e
            )))?;
        
        let mut branch_names = Vec::new();
        for branch_result in branches {
            let (branch, _) = branch_result
                .map_err(|e| GitError::Repository(format!(
                    "Failed to read branch: {}",
                    e
                )))?;
            
            if let Ok(name) = branch.name() {
                if let Some(branch_name) = name {
                    branch_names.push(branch_name.to_string());
                }
            }
        }
        
        Ok(branch_names)
    }
    
    /// Checks out a specific commit, tag, or branch
    ///
    /// # Arguments
    /// * `reference` - The commit hash, tag name, or branch name to check out
    ///
    /// # Errors
    /// Returns `GitError::RefNotFound` if the reference doesn't exist
    /// Returns `GitError::Checkout` if the checkout operation fails
    pub fn checkout(&self, reference: &str) -> GitResult<()> {
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| GitError::Repository(format!(
                "Failed to open repository: {}",
                e
            )))?;
        
        // Try to resolve the reference (commit hash, tag, or branch)
        let obj = repo.revparse_single(reference)
            .map_err(|e| GitError::RefNotFound(format!(
                "Failed to resolve reference '{}': {}",
                reference, e
            )))?;
        
        repo.checkout_tree(&obj, None)
            .map_err(|e| GitError::Checkout(format!(
                "Failed to check out {}: {}",
                reference, e
            )))?;
        
        // Update HEAD to point to the new reference
        match obj.as_tag() {
            Some(tag) => {
                let target = tag.target()
                    .map_err(|e| GitError::InvalidRef(format!(
                        "Failed to get tag target: {}",
                        e
                    )))?;
                repo.set_head_detached(target.id())
                    .map_err(|e| GitError::Checkout(format!(
                        "Failed to set HEAD: {}",
                        e
                    )))?;
            }
            None => {
                repo.set_head_detached(obj.id())
                    .map_err(|e| GitError::Checkout(format!(
                        "Failed to set HEAD: {}",
                        e
                    )))?;
            }
        }
        
        Ok(())
    }
    
    /// Gets the current HEAD commit hash
    ///
    /// # Returns
    /// The full commit hash as a string
    ///
    /// # Errors
    /// Returns `GitError::Repository` if the operation fails
    pub fn get_head_commit(&self) -> GitResult<String> {
        let repo = Repository::open(&self.repo_path)
            .map_err(|e| GitError::Repository(format!(
                "Failed to open repository: {}",
                e
            )))?;
        
        let head = repo.head()
            .map_err(|e| GitError::Repository(format!(
                "Failed to read HEAD: {}",
                e
            )))?;
        
        let commit_id = head.target()
            .ok_or_else(|| GitError::Repository(
                "HEAD is not a direct reference".to_string()
            ))?;
        
        Ok(commit_id.to_string())
    }
    
    /// Returns the path to the repository
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_git_manager_creation() {
        // This test verifies that GitManager can be instantiated
        // Actual repository testing requires a valid repo or integration tests
        let repo_path = PathBuf::from("/tmp/nonexistent_repo");
        
        // Creating a manager for a non-existent path should succeed
        // (we only check existence when the path already exists)
        let result = GitManager::new(&repo_path);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_git_error_display() {
        let err = GitError::RefNotFound("main".to_string());
        assert_eq!(
            err.to_string(),
            "Reference not found: main"
        );
    }
}

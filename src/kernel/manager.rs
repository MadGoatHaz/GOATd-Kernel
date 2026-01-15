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
                None => return Err("Cannot determine parent directory for kernel package".to_string()),
            };
            
            // Collect all matching files (main, headers, docs) before deletion
            let matching_files = collect_matching_kernel_files(parent_dir, &pkg.name, &pkg.version)?;
            
            if matching_files.is_empty() {
                return Err(format!("No kernel packages found for {} ({})", pkg.name, pkg.version));
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
                                file_path.file_name()
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
                Err("Main kernel package was not among the matching files (unexpected state)".to_string())
            }
        } else {
            Err("Kernel package has no associated path".to_string())
        }
    }
}

/// List installed kernel packages using pacman
/// Returns a Vec of kernel package strings formatted as "linux (6.18.3)" or "linux (6.18.3-GOATd-Gamer)"
/// Preserves GOATd branding and profile identifiers when present
/// Strict filtering: only actual kernels (linux, linux-zen, linux-lts, etc.)
/// Excludes: firmware, headers, docs packages
fn list_installed_kernels() -> Vec<KernelPackage> {
    use std::process::Command;
    
    let output = match Command::new("pacman")
        .args(&["-Q"])
        .output()
    {
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
    
    // Regex to match only actual kernel packages: ^(linux|linux-zen|linux-lts|linux-hardened|linux-mainline)$
    // These are packages that provide the vmlinuz file
    // Excludes: linux-firmware, linux-headers, linux-docs, linux-api-headers, etc.
    let kernel_variants = ["linux", "linux-zen", "linux-lts", "linux-hardened", "linux-mainline"];
    
    for line in output_str.lines() {
        // pacman -Q output format: "package_name version"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let pkg_name = parts[0];
            let version = parts[1];
            
            // Filter out non-kernel packages (docs, headers)
            if pkg_name.ends_with("-docs") || pkg_name.ends_with("-headers") {
                continue;
            }
            
            // Strict filter: only if package name exactly matches one of the kernel variants
            if kernel_variants.contains(&pkg_name) || pkg_name.contains("goatd") {
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
                log_info!("[KernelManager] [DEBUG] Found kernel: {} version {} (GOATd: {})", pkg_name, version, has_goatd);
            }
        }
    }
    
    // Only log summary at info level if this is a fresh scan (not every frame)
    if !kernels.is_empty() {
        log_info!("[KernelManager] Kernel scan complete: {} installed kernels found", kernels.len());
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
        log_info!("[KernelManager] Workspace path does not exist: {}", workspace_path);
        return vec![];
    }
    
    let mut kernels = vec![];
    scan_directory_recursive(&path, &mut kernels);
    
    log_info!("[KernelManager] Found {} built kernels in workspace", kernels.len());
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
    // Check for common non-kernel suffixes and substrings
    if without_arch.contains("-headers") || without_arch.contains("-docs") ||
       without_arch.contains("-firmware") || without_arch.contains("-api-headers") {
        return None;
    }
    
    // Match kernel variants: linux, linux-zen, linux-lts, linux-hardened, linux-mainline
    let kernel_name = if without_arch.starts_with("linux-zen-") {
        "linux-zen"
    } else if without_arch.starts_with("linux-lts-") {
        "linux-lts"
    } else if without_arch.starts_with("linux-hardened-") {
        "linux-hardened"
    } else if without_arch.starts_with("linux-mainline-") {
        "linux-mainline"
    } else if without_arch.starts_with("linux-goatd-") {
        // Find the next dash after linux-goatd-
        if let Some(dash_idx) = without_arch[12..].find('-') {
            &without_arch[..12 + dash_idx]
        } else {
            "linux-goatd"
        }
    } else if without_arch.starts_with("linux-") {
        "linux"
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
fn collect_matching_kernel_files(dir: &std::path::Path, kernel_name: &str, version: &str) -> Result<Vec<std::path::PathBuf>, String> {
    let mut matching_files = vec![];
    
    // Read the directory
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?;
    
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
/// Matches:
/// - Main: "linux-goatd-gaming-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - Headers: "linux-goatd-gaming-headers-6.18.3-arch1-1-x86_64.pkg.tar.zst"
/// - Docs: "linux-goatd-gaming-docs-6.18.3-arch1-1-x86_64.pkg.tar.zst"
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
    
    // Build expected patterns for matching
    let main_pattern = format!("{}-{}", kernel_variant, version);
    let headers_pattern = format!("{}-headers-{}", kernel_variant, version);
    let docs_pattern = format!("{}-docs-{}", kernel_variant, version);
    
    // Check exact matches (most reliable)
    if without_arch == main_pattern ||
       without_arch == headers_pattern ||
       without_arch == docs_pattern {
        return true;
    }
    
    // Also handle cases where version might contain extra release numbers
    // e.g., "6.18.3-arch1-2" in artifact but just match "6.18.3"
    if without_arch.starts_with(&main_pattern) ||
       without_arch.starts_with(&headers_pattern) ||
       without_arch.starts_with(&docs_pattern) {
        // Ensure what follows is a release number (dash followed by digits)
        let remainder = if without_arch.starts_with(&main_pattern) {
            &without_arch[main_pattern.len()..]
        } else if without_arch.starts_with(&headers_pattern) {
            &without_arch[headers_pattern.len()..]
        } else {
            &without_arch[docs_pattern.len()..]
        };
        
        // Should be empty or start with -<digits>
        if remainder.is_empty() || (remainder.starts_with('-') && remainder[1..].chars().next().map_or(false, |c| c.is_ascii_digit())) {
            return true;
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
                                log_info!("[KernelManager] [CANDIDATE] Found matching file: {}", filename);
                                if let Ok(()) = std::fs::remove_file(entry.path()) {
                                    if !entry.path().exists() {
                                        log_info!("[KernelManager] [SUCCESS] Kernel file deleted and verified gone: {}", filename);
                                        return true;
                                    } else {
                                        log_info!("[KernelManager] [WARNING] File deletion reported success but file still exists: {}", filename);
                                    }
                                } else {
                                    log_info!("[KernelManager] [ERROR] Failed to delete kernel: {}", filename);
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
    
    // Must NOT contain excluded package types
    let excluded = ["-headers", "-docs", "-api-headers", "-firmware"];
    for exclude in &excluded {
        if filename.contains(exclude) {
            log_info!("[KernelManager] [FILTERED] Excluding non-kernel package: {} (contains '{}')", filename, exclude);
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
    
    log_info!("[KernelManager] [MATCH] Package '{}' matches variant='{}', version='{}'", filename, kernel_variant, version);
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
         log_info!("[KernelManager] Invalid kernel display name format: {}", kernel_display_name);
         return false;
     }
     
     let kernel_variant = parts[0]; // "linux", "linux-zen", etc.
     let version_part = parts[1].trim_matches(|c| c == '(' || c == ')'); // "6.18.3-arch1-1"
     
     log_info!("[KernelManager] Searching for kernel: variant='{}', version='{}'", kernel_variant, version_part);
     
     let path = PathBuf::from(workspace_path);
     
     if !path.exists() {
         log_info!("[KernelManager] Workspace path does not exist: {}", workspace_path);
         return false;
     }
     
     // Use unified recursive search to find and delete the kernel file
     delete_kernel_recursive(&path, kernel_variant, version_part)
}

/// Helper to find a kernel package file in the workspace
/// Searches recursively for .pkg.tar.zst files matching the kernel variant and version
/// Returns the absolute path to the matching file, or None if not found
pub fn find_kernel_package_file(workspace_path: &str, kernel_variant: &str, version: &str) -> Option<PathBuf> {
     if workspace_path.is_empty() {
         log_info!("[KernelManager] Cannot find package: workspace path is empty");
         return None;
     }
     
     let path = PathBuf::from(workspace_path);
     
     if !path.exists() {
         log_info!("[KernelManager] Workspace path does not exist: {}", workspace_path);
         return None;
     }
     
     // Use unified recursive search
     find_kernel_package_recursive(&path, kernel_variant, version)
}

/// Recursively search for a kernel package file
/// Uses unified recursion instead of nested directory scans
fn find_kernel_package_recursive(dir: &std::path::Path, kernel_variant: &str, version: &str) -> Option<PathBuf> {
     match std::fs::read_dir(dir) {
         Ok(entries) => {
             for entry in entries.flatten() {
                 if let Ok(metadata) = entry.metadata() {
                     if metadata.is_file() {
                         if let Some(filename) = entry.file_name().to_str() {
                             if is_matching_kernel_package(filename, kernel_variant, version) {
                                 log_info!("[KernelManager] [MATCH] Found kernel package: {}", filename);
                                 return Some(entry.path());
                             }
                         }
                     } else if metadata.is_dir() {
                         // Recursively search subdirectories
                         if let Some(found) = find_kernel_package_recursive(&entry.path(), kernel_variant, version) {
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
     
     log_info!("[KernelManager] No matching kernel package found for variant='{}', version='{}'", kernel_variant, version);
     None
}

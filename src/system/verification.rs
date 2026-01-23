//! Post-install kernel verification module
//!
//! Provides comprehensive checks for kernel module directory structure,
//! symlinks, and headers installation after pacman installation.
//! Enables fallback symlink creation if kernel-install hooks fail.

use std::fs;
use std::path::{Path, PathBuf};

/// Verification status for a kernel installation
///
/// Tracks which components of a kernel installation are present and valid.
/// Used to determine if DKMS can proceed and what fallback actions are needed.
#[derive(Debug, Clone)]
pub struct KernelInstallationStatus {
    /// Kernel version string (e.g., "6.18.3-arch1-2")
    pub kernel_version: String,
    /// `/usr/lib/modules/{kernel_version}` or `/lib/modules/{kernel_version}` exists
    pub module_dir_exists: bool,
    /// `build` symlink exists in module directory
    pub build_symlink_exists: bool,
    /// `source` symlink exists in module directory
    pub source_symlink_exists: bool,
    /// Kernel headers are installed at `/usr/src/linux-{version}`
    pub headers_installed: bool,
    /// All prerequisites met for DKMS build
    pub ready_for_dkms: bool,
}

/// DKMS Compatibility status for kernel and driver combinations
///
/// Detects RC (release candidate) kernels and evaluates NVIDIA driver compatibility.
/// RC kernels (e.g., 6.19-rc6) are pre-release versions with unstable ABIs that may
/// cause DKMS build failures with drivers not yet patched for those kernels.
#[derive(Debug, Clone)]
pub struct DkmsCompatibility {
    /// Kernel version string
    pub kernel_version: String,
    /// True if kernel is a release candidate (contains "-rc" in version)
    pub is_rc_kernel: bool,
    /// RC version number if RC kernel (e.g., 6 from "6.19-rc6")
    pub rc_version: Option<u32>,
    /// Base kernel version without RC suffix (e.g., "6.19" from "6.19-rc6")
    pub base_kernel_version: Option<String>,
    /// NVIDIA driver version if available
    pub nvidia_driver_version: Option<String>,
    /// True if NVIDIA driver is known compatible with this kernel
    pub nvidia_compat_status: bool,
    /// Detailed reason for compatibility status
    pub compat_reason: String,
}

impl DkmsCompatibility {
    /// Create a new DkmsCompatibility struct, detecting RC status
    pub fn new(kernel_version: &str) -> Self {
        let is_rc_kernel = kernel_version.contains("-rc");
        let (rc_version, base_kernel_version) = Self::parse_rc_version(kernel_version);
        
        eprintln!("[DKMS-COMPAT] Analyzing kernel: {}", kernel_version);
        eprintln!("[DKMS-COMPAT] RC kernel detected: {}", is_rc_kernel);
        
        if is_rc_kernel {
            if let Some(rc_num) = rc_version {
                eprintln!("[DKMS-COMPAT] RC version: rc{}", rc_num);
            }
            if let Some(base) = &base_kernel_version {
                eprintln!("[DKMS-COMPAT] Base kernel version: {}", base);
            }
        }
        
        Self {
            kernel_version: kernel_version.to_string(),
            is_rc_kernel,
            rc_version,
            base_kernel_version,
            nvidia_driver_version: None,
            nvidia_compat_status: true, // Default to compatible unless proven otherwise
            compat_reason: String::new(),
        }
    }
    
    /// Parse RC version from kernel version string
    /// Returns (rc_version, base_kernel_version)
    /// Example: "6.19-rc6" -> (Some(6), Some("6.19"))
    fn parse_rc_version(kernel_version: &str) -> (Option<u32>, Option<String>) {
        if let Some(rc_pos) = kernel_version.find("-rc") {
            let base = &kernel_version[..rc_pos];
            let rc_str = &kernel_version[rc_pos + 3..];
            
            // Try to extract numeric RC version (e.g., "6" from "rc6")
            let rc_num = rc_str.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok();
            
            (rc_num, Some(base.to_string()))
        } else {
            (None, None)
        }
    }
    
    /// Check NVIDIA driver compatibility with RC kernel
    pub fn check_nvidia_compatibility(&mut self, nvidia_version: &str) {
        self.nvidia_driver_version = Some(nvidia_version.to_string());
        eprintln!("[DKMS-COMPAT] Checking NVIDIA {} against kernel {}", nvidia_version, self.kernel_version);
        
        if !self.is_rc_kernel {
            // Stable kernels are generally compatible
            self.nvidia_compat_status = true;
            self.compat_reason = format!("Stable kernel - NVIDIA {} compatible", nvidia_version);
            eprintln!("[DKMS-COMPAT] ✓ Stable kernel - NVIDIA compatible");
            return;
        }
        
        // RC kernel compatibility check
        // NVIDIA 590.48.01 has known issues with Linux 6.19-rc6+ (memremap.h struct changes)
        let nvidia_major = nvidia_version.split('.').next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        let nvidia_minor = nvidia_version.split('.').nth(1).unwrap_or("0").parse::<u32>().unwrap_or(0);
        
        if let Some(rc_num) = self.rc_version {
            if let Some(base) = &self.base_kernel_version {
                // Detect problematic kernel versions
                // 6.19-rc6+ has memremap.h struct changes that affect NVIDIA 590.x
                let is_problematic_kernel = base.starts_with("6.19") && rc_num >= 6;
                
                if is_problematic_kernel && nvidia_major == 590 && nvidia_minor < 100 {
                    self.nvidia_compat_status = false;
                    self.compat_reason = format!(
                        "KNOWN ISSUE: NVIDIA {}.{} has memremap.h compatibility issue on kernel {}-rc{}. Shim will be applied.",
                        nvidia_major, nvidia_minor, base, rc_num
                    );
                    eprintln!("[DKMS-COMPAT] ⚠ KNOWN ISSUE: NVIDIA 590.x incompatible with {}-rc{}", base, rc_num);
                    eprintln!("[DKMS-COMPAT] Reason: memremap.h struct changes in 6.19-rc6+");
                } else {
                    self.nvidia_compat_status = true;
                    self.compat_reason = format!(
                        "RC kernel {} detected but no known compatibility issues with NVIDIA {}",
                        self.kernel_version, nvidia_version
                    );
                    eprintln!("[DKMS-COMPAT] RC kernel compatibility check passed");
                }
            }
        }
    }
    
    /// Get compatibility status summary
    pub fn summary(&self) -> String {
        if !self.is_rc_kernel {
            return format!("Stable kernel {} - DKMS ready", self.kernel_version);
        }
        
        let rc_info = if let Some(rc) = self.rc_version {
            format!(" (rc{})", rc)
        } else {
            String::new()
        };
        
        let compat = if self.nvidia_compat_status {
            "✓ Compatible"
        } else {
            "⚠ Incompatible"
        };
        
        format!("RC kernel {}{} - {}: {}",
            self.kernel_version, rc_info, compat, self.compat_reason)
    }
}

/// Errors that can occur during kernel installation verification
#[derive(Debug, Clone)]
pub enum KernelInstallationError {
    /// `/usr/lib/modules/{kernel_version}` directory missing
    ModuleDirectoryMissing(String),
    /// `build` symlink missing from module directory
    BuildSymlinkMissing(String),
    /// `source` symlink missing from module directory
    SourceSymlinkMissing(String),
    /// Kernel headers not found at `/usr/src/linux-{version}`
    HeadersNotInstalled(String),
    /// Failed to create fallback symlinks
    SymlinkCreationFailed(String),
    /// Could not determine kernel version from package
    UnknownKernelVersion,
}

impl std::fmt::Display for KernelInstallationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModuleDirectoryMissing(msg) => write!(f, "Module directory missing: {}", msg),
            Self::BuildSymlinkMissing(msg) => write!(f, "Build symlink missing: {}", msg),
            Self::SourceSymlinkMissing(msg) => write!(f, "Source symlink missing: {}", msg),
            Self::HeadersNotInstalled(msg) => write!(f, "Headers not installed: {}", msg),
            Self::SymlinkCreationFailed(msg) => write!(f, "Symlink creation failed: {}", msg),
            Self::UnknownKernelVersion => write!(f, "Could not determine kernel version"),
        }
    }
}

impl std::error::Error for KernelInstallationError {}

/// Discover kernel headers location with robust fallback strategy
///
/// Attempts to locate kernel headers for the given kernel version using unified naming:
/// 1. Primary: `/usr/src/linux-{kernelrelease}` (exact match from .kernelrelease)
/// 2. Fallback: `/usr/src/linux-{version}` (rebranded variants)
/// 3. Base version: `/usr/src/linux-{base_version}` (without profile suffix)
/// 4. Scanner: Scan `/usr/src/` for any `linux-*` directory with valid headers
///
/// This function handles rebranded kernels and unified naming where headers are
/// installed to `/usr/src/linux-{kernelrelease}` matching the exact kernel version
/// that was generated during the build.
///
/// For example:
/// - Exact kernel: "6.18.3-arch1-2-goatd-gaming" → tries "/usr/src/linux-6.18.3-arch1-2-goatd-gaming"
/// - Base version: "6.18.3-arch1-2-goatd-gaming" → tries "/usr/src/linux-6.18.3-arch1-2"
/// - Legacy: "6.18.3" → tries "/usr/src/linux-6.18.3"
///
/// # Arguments
/// * `kernel_version` - Kernel version string (e.g., "6.18.3-arch1-2" or "6.18.3-arch1-2-goatd-gaming")
///
/// # Returns
/// `Option<PathBuf>` pointing to the discovered headers directory, or `None` if not found
pub fn discover_kernel_headers(kernel_version: &str) -> Option<PathBuf> {
     eprintln!("[DISCOVER_HEADERS] [UNIFIED-NAMING] Searching for headers for kernel version: {}", kernel_version);
     
     // Extract base version (remove profile suffix if present)
     // E.g., "6.18.3-arch1-2-goatd-gaming" -> "6.18.3-arch1-2"
     let base_version = if let Some(dash_pos) = kernel_version.rfind('-') {
         let potential_suffix = &kernel_version[dash_pos + 1..];
         // If the suffix looks like a profile name (all lowercase letters), strip it
         if potential_suffix.chars().all(|c| c.is_ascii_lowercase() || c == '-') &&
            potential_suffix.len() > 2 {
             &kernel_version[..dash_pos]
         } else {
             kernel_version
         }
     } else {
         kernel_version
     };
     
     eprintln!("[DISCOVER_HEADERS] [UNIFIED-NAMING] Base version extracted: {}", base_version);
     
     // STRATEGY 0 (PRIORITY): Try GOATd-branded path directly if kernel_version contains "-goatd-"
     // This prioritizes GOATd-branded headers installation paths
     if kernel_version.contains("-goatd-") {
         eprintln!("[DISCOVER_HEADERS] [STRATEGY-0] PRIORITY: Detected GOATd-branded version, trying exact match first: /usr/src/linux-{}", kernel_version);
         let goatd_path = Path::new("/usr/src").join(format!("linux-{}", kernel_version));
         if goatd_path.exists() && goatd_path.is_dir() {
             if goatd_path.join("include/linux/kernel.h").exists() {
                 eprintln!("[DISCOVER_HEADERS] [STRATEGY-0] ✓ Found GOATd-branded headers at: {}", goatd_path.display());
                 return Some(goatd_path);
             }
         }
     }
     
     // STRATEGY 1: Try exact match with full kernel version (unified naming)
     // This matches files installed using the exact .kernelrelease string
     eprintln!("[DISCOVER_HEADERS] [STRATEGY-1] Trying exact match: /usr/src/linux-{}", kernel_version);
     let exact_path = Path::new("/usr/src").join(format!("linux-{}", kernel_version));
     if exact_path.exists() && exact_path.is_dir() {
         if exact_path.join("include/linux/kernel.h").exists() {
             eprintln!("[DISCOVER_HEADERS] [STRATEGY-1] ✓ Found headers at (unified naming - exact match): {}", exact_path.display());
             return Some(exact_path);
         }
     }
     
     // STRATEGY 2: Try base version (without profile suffix)
     // This handles rebranded kernels where headers use the base version
     if kernel_version != base_version {
         eprintln!("[DISCOVER_HEADERS] [STRATEGY-2] Trying base version: /usr/src/linux-{}", base_version);
         let base_path = Path::new("/usr/src").join(format!("linux-{}", base_version));
         if base_path.exists() && base_path.is_dir() {
             if base_path.join("include/linux/kernel.h").exists() {
                 eprintln!("[DISCOVER_HEADERS] [STRATEGY-2] ✓ Found headers at (base version fallback): {}", base_path.display());
                 return Some(base_path);
             }
         }
     }
     
     // STRATEGY 3: Scan /usr/src for linux-* directories with validation
     // HARDENED: Prioritize GOATd-branded directories first in scan
     eprintln!("[DISCOVER_HEADERS] [STRATEGY-3] Scanning /usr/src for linux-* directories (prioritize GOATd-branded paths)");
     if let Ok(entries) = fs::read_dir("/usr/src") {
         let mut candidates = Vec::new();
         let mut goatd_candidates = Vec::new();
         
         for entry in entries.flatten() {
             if let Ok(metadata) = entry.metadata() {
                 if metadata.is_dir() {
                     if let Some(name) = entry.file_name().to_str() {
                         if name.starts_with("linux-") {
                             let candidate = Path::new("/usr/src").join(name);
                             
                             // Validate: must have key header files
                             if candidate.join("include/linux/kernel.h").exists() &&
                                candidate.join("Makefile").exists() {
                                 
                                 let kernelrelease_path = candidate.join(".kernelrelease");
                                 if kernelrelease_path.exists() {
                                     if let Ok(content) = fs::read_to_string(&kernelrelease_path) {
                                         let stored_version = content.trim();
                                         if stored_version == kernel_version || stored_version == base_version {
                                             // Prioritize GOATd-branded paths
                                             if name.contains("-goatd-") {
                                                 goatd_candidates.push(candidate);
                                             } else {
                                                 candidates.push(candidate);
                                             }
                                         }
                                     }
                                 } else {
                                     // Fallback: if no .kernelrelease file, accept directory if headers are valid
                                     if name.contains("-goatd-") {
                                         goatd_candidates.push(candidate);
                                     } else {
                                         candidates.push(candidate);
                                     }
                                 }
                             }
                         }
                     }
                 }
             }
         }
         
         // Return GOATd-branded candidate first (priority)
         if !goatd_candidates.is_empty() {
             eprintln!("[DISCOVER_HEADERS] [STRATEGY-3] ✓ Found GOATd-branded headers directory: {}", goatd_candidates[0].display());
             return Some(goatd_candidates[0].clone());
         }
         
         // Then return regular candidate
         if !candidates.is_empty() {
             eprintln!("[DISCOVER_HEADERS] [STRATEGY-3] ✓ Found valid headers directory: {}", candidates[0].display());
             return Some(candidates[0].clone());
         }
     }
     
     eprintln!("[DISCOVER_HEADERS] ✗ No kernel headers found for version: {}", kernel_version);
     None
}

/// Verify kernel module directory structure exists and is valid
///
/// Checks for the presence of `/usr/lib/modules/{kernel_version}` or
/// `/lib/modules/{kernel_version}` (in case of symlink).
/// Also verifies module files are present (module.dep, etc.).
///
/// # Arguments
/// * `kernel_version` - Kernel version string (e.g., "6.18.3-arch1-2")
///
/// # Returns
/// `Ok(true)` if module directory exists with kernel files, `Ok(false)` if missing
pub fn verify_kernel_module_directory(kernel_version: &str) -> Result<bool, KernelInstallationError> {
    eprintln!("[VERIFY] Checking kernel module directory for: {}", kernel_version);
    
    // Try /usr/lib/modules first (primary location)
    let usr_lib_modules = Path::new("/usr/lib/modules").join(kernel_version);
    if usr_lib_modules.exists() {
        eprintln!("[VERIFY] ✓ Module directory exists: {}", usr_lib_modules.display());
        // Check for kernel files to confirm it's a valid kernel install
        let module_dep = usr_lib_modules.join("modules.dep");
        if module_dep.exists() {
            eprintln!("[VERIFY] ✓ modules.dep found (valid kernel install)");
            return Ok(true);
        }
        eprintln!("[VERIFY] ⚠ Module directory exists but modules.dep not found");
        return Ok(false);
    }
    
    // Try /lib/modules (may be symlink from /usr/lib)
    let lib_modules = Path::new("/lib/modules").join(kernel_version);
    if lib_modules.exists() {
        eprintln!("[VERIFY] ✓ Module directory exists: {}", lib_modules.display());
        let module_dep = lib_modules.join("modules.dep");
        if module_dep.exists() {
            eprintln!("[VERIFY] ✓ modules.dep found (valid kernel install)");
            return Ok(true);
        }
        eprintln!("[VERIFY] ⚠ Module directory exists but modules.dep not found");
        return Ok(false);
    }
    
    eprintln!("[VERIFY] ✗ Module directory not found in either /usr/lib or /lib");
    Ok(false)
}

/// Verify build symlink exists and points to valid kernel headers
///
/// Checks for `/usr/lib/modules/{kernel_version}/build` symlink.
/// Validates that it points to a location containing kernel headers (Makefile, etc.).
///
/// # Arguments
/// * `kernel_version` - Kernel version string
///
/// # Returns
/// `Ok(true)` if symlink exists and points to valid headers, `Ok(false)` if missing
pub fn verify_build_symlink(kernel_version: &str) -> Result<bool, KernelInstallationError> {
    eprintln!("[VERIFY] Checking build symlink for: {}", kernel_version);
    
    // Try /usr/lib/modules first
    let build_link = Path::new("/usr/lib/modules").join(kernel_version).join("build");
    if build_link.exists() {
        eprintln!("[VERIFY] ✓ Build symlink exists: {}", build_link.display());
        
        // Verify it points to valid headers
        if let Ok(path) = build_link.canonicalize() {
            eprintln!("[VERIFY] ✓ Build symlink points to: {}", path.display());
            
            // Check for Makefile (key indicator of valid kernel headers)
            if path.join("Makefile").exists() {
                eprintln!("[VERIFY] ✓ Makefile found in build target");
                return Ok(true);
            }
            eprintln!("[VERIFY] ⚠ Build symlink points to valid path but Makefile not found");
            return Ok(false);
        }
    }
    
    // Try /lib/modules
    let build_link = Path::new("/lib/modules").join(kernel_version).join("build");
    if build_link.exists() {
        eprintln!("[VERIFY] ✓ Build symlink exists: {}", build_link.display());
        
        if let Ok(path) = build_link.canonicalize() {
            eprintln!("[VERIFY] ✓ Build symlink points to: {}", path.display());
            if path.join("Makefile").exists() {
                eprintln!("[VERIFY] ✓ Makefile found in build target");
                return Ok(true);
            }
            eprintln!("[VERIFY] ⚠ Build symlink points to valid path but Makefile not found");
            return Ok(false);
        }
    }
    
    eprintln!("[VERIFY] ✗ Build symlink not found");
    Ok(false)
}

/// Verify source symlink exists and points to valid kernel source
///
/// Checks for `/usr/lib/modules/{kernel_version}/source` symlink.
/// Validates that it points to a location containing kernel source files.
///
/// # Arguments
/// * `kernel_version` - Kernel version string
///
/// # Returns
/// `Ok(true)` if symlink exists and points to valid source, `Ok(false)` if missing
pub fn verify_source_symlink(kernel_version: &str) -> Result<bool, KernelInstallationError> {
    eprintln!("[VERIFY] Checking source symlink for: {}", kernel_version);
    
    // Try /usr/lib/modules first
    let source_link = Path::new("/usr/lib/modules").join(kernel_version).join("source");
    if source_link.exists() {
        eprintln!("[VERIFY] ✓ Source symlink exists: {}", source_link.display());
        
        if let Ok(path) = source_link.canonicalize() {
            eprintln!("[VERIFY] ✓ Source symlink points to: {}", path.display());
            
            // Check for kernel.h (key header file)
            if path.join("include/linux/kernel.h").exists() {
                eprintln!("[VERIFY] ✓ include/linux/kernel.h found in source target");
                return Ok(true);
            }
            eprintln!("[VERIFY] ⚠ Source symlink points to valid path but kernel.h not found");
            return Ok(false);
        }
    }
    
    // Try /lib/modules
    let source_link = Path::new("/lib/modules").join(kernel_version).join("source");
    if source_link.exists() {
        eprintln!("[VERIFY] ✓ Source symlink exists: {}", source_link.display());
        
        if let Ok(path) = source_link.canonicalize() {
            eprintln!("[VERIFY] ✓ Source symlink points to: {}", path.display());
            if path.join("include/linux/kernel.h").exists() {
                eprintln!("[VERIFY] ✓ include/linux/kernel.h found in source target");
                return Ok(true);
            }
            eprintln!("[VERIFY] ⚠ Source symlink points to valid path but kernel.h not found");
            return Ok(false);
        }
    }
    
    eprintln!("[VERIFY] ✗ Source symlink not found");
    Ok(false)
}

/// Verify kernel headers package was installed
///
/// Uses robust header discovery to locate kernel headers for the given version.
/// Handles rebranded kernels that may have headers under different names.
///
/// # Arguments
/// * `kernel_version` - Kernel version string
///
/// # Returns
/// `Ok(true)` if headers directory exists and contains key files, `Ok(false)` if missing
pub fn verify_kernel_headers_installed(kernel_version: &str) -> Result<bool, KernelInstallationError> {
    eprintln!("[VERIFY] Checking kernel headers installation for: {}", kernel_version);
    
    // Use robust discovery function to locate headers
    match discover_kernel_headers(kernel_version) {
        Some(headers_dir) => {
            // Validate the discovered path
            if headers_dir.join("include/linux/kernel.h").exists() {
                eprintln!("[VERIFY] ✓ include/linux/kernel.h found at: {}", headers_dir.display());
            } else {
                eprintln!("[VERIFY] ⚠ Headers directory exists but kernel.h not found");
                return Ok(false);
            }
            
            if headers_dir.join("Makefile").exists() {
                eprintln!("[VERIFY] ✓ Makefile found (build system present)");
                return Ok(true);
            } else {
                eprintln!("[VERIFY] ⚠ Makefile not found (incomplete headers)");
                return Ok(false);
            }
        }
        None => {
            eprintln!("[VERIFY] ✗ Kernel headers not found for version: {}", kernel_version);
            Ok(false)
        }
    }
}

/// Create fallback symlinks if kernel-install hook failed
///
/// Uses robust header discovery to locate kernel headers, then creates
/// `/usr/lib/modules/{kernel_version}/build` and
/// `/usr/lib/modules/{kernel_version}/source` symlinks pointing to the discovered headers.
///
/// Handles rebranded kernels by discovering headers that may be located at the base version path.
/// For example, a rebranded kernel `6.18.3-arch1-2-goatd-gaming` may have headers at
/// `/usr/src/linux-6.18.3` if the headers package uses the base kernel version.
///
/// This function should be called with elevated privileges (via pkexec).
///
/// # Arguments
/// * `kernel_version` - Kernel version string
///
/// # Returns
/// `Ok(())` if symlinks created successfully, `Err` if operation failed

pub fn create_kernel_symlinks_fallback(kernel_version: &str) -> Result<(), KernelInstallationError> {
    eprintln!("[VERIFY] Creating fallback symlinks for: {}", kernel_version);
    
    let module_dir = Path::new("/usr/lib/modules").join(kernel_version);
    
    // Verify module directory exists first
    if !module_dir.exists() {
        return Err(KernelInstallationError::ModuleDirectoryMissing(
            format!("Module directory does not exist: {}. Cannot create symlinks.", module_dir.display())
        ));
    }
    
    eprintln!("[VERIFY] Module directory verified: {}", module_dir.display());
    
    // Use robust discovery to find actual headers location
    let headers_dir = match discover_kernel_headers(kernel_version) {
        Some(path) => {
            eprintln!("[VERIFY] Discovered kernel headers at: {}", path.display());
            path
        }
        None => {
            return Err(KernelInstallationError::HeadersNotInstalled(
                format!("Could not discover kernel headers for version: {}", kernel_version)
            ));
        }
    };
    
    // Create build symlink
    let build_link = module_dir.join("build");
    if !build_link.exists() || build_link.is_symlink() {
        eprintln!("[VERIFY] Creating build symlink: {} -> {}", build_link.display(), headers_dir.display());
        
        // Remove existing broken symlink if present
        if build_link.is_symlink() || build_link.exists() {
            match fs::remove_file(&build_link) {
                Ok(()) => eprintln!("[VERIFY] Removed existing/broken build symlink"),
                Err(e) => eprintln!("[VERIFY] Warning: Failed to remove build symlink: {}", e),
            }
        }
        
        // Create symlink using std::os::unix::fs
        #[cfg(unix)]
        {
            use std::os::unix::fs as unix_fs;
            match unix_fs::symlink(&headers_dir, &build_link) {
                Ok(()) => {
                    eprintln!("[VERIFY] ✓ Build symlink created successfully");
                }
                Err(e) => {
                    return Err(KernelInstallationError::SymlinkCreationFailed(
                        format!("Failed to create build symlink: {}", e)
                    ));
                }
            }
        }
    } else {
        eprintln!("[VERIFY] ✓ Build symlink already valid");
    }
    
    // Create source symlink
    let source_link = module_dir.join("source");
    if !source_link.exists() || source_link.is_symlink() {
        eprintln!("[VERIFY] Creating source symlink: {} -> {}", source_link.display(), headers_dir.display());
        
        // Remove existing broken symlink if present
        if source_link.is_symlink() || source_link.exists() {
            match fs::remove_file(&source_link) {
                Ok(()) => eprintln!("[VERIFY] Removed existing/broken source symlink"),
                Err(e) => eprintln!("[VERIFY] Warning: Failed to remove source symlink: {}", e),
            }
        }
        
        // Create symlink using std::os::unix::fs
        #[cfg(unix)]
        {
            use std::os::unix::fs as unix_fs;
            match unix_fs::symlink(&headers_dir, &source_link) {
                Ok(()) => {
                    eprintln!("[VERIFY] ✓ Source symlink created successfully");
                }
                Err(e) => {
                    return Err(KernelInstallationError::SymlinkCreationFailed(
                        format!("Failed to create source symlink: {}", e)
                    ));
                }
            }
        }
    } else {
        eprintln!("[VERIFY] ✓ Source symlink already valid");
    }
    
    eprintln!("[VERIFY] Done: Fallback symlinks created/verified/repaired");
    Ok(())
}

/// Read MPL (Metadata Persistence Layer) and extract kernel release
///
/// The MPL file (.goatd_metadata) is the definitive source of truth for kernel version.
/// This function reads the MPL from the workspace root and extracts the GOATD_KERNELRELEASE.
///
/// # Arguments
/// * `workspace_root` - Path to the workspace root directory
///
/// # Returns
/// `Some(kernel_release)` if MPL exists and contains valid kernelrelease, `None` otherwise
pub fn read_mpl_kernelrelease(workspace_root: &Path) -> Option<String> {
    let mpl_path = workspace_root.join(".goatd_metadata");
    
    eprintln!("[VERIFY] [MPL] Reading MPL from: {}", mpl_path.display());
    
    if !mpl_path.exists() {
        eprintln!("[VERIFY] [MPL] ⚠ MPL file not found at: {}", mpl_path.display());
        return None;
    }
    
    match fs::read_to_string(&mpl_path) {
        Ok(content) => {
            // Parse shell-format MPL to extract GOATD_KERNELRELEASE
            for line in content.lines() {
                if line.starts_with("GOATD_KERNELRELEASE=") {
                    if let Some(eq_pos) = line.find('=') {
                        let value = &line[eq_pos + 1..];
                        let kernelrelease = value.trim_matches('"').to_string();
                        if !kernelrelease.is_empty() {
                            eprintln!("[VERIFY] [MPL] ✓ Found GOATD_KERNELRELEASE: {}", kernelrelease);
                            return Some(kernelrelease);
                        }
                    }
                }
            }
            eprintln!("[VERIFY] [MPL] ⚠ MPL file exists but GOATD_KERNELRELEASE not found");
            None
        }
        Err(e) => {
            eprintln!("[VERIFY] [MPL] ✗ Failed to read MPL file: {}", e);
            None
        }
    }
}

/// Verify kernel installation against MPL source of truth
///
/// Uses the MPL (Metadata Persistence Layer) as the definitive source of truth
/// for kernel version matching. This provides robust verification across build sessions.
///
/// # Arguments
/// * `installed_version` - Kernel version string installed on system
/// * `workspace_root` - Path to the workspace root (where .goatd_metadata is located)
///
/// # Returns
/// `Ok(true)` if installed version matches MPL kernelrelease, `Ok(false)` if mismatch or no MPL
pub fn verify_kernel_against_mpl(installed_version: &str, workspace_root: &Path) -> Result<bool, KernelInstallationError> {
    eprintln!("[VERIFY] [MPL] Verifying kernel against MPL source of truth");
    eprintln!("[VERIFY] [MPL] Installed version: {}", installed_version);
    
    match read_mpl_kernelrelease(workspace_root) {
        Some(expected_version) => {
            eprintln!("[VERIFY] [MPL] Expected version from MPL: {}", expected_version);
            
            if installed_version == expected_version {
                eprintln!("[VERIFY] [MPL] ✓ MATCH: Installed kernel matches MPL metadata");
                Ok(true)
            } else {
                eprintln!("[VERIFY] [MPL] ✗ MISMATCH: Installed '{}' != Expected '{}'", installed_version, expected_version);
                Ok(false)
            }
        }
        None => {
            eprintln!("[VERIFY] [MPL] ⚠ No valid MPL found, cannot perform MPL verification");
            Ok(false)
        }
    }
}

/// Master verification function: comprehensive kernel installation check
///
/// Performs all verification checks and determines DKMS readiness.
/// Reports detailed status for each component.
///
/// # Arguments
/// * `kernel_version` - Kernel version string
///
/// # Returns
/// `Ok(KernelInstallationStatus)` with detailed status info, or `Err` if verification encounters errors
pub fn verify_kernel_installation(kernel_version: &str) -> Result<KernelInstallationStatus, KernelInstallationError> {
    eprintln!("[VERIFY] ===== STARTING COMPREHENSIVE KERNEL VERIFICATION =====");
    eprintln!("[VERIFY] Kernel version: {}", kernel_version);
    
    // Run all checks
    let module_dir_exists = verify_kernel_module_directory(kernel_version)?;
    eprintln!("[VERIFY] Module directory: {}", if module_dir_exists { "✓" } else { "✗" });
    
    let build_symlink_exists = verify_build_symlink(kernel_version)?;
    eprintln!("[VERIFY] Build symlink: {}", if build_symlink_exists { "✓" } else { "✗" });
    
    let source_symlink_exists = verify_source_symlink(kernel_version)?;
    eprintln!("[VERIFY] Source symlink: {}", if source_symlink_exists { "✓" } else { "✗" });
    
    let headers_installed = verify_kernel_headers_installed(kernel_version)?;
    eprintln!("[VERIFY] Headers installed: {}", if headers_installed { "✓" } else { "✗" });
    
    // Determine DKMS readiness: all components must be present
    let ready_for_dkms = module_dir_exists && build_symlink_exists && source_symlink_exists && headers_installed;
    
    eprintln!("[VERIFY] DKMS readiness: {}", if ready_for_dkms { "✓ READY" } else { "✗ NOT READY" });
    eprintln!("[VERIFY] ===== VERIFICATION COMPLETE =====");
    
    Ok(KernelInstallationStatus {
        kernel_version: kernel_version.to_string(),
        module_dir_exists,
        build_symlink_exists,
        source_symlink_exists,
        headers_installed,
        ready_for_dkms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_installation_status_creation() {
        let status = KernelInstallationStatus {
            kernel_version: "6.18.3-arch1-2".to_string(),
            module_dir_exists: true,
            build_symlink_exists: true,
            source_symlink_exists: true,
            headers_installed: true,
            ready_for_dkms: true,
        };
        
        assert_eq!(status.kernel_version, "6.18.3-arch1-2");
        assert!(status.ready_for_dkms);
    }

    #[test]
    fn test_error_display() {
        let err = KernelInstallationError::HeadersNotInstalled(
            "Headers not found at /usr/src/linux-6.18.3".to_string()
        );
        let display = format!("{}", err);
        assert!(display.contains("Headers not installed"));
    }
}

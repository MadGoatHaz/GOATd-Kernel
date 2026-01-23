/// System module: security-validated command execution, input validation

pub mod scx;
pub mod performance;
pub mod health;
pub mod verification;

use std::path::Path;
use regex::Regex;
use std::process::Command;

/// Purify PATH environment variable for toolchain enforcement
///
/// Removes directories containing gcc, llvm, or clang to prevent compiler interference.
/// Rebuilds PATH with safe, blessed locations first.
///
/// # Arguments
/// * `llvm_bin_override` - Optional LLVM bin directory to prioritize (e.g., .llvm_bin)
///
/// # Returns
/// Purified PATH string safe for kernel compilation
pub fn purify_path(llvm_bin_override: Option<&Path>) -> String {
    let mut safe_paths = Vec::new();
    
    // Add LLVM override directory if provided
    if let Some(llvm_dir) = llvm_bin_override {
        safe_paths.push(llvm_dir.to_string_lossy().to_string());
    }
    
    // Add standard safe locations
    safe_paths.push("/usr/bin".to_string());
    safe_paths.push("/bin".to_string());
    
    // Get current PATH and filter out problematic directories
    let current_path = std::env::var("PATH").unwrap_or_default();
    let filtered_path: Vec<&str> = current_path
        .split(':')
        .filter(|p| {
            !p.contains("gcc") && !p.contains("llvm") && !p.contains("clang") && !p.is_empty()
        })
        .collect();
    
    // Combine safe paths with filtered system PATH
    let new_path = format!("{}{}{}",
        safe_paths.join(":"),
        if filtered_path.is_empty() { "" } else { ":" },
        filtered_path.join(":")
    );
    
    eprintln!("[System] [PATH-PURIFY] Constructed purified PATH ({} entries)", safe_paths.len());
    new_path
}

/// Initialize logging infrastructure (now delegated to LogCollector)
/// 
/// This is a no-op in the new unified logging system.
/// LogCollector::new() in main.rs handles all initialization.
pub fn initialize_logging() {
    eprintln!("[System] initialize_logging() called (delegated to LogCollector in main.rs)");
}

/// Flush all pending logs to disk (now delegated to LogCollector)
/// 
/// This is a no-op in the new unified logging system.
/// LogCollector::wait_for_empty() in main.rs shutdown handles this.
pub fn flush_all_logs() {
    eprintln!("[System] flush_all_logs() called (delegated to LogCollector in main.rs)");
}

/// Logging macros for convenient access
/// Now use the log crate directly for target-aware routing
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        log::info!("{}", msg);
    }}
}

#[macro_export]
macro_rules! log_parsed {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        // Use target="parsed" for high-level events
        log::info!(target: "parsed", "{}", msg);
    }}
}

use crate::ui::SystemWrapper;

/// Default production implementation of SystemWrapper
///
/// This is the concrete implementation used in production. It wraps pacman
/// with security-validated input handling and comprehensive error reporting.
///
/// # Initialization
///
/// ```ignore
/// let system = SystemImpl::new()?;
/// system.install_package(PathBuf::from("/path/to/linux.pkg.tar.zst"))?;
/// ```
pub struct SystemImpl;

impl SystemImpl {
    /// Create a new SystemImpl instance
    ///
    /// Currently always succeeds, but structured for future initialization logic.
    pub fn new() -> Result<Self, String> {
        Ok(SystemImpl)
    }
    
    // Note: ensure_sudo_session() removed. Each privileged command is now wrapped
    // directly with pkexec, ensuring PolicyKit handles authentication in a single
    // GUI-driven flow without intermediate terminal prompts.
}

/// Check if DKMS is already running (race condition prevention)
///
/// Scans process list for active dkms processes to prevent concurrent
/// DKMS invocations that could corrupt the module build state.
///
/// # Returns
/// `true` if any dkms process is currently running, `false` otherwise
fn is_dkms_running() -> bool {
    // Simple check: look for dkms in running processes
    match Command::new("pgrep")
        .arg("-f")
        .arg("dkms")
        .output()
    {
        Ok(output) => {
            // pgrep returns exit code 0 if process is found
            output.status.success()
        }
        Err(_) => {
            // If pgrep fails, assume it's safe to proceed
            eprintln!("[DKMS] [RACE] Warning: Could not check for running DKMS processes");
            false
        }
    }
}

/// Input Validation Contract (Phase 0 Requirements 1.1 & 1.2)
///
/// All operations validate inputs before OS command execution.
/// Prevents shell injection, symlink attacks, and TOCTOU vulnerabilities.
impl SystemWrapper for SystemImpl {
    fn uninstall_package(&self, pkg_name: &str) -> Result<(), String> {
        // VALIDATE: Package name must be alphanumeric with hyphens/underscores only
        // Regex: ^[a-z0-9\-_]+$
        if let Ok(re) = Regex::new(r"^[a-z0-9\-_]+$") {
            if !re.is_match(pkg_name) {
                return Err(format!(
                    "Package name contains invalid characters: {}. Only lowercase alphanumeric, hyphens, and underscores allowed.",
                    pkg_name
                ));
            }
        } else {
            return Err("Failed to compile validation regex".to_string());
        }
        
        // SAFE: Pass package name as separate argument, never interpolated into string
        // Use pkexec to wrap the entire command for GUI-driven authentication
        match Command::new("pkexec")
            .arg("pacman")
            .arg("-Rns")  // -R: remove, -n: do not save backups, -s: remove deps
            .arg("--noconfirm")
            .arg("--")  // Signals end of flags, name treated as argument
            .arg(pkg_name)  // NOT shell-interpolated
            .output()  // CAPTURE stdout and stderr instead of inheriting
        {
            Ok(output) => {
                // Capture and log stdout
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    log::info!("[pacman uninstall] stdout: {}", stdout);
                }
                
                // Capture and log stderr
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    log::info!("[pacman uninstall] stderr: {}", stderr);
                }
                
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!("pacman -Rns {} failed with status: {:?}", pkg_name, output.status.code()))
                }
            }
            Err(e) => Err(format!("Failed to execute pacman: {}", e))
        }
    }
    
    fn install_package(&self, path: std::path::PathBuf) -> Result<(), String> {
        // CANONICALIZE: Resolve symlinks and verify existence
        let absolute_path = match path.canonicalize() {
            Ok(abs_path) => abs_path,
            Err(e) => {
                return Err(format!(
                    "Failed to resolve absolute path: {}",
                    e
                ));
            }
        };
        
        // VALIDATE: Must be a .pkg.tar.zst file
        if !absolute_path.to_string_lossy().ends_with(".pkg.tar.zst") {
            return Err(
                "Package must be a .pkg.tar.zst file".to_string()
            );
        }
        
        // Log milestone: kernel installation starting
        let pkg_name = absolute_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        log_parsed!("KERNEL INSTALLATION: Starting installation of {}", pkg_name);
        
        // SAFE: Pass path as separate argument
        // Use pkexec to wrap the entire command for GUI-driven authentication
        match Command::new("pkexec")
            .arg("pacman")
            .arg("-U")
            .arg("--noconfirm")
            .arg(&absolute_path)
            .output()  // CAPTURE stdout and stderr instead of inheriting
        {
            Ok(output) => {
                // Capture and log stdout
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    log::info!("[pacman install] stdout: {}", stdout);
                }
                
                // Capture and log stderr
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    log::info!("[pacman install] stderr: {}", stderr);
                }
                
                if output.status.success() {
                    log_parsed!("KERNEL INSTALLATION: Successfully installed {}", pkg_name);
                    Ok(())
                } else {
                    log_parsed!("KERNEL INSTALLATION: Failed to install {} with status: {:?}", pkg_name, output.status.code());
                    Err(format!("pacman -U failed with status: {:?}", output.status.code()))
                }
            }
            Err(e) => {
                log_parsed!("KERNEL INSTALLATION: Error executing pacman: {}", e);
                Err(format!("Failed to execute pacman: {}", e))
            }
        }
    }
    
    fn get_booted_kernel(&self) -> String {
        match Command::new("uname")
            .args(&["-r"])
            .output()
        {
            Ok(out) => {
                if out.status.success() {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                } else {
                    "unknown".to_string()
                }
            }
            Err(e) => {
                log_info!("[SystemWrapper] Failed to get booted kernel: {}", e);
                "unknown".to_string()
            }
        }
    }

    fn install_nvidia_drivers_dkms(&self, kernel_version: &str) -> Result<(), String> {
        // VALIDATE: Kernel version must be alphanumeric with hyphens, dots, and underscores
        if let Ok(re) = Regex::new(r"^[a-zA-Z0-9\.\-_]+$") {
            if !re.is_match(kernel_version) {
                return Err(format!(
                    "Invalid kernel version format: {}. Only alphanumeric, dots, hyphens, and underscores allowed.",
                    kernel_version
                ));
            }
        } else {
            return Err("Failed to compile validation regex".to_string());
        }

        log_info!("[SystemWrapper] Attempting DKMS install for kernel version: {}", kernel_version);

        // RACE CONDITION PREVENTION: Check if DKMS is already running
        if is_dkms_running() {
            let error_msg = format!(
                "DKMS is already running. Wait for the current build to complete before starting another. \
                 Multiple concurrent DKMS sessions can corrupt the module state."
            );
            eprintln!("[DKMS] [RACE] {}", error_msg);
            log_info!("[SystemWrapper] DKMS race condition detected: {}", error_msg);
            return Err(error_msg);
        }
        eprintln!("[DKMS] [RACE] ✓ No concurrent DKMS processes detected");

        // PRE-FLIGHT CHECK: Verify kernel headers are installed before DKMS build
        // DKMS requires build, source symlinks and actual headers to be present
        eprintln!("[DKMS] [GUARD] Running pre-flight kernel header verification for: {}", kernel_version);
        
        match verification::verify_kernel_installation(kernel_version) {
            Ok(status) => {
                if !status.ready_for_dkms {
                    // Provide detailed diagnostic of what's missing
                    let missing = vec![
                        if !status.module_dir_exists { "module directory" } else { "" },
                        if !status.build_symlink_exists { "build symlink" } else { "" },
                        if !status.source_symlink_exists { "source symlink" } else { "" },
                        if !status.headers_installed { "kernel headers" } else { "" },
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                    
                    let error_msg = format!(
                        "DKMS pre-flight check failed for kernel {}: missing or invalid [{}]. \
                         DKMS cannot build without valid kernel headers and module directory. \
                         Install kernel headers package first.",
                        kernel_version, missing
                    );
                    eprintln!("[DKMS] [ERROR] {}", error_msg);
                    log_info!("[SystemWrapper] DKMS guard check failed: {}", error_msg);
                    return Err(error_msg);
                }
                eprintln!("[DKMS] [GUARD] ✓ Kernel headers verified and ready for DKMS");
            }
            Err(e) => {
                let error_msg = format!(
                    "DKMS pre-flight verification error for kernel {}: {}. \
                     Cannot proceed with DKMS build.",
                    kernel_version, e
                );
                eprintln!("[DKMS] [ERROR] {}", error_msg);
                log_info!("[SystemWrapper] DKMS guard check error: {}", error_msg);
                return Err(error_msg);
            }
        }

        // DKMS allows building kernel modules for a specific kernel version
        // Command: dkms autoinstall -k <kernel_version>
        // Use pkexec to wrap the entire command for GUI-driven authentication
        // Configure with LLVM=1 and LLVM_IAS=1 for CLANG-based kernel builds
        eprintln!("[DKMS] Building with full LLVM/Clang toolchain: LLVM=1 LLVM_IAS=1 CC=clang LD=ld.lld");
        match Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(format!("LLVM=1 LLVM_IAS=1 CC=clang LD=ld.lld dkms autoinstall -k {}", kernel_version))
            .output()  // CAPTURE stdout and stderr instead of inheriting
        {
            Ok(output) => {
                // Capture and log stdout
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    log::info!("[dkms autoinstall] stdout: {}", stdout);
                }
                
                // Capture and log stderr
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    log::info!("[dkms autoinstall] stderr: {}", stderr);
                }
                
                if output.status.success() {
                    log_info!("[SystemWrapper] DKMS autoinstall succeeded for kernel {}", kernel_version);
                    Ok(())
                } else {
                    let err_msg = format!("DKMS autoinstall failed for kernel {} with status: {:?}", kernel_version, output.status.code());
                    log_info!("[SystemWrapper] {}", err_msg);
                    Err(err_msg)
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to execute DKMS: {}", e);
                log_info!("[SystemWrapper] {}", err_msg);
                Err(err_msg)
            }
        }
    }
    
    /// Execute multiple privileged commands in sequence
    ///
    /// Joins commands with ` && ` and executes them via `pkexec sh -c`.
    /// All commands must succeed for the batch to succeed.
    ///
    /// # Important
    /// This method should NOT be used for user-level operations like GPG key imports.
    /// Use `batch_user_commands` instead for operations that should run with current user privileges.
    ///
    /// # Arguments
    /// * `commands` - Vector of command strings to execute in sequence
    ///
    /// # Returns
    /// `Ok(stdout)` if all commands succeed with captured stdout, otherwise `Err` with error message
    fn batch_privileged_commands(&self, commands: Vec<&str>) -> Result<String, String> {
        if commands.is_empty() {
            return Err("No commands provided for batch execution".to_string());
        }

        // Chain commands with && so they all must succeed
        let combined = commands.join(" && ");
        
        log_info!("[SystemWrapper] Executing batch privileged commands ({} steps)", commands.len());

        match Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg(&combined)
            .output()  // CAPTURE stdout and stderr instead of inheriting
        {
            Ok(output) => {
                // Capture and log stdout
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if !stdout.is_empty() {
                    log::info!("[batch privileged] stdout: {}", stdout);
                }
                
                // Capture and log stderr
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    log::info!("[batch privileged] stderr: {}", stderr);
                }
                
                if output.status.success() {
                    log_info!("[SystemWrapper] Batch execution succeeded");
                    Ok(stdout)
                } else {
                    let err_msg = format!("Batch privileged execution failed with status: {:?}", output.status.code());
                    log_info!("[SystemWrapper] {}", err_msg);
                    Err(err_msg)
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to execute batch privileged commands: {}", e);
                log_info!("[SystemWrapper] {}", err_msg);
                Err(err_msg)
            }
        }
    }

    /// Execute multiple commands as the current user (without privileges)
    ///
    /// Joins commands with ` && ` and executes them via `sh -c` as the current user.
    /// All commands must succeed for the batch to succeed.
    ///
    /// # Use Cases
    /// Intended for user-level operations that should NOT run with elevated privileges,
    /// such as GPG key imports, git operations, or other user-specific tasks.
    ///
    /// # Arguments
    /// * `commands` - Vector of command strings to execute in sequence
    ///
    /// # Returns
    /// `Ok(())` if all commands succeed, otherwise `Err` with error message containing
    /// exit code or detailed error information
    fn batch_user_commands(&self, commands: Vec<&str>) -> Result<(), String> {
        if commands.is_empty() {
            return Err("No commands provided for batch execution".to_string());
        }

        // Chain commands with && so they all must succeed
        let combined = commands.join(" && ");
        
        log_info!("[SystemWrapper] Executing batch user commands ({} steps)", commands.len());

        match Command::new("sh")
            .arg("-c")
            .arg(&combined)
            .output()  // CAPTURE stdout and stderr instead of inheriting
        {
            Ok(output) => {
                // Capture and log stdout
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    log::info!("[batch user] stdout: {}", stdout);
                }
                
                // Capture and log stderr
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    log::info!("[batch user] stderr: {}", stderr);
                }
                
                if output.status.success() {
                    log_info!("[SystemWrapper] Batch user execution succeeded");
                    Ok(())
                } else {
                    let err_msg = format!("Batch user execution failed with status: {:?}", output.status.code());
                    log_info!("[SystemWrapper] {}", err_msg);
                    Err(err_msg)
                }
            }
            Err(e) => {
                let err_msg = format!("Failed to execute batch user commands: {}", e);
                log_info!("[SystemWrapper] {}", err_msg);
                Err(err_msg)
            }
        }
    }

   /// Ensure DKMS Safety Net for GOATd kernels
   ///
   /// This method creates a global DKMS framework configuration file (`/etc/dkms/framework.conf.d/goatd.conf`)
   /// that enforces the LLVM/Clang toolchain for all GOATd kernel module builds.
   ///
   /// # Purpose
   /// When DKMS runs out-of-tree module builds (e.g., nvidia-dkms, virtualbox-dkms) for GOATd kernels,
   /// it must use the same LLVM/Clang toolchain that built the kernel. This safety net configuration
   /// ensures cross-compilation consistency by:
   /// - Detecting GOATd kernels (version strings containing "-goatd-")
   /// - Enforcing LLVM=1, LLVM_IAS=1, CC=clang, LD=ld.lld environment variables
   /// - Preventing compilation errors from mismatched toolchains
   ///
   /// # Execution Context
   /// This is called as part of the unified batch_privileged_commands flow in AppController::install_kernel_async(),
   /// bundled with kernel installation and DKMS autoinstall to minimize Polkit authentication prompts (1 prompt total).
   ///
   /// # Returns
   /// `Ok(())` if configuration was successfully created/updated, or `Err` with diagnostic message
   fn ensure_dkms_safety_net(&self) -> Result<(), String> {
       eprintln!("[DKMS] [SAFETY-NET] Creating global DKMS framework configuration for GOATd kernels");

       // The DKMS framework.conf.d configuration content
       // This will be sourced by DKMS and apply to all GOATd kernel module builds
       // Use raw string to preserve content literally
       let dkms_config_content = r#"# GOATd Kernel DKMS Configuration
# This configuration file ensures that DKMS module builds use the LLVM/Clang toolchain
# for all GOATd rebranded kernels, even when invoked outside the normal kernel build flow.
#
# DKMS will source this file from /etc/dkms/framework.conf.d/goatd.conf
# and apply these settings to all out-of-tree module builds.

# Target GOATd kernels (match version strings containing "-goatd-")
# Handles formats: 6.19.0-rc6-goatd, 6.19.0-rc6-goatd-gaming, etc.
if [[ "$kernelver" == *"-goatd-"* ]]; then
   # ======================================================================
   # DKMS TOOLCHAIN ENFORCEMENT FOR GOATD KERNELS
   # ======================================================================
   # CRITICAL: These variables must be set for DKMS to use LLVM/Clang
   # when building kernel modules (nvidia-dkms, etc.)
   
   # Force LLVM compiler (version 19+ if available)
   export LLVM=1
   export LLVM_IAS=1
   
   # Force Clang as the primary C compiler (DKMS uses FORCE_CC/FORCE_CXX for module builds)
   # Setting both CC and FORCE_CC ensures compatibility with different DKMS versions
   export CC=clang
   export FORCE_CC=clang
   export CXX=clang++
   export FORCE_CXX=clang++
   
   # Force LLVM linker
   export LD=ld.lld
   
   # Additional LLVM toolchain tools
   export AR=llvm-ar
   export NM=llvm-nm
   export OBJCOPY=llvm-objcopy
   export STRIP=llvm-strip
   
   # Log enforcement message to stderr for diagnostics
   printf "[DKMS-SAFETY-NET] GOATd kernel detected: %s\n" "$kernelver" >&2
   printf "[DKMS-SAFETY-NET] ✓ LLVM/Clang toolchain enforced (CC=clang, LD=ld.lld, LLVM=1)\n" >&2
fi
"#;

       // ====================================================================
       // CRITICAL: Shell Escaping Strategy for DKMS Configuration
       // ====================================================================
       // The configuration content includes shell variable references like $kernelver
       // that MUST be interpreted by bash when the config file is sourced by DKMS,
       // NOT when printf creates the file.
       //
       // Strategy:
       // 1. Replace single quotes in content with '\'' (shell escape sequence)
       // 2. Wrap entire printf argument in single quotes to prevent all expansions
       // 3. This ensures $kernelver is written literally, then expanded by sourcing script
       //
       // Example: 'content with $var' becomes 'content with $var'
       //          and bash interpreter expands $var when file is sourced
       // ====================================================================
       let escaped_content = dkms_config_content.replace("'", "'\\''");
       
       // Build printf command with escaped content wrapped in single quotes
       // This prevents shell interpretation during file creation while preserving
       // the $kernelver variable so DKMS can expand it when sourcing the config
       let config_write_cmd = format!("printf '%s' '{}' > /etc/dkms/framework.conf.d/goatd.conf", escaped_content);

       // Commands to create the configuration (all &str references have stable lifetimes)
       let commands = vec![
           // Step 1: Create the DKMS framework.conf.d directory if it doesn't exist
           "mkdir -p /etc/dkms/framework.conf.d",
           // Step 2: Write the GOATd configuration to framework.conf.d using printf
           //         Uses single-quote escaping to preserve $kernelver for later expansion
           config_write_cmd.as_str(),
           // Step 3: Set proper permissions (readable by all, writable by root only)
           "chmod 644 /etc/dkms/framework.conf.d/goatd.conf",
       ];

       eprintln!("[DKMS] [SAFETY-NET] Creating /etc/dkms/framework.conf.d/goatd.conf with toolchain enforcement");

       // Execute the configuration creation via batch_privileged_commands (chains with &&)
       match self.batch_privileged_commands(commands) {
           Ok(_stdout) => {
               eprintln!("[DKMS] [SAFETY-NET] ✓ SUCCESS: DKMS safety net configuration created");
               log_info!("[DKMS] [SAFETY-NET] Successfully created global DKMS framework configuration for GOATd kernels");
               Ok(())
           }
           Err(e) => {
               eprintln!("[DKMS] [SAFETY-NET] ✗ FAILED: {}", e);
               log_info!("[DKMS] [SAFETY-NET] Failed to create DKMS safety net: {}", e);
               Err(format!("Failed to create DKMS safety net configuration: {}", e))
           }
       }
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_name_validation_valid() {
        let re = Regex::new(r"^[a-z0-9\-_]+$").unwrap();
        assert!(re.is_match("linux"));
        assert!(re.is_match("linux-zen"));
        assert!(re.is_match("linux_firmware"));
        assert!(re.is_match("linux-lts-6-1"));
    }

    #[test]
    fn test_package_name_validation_invalid() {
        let re = Regex::new(r"^[a-z0-9\-_]+$").unwrap();
        assert!(!re.is_match("Linux")); // uppercase
        assert!(!re.is_match("linux; rm -rf")); // shell injection
        assert!(!re.is_match("linux$(whoami)")); // command substitution
    }
}

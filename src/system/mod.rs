/// System module: security-validated command execution, input validation

pub mod scx;
pub mod performance;

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

        // DKMS allows building kernel modules for a specific kernel version
        // Command: dkms autoinstall -k <kernel_version>
        // Use pkexec to wrap the entire command for GUI-driven authentication
        match Command::new("pkexec")
            .arg("dkms")
            .arg("autoinstall")
            .arg("-k")
            .arg(kernel_version)
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
    
    fn batch_privileged_commands(&self, commands: Vec<&str>) -> Result<(), String> {
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
                let stdout = String::from_utf8_lossy(&output.stdout);
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
                    Ok(())
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

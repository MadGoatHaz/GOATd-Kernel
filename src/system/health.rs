/// System Health Manager
///
/// Silent system health checks without terminal prompts.
/// Reports missing dependencies, GPG keys, and optional tools.

use std::process::Command;
use serde::{Deserialize, Serialize};

/// Package repository type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageType {
    /// Official Arch repository package
    Official,
    /// AUR (Arch User Repository) package
    AUR,
}

/// Health status levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// All critical dependencies and optional tools installed
    Excellent,
    /// All critical dependencies installed, some optional tools missing
    Good,
    /// Critical dependencies installed but GPG keys missing
    Incomplete,
    /// Critical dependencies missing
    Poor,
}

impl HealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthStatus::Excellent => "Excellent",
            HealthStatus::Good => "Good",
            HealthStatus::Incomplete => "Incomplete",
            HealthStatus::Poor => "Poor",
        }
    }

    pub fn needs_fix(&self) -> bool {
        matches!(self, HealthStatus::Poor | HealthStatus::Incomplete)
    }
}

/// System health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub missing_official_packages: Vec<String>,
    pub missing_aur_packages: Vec<String>,
    pub missing_gpg_keys: Vec<String>,
    pub missing_optional_tools: Vec<String>,
    pub message: String,
}

impl Default for HealthReport {
    fn default() -> Self {
        HealthReport {
            status: HealthStatus::Excellent,
            missing_official_packages: Vec::new(),
            missing_aur_packages: Vec::new(),
            missing_gpg_keys: Vec::new(),
            missing_optional_tools: Vec::new(),
            message: "System is healthy".to_string(),
        }
    }
}

impl HealthReport {
    /// Get all missing packages (both official and AUR)
    pub fn all_missing_packages(&self) -> Vec<String> {
        let mut all = self.missing_official_packages.clone();
        all.extend(self.missing_aur_packages.clone());
        all
    }
}

/// System Health Manager
pub struct HealthManager;

impl HealthManager {
    /// Map of known AUR packages
    fn aur_packages() -> std::collections::HashSet<&'static str> {
        let mut set = std::collections::HashSet::new();
        set.insert("modprobed-db");
        set
    }

    /// Perform a silent system health check
    pub fn check_system_health() -> HealthReport {
        let mut report = HealthReport::default();

        // Check if we're on Arch Linux
        let is_arch = Self::is_arch_system();

        if !is_arch {
            // Non-Arch systems: only check for cargo
            if !Self::command_exists("cargo") {
                report.status = HealthStatus::Poor;
                report.missing_official_packages.push("cargo (Rust)".to_string());
                report.message = "Rust toolchain (cargo) is required".to_string();
            }
            return report;
        }

        // On Arch: check packages and GPG keys
         // Includes all kernel build dependencies: bc, pahole, python, python-sphinx, texinfo, base-devel, git
         let critical_packages = vec![
             "rust", "base-devel", "git", "bc", "pahole", "python", "python-sphinx",
             "texinfo", "rust-bindgen", "rust-src",
             "graphviz", "texlive-latexextra", "llvm",
             "clang", "lld", "polly",
         ];

        let optional_tools = vec![
            "modprobed-db",
            "scx-tools",
            "scx-scheds",
        ];

        let aur_pkgs = Self::aur_packages();

        // Check critical packages
        for pkg in critical_packages {
            if !Self::is_package_installed(pkg) {
                report.missing_official_packages.push(pkg.to_string());
            }
        }

        // Check optional tools
        for tool in optional_tools {
            // For scx-tools and scx-scheds, check if packages are installed, not commands
            let is_missing = if tool == "scx-tools" || tool == "scx-scheds" {
                !Self::is_package_installed(tool)
            } else {
                !Self::command_exists(tool)
            };

            if is_missing {
                // Categorize optional tools into AUR vs Official
                if aur_pkgs.contains(tool) {
                    report.missing_aur_packages.push(tool.to_string());
                } else {
                    report.missing_optional_tools.push(tool.to_string());
                }
            }
        }

        // Check GPG keys with verification
        let gpg_keys = vec![
            ("38DBBDC86092693E", "6092693E"), // Greg Kroah-Hartman
            ("B8AC08600F108CDF", "0F108CDF"), // Jan Alexander Steffens/heftig
        ];

        for (key_id, fp_suffix) in gpg_keys {
            if !Self::is_gpg_key_imported_with_verification(key_id, fp_suffix) {
                report.missing_gpg_keys.push(key_id.to_string());
            }
        }

        // Determine health status
        if !report.missing_official_packages.is_empty() {
            report.status = HealthStatus::Poor;
            report.message = format!(
                "Missing {} critical package(s)",
                report.missing_official_packages.len()
            );
        } else if !report.missing_gpg_keys.is_empty() {
            report.status = HealthStatus::Incomplete;
            report.message = format!(
                "Missing {} GPG key(s) (needed for kernel verification)",
                report.missing_gpg_keys.len()
            );
        } else if !report.missing_aur_packages.is_empty() {
            report.status = HealthStatus::Good;
            report.message = format!(
                "System ready (AUR packages missing: {})",
                report.missing_aur_packages.join(", ")
            );
        } else if !report.missing_optional_tools.is_empty() {
            report.status = HealthStatus::Good;
            report.message = format!(
                "System ready (optional tools missing: {})",
                report.missing_optional_tools.join(", ")
            );
        } else {
            report.status = HealthStatus::Excellent;
            report.message = "All dependencies and tools installed".to_string();
        }

        report
    }

    /// Check if a command exists in PATH
    fn command_exists(cmd: &str) -> bool {
        Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {}", cmd))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if an Arch package is installed
    fn is_package_installed(pkg: &str) -> bool {
        Command::new("pacman")
            .arg("-Q")
            .arg(pkg)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if a GPG key is imported
    fn is_gpg_key_imported(key_id: &str) -> bool {
        Command::new("gpg")
            .arg("--list-keys")
            .arg(key_id)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if a GPG key is imported with fingerprint verification
    /// Uses `gpg --with-colons` to extract and verify the fingerprint suffix
    fn is_gpg_key_imported_with_verification(key_id: &str, expected_fp_suffix: &str) -> bool {
        let output = match Command::new("gpg")
            .arg("--with-colons")
            .arg("--list-keys")
            .arg(key_id)
            .output() {
            Ok(o) => o,
            Err(_) => return false,
        };

        if !output.status.success() {
            return false;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse GPG output in colon-delimited format
        // Format: fpr:::::::::FINGERPRINT:
        for line in stdout.lines() {
            if line.starts_with("fpr:") {
                // Extract fingerprint from the colon-separated fields
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 9 {
                    let fingerprint = parts[9];
                    if fingerprint.ends_with(expected_fp_suffix) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Detect if running on Arch Linux
    fn is_arch_system() -> bool {
        // Check for pacman package manager
        Command::new("sh")
            .arg("-c")
            .arg("command -v pacman")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Generate GPG setup commands for missing keys using multiple keyservers
    /// Constructs robust shell commands that try multiple keyserver sources
    pub fn generate_gpg_setup_commands(report: &HealthReport) -> Vec<String> {
        let mut commands = Vec::new();

        if !report.missing_gpg_keys.is_empty() {
            for key_id in &report.missing_gpg_keys {
                // Try multiple keyservers in sequence: Ubuntu, OpenPGP, MIT
                let cmd = format!(
                    "timeout 15 gpg --keyserver hkps://keyserver.ubuntu.com --recv-keys {} || timeout 15 gpg --keyserver hkps://keys.openpgp.org --recv-keys {} || timeout 15 gpg --keyserver hkps://pgp.mit.edu --recv-keys {}",
                    key_id, key_id, key_id
                );
                commands.push(cmd);
            }
        }

        commands
    }

    /// Generate a privileged command batch to fix environment
    /// Returns a single shell command string that can be passed to pkexec
    /// Only includes official packages and GPG keys, NOT AUR packages
    pub fn generate_fix_command(report: &HealthReport) -> String {
        let mut commands = Vec::new();

        // Install missing OFFICIAL packages and optional tools (not AUR)
        let mut all_official = report.missing_official_packages.clone();
        all_official.extend(report.missing_optional_tools.clone());
        
        if !all_official.is_empty() {
            let pkg_list = all_official.join(" ");
            commands.push(format!("pacman -S --needed --noconfirm {}", pkg_list));
        }

        // Setup GPG keys using the new command generator
        commands.extend(Self::generate_gpg_setup_commands(report));

        // Join with && so all must succeed
        if commands.is_empty() {
            "true".to_string() // No-op command
        } else {
            commands.join(" && ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_needs_fix() {
        assert!(HealthStatus::Poor.needs_fix());
        assert!(HealthStatus::Incomplete.needs_fix());
        assert!(!HealthStatus::Good.needs_fix());
        assert!(!HealthStatus::Excellent.needs_fix());
    }

    #[test]
    fn test_health_status_as_str() {
        assert_eq!(HealthStatus::Excellent.as_str(), "Excellent");
        assert_eq!(HealthStatus::Good.as_str(), "Good");
        assert_eq!(HealthStatus::Incomplete.as_str(), "Incomplete");
        assert_eq!(HealthStatus::Poor.as_str(), "Poor");
    }

    #[test]
    fn test_generate_fix_command_empty() {
        let report = HealthReport::default();
        let cmd = HealthManager::generate_fix_command(&report);
        assert_eq!(cmd, "true");
    }

    #[test]
    fn test_generate_fix_command_with_packages() {
        let mut report = HealthReport::default();
        report.missing_official_packages = vec!["rust".to_string(), "git".to_string()];
        let cmd = HealthManager::generate_fix_command(&report);
        assert!(cmd.contains("pacman -S --needed --noconfirm"));
        assert!(cmd.contains("rust"));
        assert!(cmd.contains("git"));
    }

    #[test]
    fn test_generate_fix_command_with_gpg_keys() {
        let mut report = HealthReport::default();
        report.missing_gpg_keys = vec!["38DBBDC86092693E".to_string()];
        let cmd = HealthManager::generate_fix_command(&report);
        assert!(cmd.contains("gpg"));
        assert!(cmd.contains("38DBBDC86092693E"));
    }

    #[test]
    fn test_generate_gpg_setup_commands_multiple_keyservers() {
        let mut report = HealthReport::default();
        report.missing_gpg_keys = vec!["38DBBDC86092693E".to_string()];
        let cmds = HealthManager::generate_gpg_setup_commands(&report);
        assert_eq!(cmds.len(), 1);
        let cmd = &cmds[0];
        // Verify all three keyservers are included
        assert!(cmd.contains("hkps://keyserver.ubuntu.com"));
        assert!(cmd.contains("hkps://keys.openpgp.org"));
        assert!(cmd.contains("hkps://pgp.mit.edu"));
        // Verify key ID is present
        assert!(cmd.contains("38DBBDC86092693E"));
    }

    #[test]
    fn test_generate_gpg_setup_commands_multiple_keys() {
        let mut report = HealthReport::default();
        report.missing_gpg_keys = vec![
            "38DBBDC86092693E".to_string(),
            "B8AC08600F108CDF".to_string(),
        ];
        let cmds = HealthManager::generate_gpg_setup_commands(&report);
        assert_eq!(cmds.len(), 2);
        assert!(cmds[0].contains("38DBBDC86092693E"));
        assert!(cmds[1].contains("B8AC08600F108CDF"));
    }
}

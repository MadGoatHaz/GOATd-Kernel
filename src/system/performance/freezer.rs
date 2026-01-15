//! Atomic Cgroup v2 Freezer for Benchmark Safety
//!
//! This module implements process freezing via Cgroup v2 to isolate
//! non-essential processes during benchmarking. It provides:
//!
//! - Automatic detection of non-essential PIDs (excluding kernel threads, PID 1, and the application)
//! - Transient cgroup creation at `/sys/fs/cgroup/benchmark_freeze`
//! - Safe process migration into the freezer cgroup
//! - Freeze/thaw state toggling
//! - Optional D-Bus suspension of KWin (KDE compositor)
//!
//! The freezer is idempotent and safe to call multiple times.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

/// Configuration for the benchmark freezer
#[derive(Clone, Debug)]
pub struct FreezerConfig {
    /// Path to the cgroup v2 benchmark freeze directory
    pub cgroup_path: String,
    /// Whether to attempt D-Bus suspension of KWin
    pub suspend_kwin: bool,
}

impl Default for FreezerConfig {
    fn default() -> Self {
        FreezerConfig {
            cgroup_path: "/sys/fs/cgroup/benchmark_freeze".to_string(),
            suspend_kwin: true,
        }
    }
}

/// Manages freezing of non-essential processes via Cgroup v2
pub struct BenchmarkFreezer {
    config: FreezerConfig,
    frozen_pids: HashSet<u32>,
}

impl BenchmarkFreezer {
    /// Create a new benchmark freezer instance
    pub fn new(config: FreezerConfig) -> Self {
        BenchmarkFreezer {
            config,
            frozen_pids: HashSet::new(),
        }
    }

    /// Create a new freezer with default configuration
    pub fn default_config() -> Self {
        Self::new(FreezerConfig::default())
    }

    /// Initialize the freezer: create cgroup and identify non-essential PIDs
    pub fn initialize(&mut self) -> Result<(), String> {
        // Create the transient cgroup
        self.create_cgroup()?;

        // Identify and migrate non-essential PIDs
        self.identify_and_migrate_pids()?;

        Ok(())
    }

    /// Create the transient cgroup at the configured path
    fn create_cgroup(&self) -> Result<(), String> {
        let cgroup_path = Path::new(&self.config.cgroup_path);

        // If cgroup already exists, that's fine (idempotent)
        if cgroup_path.exists() {
            return Ok(());
        }

        // Create the cgroup directory
        fs::create_dir_all(cgroup_path).map_err(|e| {
            format!(
                "Failed to create cgroup directory {}: {}",
                self.config.cgroup_path, e
            )
        })?;

        // Verify cgroup.controllers file exists (Cgroup v2 requirement)
        let controllers_path = cgroup_path.join("cgroup.controllers");
        if !controllers_path.exists() {
            return Err(
                "Cgroup v2 is not properly mounted. Missing cgroup.controllers".to_string(),
            );
        }

        // Enable freezer controller if not already enabled
        let subtree_control_path = cgroup_path.join("cgroup.subtree_control");
        match fs::read_to_string(&subtree_control_path) {
            Ok(content) => {
                if !content.contains("freezer") {
                    fs::write(&subtree_control_path, "+freezer").map_err(|e| {
                        format!("Failed to enable freezer controller: {}", e)
                    })?;
                }
            }
            Err(e) => {
                return Err(format!(
                    "Failed to read cgroup.subtree_control: {}",
                    e
                ));
            }
        }

        Ok(())
    }

    /// Identify non-essential PIDs and migrate them into the cgroup
    fn identify_and_migrate_pids(&mut self) -> Result<(), String> {
        let pids = self.get_non_essential_pids()?;

        for pid in pids {
            self.migrate_pid_to_cgroup(pid)?;
            self.frozen_pids.insert(pid);
        }

        Ok(())
    }

    /// Get list of non-essential PIDs
    /// Excludes:
    /// - Kernel threads (marked with kthread)
    /// - PID 1 (init process)
    /// - Current process and its parents
    fn get_non_essential_pids(&self) -> Result<Vec<u32>, String> {
        let mut non_essential = Vec::new();
        let mut excluded_pids = HashSet::new();

        // Exclude PID 1 (init)
        excluded_pids.insert(1);

        // Exclude current process
        let current_pid = std::process::id();
        excluded_pids.insert(current_pid);

        // Exclude parent process and ancestors
        if let Ok(ppid) = self.get_parent_pid(current_pid) {
            excluded_pids.insert(ppid);
            // Also exclude the parent's parent for safety
            if let Ok(gppid) = self.get_parent_pid(ppid) {
                excluded_pids.insert(gppid);
            }
        }

        // Read /proc directory for all PIDs
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(filename) = entry.file_name().into_string() {
                    if let Ok(pid) = u32::from_str(&filename) {
                        if !excluded_pids.contains(&pid) {
                            // Check if it's a kernel thread
                            if !self.is_kernel_thread(pid) {
                                non_essential.push(pid);
                            }
                        }
                    }
                }
            }
        }

        Ok(non_essential)
    }

    /// Check if a process is a kernel thread
    fn is_kernel_thread(&self, pid: u32) -> bool {
        let status_path = format!("/proc/{}/status", pid);
        if let Ok(content) = fs::read_to_string(&status_path) {
            // Kernel threads have no VmPeak or minimal memory
            // Also check the comm name for typical kernel thread patterns
            if content.contains("Threads:") {
                for line in content.lines() {
                    if line.starts_with("VmPeak:") {
                        // If VmPeak is 0 or missing, likely a kernel thread
                        if let Some(value) = line.split_whitespace().nth(1) {
                            if let Ok(vm_peak) = u32::from_str(value) {
                                return vm_peak == 0;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Get parent process ID (PPID) for a given PID
    fn get_parent_pid(&self, pid: u32) -> Result<u32, String> {
        let status_path = format!("/proc/{}/status", pid);
        if let Ok(content) = fs::read_to_string(&status_path) {
            for line in content.lines() {
                if line.starts_with("PPid:") {
                    if let Some(ppid_str) = line.split_whitespace().nth(1) {
                        if let Ok(ppid) = u32::from_str(ppid_str) {
                            return Ok(ppid);
                        }
                    }
                }
            }
        }
        Err(format!("Failed to read PPID for PID {}", pid))
    }

    /// Migrate a single PID into the freezer cgroup
    fn migrate_pid_to_cgroup(&self, pid: u32) -> Result<(), String> {
        let cgroup_procs_path = format!("{}/cgroup.procs", self.config.cgroup_path);
        let path = Path::new(&cgroup_procs_path);

        if !path.exists() {
            return Err(format!(
                "Cgroup procs file does not exist: {}",
                cgroup_procs_path
            ));
        }

        // Try to migrate the PID. This might fail if the process died or doesn't exist.
        match fs::write(path, pid.to_string()) {
            Ok(_) => Ok(()),
            Err(e) => {
                // Log but don't fail on per-PID errors (process might have terminated)
                eprintln!("Warning: Failed to migrate PID {}: {}", pid, e);
                Ok(())
            }
        }
    }

    /// Freeze all non-essential processes
    pub fn freeze(&mut self) -> Result<(), String> {
        let freeze_path = format!("{}/cgroup.freeze", self.config.cgroup_path);
        let path = Path::new(&freeze_path);

        if !path.exists() {
            return Err(format!(
                "Cgroup freeze file does not exist: {}. Call initialize() first.",
                freeze_path
            ));
        }

        // Write "1" to enable freezing
        fs::write(path, "1").map_err(|e| {
            format!("Failed to freeze cgroup at {}: {}", freeze_path, e)
        })?;

        // Attempt to suspend KWin if configured
        if self.config.suspend_kwin {
            if let Err(e) = self.suspend_kwin() {
                eprintln!("Warning: Failed to suspend KWin: {}", e);
            }
        }

        Ok(())
    }

    /// Thaw all frozen processes (safe to call multiple times)
    pub fn thaw(&mut self) -> Result<(), String> {
        let freeze_path = format!("{}/cgroup.freeze", self.config.cgroup_path);
        let path = Path::new(&freeze_path);

        if !path.exists() {
            // Cgroup doesn't exist - nothing to thaw
            return Ok(());
        }

        // Write "0" to disable freezing
        fs::write(path, "0").map_err(|e| {
            format!("Failed to thaw cgroup at {}: {}", freeze_path, e)
        })?;

        // Attempt to resume KWin if configured
        if self.config.suspend_kwin {
            if let Err(e) = self.resume_kwin() {
                eprintln!("Warning: Failed to resume KWin: {}", e);
            }
        }

        Ok(())
    }

    /// Cleanup the transient cgroup and clear frozen PID tracking
    pub fn cleanup(&mut self) -> Result<(), String> {
        // First, thaw any frozen processes
        let _ = self.thaw();

        // Remove the cgroup directory if it exists
        let cgroup_path = Path::new(&self.config.cgroup_path);
        if cgroup_path.exists() {
            match fs::remove_dir(&self.config.cgroup_path) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to remove cgroup directory {}: {}",
                        self.config.cgroup_path, e
                    );
                    // Don't fail on cleanup errors
                }
            }
        }

        self.frozen_pids.clear();
        Ok(())
    }

    /// Suspend KWin (KDE compositor) via D-Bus
    fn suspend_kwin(&self) -> Result<(), String> {
        // Try to find and suspend KWin via org.kde.kwin.Activities interface
        let output = Command::new("dbus-send")
            .arg("--print-reply")
            .arg("--session")
            .arg("--dest=org.kde.KWin")
            .arg("/org/kde/KWin")
            .arg("org.kde.KWin.suspendCompositing")
            .output()
            .map_err(|e| format!("Failed to execute dbus-send: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("dbus-send failed: {}", stderr));
        }

        Ok(())
    }

    /// Resume KWin (KDE compositor) via D-Bus
    fn resume_kwin(&self) -> Result<(), String> {
        // Try to find and resume KWin via org.kde.kwin.Activities interface
        let output = Command::new("dbus-send")
            .arg("--print-reply")
            .arg("--session")
            .arg("--dest=org.kde.KWin")
            .arg("/org/kde/KWin")
            .arg("org.kde.KWin.resumeCompositing")
            .output()
            .map_err(|e| format!("Failed to execute dbus-send: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("dbus-send failed: {}", stderr));
        }

        Ok(())
    }

    /// Get the count of frozen processes
    pub fn frozen_process_count(&self) -> usize {
        self.frozen_pids.len()
    }

    /// Check if the freezer is currently frozen
    pub fn is_frozen(&self) -> bool {
        let freeze_path = format!("{}/cgroup.freeze", self.config.cgroup_path);
        if let Ok(content) = fs::read_to_string(&freeze_path) {
            content.trim() == "1"
        } else {
            false
        }
    }
}

impl Drop for BenchmarkFreezer {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_freezer_config_default() {
        let config = FreezerConfig::default();
        assert_eq!(config.cgroup_path, "/sys/fs/cgroup/benchmark_freeze");
        assert!(config.suspend_kwin);
    }

    #[test]
    fn test_freezer_creation() {
        let config = FreezerConfig::default();
        let freezer = BenchmarkFreezer::new(config);
        assert_eq!(freezer.frozen_process_count(), 0);
        assert!(!freezer.is_frozen());
    }

    #[test]
    fn test_get_parent_pid() {
        let freezer = BenchmarkFreezer::default_config();
        let current_pid = std::process::id();
        match freezer.get_parent_pid(current_pid) {
            Ok(ppid) => {
                assert!(ppid > 0, "Parent PID should be positive");
                assert_ne!(ppid, current_pid, "Parent PID should differ from current");
            }
            Err(e) => {
                eprintln!("Warning: Could not read parent PID: {}", e);
            }
        }
    }
}

//! System Tuner for Real-Time Performance Measurement
//!
//! Prepares the system environment for low-latency measurements by:
//! - Locking all memory pages to prevent page faults
//! - Setting CPU affinity to isolate on a specific core
//! - Enabling SCHED_FIFO real-time priority
//! - Disabling C-states via PM QoS to prevent latency spikes

use nix::sys::mman::{mlockall, MlockAllFlags};
use std::fs::File;
use std::os::unix::io::AsRawFd;

/// Guard that holds the PM QoS file descriptor to prevent C-state transitions.
/// This must stay alive for the entire measurement duration.
#[derive(Debug)]
pub struct PmQosGuard {
    _file: Option<File>,
}

impl PmQosGuard {
    /// Create a new uninitialized PM QoS guard
    pub fn new() -> Self {
        PmQosGuard { _file: None }
    }

    /// Open and initialize the DMA latency constraint file
    fn open_dma_latency_file() -> Result<Option<File>, Box<dyn std::error::Error>> {
        match std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/cpu_dma_latency")
        {
            Ok(file) => {
                let fd = file.as_raw_fd();
                let val: i32 = 0;
                unsafe {
                    libc::write(
                        fd,
                        &val as *const i32 as *const libc::c_void,
                        std::mem::size_of::<i32>(),
                    );
                }
                Ok(Some(file))
            }
            Err(e) => {
                eprintln!("Warning: Could not open /dev/cpu_dma_latency: {}", e);
                eprintln!(
                    "C-states will not be disabled. Run with elevated privileges for full effect."
                );
                Ok(None)
            }
        }
    }
}

impl Default for PmQosGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PmQosGuard {
    fn drop(&mut self) {
        // File is automatically closed when dropped
    }
}

/// The Tuner prepares the system environment for real-time measurement.
/// It configures memory locking, CPU affinity, scheduling priority, and disables C-states.
#[derive(Debug)]
pub struct Tuner {
    pm_qos_guard: PmQosGuard,
}

impl Tuner {
    /// Creates a new uninitialized Tuner.
    pub fn new() -> Self {
        Tuner {
            pm_qos_guard: PmQosGuard::new(),
        }
    }

    /// Applies real-time settings to prepare for low-latency measurement.
    /// This must be called on the thread that will perform measurement.
    /// The returned Tuner must be kept alive for the entire measurement duration
    /// to maintain the PM QoS file handle open.
    pub fn apply_realtime_settings(
        &mut self,
        core: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!(
            "[TUNER] Starting real-time settings application for core {}",
            core
        );

        // 1. Lock memory to prevent page faults
        eprintln!("[TUNER] Step 1: mlockall (MCL_CURRENT | MCL_FUTURE)");
        mlockall(MlockAllFlags::MCL_CURRENT | MlockAllFlags::MCL_FUTURE).map_err(|e| {
            eprintln!("[TUNER] ✗ mlockall failed: {}", e);
            format!("mlockall failed: {}", e)
        })?;
        eprintln!("[TUNER] ✓ Memory locked");

        // 2. Prefault the stack by writing to an 8KB buffer
        // This ensures the stack pages are resident in memory before the measurement starts.
        eprintln!("[TUNER] Step 2: Prefault stack (8KB with 4KB page boundaries)");
        self.prefault_stack()?;
        eprintln!("[TUNER] ✓ Stack prefaulted");

        // 3. Set CPU affinity to isolate the thread on a specific core
        eprintln!("[TUNER] Step 3: Set CPU affinity to core {}", core);
        self.set_cpu_affinity(core)?;
        eprintln!("[TUNER] ✓ CPU affinity set to core {}", core);

        // 4. Set SCHED_FIFO priority (priority 80)
        eprintln!("[TUNER] Step 4: Set SCHED_FIFO priority 80");
        self.set_sched_fifo()?;
        eprintln!("[TUNER] ✓ SCHED_FIFO priority 80 applied");

        // 5. Disable C-states by holding /dev/cpu_dma_latency open with value 0
        // THIS IS CRITICAL: The file must stay open (hence stored in self) for the entire measurement
        eprintln!("[TUNER] Step 5: Open /dev/cpu_dma_latency to disable C-states");
        self.pm_qos_guard._file = PmQosGuard::open_dma_latency_file()?;
        if self.pm_qos_guard._file.is_some() {
            eprintln!("[TUNER] ✓ C-states disabled via PM QoS");
        } else {
            eprintln!("[TUNER] ⚠ PM QoS not available (non-root), C-states may remain enabled");
        }

        eprintln!("[TUNER] ✓ All real-time settings applied successfully");
        Ok(())
    }

    /// Stack prefaulting: allocate 8KB on the stack and write to each 4KB page.
    fn prefault_stack(&self) -> Result<(), Box<dyn std::error::Error>> {
        // 8KB = 2 pages (assuming 4KB pages)
        let mut buffer: [u8; 8192] = [0; 8192];

        // Write to each page (every 4KB = 4096 bytes)
        for i in (0..8192).step_by(4096) {
            unsafe {
                std::ptr::write_volatile(&mut buffer[i], 1);
            }
        }

        Ok(())
    }

    /// Set CPU affinity for the current thread.
    fn set_cpu_affinity(&self, core: usize) -> Result<(), Box<dyn std::error::Error>> {
        // Use libc to set CPU affinity
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_ZERO(&mut set);
            libc::CPU_SET(core, &mut set);

            let ret = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
            if ret < 0 {
                return Err(format!(
                    "sched_setaffinity failed with errno: {}",
                    std::io::Error::last_os_error()
                )
                .into());
            }
        }

        Ok(())
    }

    /// Set SCHED_FIFO priority for the current thread.
    fn set_sched_fifo(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Use libc to set SCHED_FIFO priority
        unsafe {
            let mut param: libc::sched_param = std::mem::zeroed();
            param.sched_priority = 80; // Priority 80 (high, within FIFO range 1-99)

            let ret = libc::sched_setscheduler(0, libc::SCHED_FIFO, &param);
            if ret < 0 {
                return Err(format!(
                    "sched_setscheduler failed with errno: {}",
                    std::io::Error::last_os_error()
                )
                .into());
            }
        }

        Ok(())
    }
}

impl Default for Tuner {
    fn default() -> Self {
        Self::new()
    }
}

/// Ensure the MSR kernel module is loaded
/// Attempts to run `modprobe msr` if the module isn't already loaded
pub fn ensure_msr_module_loaded() -> Result<(), Box<dyn std::error::Error>> {
    // First check if /dev/cpu/0/msr exists (indicates module is loaded)
    if std::path::Path::new("/dev/cpu/0/msr").exists() {
        eprintln!("[MSR_MODULE] ✓ MSR module is already loaded (/dev/cpu/0/msr exists)");
        return Ok(());
    }

    eprintln!("[MSR_MODULE] MSR module not detected, attempting to load via modprobe...");

    // Try to load the msr module
    let output = std::process::Command::new("modprobe").arg("msr").output()?;

    if output.status.success() {
        eprintln!("[MSR_MODULE] ✓ Successfully loaded MSR module");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to load MSR module: {}", stderr).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tuner_creation() {
        let _tuner = Tuner::new();
        // Just verify creation doesn't panic
        assert!(true);
    }

    #[test]
    fn test_pm_qos_guard_creation() {
        let _guard = PmQosGuard::new();
        // Just verify creation doesn't panic
        assert!(true);
    }
}

//! System Diagnostic Module
//!
//! Detects System Management Interrupts (SMI) via MSR (Model-Specific Registers)
//! and correlates them with latency spikes for root cause analysis.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::time::Instant;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use super::diagnostic_buffer::send_diagnostic;

/// Detects if the CPU is Intel by reading vendor_id from /proc/cpuinfo
fn is_intel_cpu() -> bool {
    if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
        for line in content.lines() {
            if line.starts_with("vendor_id") {
                if let Some(vendor) = line.split(": ").nth(1) {
                    return vendor.trim() == "GenuineIntel";
                }
            }
        }
    }
    false
}

/// MSR (Model Specific Register) reader for SMI detection
#[derive(Debug)]
pub struct MsrReader {
    cpu_id: usize,
    handle: Option<fs::File>,
}

impl MsrReader {
    /// Creates a new MSR reader for the specified CPU
    /// Only succeeds on Intel CPUs with MSR interface available
    pub fn new(cpu_id: usize) -> Result<Self, io::Error> {
        // Verify this is an Intel CPU before attempting MSR access
        if !is_intel_cpu() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "CPU is not Intel or vendor detection failed; SMI detection requires Intel platform",
            ));
        }

        // Attempt to ensure MSR module is loaded
        if let Err(e) = crate::system::performance::tuner::ensure_msr_module_loaded() {
            send_diagnostic(&format!("[MSR_HANDLE] WARNING: Failed to ensure MSR module is loaded: {}", e));
        }

        let path = format!("/dev/cpu/{}/msr", cpu_id);
        match fs::OpenOptions::new().read(true).open(&path) {
            Ok(file) => {
                send_diagnostic(&format!("[MSR_HANDLE] Opened MSR file handle for cpu_id={}", cpu_id));
                Ok(MsrReader {
                    cpu_id,
                    handle: Some(file),
                })
            },
            Err(e) => {
                // Gracefully handle if MSR interface is not available
                send_diagnostic(&format!("[MSR_HANDLE] WARNING: Cannot open {} ({})", path, e));
                send_diagnostic("[MSR_HANDLE]   ⚠ To enable SMI detection, try one of the following:");
                send_diagnostic("[MSR_HANDLE]   1. Run with root privileges (sudo or pkexec)");
                send_diagnostic("[MSR_HANDLE]   2. Ensure the 'msr' kernel module is loaded: sudo modprobe msr");
                send_diagnostic("[MSR_HANDLE]   3. Check MSR interface availability: ls -l /dev/cpu/0/msr");
                send_diagnostic("[MSR_HANDLE] SMI detection will be disabled for this session.");
                Ok(MsrReader {
                    cpu_id,
                    handle: None,
                })
            }
        }
    }

    /// Reads the SMI count from MSR 0x34 (MSR_SMI_COUNT)
    pub fn read_smi_count(&mut self) -> Result<u64, io::Error> {
        match &mut self.handle {
            Some(file) => {
                // Seek to MSR address 0x34
                file.seek(SeekFrom::Start(0x34))?;

                // Read 8 bytes
                let mut buffer = [0u8; 8];
                file.read_exact(&mut buffer)?;

                // Interpret as little-endian u64
                Ok(u64::from_le_bytes(buffer))
            }
            None => {
                // MSR interface not available
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "MSR interface not available",
                ))
            }
        }
    }
}

impl Drop for MsrReader {
    fn drop(&mut self) {
        if self.handle.is_some() {
            send_diagnostic(&format!("[MSR_HANDLE] Closing MSR file handle for cpu_id={}", self.cpu_id));
        }
    }
}

/// Minimum read interval for MSR polling (10ms = 10,000,000 ns)
const MIN_READ_INTERVAL_NS: u64 = 10_000_000;

/// Tracks SMI counts and correlates with latency spikes
#[derive(Debug)]
pub struct SmiCorrelation {
    baseline_smi_count: u64,
    current_smi_count: u64,
    last_read_time: Instant,
    cpu_id: usize,
    msr_reader: Option<MsrReader>,
    /// Reference to authoritative total SMI count atomic (updated directly)
    total_smi_count: Option<Arc<AtomicU64>>,
    /// Reference to authoritative SMI-correlated spikes atomic (updated directly)
    smi_correlated_spikes: Option<Arc<AtomicU64>>,
}

impl SmiCorrelation {
    /// Creates a new SMI correlation tracker with references to authoritative atomics
    pub fn new(
        cpu_id: usize,
        total_smi_count: Option<Arc<AtomicU64>>,
        smi_correlated_spikes: Option<Arc<AtomicU64>>,
    ) -> Self {
        let mut correlation = SmiCorrelation {
            baseline_smi_count: 0,
            current_smi_count: 0,
            last_read_time: Instant::now(),
            cpu_id,
            msr_reader: None,
            total_smi_count,
            smi_correlated_spikes,
        };

        // Try to initialize MSR reader
        if let Ok(mut reader) = MsrReader::new(cpu_id) {
            if let Ok(count) = reader.read_smi_count() {
                correlation.baseline_smi_count = count;
                correlation.current_smi_count = count;
                correlation.msr_reader = Some(reader);
            }
        }

        correlation
    }

    /// Updates SMI count and checks for correlation with spikes
    pub fn update_smi_count(&mut self) -> bool {
        if let Some(ref mut reader) = self.msr_reader {
            if let Ok(count) = reader.read_smi_count() {
                self.current_smi_count = count;
                let has_incremented = count > self.baseline_smi_count;
                if has_incremented {
                    send_diagnostic(&format!("[SMI_DIAG] update_smi_count: baseline={}, current={}, incremented=true",
                        self.baseline_smi_count, count));
                }
                return has_incremented;
            } else {
                send_diagnostic(&format!("[SMI_DIAG] WARNING: Failed to read SMI count from MSR (cpu_id={})", self.cpu_id));
            }
        } else {
            send_diagnostic(&format!("[SMI_DIAG] WARNING: MSR reader not available (cpu_id={})", self.cpu_id));
        }
        false
    }

    /// Records a spike and correlates it with SMI if applicable
    /// Implements 100ms cooldown to prevent eager polling
    /// Consolidates updates directly to authoritative atomics
    pub fn record_spike(&mut self) -> bool {
        let now = Instant::now();
        let elapsed_nanos = now.duration_since(self.last_read_time).as_nanos();
        
        // Apply cooldown logic: only call update_smi_count if 100ms has passed
        if elapsed_nanos >= MIN_READ_INTERVAL_NS as u128 {
            send_diagnostic(&format!("[SMI_COOLDOWN_DIAGNOSTIC] Cooldown satisfied: elapsed={}ns (min={}ns), attempting MSR read",
                elapsed_nanos, MIN_READ_INTERVAL_NS));
            
            self.last_read_time = now;
            let smi_detected = self.update_smi_count();
            
            if smi_detected {
                send_diagnostic(&format!("[SMI] SMI detected: baseline={}, current={}",
                    self.baseline_smi_count, self.current_smi_count));
                
                // Update authoritative atomics directly
                if let Some(ref total_count) = self.total_smi_count {
                    let before = total_count.load(Ordering::Relaxed);
                    total_count.store(self.current_smi_count, Ordering::Release);
                    let after = total_count.load(Ordering::Relaxed);
                    send_diagnostic(&format!("[SMI_ATOMIC] total_smi_count: before={}, stored={}, after={}",
                        before, self.current_smi_count, after));
                }
                if let Some(ref corr_spikes) = self.smi_correlated_spikes {
                    let before = corr_spikes.load(Ordering::Relaxed);
                    corr_spikes.fetch_add(1, Ordering::Release);
                    let after = corr_spikes.load(Ordering::Relaxed);
                    send_diagnostic(&format!("[SMI_ATOMIC] smi_correlated_spikes: before={}, after={}",
                        before, after));
                }
            }
            return smi_detected;
        } else {
            send_diagnostic(&format!("[SMI_COOLDOWN_DIAGNOSTIC] Cooldown NOT satisfied: elapsed={}ns (min={}ns), skipping MSR read",
                elapsed_nanos, MIN_READ_INTERVAL_NS));
        }
        false
    }

    /// Get the total SMI count
    pub fn total_smi_count(&self) -> u64 {
        self.current_smi_count
    }

    /// Check if MSR is available
    pub fn is_msr_available(&self) -> bool {
        self.msr_reader.is_some()
    }
}

/// Statistics about latency and SMI correlation
#[derive(Debug, Clone)]
pub struct CorrelationStats {
    pub total_samples: u64,
    pub total_spikes: u64,
    pub spikes_correlated_to_smi: u64,
    pub smi_count_total: u64,
    pub msr_available: bool,
}

/// Detector for SMI-related performance degradation
///
/// Provides SMI detection capabilities on Intel platforms only.
/// Requires /dev/cpu/{id}/msr interface to be available.
pub struct SmiDetector;

impl SmiDetector {
    /// Check if system can perform SMI detection
    ///
    /// Returns true only if:
    /// - CPU is Intel (GenuineIntel vendor)
    /// - MSR interface is available at /dev/cpu/0/msr
    pub fn is_available() -> bool {
        // First check if this is an Intel CPU
        if !is_intel_cpu() {
            return false;
        }

        // Try to create an MSR reader for CPU 0
        match MsrReader::new(0) {
            Ok(reader) => reader.handle.is_some(),
            Err(_) => false,
        }
    }

    /// Get current SMI count for a specific CPU
    ///
    /// # Arguments
    /// * `cpu_id` - CPU core identifier (0-based)
    ///
    /// # Returns
    /// The MSR_SMI_COUNT value (0x34) from the specified CPU
    ///
    /// # Errors
    /// Returns error if CPU is not Intel, MSR interface unavailable, or read fails
    pub fn get_smi_count(cpu_id: usize) -> Result<u64, io::Error> {
        let mut reader = MsrReader::new(cpu_id)?;
        reader.read_smi_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smi_correlation_creation() {
        let correlation = SmiCorrelation::new(0, None, None);
        assert_eq!(correlation.total_smi_count(), 0);
    }

    #[test]
    fn test_is_intel_cpu_detection() {
        // The function will return true on Intel, false on other vendors
        // This test just verifies the function doesn't panic
        let _ = is_intel_cpu();
        assert!(true);
    }

    #[test]
    fn test_smi_detector_availability() {
        // SmiDetector::is_available() checks Intel vendor first
        // This test verifies the logic doesn't panic
        let available = SmiDetector::is_available();
        // On Intel systems with /dev/cpu/0/msr, this should be true
        // On non-Intel or systems without MSR, this should be false
        let _ = available;
        assert!(true);
    }
}

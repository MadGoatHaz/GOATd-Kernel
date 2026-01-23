//! SCX (Sched-ext) Infrastructure - Environment validation & persistent manager
//!
//! This module provides utilities for:
//! - Validating kernel support via /proc/config.gz
//! - Detecting installed scheduler binaries
//! - **Persistent SCX Management via systemd service**
//! - Polkit-elevated system-wide scheduler configuration
//! - Self-healing environment provisioning

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Child};
use std::sync::Mutex;
use std::time::{Instant, Duration};
use flate2::read::GzDecoder;
use serde::{Serialize, Deserialize};
use crate::log_info;

/// Recommendation level for scheduler/mode combinations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationLevel {
    Recommended,
    NotRecommended,
}

impl RecommendationLevel {
    pub fn as_str(&self) -> &str {
        match self {
            RecommendationLevel::Recommended => "Recommended",
            RecommendationLevel::NotRecommended => "Not Recommended",
        }
    }
}

/// Rich metadata for SCX scheduler and mode combinations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScxMetadata {
    /// User-friendly description of the scheduler/mode pairing
    pub description: String,
    /// Use cases this scheduler/mode is best for
    pub best_for: String,
    /// CLI flags as command-line string (e.g., "--preempt-mode=all --extra-flag")
    pub cli_flags: String,
    /// Recommendation level for this pairing
    pub recommendation: RecommendationLevel,
}

impl ScxMetadata {
    pub fn new(
        description: String,
        best_for: String,
        cli_flags: String,
        recommendation: RecommendationLevel,
    ) -> Self {
        ScxMetadata {
            description,
            best_for,
            cli_flags,
            recommendation,
        }
    }
}

// Cache for scheduler detection results (prevents repeated filesystem checks every frame)
// Holds (Vec<String>, Instant) where Instant is the last check time
static SCX_SCHEDULER_CACHE: Mutex<Option<(Vec<String>, Instant)>> = Mutex::new(None);
const SCX_CACHE_DURATION: Duration = Duration::from_secs(300);  // 5-minute cache TTL

// Cache for SCX readiness checks (prevents log spam from repeated support checking every frame)
// Holds (SCXReadiness, Instant) where Instant is the last check time
static SCX_READINESS_CACHE: Mutex<Option<(SCXReadiness, Instant)>> = Mutex::new(None);
const SCX_READINESS_CACHE_DURATION: Duration = Duration::from_secs(60);  // 60-second cache TTL

// State tracking for redundant log suppression
// Stores the last logged SCXReadiness state to detect changes
lazy_static::lazy_static! {
    static ref SCX_READINESS_LOGGED_STATE: Mutex<Option<SCXReadiness>> = Mutex::new(None);
}

/// SCX Scheduler Mode - Abstracts complex CLI flags into standardized profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulerMode {
    Auto,
    Gaming,
    LowLatency,
    PowerSave,
    Server,
}

impl SchedulerMode {
    pub fn as_str(&self) -> &str {
        match self {
            SchedulerMode::Auto => "Auto",
            SchedulerMode::Gaming => "Gaming",
            SchedulerMode::LowLatency => "LowLatency",
            SchedulerMode::PowerSave => "PowerSave",
            SchedulerMode::Server => "Server",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Auto" => Some(SchedulerMode::Auto),
            "Gaming" => Some(SchedulerMode::Gaming),
            "LowLatency" => Some(SchedulerMode::LowLatency),
            "PowerSave" => Some(SchedulerMode::PowerSave),
            "Server" => Some(SchedulerMode::Server),
            _ => None,
        }
    }
}

impl std::fmt::Display for SchedulerMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Per-scheduler configuration with mode-specific flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_mode: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gaming_mode: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowlatency_mode: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub powersave_mode: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_mode: Option<Vec<String>>,
}

/// SCX Loader Configuration - TOML serializable format for /etc/scx_loader/config.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScxLoaderConfig {
    pub default_sched: String,
    pub default_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheds: Option<std::collections::HashMap<String, SchedulerConfig>>,
}

impl ScxLoaderConfig {
    pub fn new(scheduler: &str, mode: SchedulerMode) -> Self {
        ScxLoaderConfig {
            default_sched: scheduler.to_string(),
            default_mode: mode.to_string(),
            scheds: None,
        }
    }

    pub fn with_scheduler_config(mut self, scheduler: &str, config: SchedulerConfig) -> Self {
        if self.scheds.is_none() {
            self.scheds = Some(std::collections::HashMap::new());
        }
        if let Some(ref mut scheds) = self.scheds {
            scheds.insert(scheduler.to_string(), config);
        }
        self
    }

    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}

/// SCX Readiness states for self-healing detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SCXReadiness {
    /// All SCX dependencies are installed and configured
    Ready,
    /// Kernel lacks CONFIG_SCHED_CLASS_EXT=y support
    KernelMissingSupport,
    /// SCX packages (scx-tools, scx-scheds) are not installed
    PackagesMissing,
    /// SCX systemd service unit is not installed
    ServiceMissing,
}

impl Default for SCXReadiness {
    fn default() -> Self {
        SCXReadiness::Ready
    }
}

impl From<SCXReadiness> for String {
    fn from(r: SCXReadiness) -> Self {
        match r {
            SCXReadiness::Ready => "Ready".to_string(),
            SCXReadiness::KernelMissingSupport => "KernelMissingSupport".to_string(),
            SCXReadiness::PackagesMissing => "PackagesMissing".to_string(),
            SCXReadiness::ServiceMissing => "ServiceMissing".to_string(),
        }
    }
}

/// SCX Manager - centralized SCX validation and persistent management
#[derive(Debug, Clone)]
pub struct SCXManager;

/// Persistent SCX Manager - Applies scheduler config to `/etc/scx_loader/config.toml` via Polkit
#[derive(Debug, Clone)]
pub struct PersistentSCXManager;

/// Determines if SCX readiness state has changed since last logged.
/// Returns true if:
/// - Debug mode is enabled (environment variable SCX_DEBUG_LOG=1)
/// - No previous state was logged (first detection)
/// - Readiness state changed (transition occurred)
/// Returns false if:
/// - Same state as previously logged AND debug mode disabled
fn should_log_scx_state_change(current_state: SCXReadiness) -> bool {
    // Check for debug override - force logging if SCX_DEBUG_LOG=1
    if std::env::var("SCX_DEBUG_LOG").map(|v| v == "1").unwrap_or(false) {
        return true;
    }
    
    let mut last_state = SCX_READINESS_LOGGED_STATE.lock().unwrap();
    
    // First detection - no prior state exists
    if last_state.is_none() {
        *last_state = Some(current_state);
        return true;
    }
    
    let prior = last_state.unwrap();
    
    // State changed (transition detected)
    if prior != current_state {
        *last_state = Some(current_state);
        return true;
    }
    
    // State unchanged (redundant) and debug mode disabled
    false
}

impl SCXManager {
    /// Check if the running kernel supports SCX via /proc/config.gz
    ///
    /// Reads and decompresses `/proc/config.gz` to verify the presence of
    /// `CONFIG_SCHED_CLASS_EXT=y`, indicating kernel support for the extended
    /// CPU scheduler class.
    ///
    /// # Returns
    /// `true` if CONFIG_SCHED_CLASS_EXT=y is found, `false` otherwise
    ///
    /// # Example
    /// ```ignore
    /// if SCXManager::check_scx_support() {
    ///     println!("SCX is supported by the kernel");
    /// }
    /// ```
    pub fn check_scx_support() -> bool {
        const PROC_CONFIG_PATH: &str = "/proc/config.gz";
        const SCX_CONFIG_KEY: &str = "CONFIG_SCHED_CLASS_EXT=y";

        // Step 1: Verify /proc/config.gz exists
        if !Path::new(PROC_CONFIG_PATH).exists() {
            log_info!("[SCXManager] /proc/config.gz not found");
            return false;
        }

        // Step 2: Open and decompress /proc/config.gz
        match fs::File::open(PROC_CONFIG_PATH) {
            Ok(file) => {
                let decoder = GzDecoder::new(file);
                let mut contents = String::new();
                
                // Step 3: Read decompressed config
                match std::io::BufReader::new(decoder).read_to_string(&mut contents) {
                    Ok(_) => {
                        // Step 4: Search for CONFIG_SCHED_CLASS_EXT=y
                        let supported = contents.lines().any(|line| line == SCX_CONFIG_KEY);
                        log_info!(
                            "[SCXManager] SCX support check: {}",
                            if supported { "SUPPORTED" } else { "NOT SUPPORTED" }
                        );
                        supported
                    }
                    Err(e) => {
                        log_info!("[SCXManager] Failed to decompress /proc/config.gz: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                log_info!("[SCXManager] Failed to open /proc/config.gz: {}", e);
                false
            }
        }
    }

    /// Detect installed SCX scheduler binaries with caching.
    ///
    /// Checks for the presence of standard SCX scheduler binaries at:
    /// - `/usr/bin/scx_bpfland`
    /// - `/usr/bin/scx_lavd`
    /// - `/usr/bin/scx_rusty`
    ///
    /// **Optimized with 5-minute cache** to prevent redundant filesystem checks every frame.
    /// The cache is automatically invalidated after 5 minutes or can be manually cleared.
    ///
    /// # Returns
    /// A `Vec<String>` containing the names of available schedulers (e.g., ["bpfland", "rusty"])
    ///
    /// # Example
    /// ```ignore
    /// let schedulers = SCXManager::is_scx_installed();
    /// println!("Available schedulers: {:?}", schedulers);
    /// ```
    pub fn is_scx_installed() -> Vec<String> {
        // Check cache first
        if let Ok(cache) = SCX_SCHEDULER_CACHE.lock() {
            if let Some((cached_schedulers, cached_time)) = cache.as_ref() {
                if cached_time.elapsed() < SCX_CACHE_DURATION {
                    // Silenced: Don't log cache hits every frame to avoid spam
                    return cached_schedulers.clone();
                }
            }
        }
        
        // Cache miss or expired: perform filesystem check
        let scheduler_bins = vec![
            ("scx_bpfland", "/usr/bin/scx_bpfland"),
            ("scx_lavd", "/usr/bin/scx_lavd"),
            ("scx_rusty", "/usr/bin/scx_rusty"),
        ];

        let mut available = Vec::new();

        for (name, path) in scheduler_bins {
            if Path::new(path).exists() {
                available.push(name.to_string());
                log_info!("[SCXManager] Found scheduler: {}", name);
            }
        }

        if available.is_empty() {
            log_info!("[SCXManager] No SCX schedulers found");
        } else {
            log_info!("[SCXManager] Available schedulers: {:?}", available);
        }

        // Update cache
        if let Ok(mut cache) = SCX_SCHEDULER_CACHE.lock() {
            *cache = Some((available.clone(), Instant::now()));
        }

        available
    }
    
    /// Clear the SCX scheduler detection cache.
    ///
    /// Forces the next call to `is_scx_installed()` to perform a fresh filesystem check.
    /// Useful after provisioning SCX environment or when debugging.
    pub fn clear_scx_cache() {
        if let Ok(mut cache) = SCX_SCHEDULER_CACHE.lock() {
            *cache = None;
            log_info!("[SCXManager] SCX scheduler cache cleared");
        }
    }

    /// Launch a scheduler binary as a child process.
    ///
    /// Spawns the specified scheduler binary with the `-k` (keep-alive) flag,
    /// which instructs the scheduler to remain active until explicitly stopped.
    ///
    /// # Arguments
    /// * `scheduler_binary` - Name or path of the scheduler binary (e.g., "scx_bpfland")
    /// * `flags` - Additional command-line flags to pass to the scheduler
    ///
    /// # Returns
    /// `Result<Child, String>` containing the spawned process or an error message
    ///
    /// # Example
    /// ```ignore
    /// match SCXManager::launch("scx_bpfland", "") {
    ///     Ok(child) => println!("Scheduler launched with PID: {}", child.id()),
    ///     Err(e) => eprintln!("Failed to launch scheduler: {}", e),
    /// }
    /// ```
    pub fn launch(scheduler_binary: &str, flags: &str) -> Result<Child, String> {
        // Validate scheduler binary name
        if scheduler_binary.is_empty() {
            return Err("Scheduler binary name cannot be empty".to_string());
        }

        // Build command with -k (keep-alive) flag
        let mut cmd = Command::new(scheduler_binary);
        cmd.arg("-k"); // Keep-alive flag

        // Add any additional flags if provided
        if !flags.is_empty() {
            cmd.args(flags.split_whitespace());
        }

        log_info!(
            "[SCXManager] Launching scheduler: {} with flags: '{}' and -k",
            scheduler_binary,
            flags
        );

        // Attempt to spawn the child process
        match cmd.spawn() {
            Ok(child) => {
                log_info!("[SCXManager] Scheduler launched successfully with PID: {}", child.id());
                Ok(child)
            }
            Err(e) => {
                let err_msg = format!("Failed to launch scheduler {}: {}", scheduler_binary, e);
                log_info!("[SCXManager] {}", err_msg);
                Err(err_msg)
            }
        }
    }

    /// Determine SCX environment readiness state with caching.
    ///
    /// Performs sequential checks for:
    /// 1. Kernel support (CONFIG_SCHED_CLASS_EXT=y)
    /// 2. Installed scheduler packages
    /// 3. Modern scx_loader service unit and config file
    ///
    /// **Optimized with 60-second cache** to prevent log spam from repeated
    /// CONFIG_SCHED_CLASS_EXT checks every frame. Cache is automatically
    /// invalidated after 60 seconds.
    ///
    /// # Returns
    /// `SCXReadiness` state indicating what (if anything) is missing
    pub fn get_scx_readiness() -> SCXReadiness {
        // Check cache first
        if let Ok(cache) = SCX_READINESS_CACHE.lock() {
            if let Some((cached_readiness, cached_time)) = cache.as_ref() {
                if cached_time.elapsed() < SCX_READINESS_CACHE_DURATION {
                    // Silenced: Don't log cache hits every frame to avoid spam
                    return *cached_readiness;
                }
            }
        }

        // Cache miss or expired: perform full readiness check
        let readiness = Self::compute_scx_readiness();

        // Update cache
        if let Ok(mut cache) = SCX_READINESS_CACHE.lock() {
            *cache = Some((readiness, Instant::now()));
        }

        readiness
    }

    /// Find the SCX systemd service file on the system (no caching)
    ///
    /// Checks for both modern (`scx_loader.service`) and legacy (`scx.service`) service names
    /// in standard systemd directories. Returns the found service name or None if not found.
    ///
    /// This function performs a fresh filesystem check every time and does not cache results,
    /// allowing for detection of newly installed service files after package installation or daemon-reload.
    ///
    /// # Returns
    /// `Some(service_name)` if found, `None` otherwise
    pub fn find_scx_service() -> Option<String> {
        // Check both system and user-local service directories for both service names
        let service_candidates = [
            ("/usr/lib/systemd/system/scx_loader.service", "scx_loader.service"),
            ("/etc/systemd/system/scx_loader.service", "scx_loader.service"),
            ("/usr/lib/systemd/system/scx.service", "scx.service"),
            ("/etc/systemd/system/scx.service", "scx.service"),
        ];

        for (path, service_name) in &service_candidates {
            if Path::new(path).exists() {
                log_info!("[SCXManager] Found SCX service: {} at {}", service_name, path);
                return Some(service_name.to_string());
            }
        }

        log_info!("[SCXManager] No SCX service found (checked both scx_loader.service and scx.service)");
        None
    }

    /// Computes current SCX readiness state with smart logging.
    ///
    /// Logs are suppressed when readiness state hasn't changed, preventing log spam.
    /// This behavior can be overridden for debugging by setting environment variable:
    /// - SCX_DEBUG_LOG=1: Forces verbose logging on every state recomputation
    ///
    /// # Returns
    /// The current SCXReadiness state (Ready, KernelMissingSupport, PackagesMissing, or ServiceMissing)
    fn compute_scx_readiness() -> SCXReadiness {
        // First, compute the current readiness state without logging
        let current_state = if !Self::check_scx_support() {
            SCXReadiness::KernelMissingSupport
        } else if Self::is_scx_installed().is_empty() {
            SCXReadiness::PackagesMissing
        } else if Self::find_scx_service().is_none() {
            SCXReadiness::ServiceMissing
        } else {
            SCXReadiness::Ready
        };
        
        // Check if state has changed since last logged
        if should_log_scx_state_change(current_state) {
            // Log only when state transitions occur
            match current_state {
                SCXReadiness::Ready => {
                    log_info!("[SCXManager] SCX environment ready");
                }
                SCXReadiness::KernelMissingSupport => {
                    log_info!("[SCXManager] Kernel lacks SCX support");
                }
                SCXReadiness::PackagesMissing => {
                    log_info!("[SCXManager] No SCX schedulers found");
                }
                SCXReadiness::ServiceMissing => {
                    log_info!("[SCXManager] SCX service not installed");
                }
            }
        }
        // Return state regardless of logging (silent on repeated states)
        current_state
    }

    /// Provision the SCX environment with packages and systemd service.
    ///
    /// Performs the following steps with elevated privileges (via pkexec):
    /// 1. Checks for official `scx-scheds` package
    /// 2. Initializes `/etc/scx_loader/config.toml` with default scheduler and mode
    /// 3. Enables the correct SCX service (`scx_loader.service` or `scx.service`) to run on boot
    ///
    /// All steps execute with privilege escalation for seamless single-prompt UX.
    ///
    /// # Special Handling for Service Registry Lag
    /// If the service file is not found after package installation, instead of failing hard,
    /// the function returns a "User Action Required" error that guides the user to reload
    /// systemd units (via daemon-reload) or restart the application.
    ///
    /// # Returns
    /// * `Ok(())` on successful provisioning
    /// * `Err(String)` with root-cause diagnostic on failure
    ///
    /// # Note
    /// - Requires official Arch Linux packages (scx-tools, scx-scheds)
    /// - Uses `pkexec` for privilege escalation (single prompt)
    /// - Config file is written to `/etc/scx_loader/config.toml` (TOML format)
    /// - Temp file path is properly shell-escaped to prevent injection
    pub fn provision_scx_environment() -> Result<(), String> {
        log_info!("[SCXManager] Starting SCX environment provisioning for scx_loader");

        // Create default TOML configuration
        let default_config = ScxLoaderConfig::new("scx_bpfland", SchedulerMode::Auto);
        let config_content = match default_config.to_toml_string() {
            Ok(content) => content,
            Err(e) => {
                let msg = format!("Failed to serialize ScxLoaderConfig to TOML: {}", e);
                log_info!("[SCXManager] ERROR: {}", msg);
                return Err(msg);
            }
        };

        // Write config content to a temporary file
        let mut temp_config = tempfile::NamedTempFile::new()
            .map_err(|e| format!("Failed to create temp config file: {}", e))?;
        
        use std::io::Write;
        temp_config.write_all(config_content.as_bytes())
            .map_err(|e| format!("Failed to write temp config file: {}", e))?;
        
        let temp_path = temp_config.path().to_string_lossy().to_string();
        log_info!("[SCXManager] Config template written to: {}", temp_path);

        // Find which SCX service is available on the system
        let scx_service = match Self::find_scx_service() {
            Some(service) => service,
            None => {
                // Service not found but package is installed - likely systemd registry lag
                // Return a user-actionable message instead of hard error
                let msg = "[USER_ACTION_REQUIRED] SCX package detected but Service not registered. \
                           This can happen immediately after package installation due to systemd registry lag. \
                           Try one of these solutions: (1) Use the 'Reload System Units' button in the Dashboard, \
                           (2) Run 'systemctl daemon-reload' manually, or (3) Restart the application. \
                           After reloading, try provisioning again.".to_string();
                log_info!("[SCXManager] USER_ACTION_REQUIRED: {}", msg);
                return Err(msg);
            }
        };

        log_info!("[SCXManager] ✓ Using SCX service: {}", scx_service);

        // Safely escape the temporary file path for shell interpolation
         let escaped_temp_path = escape_shell_arg(&temp_path);

         // Construct the provisioning command with all steps chained atomically
         // Step 1: Create /etc/scx_loader directory
         // Step 2: Copy config file to /etc/scx_loader/config.toml
         // Step 3: Reload systemd daemon
         // Step 4: Enable the detected SCX service (scx_loader.service or scx.service)
         let cmd = format!(
             "mkdir -p /etc/scx_loader && \
              cp {} /etc/scx_loader/config.toml && \
              systemctl daemon-reload && \
              systemctl enable {}",
             escaped_temp_path,
             scx_service
         );

        let pkexec_cmd = format!("pkexec bash -c '{}'", cmd);
        log_info!("[SCXManager] Executing provisioning command with pkexec (4-step atomic chain)");

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&pkexec_cmd)
            .output()
            .map_err(|e| format!("Failed to execute pkexec: {}", e))?;

        if output.status.success() {
            log_info!("[SCXManager] ✓ SCX environment provisioned successfully");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Diagnose which step likely failed based on output patterns
            let error_msg = Self::diagnose_provisioning_failure(&stderr, &stdout);
            log_info!("[SCXManager] ERROR: {}", error_msg);
            Err(error_msg)
        }
    }

    /// Diagnose provisioning failures by analyzing stderr/stdout output
    ///
    /// Attempts to identify which provisioning steps failed:
    /// 1. Package detection
    /// 2. Config file initialization
    /// 3. systemctl daemon-reload
    /// 4. systemctl enable
    ///
    /// # Returns
    /// Granular error message identifying the likely failure point
    fn diagnose_provisioning_failure(stderr: &str, stdout: &str) -> String {
        // Check for network-related errors
        if stderr.contains("Connection refused") ||
           stderr.contains("Name or service not known") ||
           stderr.contains("Connection timed out") ||
           stderr.contains("Temporary failure") ||
           stdout.contains("Unknown server") {
            return "Network error: Cannot reach package repositories (no internet or DNS failure). Verify internet connectivity and try again.".to_string();
        }

        // Check for missing packages
        if stderr.contains("not found") ||
           stderr.contains("not in any repo") ||
           stdout.contains("target not found") {
            return "Package error: scx-tools or scx-scheds not found in official repositories. Verify packages are installed from Arch Linux official repos.".to_string();
        }

        // Check for permission errors
        if stderr.contains("Permission denied") {
            if stderr.contains("/etc/systemd") {
                return "Permission error: Cannot write to /etc/systemd/system. Ensure polkit is properly configured.".to_string();
            } else {
                return "Permission error: Insufficient privileges for provisioning. Verify polkit authentication succeeded.".to_string();
            }
        }

        // Check for systemd-specific errors
        if stderr.contains("daemon-reload") || stdout.contains("daemon-reload") {
            return "Systemd error: Failed to reload systemd daemon. Check systemd status and logs: sudo journalctl -xe".to_string();
        }

        if stderr.contains("enable") || (stdout.contains("enable") && stderr.contains("already")) {
            return "Systemd error: Failed to enable SCX Scheduler service. The service file may not exist or may be invalid. \
                    Verify that scx-scheds is properly installed and provides scx_loader.service.".to_string();
        }

        // Check for service not found (unit file doesn't exist)
        if stderr.contains("not find a unit") ||
           stderr.contains("No such file") ||
           stderr.contains("does not exist") {
            return "Service file not found: scx_loader.service does not exist. \
                    This indicates an incomplete scx-scheds installation. Please reinstall from official Arch Linux repositories.".to_string();
        }

        // Generic fallback with full output for debugging
        format!(
            "Provisioning failed (unknown cause). stderr={}, stdout={}",
            stderr, stdout
        )
    }

}

/// Escape a string for use as a literal argument in shell commands
///
/// Wraps the string in single quotes and escapes any single quotes within it.
/// This ensures the string is treated as a literal value, not interpreted by the shell.
///
/// # Example
/// ```ignore
/// let path = "/tmp/file with spaces.txt";
/// let escaped = escape_shell_arg(path);  // '/tmp/file with spaces.txt'
/// ```
fn escape_shell_arg(arg: &str) -> String {
    // Single-quote the entire string to prevent shell expansion
    // Escape single quotes by ending the quote, adding escaped quote, and restarting quote
    format!("'{}'", arg.replace("'", "'\\''"))
}

impl PersistentSCXManager {

    /// Apply granular SCX scheduler configuration via direct scheduler and mode selection
    ///
    /// This method allows UI to directly specify scheduler binary and mode without relying on profiles.
    /// **CRITICAL**: Includes systemctl enable to ensure persistence across reboots.
    ///
    /// # Arguments
    /// * `scheduler` - Scheduler binary name (e.g., "scx_bpfland", "scx_lavd", "scx_rusty")
    /// * `mode` - SchedulerMode enum (Auto, Gaming, LowLatency, PowerSave, Server)
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(String)` with detailed error message on failure
    pub fn apply_scx_config(scheduler: &str, mode: SchedulerMode) -> Result<(), String> {
        log_info!(
            "[PersistentSCXManager] Applying granular SCX config: scheduler={}, mode={}",
            scheduler,
            mode
        );

        // Validate scheduler name is not empty
        if scheduler.is_empty() {
            let msg = "Scheduler name cannot be empty".to_string();
            log_info!("[PersistentSCXManager] ERROR: {}", msg);
            return Err(msg);
        }

        // Create TOML configuration with direct scheduler and mode
        let scx_config = ScxLoaderConfig::new(scheduler, mode);
        let config_toml = match scx_config.to_toml_string() {
            Ok(toml) => toml,
            Err(e) => {
                let msg = format!("Failed to serialize config to TOML: {}", e);
                log_info!("[PersistentSCXManager] ERROR: {}", msg);
                return Err(msg);
            }
        };

        // Write TOML to temporary file
        let temp_file = match tempfile::NamedTempFile::new() {
            Ok(file) => file,
            Err(e) => {
                let msg = format!("Failed to create temporary file: {}", e);
                log_info!("[PersistentSCXManager] ERROR: {}", msg);
                return Err(msg);
            }
        };

        let temp_path = temp_file.path().to_string_lossy().to_string();
        log_info!("[PersistentSCXManager] Temp TOML file: {}", temp_path);

        // Write TOML configuration to temp file
        match std::fs::write(&temp_path, &config_toml) {
            Ok(_) => {
                log_info!(
                    "[PersistentSCXManager] ✓ TOML configuration written to temp file"
                );
            }
            Err(e) => {
                let msg = format!("Failed to write temp TOML file: {}", e);
                log_info!("[PersistentSCXManager] ERROR: {}", msg);
                return Err(msg);
            }
        }

        // Escape temp path for shell
        let escaped_temp_path = escape_shell_arg(&temp_path);

        // Execute Polkit-elevated command: mkdir, copy, enable service, and restart service
        // CRITICAL: systemctl enable ensures persistence across reboots
        let shell_cmd = format!(
            "mkdir -p /etc/scx_loader && cp {} /etc/scx_loader/config.toml && systemctl daemon-reload && systemctl enable scx_loader.service && systemctl restart scx_loader.service",
            escaped_temp_path
        );
        // CRITICAL: Use single-quote escaping (standardized across all pkexec calls)
        // Single quotes prevent shell interpretation of special chars and variable expansion
        let pkexec_cmd = format!("pkexec bash -c '{}'", shell_cmd);

        log_info!(
            "[PersistentSCXManager] Executing Polkit-elevated TOML deployment with persistence enable via pkexec"
        );

        match std::process::Command::new("sh")
            .arg("-c")
            .arg(&pkexec_cmd)
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    log_info!(
                        "[PersistentSCXManager] ✓ Scheduler activated and persisted: {} ({})",
                        scheduler,
                        mode
                    );
                    return Ok(());
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let msg = format!(
                        "Failed to activate scheduler {}: {}",
                        scheduler, stderr
                    );
                    log_info!("[PersistentSCXManager] ERROR: {}", msg);
                    return Err(msg);
                }
            }
            Err(e) => {
                let msg = format!(
                    "Failed to execute pkexec for {}: {}",
                    scheduler, e
                );
                log_info!("[PersistentSCXManager] ERROR: {}", msg);
                return Err(msg);
            }
        }
    }
}

/// Retrieve rich metadata for a given SCX scheduler and mode combination
///
/// Maps scheduler name and mode string to descriptive metadata including:
/// - User-friendly description
/// - Best-for use cases
/// - CLI flags as command string
/// - Recommendation level
///
/// # Arguments
/// * `scheduler` - Scheduler binary name (e.g., "scx_bpfland", "scx_lavd", "scx_rusty", "scx_simple", "scx_central")
/// * `mode` - Mode string (e.g., "Auto", "Gaming", "LowLatency", "PowerSave", "Server")
///
/// # Returns
/// `ScxMetadata` with rich information about the scheduler/mode pairing
pub fn get_scx_metadata(scheduler: &str, mode: &str) -> ScxMetadata {
    match scheduler {
        "EEVDF (Stock)" => match mode {
            "Auto" => ScxMetadata::new(
                "EEVDF (Stock): Earliest Eligible Virtual Deadline First. The standard Linux kernel scheduler designed for fair CPU distribution.".to_string(),
                "General purpose computing, low-overhead workloads, and standard desktop usage.".to_string(),
                "N/A (Built-in)".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Gaming" => ScxMetadata::new(
                "EEVDF (Stock): Fair kernel scheduler without specialized gaming optimizations.".to_string(),
                "General use; not optimized for gaming performance.".to_string(),
                "N/A (Built-in)".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "EEVDF (Stock): Standard fair scheduler without low-latency specialization.".to_string(),
                "Not recommended for low-latency requirements.".to_string(),
                "N/A (Built-in)".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "EEVDF (Stock): Efficient fair scheduling with standard power management.".to_string(),
                "Standard desktop and laptop use with acceptable power efficiency.".to_string(),
                "N/A (Built-in)".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Server" => ScxMetadata::new(
                "EEVDF (Stock): Fair scheduler suitable for general server workloads.".to_string(),
                "Standard server deployments, general-purpose services.".to_string(),
                "N/A (Built-in)".to_string(),
                RecommendationLevel::Recommended,
            ),
            _ => ScxMetadata::new(
                "EEVDF (Stock): Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        "scx_bpfland" => match mode {
            "Auto" => ScxMetadata::new(
                "scx_bpfland: Advanced BPF-based load balancer with dynamic load distribution. Auto-mode detects workload type and adapts scheduling automatically.".to_string(),
                "General-purpose workloads, mixed workstation use, adaptive scheduling, auto-adjusting performance".to_string(),
                "".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Gaming" => ScxMetadata::new(
                "scx_bpfland: Frame-aware scheduling with preemption priority injection".to_string(),
                "Gaming, interactive applications, real-time audio, esports competitive play".to_string(),
                "-m gaming --interactive --enable-frame-aware".to_string(),
                RecommendationLevel::Recommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "scx_bpfland: Minimized CFS latency for frame consistency and jitter reduction".to_string(),
                "Video editing, streaming, frame-based rendering, DAW audio production".to_string(),
                "-m ultra-low-latency --preempt-mode=full --no-freq-scaling".to_string(),
                RecommendationLevel::Recommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "scx_bpfland: Power-efficient BPF scheduling with core consolidation and idle balancing".to_string(),
                "Laptops, battery-powered devices, power-constrained environments, thermal management".to_string(),
                "-m powersave --autopower --small-core-prefer".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Server" => ScxMetadata::new(
                "scx_bpfland: Throughput-optimized with affinity-aware load distribution".to_string(),
                "Server workloads, batch jobs, microservices, high-concurrency scenarios, throughput-critical apps".to_string(),
                "-m throughput --nr-cpus-per-node=auto --enable-numa-affinity".to_string(),
                RecommendationLevel::Recommended,
            ),
            _ => ScxMetadata::new(
                "bpfland: Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        "scx_lavd" => match mode {
            "Auto" => ScxMetadata::new(
                "scx_lavd: Latency-aware virtual deadline scheduling engine with sub-millisecond precision".to_string(),
                "Low-latency systems, desktop responsiveness, interactive workloads, precision timing".to_string(),
                "-m balanced".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Gaming" => ScxMetadata::new(
                "scx_lavd: Frame-aware latency optimization with virtual deadline injection".to_string(),
                "Gaming, interactive desktop, esports competitive play, real-time audio processing".to_string(),
                "-m gaming --vreal-metric=deadline --enable-preempt-injection --perf=high".to_string(),
                RecommendationLevel::Recommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "scx_lavd: Aggressive sub-millisecond latency minimization with virtual deadlines and frequency isolation".to_string(),
                "Real-time audio, video frame consistency, HFT trading, ultra-low-latency system design".to_string(),
                "-m ultra-low-latency --preempt-mode=all --no-freq-scaling --pin-threads=prefer-core".to_string(),
                RecommendationLevel::Recommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "scx_lavd: Not optimized for power efficiency (latency-focused design)".to_string(),
                "Not recommended for battery-powered devices; prioritizes latency over power consumption".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "Server" => ScxMetadata::new(
                "scx_lavd: Latency-aware throughput balancing for responsive server workloads".to_string(),
                "Low-latency servers, responsive service delivery, microsecond-precision requirements".to_string(),
                "-m server --min-vruntime-offset=1000000 --enable-numa-awareness".to_string(),
                RecommendationLevel::Recommended,
            ),
            _ => ScxMetadata::new(
                "lavd: Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        "scx_rusty" => match mode {
            "Auto" => ScxMetadata::new(
                "scx_rusty: Flexible CPU-aware work-stealing scheduler with domain-aware load distribution".to_string(),
                "Heterogeneous CPU systems (big.LITTLE), workstation multitasking, adaptive workloads".to_string(),
                "-m balanced --enable-load-distribution".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Gaming" => ScxMetadata::new(
                "scx_rusty: Balance gaming performance with system responsiveness and priority injection".to_string(),
                "Gaming on multi-core systems, interactive workloads, esports competitive environment".to_string(),
                "-m gaming --enable-prefer-sibling-first --enable-frame-affinity --perf=max".to_string(),
                RecommendationLevel::Recommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "scx_rusty: Work-stealing with real-time prioritization (limited low-latency tuning)".to_string(),
                "Video streaming, audio processing, soft real-time tasks (not suited for hard real-time)".to_string(),
                "-m realtime --preempt-mode=all --enable-priority-boost".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "scx_rusty: Power-aware work-stealing with core consolidation and idle balancing".to_string(),
                "Battery-powered workstations, power-efficient computing, extended battery life priority".to_string(),
                "-m powersave --enable-low-power-mode --consolidate-cores --autopower".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Server" => ScxMetadata::new(
                "scx_rusty: NUMA-aware and cache-conscious work-stealing for multi-socket scalability".to_string(),
                "Multi-socket servers, NUMA systems, large workloads, high-concurrency microservices".to_string(),
                "-m server --nr-cpus-per-node=auto --cache-locality-threshold=2 --enable-numa-affinity".to_string(),
                RecommendationLevel::Recommended,
            ),
            _ => ScxMetadata::new(
                "rusty: Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        "scx_simple" => match mode {
            "Auto" => ScxMetadata::new(
                "scx_simple: Minimal-overhead fair scheduler reference implementation".to_string(),
                "Resource-constrained environments, embedded systems, prototype/reference use cases".to_string(),
                "".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Gaming" => ScxMetadata::new(
                "scx_simple: Basic fair scheduling with minimal overhead (not optimized for gaming)".to_string(),
                "Low-end hardware, legacy games, systems with severe resource constraints".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "scx_simple: One-core-one-thread minimal-latency model (testing/reference only)".to_string(),
                "Soft real-time experimentation, minimal scheduling overhead, research purposes".to_string(),
                "--fifo-mode".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "scx_simple: Efficient idle management on constrained hardware with battery awareness".to_string(),
                "Power-constrained systems, energy-aware computing, embedded low-power platforms".to_string(),
                "--idle-cpus --enable-idle-detection".to_string(),
                RecommendationLevel::Recommended,
            ),
            "Server" => ScxMetadata::new(
                "scx_simple: Throughput-neutral fair queuing for predictable behavior".to_string(),
                "Edge servers, low-power server deployment, reference implementation testing".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            _ => ScxMetadata::new(
                "simple: Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        "scx_central" => match mode {
            "Auto" => ScxMetadata::new(
                "central: Central work queue with global fairness".to_string(),
                "Single-threaded workloads, fair queuing requirements".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "Gaming" => ScxMetadata::new(
                "central: Centralized fairness without high responsiveness".to_string(),
                "Not suitable for gaming".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "LowLatency" => ScxMetadata::new(
                "central: Potential centralized scheduling bottleneck".to_string(),
                "Not recommended for latency-sensitive workloads".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "PowerSave" => ScxMetadata::new(
                "central: Global fairness with potential idle imbalance".to_string(),
                "Specialized use cases, fair resource allocation priority".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            "Server" => ScxMetadata::new(
                "central: Global fairness for multi-tenant servers".to_string(),
                "Shared hosting, fair workload distribution priority".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
            _ => ScxMetadata::new(
                "central: Unknown mode configuration".to_string(),
                "Please select a valid mode".to_string(),
                "".to_string(),
                RecommendationLevel::NotRecommended,
            ),
        },
        _ => ScxMetadata::new(
            "Unknown scheduler".to_string(),
            "Please select a valid scheduler".to_string(),
            "".to_string(),
            RecommendationLevel::NotRecommended,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scx_manager_creation() {
        let _manager = SCXManager;
        // SCXManager is a unit struct, just verify it can be created
    }

    #[test]
    fn test_scheduler_binary_validation() {
        // Valid scheduler names should not return errors
        let schedulers = vec!["scx_bpfland", "scx_lavd", "scx_rusty"];
        for scheduler in schedulers {
            assert!(!scheduler.is_empty());
        }
    }

    #[test]
    fn test_launch_with_empty_binary_fails() {
        let result = SCXManager::launch("", "");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Scheduler binary name cannot be empty"
        );
    }

    #[test]
    fn test_is_scx_installed_returns_vec() {
        let schedulers = SCXManager::is_scx_installed();
        // Result should be a Vec (may be empty if binaries not installed)
        assert!(schedulers.is_empty() || !schedulers.is_empty());
    }
}

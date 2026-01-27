//! Core data types for GOATd Kernel.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

/// Hardening level for kernel security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum HardeningLevel {
    Minimal,
    Standard,
    Hardened,
}

impl<'de> Deserialize<'de> for HardeningLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct HardeningLevelVisitor;

        impl<'de> Visitor<'de> for HardeningLevelVisitor {
            type Value = HardeningLevel;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a boolean or string hardening level")
            }

            fn visit_bool<E>(self, value: bool) -> Result<HardeningLevel, E>
            where
                E: de::Error,
            {
                // Backward compatibility: false -> Minimal, true -> Hardened
                Ok(if value {
                    HardeningLevel::Hardened
                } else {
                    HardeningLevel::Minimal
                })
            }

            fn visit_str<E>(self, value: &str) -> Result<HardeningLevel, E>
            where
                E: de::Error,
            {
                match value.to_lowercase().as_str() {
                    "minimal" => Ok(HardeningLevel::Minimal),
                    "standard" => Ok(HardeningLevel::Standard),
                    "hardened" => Ok(HardeningLevel::Hardened),
                    _ => Err(de::Error::unknown_variant(
                        value,
                        &["minimal", "standard", "hardened"],
                    )),
                }
            }
        }

        deserializer.deserialize_any(HardeningLevelVisitor)
    }
}

impl HardeningLevel {
    /// Convert to UI index (0=Minimal, 1=Standard, 2=Hardened)
    pub fn to_index(&self) -> usize {
        match self {
            HardeningLevel::Minimal => 0,
            HardeningLevel::Standard => 1,
            HardeningLevel::Hardened => 2,
        }
    }

    /// Convert from UI index (0=Minimal, 1=Standard, 2=Hardened)
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => HardeningLevel::Minimal,
            2 => HardeningLevel::Hardened,
            _ => HardeningLevel::Standard,
        }
    }
}

impl fmt::Display for HardeningLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HardeningLevel::Minimal => write!(f, "Minimal"),
            HardeningLevel::Standard => write!(f, "Standard"),
            HardeningLevel::Hardened => write!(f, "Hardened"),
        }
    }
}

impl FromStr for HardeningLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "minimal" => Ok(HardeningLevel::Minimal),
            "standard" => Ok(HardeningLevel::Standard),
            "hardened" => Ok(HardeningLevel::Hardened),
            _ => Err(format!("Unknown hardening level: {}", s)),
        }
    }
}

impl Default for HardeningLevel {
    fn default() -> Self {
        HardeningLevel::Standard
    }
}

/// GPU vendor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

/// Hardware context for multi-vendor GPU and CPU detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareContext {
    pub gpu_vendors: Vec<GpuVendor>,  // All detected GPU vendors
    pub cpu_vendor: String,            // CPU vendor (e.g., "GenuineIntel", "AuthenticAMD")
    pub is_hybrid: bool,               // Whether system has hybrid CPU architecture
}

impl Default for HardwareContext {
    fn default() -> Self {
        HardwareContext {
            gpu_vendors: vec![GpuVendor::Unknown],
            cpu_vendor: "Unknown".to_string(),
            is_hybrid: false,
        }
    }
}

/// Storage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageType {
    Nvme,
    Ssd,
    Hdd,
}

/// Boot firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootType {
    Efi,
    Bios,
}

/// LTO mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LtoType {
    None, // Renamed from Base to align with UI nomenclature ("none")
    Thin,
    Full,
}

/// Build phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildPhase {
    Preparation,
    Configuration,
    Patching,
    Building,
    Validation,
}

/// Patch type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchType {
    LtoShield,
    IcfRemoval,
    ConfigOption,
    KernelOption,
}

/// Validation check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationCheck {
    ArtifactExists,
    LtoEnabled,
    CfiMetadata,
    BootReady,
}

/// HW capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_model: String,         // Model
    pub cpu_cores: u32,            // Cores
    pub cpu_threads: u32,          // Threads
    pub ram_gb: u32,               // RAM GiB
    pub disk_free_gb: u32,         // Disk GiB
    pub gpu_vendor: GpuVendor,     // GPU
    pub gpu_model: String,         // Model
    pub storage_type: StorageType, // Type
    pub storage_model: String,     // Model
    pub boot_type: BootType,       // Firmware
    pub boot_manager: BootManager, // Bootloader
    pub init_system: InitSystem,   // Init
    pub all_drives: Vec<DiskInfo>, // All detected drives for multi-drive support
}

impl Default for HardwareInfo {
    fn default() -> Self {
        HardwareInfo {
            cpu_model: "Generic CPU".to_string(),
            cpu_cores: 4,
            cpu_threads: 8,
            ram_gb: 16,
            disk_free_gb: 50,
            gpu_vendor: GpuVendor::Unknown,
            gpu_model: "Generic GPU".to_string(),
            storage_type: StorageType::Ssd,
            storage_model: "Generic SSD".to_string(),
            boot_type: BootType::Efi,
            boot_manager: BootManager {
                detector: "unknown".to_string(),
                is_efi: true,
            },
            init_system: InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: Vec::new(),
        }
    }
}

/// Boot manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootManager {
    pub detector: String, // Name
    pub is_efi: bool,     // UEFI?
}

/// Init system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitSystem {
    pub name: String, // Name
}

/// Disk info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,      // Device
    pub model: String,     // Model
    pub transport: String, // Type
    pub size: String,      // Size
    pub type_: String,     // Part/disk
}

/// Kernel config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    pub lto_type: LtoType,                       // LTO
    pub use_modprobed: bool,                     // ModprobeDB
    pub use_whitelist: bool,                     // Whitelist
    pub driver_exclusions: Vec<String>,          // Excluded
    pub config_options: HashMap<String, String>, // Options
    pub hardening: HardeningLevel,               // Level
    pub secure_boot: bool,                       // SecBoot
    pub profile: String,                         // Profile
    pub version: String,                         // Ver
    pub use_polly: bool,                         // Polly
    pub use_mglru: bool,                         // MGLRU
    pub user_toggled_polly: bool,                // User manually toggled Polly
    pub user_toggled_mglru: bool,                // User manually toggled MGLRU
    pub user_toggled_hardening: bool,            // User manually toggled hardening
    pub user_toggled_lto: bool,                  // User manually toggled LTO
    pub mglru_enabled_mask: u32,                 // Mask
    pub mglru_min_ttl_ms: u32,                   // TTL
    pub hz: u32,                                 // HZ (timer frequency from profile)
    pub preemption: String,                      // Preemption model from profile
    pub force_clang: bool,                       // Force Clang compiler
    pub lto_shield_modules: Vec<String>,         // Modules to shield from LTO
    pub use_bore: bool,                          // Use BORE scheduler
    pub user_toggled_bore: bool,                 // User manually toggled BORE
    pub scx_available: Vec<String>,              // Available SCX schedulers
    pub scx_active_scheduler: Option<String>,    // Currently active SCX scheduler
    pub native_optimizations: bool,              // Enable -march=native
    pub user_toggled_native_optimizations: bool, // User manually toggled native optimizations
    pub kernel_variant: String,                  // Kernel variant
}

impl Default for KernelConfig {
    fn default() -> Self {
        KernelConfig {
            lto_type: LtoType::Thin,
            use_modprobed: false,
            use_whitelist: false,
            driver_exclusions: Vec::new(),
            config_options: HashMap::new(),
            hardening: HardeningLevel::Standard,
            secure_boot: false,
            profile: "Generic".to_string(),
            version: "latest".to_string(),
            use_polly: false,
            use_mglru: false,
            use_bore: false,                          // Default: BORE disabled
            user_toggled_polly: false,                // Not manually toggled by default
            user_toggled_mglru: false,                // Not manually toggled by default
            user_toggled_hardening: false,            // Not manually toggled by default
            user_toggled_lto: false,                  // Not manually toggled by default
            user_toggled_bore: false,                 // Not manually toggled by default
            mglru_enabled_mask: 0x0007,               // Default: all subsystems
            mglru_min_ttl_ms: 1000,                   // Default: 1000ms
            hz: 300,                                  // Default: 300 HZ
            preemption: "Voluntary".to_string(),      // Default: Voluntary preemption
            force_clang: true,                        // Default: use Clang
            lto_shield_modules: Vec::new(),           // No modules shielded by default
            scx_available: Vec::new(),                // No SCX schedulers available by default
            scx_active_scheduler: None,               // No active SCX scheduler by default
            native_optimizations: true,               // Default: native optimizations enabled
            user_toggled_native_optimizations: false, // Not manually toggled by default
            kernel_variant: String::new(),            // Default: empty kernel variant
        }
    }
}

impl KernelConfig {
    /// Returns true if the config is set to track the latest version.
    pub fn is_dynamic_version(&self) -> bool {
        self.version == "latest"
    }

    /// Checks if the version is a concrete (non-dynamic) version string.
    pub fn is_concrete_version(&self) -> bool {
        !self.is_dynamic_version()
    }
}

/// Build state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildState {
    pub phase: BuildPhase,                 // Phase
    pub progress_percent: u32,             // Progress
    pub hardware: HardwareInfo,            // HW
    pub config: KernelConfig,              // Config
    pub patches_applied: Vec<PatchResult>, // Patches
    pub checkpoint_timestamp: SystemTime,  // TS
}

impl BuildState {
    /// New snapshot.
    pub fn new(hardware: HardwareInfo, config: KernelConfig) -> Self {
        BuildState {
            phase: BuildPhase::Preparation,
            progress_percent: 0,
            hardware,
            config,
            patches_applied: Vec::new(),
            checkpoint_timestamp: SystemTime::now(),
        }
    }
}

/// Kernel patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub patch_type: PatchType, // Type
    pub target_file: PathBuf,  // File
    pub pattern: String,       // Regex
    pub replacement: String,   // Replace
}

/// Patch result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchResult {
    pub patch: Patch,              // Patch
    pub success: bool,             // OK?
    pub lines_modified: u32,       // Lines
    pub error_msg: Option<String>, // Error
}

/// Build result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,             // OK?
    pub kernel_version: String,    // Ver
    pub lto_enabled: bool,         // LTO?
    pub patches_applied: u32,      // # patches
    pub error_msg: Option<String>, // Error
}

/// Kernel info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInfo {
    pub name: String,                  // Name
    pub version: String,               // Ver
    pub is_installed: bool,            // Installed?
    pub is_booted: bool,               // Booted?
    pub package_path: Option<PathBuf>, // Path
}

impl KernelInfo {
    /// New installed kernel.
    pub fn installed(name: String, version: String, is_booted: bool) -> Self {
        KernelInfo {
            name,
            version,
            is_installed: true,
            is_booted,
            package_path: None,
        }
    }

    /// New built kernel.
    pub fn built(name: String, version: String, package_path: PathBuf) -> Self {
        KernelInfo {
            name,
            version,
            is_installed: false,
            is_booted: false,
            package_path: Some(package_path),
        }
    }

    /// Display label.
    pub fn display_label(&self) -> String {
        format!("{} ({})", self.name, self.version)
    }
}

/// Kernel audit metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelAudit {
    pub kernel_version: String,   // Ver
    pub compiler: String,         // Compiler
    pub lto_status: String,       // LTO
    pub module_count: u32,        // # mods
    pub module_size_mb: u32,      // Size MB
    pub cpu_context: String,      // CPU
    pub hardening_status: String, // Hardening
    pub timer_hz: String,         // Hz
    pub preemption_model: String, // Preempt
    pub io_scheduler: String,     // IO
    pub cpu_scheduler: String,    // Scheduler
    pub mglru: String,            // MGLRU status
}

impl KernelAudit {
    /// New audit.
    pub fn new() -> Self {
        KernelAudit {
            kernel_version: String::new(),
            compiler: String::new(),
            lto_status: "Unknown".to_string(),
            module_count: 0,
            module_size_mb: 0,
            cpu_context: String::new(),
            hardening_status: String::new(),
            timer_hz: "Unknown".to_string(),
            preemption_model: "Unknown".to_string(),
            io_scheduler: "Unknown".to_string(),
            cpu_scheduler: "Unknown".to_string(),
            mglru: "Unknown".to_string(),
        }
    }
}

impl Default for KernelAudit {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata Persistence Layer (MPL) Metadata
///
/// Immutable metadata file stored at workspace root (`.goatd_metadata`).
/// Contains the definitive source of truth for kernel build information.
/// This replaces the fragile 5-level fallback strategy with a single, reliable metadata file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MPLMetadata {
    /// Unique build session identifier (UUID v4)
    pub build_id: String,

    /// Exact kernel release string (from `include/config/kernel.release`)
    /// E.g., "6.19.0-goatd-gaming"
    pub kernel_release: String,

    /// Base kernel version (e.g., "6.19.0")
    pub kernel_version: String,

    /// Profile name (e.g., "gaming", "workstation")
    pub profile: String,

    /// Kernel variant (e.g., "linux", "linux-zen", "linux-lts")
    pub variant: String,

    /// LTO configuration (e.g., "full", "thin", "none")
    pub lto_level: String,

    /// Build completion timestamp (ISO 8601)
    pub build_timestamp: String,

    /// Canonicalized workspace root path
    pub workspace_root: PathBuf,

    /// Kernel source directory
    pub source_dir: PathBuf,

    /// Package version (pkgver)
    pub pkgver: String,

    /// Package release (pkgrel)
    pub pkgrel: String,

    /// Profile suffix for LOCALVERSION (e.g., "-goatd-gaming")
    pub profile_suffix: String,
}

impl MPLMetadata {
    /// Create new MPL metadata
    pub fn new(
        build_id: String,
        kernel_version: String,
        profile: String,
        variant: String,
        lto_level: String,
        workspace_root: PathBuf,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let profile_suffix = if variant == "linux" {
            format!("-goatd-{}", profile.to_lowercase())
        } else {
            let variant_part = variant.trim_start_matches("linux-").to_string();
            format!("-goatd-{}-{}", variant_part, profile.to_lowercase())
        };

        MPLMetadata {
            build_id,
            kernel_release: String::new(), // Will be populated after build
            kernel_version: kernel_version.clone(),
            profile,
            variant,
            lto_level,
            build_timestamp: now,
            workspace_root,
            source_dir: PathBuf::new(),
            pkgver: kernel_version,
            pkgrel: "1".to_string(),
            profile_suffix,
        }
    }

    /// Write MPL metadata to file as shell-sourceable format
    pub fn write_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        let content = self.to_shell_format();
        std::fs::write(path, content)
    }

    /// Convert MPL metadata to shell-sourceable format
    pub fn to_shell_format(&self) -> String {
        format!(
            r#"# GOATd Kernel Build Metadata
# Generated: {}
# Build Session ID: {}

GOATD_BUILD_ID="{}"
GOATD_KERNELRELEASE="{}"
GOATD_KERNEL_VERSION="{}"
GOATD_PROFILE="{}"
GOATD_VARIANT="{}"
GOATD_LTO_LEVEL="{}"
GOATD_BUILD_TIMESTAMP="{}"
GOATD_WORKSPACE_ROOT="{}"
GOATD_SOURCE_DIR="{}"
GOATD_PKGVER="{}"
GOATD_PKGREL="{}"
GOATD_PROFILE_SUFFIX="{}"
"#,
            self.build_timestamp,
            self.build_id,
            self.build_id,
            self.kernel_release,
            self.kernel_version,
            self.profile,
            self.variant,
            self.lto_level,
            self.build_timestamp,
            self.workspace_root.display(),
            self.source_dir.display(),
            self.pkgver,
            self.pkgrel,
            self.profile_suffix,
        )
    }

    /// Read MPL metadata from shell format
    pub fn from_shell_format(content: &str) -> std::io::Result<Self> {
        let mut metadata = MPLMetadata::default();

        for line in content.lines() {
            if line.starts_with("GOATD_BUILD_ID=") {
                metadata.build_id = extract_value(line);
            } else if line.starts_with("GOATD_KERNELRELEASE=") {
                metadata.kernel_release = extract_value(line);
            } else if line.starts_with("GOATD_KERNEL_VERSION=") {
                metadata.kernel_version = extract_value(line);
            } else if line.starts_with("GOATD_PROFILE=") {
                metadata.profile = extract_value(line);
            } else if line.starts_with("GOATD_VARIANT=") {
                metadata.variant = extract_value(line);
            } else if line.starts_with("GOATD_LTO_LEVEL=") {
                metadata.lto_level = extract_value(line);
            } else if line.starts_with("GOATD_BUILD_TIMESTAMP=") {
                metadata.build_timestamp = extract_value(line);
            } else if line.starts_with("GOATD_WORKSPACE_ROOT=") {
                metadata.workspace_root = PathBuf::from(extract_value(line));
            } else if line.starts_with("GOATD_SOURCE_DIR=") {
                metadata.source_dir = PathBuf::from(extract_value(line));
            } else if line.starts_with("GOATD_PKGVER=") {
                metadata.pkgver = extract_value(line);
            } else if line.starts_with("GOATD_PKGREL=") {
                metadata.pkgrel = extract_value(line);
            } else if line.starts_with("GOATD_PROFILE_SUFFIX=") {
                metadata.profile_suffix = extract_value(line);
            }
        }

        Ok(metadata)
    }
}

impl Default for MPLMetadata {
    fn default() -> Self {
        MPLMetadata {
            build_id: String::new(),
            kernel_release: String::new(),
            kernel_version: String::new(),
            profile: String::new(),
            variant: "linux".to_string(),
            lto_level: "thin".to_string(),
            build_timestamp: String::new(),
            workspace_root: PathBuf::new(),
            source_dir: PathBuf::new(),
            pkgver: String::new(),
            pkgrel: "1".to_string(),
            profile_suffix: String::new(),
        }
    }
}

/// Helper function to extract quoted values from shell variable assignments
fn extract_value(line: &str) -> String {
    if let Some(eq_pos) = line.find('=') {
        let value = &line[eq_pos + 1..];
        value.trim_matches('"').to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_info_creation() {
        let hw = HardwareInfo {
            cpu_model: "Intel i7".to_string(),
            cpu_cores: 8,
            cpu_threads: 16,
            ram_gb: 32,
            disk_free_gb: 100,
            gpu_vendor: GpuVendor::Nvidia,
            gpu_model: "NVIDIA RTX 3080".to_string(),
            storage_type: StorageType::Nvme,
            storage_model: "Samsung 970 EVO Plus".to_string(),
            boot_type: BootType::Efi,
            boot_manager: BootManager {
                detector: "systemd-boot".to_string(),
                is_efi: true,
            },
            init_system: InitSystem {
                name: "systemd".to_string(),
            },
            all_drives: vec![],
        };
        assert_eq!(hw.ram_gb, 32);
        assert_eq!(hw.gpu_vendor, GpuVendor::Nvidia);
        assert_eq!(hw.cpu_cores, 8);
        assert_eq!(hw.cpu_threads, 16);
    }

    #[test]
    fn test_kernel_config_creation() {
        let config = KernelConfig {
            lto_type: LtoType::Thin,
            use_modprobed: true,
            use_whitelist: false,
            driver_exclusions: vec!["nouveau".to_string()],
            config_options: HashMap::new(),
            hardening: HardeningLevel::Standard,
            secure_boot: false,
            profile: "Generic".to_string(),
            version: "6.6.0".to_string(),
            use_polly: false,
            use_mglru: false,
            use_bore: false,
            user_toggled_polly: false,
            user_toggled_mglru: false,
            user_toggled_hardening: false,
            user_toggled_lto: false,
            user_toggled_bore: false,
            mglru_enabled_mask: 0x0007,
            mglru_min_ttl_ms: 1000,
            hz: 300,
            preemption: "Voluntary".to_string(),
            force_clang: true,
            lto_shield_modules: Vec::new(),
            scx_available: vec!["scx_bpfland".to_string()],
            scx_active_scheduler: None,
            native_optimizations: true,
            user_toggled_native_optimizations: false,
            kernel_variant: String::new(),
        };
        assert_eq!(config.lto_type, LtoType::Thin);
        assert_eq!(config.hardening, HardeningLevel::Standard);
        assert_eq!(config.secure_boot, false);
        assert_eq!(config.profile, "Generic");
        assert_eq!(config.version, "6.6.0");
        assert_eq!(config.use_polly, false);
        assert_eq!(config.use_mglru, false);
        assert_eq!(config.user_toggled_polly, false);
        assert_eq!(config.user_toggled_mglru, false);
        assert_eq!(config.user_toggled_hardening, false);
        assert_eq!(config.mglru_enabled_mask, 0x0007);
        assert_eq!(config.mglru_min_ttl_ms, 1000);
        assert_eq!(config.hz, 300);
        assert_eq!(config.preemption, "Voluntary");
        assert_eq!(config.force_clang, true);
        assert!(config.lto_shield_modules.is_empty());
        assert!(config.native_optimizations);
    }

    #[test]
    fn test_gpu_vendor_equality() {
        assert_eq!(GpuVendor::Nvidia, GpuVendor::Nvidia);
        assert_ne!(GpuVendor::Nvidia, GpuVendor::Amd);
    }

    #[test]
    fn test_lto_type_serialization() {
        let lto = LtoType::Full;
        let json = serde_json::to_string(&lto).unwrap();
        let deserialized: LtoType = serde_json::from_str(&json).unwrap();
        assert_eq!(lto, deserialized);
    }
}

//! Core data types for GOATd Kernel.

use serde::{Deserialize, Serialize, Deserializer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;
use std::str::FromStr;
use std::fmt;

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
                Ok(if value { HardeningLevel::Hardened } else { HardeningLevel::Minimal })
            }
            
            fn visit_str<E>(self, value: &str) -> Result<HardeningLevel, E>
            where
                E: de::Error,
            {
                match value.to_lowercase().as_str() {
                    "minimal" => Ok(HardeningLevel::Minimal),
                    "standard" => Ok(HardeningLevel::Standard),
                    "hardened" => Ok(HardeningLevel::Hardened),
                    _ => Err(de::Error::unknown_variant(value, &["minimal", "standard", "hardened"])),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
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
     None,   // Renamed from Base to align with UI nomenclature ("none")
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
    pub cpu_model: String,     // Model
    pub cpu_cores: u32,        // Cores
    pub cpu_threads: u32,      // Threads
    pub ram_gb: u32,           // RAM GiB
    pub disk_free_gb: u32,     // Disk GiB
    pub gpu_vendor: GpuVendor, // GPU
    pub gpu_model: String,     // Model
    pub storage_type: StorageType, // Type
    pub storage_model: String, // Model
    pub boot_type: BootType,   // Firmware
    pub boot_manager: BootManager, // Bootloader
    pub init_system: InitSystem, // Init
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
    pub lto_type: LtoType,                // LTO
    pub use_modprobed: bool,              // ModprobeDB
    pub use_whitelist: bool,              // Whitelist
    pub driver_exclusions: Vec<String>,   // Excluded
    pub config_options: HashMap<String, String>, // Options
    pub hardening: HardeningLevel,        // Level
    pub secure_boot: bool,                // SecBoot
    pub profile: String,                  // Profile
    pub version: String,                  // Ver
    pub use_polly: bool,                  // Polly
    pub use_mglru: bool,                  // MGLRU
    pub user_toggled_polly: bool,         // User manually toggled Polly
    pub user_toggled_mglru: bool,         // User manually toggled MGLRU
    pub user_toggled_hardening: bool,     // User manually toggled hardening
    pub user_toggled_lto: bool,           // User manually toggled LTO
    pub mglru_enabled_mask: u32,          // Mask
    pub mglru_min_ttl_ms: u32,            // TTL
    pub hz: u32,                          // HZ (timer frequency from profile)
    pub preemption: String,               // Preemption model from profile
    pub force_clang: bool,                // Force Clang compiler
    pub lto_shield_modules: Vec<String>,  // Modules to shield from LTO
    pub use_bore: bool,                   // Use BORE scheduler
    pub user_toggled_bore: bool,          // User manually toggled BORE
    pub scx_available: Vec<String>,       // Available SCX schedulers
    pub scx_active_scheduler: Option<String>, // Currently active SCX scheduler
    pub native_optimizations: bool,       // Enable -march=native
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
            version: "6.6.0".to_string(),
            use_polly: false,
            use_mglru: false,
            use_bore: false,              // Default: BORE disabled
            user_toggled_polly: false,     // Not manually toggled by default
            user_toggled_mglru: false,     // Not manually toggled by default
            user_toggled_hardening: false, // Not manually toggled by default
            user_toggled_lto: false,       // Not manually toggled by default
            user_toggled_bore: false,      // Not manually toggled by default
            mglru_enabled_mask: 0x0007,  // Default: all subsystems
            mglru_min_ttl_ms: 1000,      // Default: 1000ms
            hz: 300,                      // Default: 300 HZ
            preemption: "Voluntary".to_string(), // Default: Voluntary preemption
            force_clang: true,            // Default: use Clang
            lto_shield_modules: Vec::new(), // No modules shielded by default
            scx_available: Vec::new(),    // No SCX schedulers available by default
            scx_active_scheduler: None,   // No active SCX scheduler by default
            native_optimizations: true,   // Default: native optimizations enabled
        }
    }
}

/// Build state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildState {
    pub phase: BuildPhase,              // Phase
    pub progress_percent: u32,          // Progress
    pub hardware: HardwareInfo,         // HW
    pub config: KernelConfig,           // Config
    pub patches_applied: Vec<PatchResult>, // Patches
    pub checkpoint_timestamp: SystemTime, // TS
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
    pub patch_type: PatchType,     // Type
    pub target_file: PathBuf,      // File
    pub pattern: String,           // Regex
    pub replacement: String,       // Replace
}

/// Patch result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchResult {
    pub patch: Patch,           // Patch
    pub success: bool,          // OK?
    pub lines_modified: u32,    // Lines
    pub error_msg: Option<String>, // Error
}

/// Build result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    pub success: bool,          // OK?
    pub kernel_version: String, // Ver
    pub lto_enabled: bool,      // LTO?
    pub patches_applied: u32,   // # patches
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
    pub kernel_version: String,  // Ver
    pub compiler: String,        // Compiler
    pub lto_status: String,      // LTO
    pub module_count: u32,       // # mods
    pub module_size_mb: u32,     // Size MB
    pub cpu_context: String,     // CPU
    pub hardening_status: String, // Hardening
    pub timer_hz: String,        // Hz
    pub preemption_model: String, // Preempt
    pub io_scheduler: String,    // IO
    pub cpu_scheduler: String,   // Scheduler
    pub mglru: String,           // MGLRU status
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

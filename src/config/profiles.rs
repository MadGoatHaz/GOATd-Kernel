//! Kernel build profiles.

use crate::models::{LtoType, HardeningLevel};
use std::collections::HashMap;
use lazy_static::lazy_static;

lazy_static! {
    static ref PROFILES: HashMap<String, ProfileDefinition> = {
        let mut profiles = HashMap::new();
        
        profiles.insert(
            "generic".to_string(),
            ProfileDefinition::new(
                "Generic".to_string(),
                "Balanced (Clang, EEVDF, -O2)".to_string(),
                "A reasonable default for most systems. Balances performance and stability.".to_string(),
                true, LtoType::Thin, false, HardeningLevel::Standard,
                "Voluntary".to_string(), 300, false, false, true,
            ),
        );

        profiles.insert(
            "gaming".to_string(),
            ProfileDefinition::new(
                "Gaming".to_string(),
                "High-performance (Clang, 1000Hz, Polly)".to_string(),
                "Optimizes for ultra-low latency and maximum FPS. Best for competitive gaming and real-time interactive apps.".to_string(),
                true, LtoType::Thin, true, HardeningLevel::Standard,
                "Full".to_string(), 1000, true, true, true,
            ),
        );

        profiles.insert(
            "workstation".to_string(),
            ProfileDefinition::new(
                "Workstation".to_string(),
                "Professional (Clang, Hardened)".to_string(),
                "Balances aggressive performance with hardened security and rock-solid stability. Best for professional development.".to_string(),
                true, LtoType::Thin, true, HardeningLevel::Hardened,
                "Full".to_string(), 1000, false, true, true,
            ),
        );

        profiles.insert(
            "server".to_string(),
            ProfileDefinition::new(
                "Server".to_string(),
                "High-throughput (Clang, Full LTO, EEVDF, 100Hz)".to_string(),
                "Maximizes throughput and multi-core efficiency for non-interactive workloads. Best for hosting and databases.".to_string(),
                true, LtoType::Full, true, HardeningLevel::Hardened,
                "Server".to_string(), 100, false, true, true,
            ),
        );

        profiles.insert(
            "laptop".to_string(),
            ProfileDefinition::new(
                "Laptop".to_string(),
                "Power-efficient (Clang, EEVDF, Thin LTO)".to_string(),
                "Prioritizes power efficiency and thermal management while maintaining responsiveness. Best for maximizing battery life.".to_string(),
                true, LtoType::Thin, true, HardeningLevel::Standard,
                "Voluntary".to_string(), 300, false, true, true,
            ),
        );

        profiles
    };
}

/// Profile definition.
#[derive(Debug, Clone)]
pub struct ProfileDefinition {
    pub name: String,                   // Name
    pub description: String,            // Desc
    pub explanation: String,            // User-friendly explanation
    pub use_clang: bool,                // Clang?
    pub default_lto: LtoType,           // LTO
    pub enable_module_stripping: bool,  // Strip?
    pub hardening_level: HardeningLevel, // Hardening level
    pub preemption: String,             // Preempt
    pub hz: u32,                        // Hz
    pub use_polly: bool,                // Polly?
    pub use_mglru: bool,                // MGLRU?
    pub native_optimizations: bool,     // Native optimizations (-march=native)?
}

impl ProfileDefinition {
    /// Create a new profile definition.
    pub fn new(
        name: String,
        description: String,
        explanation: String,
        use_clang: bool,
        default_lto: LtoType,
        enable_module_stripping: bool,
        hardening_level: HardeningLevel,
        preemption: String,
        hz: u32,
        use_polly: bool,
        use_mglru: bool,
        native_optimizations: bool,
    ) -> Self {
        ProfileDefinition {
            name,
            description,
            explanation,
            use_clang,
            default_lto,
            enable_module_stripping,
            hardening_level,
            preemption,
            hz,
            use_polly,
            use_mglru,
            native_optimizations,
        }
    }
}

/// Get available profiles.
/// Returns a clone of the lazily-initialized profiles HashMap.
pub fn get_available_profiles() -> HashMap<String, ProfileDefinition> {
    PROFILES.clone()
}

/// Get profile by name.
pub fn get_profile(name: &str) -> Option<ProfileDefinition> {
    let lowercase_name = name.to_lowercase();
    PROFILES.get(&lowercase_name).cloned()
}

// NOTE: apply_profile has been removed. Profile application logic is now centralized
// in the Finalizer (config::finalizer) which applies the hierarchy:
// Hardware > User Overrides > Profile Defaults
//
// profiles.rs is now a pure data provider - it only exposes get_available_profiles()
// and get_profile() for retrieving ProfileDefinition data structures.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_available_profiles() {
        let profiles = get_available_profiles();
        assert!(profiles.contains_key("generic"));
        assert!(profiles.contains_key("gaming"));
        assert!(profiles.contains_key("workstation"));
        assert!(profiles.contains_key("server"));
        assert!(profiles.contains_key("laptop"));
    }

    #[test]
    fn test_get_gaming_profile() {
        let profile = get_profile("gaming").expect("gaming profile not found");
        assert_eq!(profile.name, "Gaming");
        assert!(profile.use_clang);
        assert_eq!(profile.default_lto, LtoType::Thin);
        assert!(profile.enable_module_stripping);
        assert_eq!(profile.hardening_level, HardeningLevel::Standard);
        assert_eq!(profile.preemption, "Full");
        assert_eq!(profile.hz, 1000);
        assert!(profile.use_polly);
        assert!(profile.use_mglru);
        assert!(profile.native_optimizations);
    }

    #[test]
    fn test_get_generic_profile() {
        let profile = get_profile("generic").expect("generic profile not found");
        assert_eq!(profile.name, "Generic");
        assert!(profile.use_clang); // Generic now uses Clang like all profiles
        assert_eq!(profile.default_lto, LtoType::Thin);
        assert!(!profile.enable_module_stripping);
        assert_eq!(profile.hardening_level, HardeningLevel::Standard);
        assert!(profile.native_optimizations);
    }

    #[test]
    fn test_get_server_profile() {
        let profile = get_profile("server").expect("server profile not found");
        assert_eq!(profile.name, "Server");
        assert!(profile.use_clang);
        assert_eq!(profile.default_lto, LtoType::Full);
        assert!(profile.enable_module_stripping);
        assert_eq!(profile.hardening_level, HardeningLevel::Hardened);
        assert!(profile.use_mglru);  // Server uses MGLRU for memory efficiency
        assert!(profile.native_optimizations);
    }

    #[test]
    fn test_get_laptop_profile() {
        let profile = get_profile("laptop").expect("laptop profile not found");
        assert_eq!(profile.name, "Laptop");
        assert!(profile.use_clang);
        assert_eq!(profile.default_lto, LtoType::Thin);
        assert!(profile.enable_module_stripping);
        assert_eq!(profile.hardening_level, HardeningLevel::Standard);
        assert_eq!(profile.preemption, "Voluntary");
        assert_eq!(profile.hz, 300);
        assert!(profile.native_optimizations);
    }

    #[test]
    fn test_get_workstation_profile() {
        let profile = get_profile("workstation").expect("workstation profile not found");
        assert_eq!(profile.name, "Workstation");
        assert!(profile.use_clang);
        assert_eq!(profile.default_lto, LtoType::Thin);
        assert!(profile.enable_module_stripping);
        assert_eq!(profile.hardening_level, HardeningLevel::Hardened);
        assert_eq!(profile.preemption, "Full");
        assert_eq!(profile.hz, 1000);
        assert!(profile.native_optimizations);
    }
}

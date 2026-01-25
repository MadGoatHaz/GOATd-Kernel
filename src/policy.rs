//! Policy engine for GPU, LTO, and drivers.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// GPU decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuDecision {
    NvidiaOnly,
    AmdOnly,
    Both,
    IntelOnly,
    None,
}

impl GpuDecision {
    /// Human-readable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            GpuDecision::NvidiaOnly => "nvidia_only",
            GpuDecision::AmdOnly => "amd_only",
            GpuDecision::Both => "both",
            GpuDecision::IntelOnly => "intel_only",
            GpuDecision::None => "none",
        }
    }
}

/// LTO decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LtoDecision {
    FullLto,
    ThinLto,
    NoLto,
}

impl LtoDecision {
    /// Human-readable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            LtoDecision::FullLto => "full",
            LtoDecision::ThinLto => "thin",
            LtoDecision::NoLto => "none",
        }
    }

    /// Is LTO enabled?
    pub fn is_enabled(&self) -> bool {
        matches!(self, LtoDecision::FullLto | LtoDecision::ThinLto)
    }
}

/// Driver policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPolicy {
    pub name: String,              // Name
    pub include: Vec<String>,      // Include
    pub exclude: Vec<String>,      // Exclude
    pub requires_lto_shield: bool, // Shield?
    pub reason: String,            // Reason
}

impl DriverPolicy {
    /// New policy.
    pub fn new(
        name: impl Into<String>,
        include: Vec<String>,
        exclude: Vec<String>,
        requires_lto_shield: bool,
        reason: impl Into<String>,
    ) -> Self {
        DriverPolicy {
            name: name.into(),
            include,
            exclude,
            requires_lto_shield,
            reason: reason.into(),
        }
    }

    /// Exclude as set.
    pub fn exclude_set(&self) -> HashSet<String> {
        self.exclude.iter().cloned().collect()
    }

    /// Include as set.
    pub fn include_set(&self) -> HashSet<String> {
        self.include.iter().cloned().collect()
    }
}

/// HW policy output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwarePolicy {
    pub gpu_decision: GpuDecision,         // GPU
    pub lto_decision: LtoDecision,         // LTO
    pub driver_policy: DriverPolicy,       // Drivers
    pub lto_auto_downgraded: bool,         // Downgraded?
    pub mutual_exclusivity_enforced: bool, // MutEx?
    pub rationale: String,                 // Reason
}

impl HardwarePolicy {
    /// New policy.
    pub fn new(
        gpu_decision: GpuDecision,
        lto_decision: LtoDecision,
        driver_policy: DriverPolicy,
    ) -> Self {
        HardwarePolicy {
            gpu_decision,
            lto_decision,
            driver_policy,
            lto_auto_downgraded: false,
            mutual_exclusivity_enforced: false,
            rationale: String::new(),
        }
    }

    /// Mark LTO downgraded.
    pub fn mark_lto_auto_downgraded(&mut self, reason: impl Into<String>) {
        self.lto_auto_downgraded = true;
        self.rationale = reason.into();
    }

    /// Mark mutual exclusivity enforced.
    pub fn mark_mutual_exclusivity_enforced(&mut self) {
        self.mutual_exclusivity_enforced = true;
    }
}

/// Policy application result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyApplicationResult {
    pub success: bool,                 // OK?
    pub policy: HardwarePolicy,        // Policy
    pub drivers_excluded: Vec<String>, // Excluded
    pub error: Option<String>,         // Error
}

/// GPU detection info.
#[derive(Debug, Clone, Copy)]
pub struct GpuDetectionInfo {
    pub has_nvidia: bool, // NVIDIA?
    pub has_amd: bool,    // AMD?
    pub has_intel: bool,  // Intel?
}

impl GpuDetectionInfo {
    /// To GPU decision.
    pub fn to_decision(&self) -> GpuDecision {
        match (self.has_nvidia, self.has_amd, self.has_intel) {
            (true, false, _) => GpuDecision::NvidiaOnly,
            (false, true, _) => GpuDecision::AmdOnly,
            (true, true, _) => GpuDecision::Both,
            (false, false, true) => GpuDecision::IntelOnly,
            _ => GpuDecision::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_decision_str() {
        assert_eq!(GpuDecision::NvidiaOnly.as_str(), "nvidia_only");
        assert_eq!(GpuDecision::AmdOnly.as_str(), "amd_only");
        assert_eq!(GpuDecision::Both.as_str(), "both");
    }

    #[test]
    fn test_lto_decision_enabled() {
        assert!(LtoDecision::FullLto.is_enabled());
        assert!(LtoDecision::ThinLto.is_enabled());
        assert!(!LtoDecision::NoLto.is_enabled());
    }

    #[test]
    fn test_driver_policy_creation() {
        let policy = DriverPolicy::new(
            "nvidia_only",
            vec!["nvidia".to_string()],
            vec!["amdgpu".to_string()],
            false,
            "NVIDIA GPU detected",
        );
        assert_eq!(policy.name, "nvidia_only");
        assert_eq!(policy.exclude_set().len(), 1);
    }

    #[test]
    fn test_gpu_detection_info_to_decision() {
        let info = GpuDetectionInfo {
            has_nvidia: true,
            has_amd: false,
            has_intel: false,
        };
        assert_eq!(info.to_decision(), GpuDecision::NvidiaOnly);

        let info_both = GpuDetectionInfo {
            has_nvidia: true,
            has_amd: true,
            has_intel: true,
        };
        assert_eq!(info_both.to_decision(), GpuDecision::Both);
    }

    #[test]
    fn test_hardware_policy_serialization() {
        let policy = HardwarePolicy::new(
            GpuDecision::NvidiaOnly,
            LtoDecision::FullLto,
            DriverPolicy::new("nvidia_only", vec![], vec![], false, "Test"),
        );
        let json = serde_json::to_string(&policy).unwrap();
        let deserialized: HardwarePolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.gpu_decision, policy.gpu_decision);
        assert_eq!(deserialized.lto_decision, policy.lto_decision);
    }

    #[test]
    fn test_mark_lto_auto_downgraded() {
        let mut policy = HardwarePolicy::new(
            GpuDecision::AmdOnly,
            LtoDecision::FullLto,
            DriverPolicy::new("amd", vec![], vec![], true, "AMD incompatible"),
        );
        policy.mark_lto_auto_downgraded("AMD + Full LTO incompatible");
        assert!(policy.lto_auto_downgraded);
    }
}

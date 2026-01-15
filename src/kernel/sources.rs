//! Kernel Source URL Management
//!
//! Maps kernel variant names to their corresponding git repository URLs and raw PKGBUILD URLs.
//! This module provides a centralized place to manage which remote sources
//! are used when cloning kernel repositories and polling version information.

use std::collections::HashMap;

/// Kernel variant enumeration matching UI options
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelVariant {
    /// Stable Arch Linux kernel (https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git)
    Linux,
    /// Mainline kernel (https://aur.archlinux.org/linux-mainline.git)
    Mainline,
    /// Long-term support kernel (https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git)
    LongTermSupport,
    /// Hardened kernel variant (https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git)
    Hardened,
    /// Zen kernel variant (https://gitlab.archlinux.org/archlinux/packaging/packages/linux-zen.git)
    Zen,
    /// TKG kernel variant (https://github.com/Frogging-Family/linux-tkg.git)
    Tkg,
}

impl KernelVariant {
    /// Parse a variant from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "linux" => Some(KernelVariant::Linux),
            "linux-mainline" => Some(KernelVariant::Mainline),
            "linux-lts" => Some(KernelVariant::LongTermSupport),
            "linux-hardened" => Some(KernelVariant::Hardened),
            "linux-zen" => Some(KernelVariant::Zen),
            "linux-tkg" => Some(KernelVariant::Tkg),
            _ => None,
        }
    }

    /// Get the canonical name for this variant
    pub fn canonical_name(&self) -> &str {
        match self {
            KernelVariant::Linux => "linux",
            KernelVariant::Mainline => "linux-mainline",
            KernelVariant::LongTermSupport => "linux-lts",
            KernelVariant::Hardened => "linux-hardened",
            KernelVariant::Zen => "linux-zen",
            KernelVariant::Tkg => "linux-tkg",
        }
    }
}

/// Consolidated kernel source configuration: both Git and PKGBUILD URLs
#[derive(Debug, Clone)]
pub struct KernelSource {
    /// Git repository URL (for cloning)
    pub git_url: String,
    /// Raw PKGBUILD URL (for version polling via reqwest)
    pub pkgbuild_url: String,
}

/// Kernel source URL database
pub struct KernelSourceDB {
    sources: HashMap<String, KernelSource>,
}

impl KernelSourceDB {
    /// Create a new kernel source database with all known sources
    pub fn new() -> Self {
        let mut sources = HashMap::new();

        // Stable Arch Linux kernel
        // Points to the official Arch Linux packaging repository for the stable linux kernel
        sources.insert(
            "linux".to_string(),
            KernelSource {
                git_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git".to_string(),
                pkgbuild_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/PKGBUILD".to_string(),
            },
        );

        // Long-term support kernel
        // Points to the official Arch Linux packaging repository for LTS kernel
        sources.insert(
            "linux-lts".to_string(),
            KernelSource {
                git_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git".to_string(),
                pkgbuild_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts/-/raw/main/PKGBUILD".to_string(),
            },
        );

        // Hardened kernel variant
        // Points to the Arch Linux hardened kernel packaging
        sources.insert(
            "linux-hardened".to_string(),
            KernelSource {
                git_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git".to_string(),
                pkgbuild_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened/-/raw/main/PKGBUILD".to_string(),
            },
        );

        // Zen kernel variant
        // Points to the official Arch Linux packaging repository for Zen kernel
        sources.insert(
            "linux-zen".to_string(),
            KernelSource {
                git_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-zen.git".to_string(),
                pkgbuild_url: "https://gitlab.archlinux.org/archlinux/packaging/packages/linux-zen/-/raw/main/PKGBUILD".to_string(),
            },
        );

        // Mainline kernel
        // Points to the AUR maintainer's linux-mainline package
        sources.insert(
            "linux-mainline".to_string(),
            KernelSource {
                git_url: "https://aur.archlinux.org/linux-mainline.git".to_string(),
                pkgbuild_url: "https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=linux-mainline".to_string(),
            },
        );

        // TKG kernel variant (community-maintained build script repo)
        // Points to the Frogging-Family linux-tkg project on GitHub
        sources.insert(
            "linux-tkg".to_string(),
            KernelSource {
                git_url: "https://github.com/Frogging-Family/linux-tkg.git".to_string(),
                pkgbuild_url: "https://raw.githubusercontent.com/Frogging-Family/linux-tkg/master/PKGBUILD".to_string(),
            },
        );

        KernelSourceDB { sources }
    }

    /// Get the Git source URL for a given kernel variant
    ///
    /// # Arguments
    /// * `variant` - The kernel variant string (e.g., "linux", "linux-mainline")
    ///
    /// # Returns
    /// The git repository URL for the variant, or None if not found
    pub fn get_source_url(&self, variant: &str) -> Option<&str> {
        self.sources.get(variant).map(|s| s.git_url.as_str())
    }

    /// Get the PKGBUILD source URL for version polling
    ///
    /// # Arguments
    /// * `variant` - The kernel variant string (e.g., "linux", "linux-mainline")
    ///
    /// # Returns
    /// The raw PKGBUILD URL for the variant, or None if not found
    pub fn get_pkgbuild_url(&self, variant: &str) -> Option<&str> {
        self.sources.get(variant).map(|s| s.pkgbuild_url.as_str())
    }

    /// Get all available kernel variants
    pub fn available_variants(&self) -> Vec<&str> {
        let mut variants: Vec<&str> = self.sources.keys().map(|k| k.as_str()).collect();
        variants.sort();
        variants
    }
}

impl Default for KernelSourceDB {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_variant_from_str() {
        assert_eq!(KernelVariant::from_str("linux"), Some(KernelVariant::Linux));
        assert_eq!(
            KernelVariant::from_str("linux-mainline"),
            Some(KernelVariant::Mainline)
        );
        assert_eq!(
            KernelVariant::from_str("linux-lts"),
            Some(KernelVariant::LongTermSupport)
        );
        assert_eq!(
            KernelVariant::from_str("linux-hardened"),
            Some(KernelVariant::Hardened)
        );
        assert_eq!(
            KernelVariant::from_str("linux-zen"),
            Some(KernelVariant::Zen)
        );
        assert_eq!(
            KernelVariant::from_str("linux-tkg"),
            Some(KernelVariant::Tkg)
        );
        assert_eq!(KernelVariant::from_str("invalid"), None);
    }

    #[test]
    fn test_kernel_variant_canonical_name() {
        assert_eq!(KernelVariant::Linux.canonical_name(), "linux");
        assert_eq!(KernelVariant::Mainline.canonical_name(), "linux-mainline");
        assert_eq!(KernelVariant::LongTermSupport.canonical_name(), "linux-lts");
        assert_eq!(KernelVariant::Hardened.canonical_name(), "linux-hardened");
        assert_eq!(KernelVariant::Zen.canonical_name(), "linux-zen");
        assert_eq!(KernelVariant::Tkg.canonical_name(), "linux-tkg");
    }

    #[test]
    fn test_kernel_source_db_creation() {
        let db = KernelSourceDB::new();
        assert_eq!(
            db.get_source_url("linux"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux.git")
        );
    }

    #[test]
    fn test_kernel_source_db_git_urls() {
        let db = KernelSourceDB::new();
        assert_eq!(
            db.get_source_url("linux-mainline"),
            Some("https://aur.archlinux.org/linux-mainline.git")
        );
        assert_eq!(
            db.get_source_url("linux-lts"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts.git")
        );
        assert_eq!(
            db.get_source_url("linux-hardened"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened.git")
        );
        assert_eq!(
            db.get_source_url("linux-zen"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-zen.git")
        );
        assert_eq!(
            db.get_source_url("linux-tkg"),
            Some("https://github.com/Frogging-Family/linux-tkg.git")
        );
    }

    #[test]
    fn test_kernel_source_db_pkgbuild_urls() {
        let db = KernelSourceDB::new();
        assert_eq!(
            db.get_pkgbuild_url("linux"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/PKGBUILD")
        );
        assert_eq!(
            db.get_pkgbuild_url("linux-lts"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-lts/-/raw/main/PKGBUILD")
        );
        assert_eq!(
            db.get_pkgbuild_url("linux-hardened"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-hardened/-/raw/main/PKGBUILD")
        );
        assert_eq!(
            db.get_pkgbuild_url("linux-zen"),
            Some("https://gitlab.archlinux.org/archlinux/packaging/packages/linux-zen/-/raw/main/PKGBUILD")
        );
        assert_eq!(
            db.get_pkgbuild_url("linux-mainline"),
            Some("https://aur.archlinux.org/cgit/aur.git/plain/PKGBUILD?h=linux-mainline")
        );
        assert_eq!(
            db.get_pkgbuild_url("linux-tkg"),
            Some("https://raw.githubusercontent.com/Frogging-Family/linux-tkg/master/PKGBUILD")
        );
    }

    #[test]
    fn test_kernel_source_db_available_variants() {
        let db = KernelSourceDB::new();
        let variants = db.available_variants();
        assert!(variants.contains(&"linux"));
        assert!(variants.contains(&"linux-mainline"));
        assert!(variants.contains(&"linux-lts"));
        assert!(variants.contains(&"linux-hardened"));
        assert!(variants.contains(&"linux-zen"));
        assert!(variants.contains(&"linux-tkg"));
    }

    #[test]
    fn test_kernel_source_db_missing_variant() {
        let db = KernelSourceDB::new();
        assert_eq!(db.get_source_url("linux-unknown"), None);
        assert_eq!(db.get_pkgbuild_url("linux-unknown"), None);
    }
}

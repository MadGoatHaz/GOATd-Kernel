//! PKGBUILD Version Polling
//!
//! Lightweight version polling by fetching raw PKGBUILD files from package repositories
//! and extracting version information using regex patterns.
//!
//! Uses the KernelSourceDB to locate PKGBUILD URLs based on canonical variant names.

use crate::kernel::sources::KernelSourceDB;
use log;
use regex::Regex;

/// Maps kernel variant names (canonical form) to their raw PKGBUILD URLs
/// Uses the centralized KernelSourceDB for consistency
fn get_pkgbuild_url_for_variant(variant: &str) -> Option<String> {
    let db = KernelSourceDB::new();
    db.get_pkgbuild_url(variant).map(|url| url.to_string())
}

/// Fetch and parse the latest kernel version from a PKGBUILD file
///
/// # Arguments
/// * `url` - The raw PKGBUILD URL to fetch
///
/// # Returns
/// * `Ok(String)` - A version string in the format `{pkgver}-{pkgrel}`
/// * `Err(String)` - An error description if fetching or parsing fails
///
/// # Example
/// ```ignore
/// let version = get_latest_version_from_pkgbuild(
///     "https://gitlab.archlinux.org/archlinux/packaging/packages/linux/-/raw/main/PKGBUILD"
/// ).await?;
/// assert!(version.contains("-")); // Should be in format X.Y.Z-N
/// ```
pub async fn get_latest_version_from_pkgbuild(url: &str) -> Result<String, String> {
    log::debug!("[PKGBUILD] Fetching: {}", url);

    // Fetch the raw PKGBUILD content
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch PKGBUILD: {}", e))?;

    let content = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    log::debug!("[PKGBUILD] Successfully fetched {} bytes", content.len());

    // Parse pkgver and pkgrel using regex
    let pkgver = extract_pkgver(&content)?;
    let pkgrel = extract_pkgrel(&content)?;

    let version = format!("{}-{}", pkgver, pkgrel);
    log::debug!("[PKGBUILD] Extracted version: {}", version);

    Ok(version)
}

/// Extract pkgver from PKGBUILD content
///
/// Uses regex to match: `pkgver=` followed by optional quotes, then version string
///
/// Handles three cases:
/// - `pkgver=6.18.3` (no quotes)
/// - `pkgver="6.18.3"` (double quotes)
/// - `pkgver='6.18.3'` (single quotes)
/// Version extraction stops at whitespace or comment (#)
pub fn extract_pkgver(content: &str) -> Result<String, String> {
    // Match pkgver= followed by optional quotes and capture the version value
    // This pattern handles all three cases: unquoted, single-quoted, or double-quoted
    let regex = Regex::new(r#"pkgver\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s#]+))"#)
        .map_err(|e| format!("Failed to compile pkgver regex: {}", e))?;

    regex
        .captures(content)
        .and_then(|caps| {
            // Try double-quoted first, then single-quoted, then unquoted
            caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3))
        })
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| "Failed to extract pkgver from PKGBUILD".to_string())
}

/// Extract pkgrel from PKGBUILD content
///
/// Uses regex to match: `pkgrel=` followed by optional quotes, then release string
///
/// Handles three cases:
/// - `pkgrel=2` (no quotes)
/// - `pkgrel="2"` (double quotes)
/// - `pkgrel='2'` (single quotes)
/// Release extraction stops at whitespace or comment (#)
pub fn extract_pkgrel(content: &str) -> Result<String, String> {
    // Match pkgrel= followed by optional quotes and capture the release value
    // This pattern handles all three cases: unquoted, single-quoted, or double-quoted
    let regex = Regex::new(r#"pkgrel\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s#]+))"#)
        .map_err(|e| format!("Failed to compile pkgrel regex: {}", e))?;

    regex
        .captures(content)
        .and_then(|caps| {
            // Try double-quoted first, then single-quoted, then unquoted
            caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3))
        })
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| "Failed to extract pkgrel from PKGBUILD".to_string())
}

/// Convenience function: fetch PKGBUILD version by kernel variant name (canonical names)
///
/// # Arguments
/// * `variant` - The kernel variant name (canonical): "linux", "linux-lts", "linux-hardened", "linux-mainline", "linux-zen", "linux-tkg"
///
/// # Returns
/// * `Ok(String)` - Version string in format `{pkgver}-{pkgrel}`
/// * `Err(String)` - Error message if variant is unknown, fetch fails, or parsing fails
///
/// # Example
/// ```ignore
/// let version = get_latest_version_by_variant("linux").await?;
/// ```
pub async fn get_latest_version_by_variant(variant: &str) -> Result<String, String> {
    eprintln!(
        "[PKGBUILD] [GET_VERSION] Starting version fetch for variant: '{}'",
        variant
    );
    log::info!(
        "[PKGBUILD] [GET_VERSION] Fetching latest version for variant: '{}'",
        variant
    );

    let url = get_pkgbuild_url_for_variant(variant).ok_or_else(|| {
        let err_msg = format!("Unknown kernel variant: {}", variant);
        eprintln!("[PKGBUILD] [GET_VERSION] [ERROR] {}", err_msg);
        err_msg
    })?;

    eprintln!(
        "[PKGBUILD] [GET_VERSION] Resolved variant '{}' to URL: {}",
        variant, url
    );
    log::debug!(
        "[PKGBUILD] [GET_VERSION] URL resolved for variant '{}': {}",
        variant,
        url
    );

    let result = get_latest_version_from_pkgbuild(&url).await;

    match &result {
        Ok(version) => {
            eprintln!(
                "[PKGBUILD] [GET_VERSION] [SUCCESS] Version fetched for '{}': '{}'",
                variant, version
            );
            log::info!(
                "[PKGBUILD] [GET_VERSION] Successfully fetched version for '{}': '{}'",
                variant,
                version
            );
        }
        Err(e) => {
            eprintln!(
                "[PKGBUILD] [GET_VERSION] [ERROR] Failed to fetch version for '{}': {}",
                variant, e
            );
            log::error!(
                "[PKGBUILD] [GET_VERSION] Error fetching version for '{}': {}",
                variant,
                e
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_pkgver_without_quotes() {
        let content = "pkgver=6.18.3\n";
        assert_eq!(extract_pkgver(content).unwrap(), "6.18.3");
    }

    #[test]
    fn test_extract_pkgver_with_double_quotes() {
        let content = r#"pkgver="6.18.3""#;
        assert_eq!(extract_pkgver(content).unwrap(), "6.18.3");
    }

    #[test]
    fn test_extract_pkgver_with_single_quotes() {
        let content = "pkgver='6.18.3'";
        assert_eq!(extract_pkgver(content).unwrap(), "6.18.3");
    }

    #[test]
    fn test_extract_pkgver_with_comment() {
        let content = "pkgver=6.18.3 # kernel version\n";
        assert_eq!(extract_pkgver(content).unwrap(), "6.18.3");
    }

    #[test]
    fn test_extract_pkgrel_without_quotes() {
        let content = "pkgrel=2\n";
        assert_eq!(extract_pkgrel(content).unwrap(), "2");
    }

    #[test]
    fn test_extract_pkgrel_with_quotes() {
        let content = r#"pkgrel="2""#;
        assert_eq!(extract_pkgrel(content).unwrap(), "2");
    }

    #[test]
    fn test_extract_pkgrel_with_comment() {
        let content = "pkgrel=2 # release number\n";
        assert_eq!(extract_pkgrel(content).unwrap(), "2");
    }

    #[test]
    fn test_get_pkgbuild_url_canonical_variants() {
        // Test canonical variant names (from KernelSourceDB)
        assert!(get_pkgbuild_url_for_variant("linux").is_some());
        assert!(get_pkgbuild_url_for_variant("linux-lts").is_some());
        assert!(get_pkgbuild_url_for_variant("linux-hardened").is_some());
        assert!(get_pkgbuild_url_for_variant("linux-mainline").is_some());
        assert!(get_pkgbuild_url_for_variant("linux-zen").is_some());
        assert!(get_pkgbuild_url_for_variant("linux-tkg").is_some());
    }

    #[test]
    fn test_get_pkgbuild_url_canonical_urls() {
        // Verify URL structure matches KernelSourceDB
        let url_linux = get_pkgbuild_url_for_variant("linux").unwrap();
        assert!(url_linux.contains("linux/-/raw"));

        let url_lts = get_pkgbuild_url_for_variant("linux-lts").unwrap();
        assert!(url_lts.contains("linux-lts"));

        let url_zen = get_pkgbuild_url_for_variant("linux-zen").unwrap();
        assert!(url_zen.contains("linux-zen"));

        let url_mainline = get_pkgbuild_url_for_variant("linux-mainline").unwrap();
        assert!(url_mainline.contains("aur.archlinux.org"));

        let url_tkg = get_pkgbuild_url_for_variant("linux-tkg").unwrap();
        assert!(url_tkg.contains("github.com") || url_tkg.contains("raw.githubusercontent.com"));
    }

    #[test]
    fn test_get_pkgbuild_url_unknown() {
        assert!(get_pkgbuild_url_for_variant("unknown").is_none());
        assert!(get_pkgbuild_url_for_variant("linux-invalid").is_none());
    }
}

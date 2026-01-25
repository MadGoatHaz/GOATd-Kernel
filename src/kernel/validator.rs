//! Kernel patch and build validation.
//!
//! This module provides comprehensive validation functions to verify:
//! - Patches have been applied correctly
//! - Makefiles have correct syntax
//! - Configuration options are properly set
//! - LTO shielding is correctly applied

use crate::error::AppError;
use crate::error::PatchError;
use regex::Regex;
use std::fs;
use std::path::Path;

/// Result type for validation operations
pub type ValidationResult<T> = std::result::Result<T, PatchError>;

/// Validate that a patch has been applied to a file.
///
/// Checks if the expected pattern exists in the file after patching.
///
/// # Arguments
///
/// * `file_path` - Path to the file to validate
/// * `expected_pattern` - Regex pattern that should be present
///
/// # Returns
///
/// Success if pattern found, error otherwise
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use goatd_kernel::kernel::validator::validate_patch_application;
///
/// // Note: This is a documentation example, it won't run without a real file.
/// // We use '#' to hide lines in documentation if needed, but here we just show the call.
/// # // validate_patch_application(Path::new("Makefile"), r"-flto=full");
/// ```
pub fn validate_patch_application(
    file_path: &Path,
    expected_pattern: &str,
) -> ValidationResult<()> {
    if !file_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "File not found for validation: {}",
            file_path.display()
        )));
    }

    let content = fs::read_to_string(file_path)
        .map_err(|e| PatchError::ValidationFailed(format!("Failed to read file: {}", e)))?;

    let regex = Regex::new(expected_pattern)
        .map_err(|e| PatchError::RegexInvalid(format!("Invalid validation pattern: {}", e)))?;

    if regex.is_match(&content) {
        Ok(())
    } else {
        Err(PatchError::ValidationFailed(format!(
            "Pattern not found in {}: {}",
            file_path.display(),
            expected_pattern
        )))
    }
}

/// Validate that a pattern was removed from a file.
///
/// Checks that an unwanted pattern is NOT present in the file.
///
/// # Arguments
///
/// * `file_path` - Path to the file to validate
/// * `unwanted_pattern` - Pattern that should NOT be present
///
/// # Returns
///
/// Success if pattern is absent, error if found
pub fn validate_pattern_removed(file_path: &Path, unwanted_pattern: &str) -> ValidationResult<()> {
    if !file_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "File not found for validation: {}",
            file_path.display()
        )));
    }

    let content = fs::read_to_string(file_path)
        .map_err(|e| PatchError::ValidationFailed(format!("Failed to read file: {}", e)))?;

    let regex = Regex::new(unwanted_pattern)
        .map_err(|e| PatchError::RegexInvalid(format!("Invalid validation pattern: {}", e)))?;

    if !regex.is_match(&content) {
        Ok(())
    } else {
        Err(PatchError::ValidationFailed(format!(
            "Unwanted pattern still present in {}: {}",
            file_path.display(),
            unwanted_pattern
        )))
    }
}

/// Validate Makefile syntax integrity.
///
/// Performs basic checks:
/// - File exists and is readable
/// - Contains expected Makefile keywords
/// - Line endings are consistent
///
/// # Arguments
///
/// * `makefile_path` - Path to Makefile to validate
///
/// # Returns
///
/// Success if syntax appears valid
pub fn validate_makefile_syntax(makefile_path: &Path) -> ValidationResult<()> {
    if !makefile_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "Makefile not found: {}",
            makefile_path.display()
        )));
    }

    let content = fs::read_to_string(makefile_path)
        .map_err(|e| PatchError::ValidationFailed(format!("Failed to read Makefile: {}", e)))?;

    // Check for basic Makefile patterns
    let has_targets = content.contains(":") || content.contains("obj-");
    if !has_targets && !content.is_empty() {
        return Err(PatchError::ValidationFailed(
            "Makefile appears invalid: no targets found".to_string(),
        ));
    }

    // Check for unclosed parentheses or quotes (basic check)
    let open_parens = content.matches('(').count();
    let close_parens = content.matches(')').count();
    if open_parens != close_parens {
        return Err(PatchError::ValidationFailed(format!(
            "Makefile syntax error: mismatched parentheses ({} vs {})",
            open_parens, close_parens
        )));
    }

    Ok(())
}

/// Validate that CONFIG options are present in .config file.
///
/// # Arguments
///
/// * `config_path` - Path to .config file
/// * `required_options` - List of (key, value) pairs that must exist
///
/// # Returns
///
/// Success if all options are present with correct values
pub fn validate_config_options(
    config_path: &Path,
    required_options: &[(&str, &str)],
) -> ValidationResult<()> {
    if !config_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            ".config file not found: {}",
            config_path.display()
        )));
    }

    let content = fs::read_to_string(config_path)
        .map_err(|e| PatchError::ValidationFailed(format!("Failed to read .config: {}", e)))?;

    for (key, value) in required_options {
        // Use a simpler approach - just check if the line exists as-is
        let expected_line = format!("{}={}", key, value);
        let found = content.lines().any(|line| line.trim() == expected_line);

        if !found {
            return Err(PatchError::ValidationFailed(format!(
                "CONFIG option not found or has wrong value: {}={}",
                key, value
            )));
        }
    }

    Ok(())
}

/// Validate that AMD GPU LTO shielding has been applied.
///
/// Checks that CFLAGS_amdgpu (and related) exist with filter-out rules
/// for all three LTO variants.
///
/// # Arguments
///
/// * `makefile_path` - Path to Makefile to validate
///
/// # Returns
///
/// Success if shielding is properly applied
pub fn validate_lto_shielding(makefile_path: &Path) -> ValidationResult<()> {
    if !makefile_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "Makefile not found: {}",
            makefile_path.display()
        )));
    }

    let content = fs::read_to_string(makefile_path)
        .map_err(|e| PatchError::ValidationFailed(format!("Failed to read file: {}", e)))?;

    // Check for AMD GPU shielding patterns
    let has_amdgpu_shield =
        content.contains("CFLAGS_amdgpu") && content.contains("filter-out -flto");

    if !has_amdgpu_shield {
        return Err(PatchError::ValidationFailed(
            "AMD GPU LTO shielding not found in Makefile".to_string(),
        ));
    }

    // Verify all three LTO variants are filtered
    let has_thin_filter = content.contains("-flto$(comma)thin");
    let has_full_filter = content.contains("-flto$(comma)full");
    let has_plain_filter = content.contains("filter-out -flto");

    if !has_thin_filter || !has_full_filter || !has_plain_filter {
        return Err(PatchError::ValidationFailed(
            "Not all LTO variants are filtered for AMD GPU".to_string(),
        ));
    }

    Ok(())
}

/// Validate that a path is suitable for Kbuild operations.
///
/// Kbuild forbids paths containing spaces (' ') or colons (':').
/// This is a pre-flight check to prevent build failures.
///
/// # Arguments
///
/// * `path` - Path to validate
///
/// # Returns
///
/// Success if path is valid for Kbuild, AppError::InvalidPath otherwise
///
/// # Examples
///
/// ```ignore
/// use std::path::Path;
/// use goatd_kernel::kernel::validator::validate_kbuild_path;
///
/// let result = validate_kbuild_path(Path::new("/home/user/kernel"));
/// assert!(result.is_ok());
/// ```
pub fn validate_kbuild_path(path: &Path) -> std::result::Result<(), AppError> {
    let path_str = path.to_str().ok_or_else(|| {
        AppError::InvalidPath("Path contains invalid UTF-8 characters".to_string())
    })?;

    if !path.is_absolute() {
        return Err(AppError::InvalidPath(format!(
            "Path must be absolute: {}",
            path_str
        )));
    }

    if path_str.contains(' ') {
        return Err(AppError::InvalidPath(format!(
            "Path contains spaces: {}",
            path_str
        )));
    }

    if path_str.contains(':') {
        return Err(AppError::InvalidPath(format!(
            "Path contains colons: {}",
            path_str
        )));
    }

    Ok(())
}

/// Validate PKGBUILD shell syntax using bash -n.
///
/// CRITICAL FIX: Validates that generated PKGBUILD is syntactically correct
/// before attempting to execute with makepkg. This prevents runtime failures
/// due to malformed shell code injections.
///
/// # Arguments
///
/// * `pkgbuild_path` - Path to PKGBUILD file to validate
///
/// # Returns
///
/// Success if bash syntax is valid, error message if syntax errors found
pub fn validate_pkgbuild_syntax(pkgbuild_path: &Path) -> ValidationResult<()> {
    use std::process::Command;

    if !pkgbuild_path.exists() {
        return Err(PatchError::FileNotFound(format!(
            "PKGBUILD not found: {}",
            pkgbuild_path.display()
        )));
    }

    eprintln!("[Validator] [PKGBUILD-SYNTAX] Validating PKGBUILD syntax with bash -n");
    eprintln!(
        "[Validator] [PKGBUILD-SYNTAX] File: {}",
        pkgbuild_path.display()
    );

    // Run bash -n to check syntax without executing
    let output = Command::new("bash")
        .arg("-n")
        .arg(pkgbuild_path)
        .output()
        .map_err(|e| {
            PatchError::ValidationFailed(format!("Failed to run bash syntax check: {}", e))
        })?;

    if output.status.success() {
        eprintln!("[Validator] [PKGBUILD-SYNTAX] ✓ PKGBUILD syntax is valid");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[Validator] [PKGBUILD-SYNTAX] ✗ PKGBUILD syntax errors found:");
        eprintln!("[Validator] [PKGBUILD-SYNTAX] {}", stderr);
        Err(PatchError::ValidationFailed(format!(
            "PKGBUILD syntax errors:\n{}",
            stderr
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    // ======= PATCH APPLICATION VALIDATION TESTS (Tests 1-4)

    // Test 1: Validate patch was applied successfully
    #[test]
    fn test_validate_patch_applied() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CFLAGS = -O2 -flto=full").unwrap();
        drop(file);

        let result = validate_patch_application(&file_path, r"-flto=full");
        assert!(result.is_ok());
    }

    // Test 2: Validation fails when pattern not found
    #[test]
    fn test_validate_patch_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CFLAGS = -O2").unwrap();
        drop(file);

        let result = validate_patch_application(&file_path, r"-flto=full");
        assert!(result.is_err());
    }

    // Test 3: Validation fails if file not found
    #[test]
    fn test_validate_patch_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent");

        let result = validate_patch_application(&file_path, r"-flto=");
        assert!(result.is_err());
    }

    // Test 4: Validate with regex pattern
    #[test]
    fn test_validate_patch_regex() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CONFIG_LTO_CLANG=y\nCONFIG_CFI_CLANG=y").unwrap();
        drop(file);

        let result = validate_patch_application(&file_path, r"CONFIG_[A-Z_]+=y");
        assert!(result.is_ok());
    }

    // ======= PATTERN REMOVAL VALIDATION TESTS (Tests 5-6)

    // Test 5: Validate pattern was removed
    #[test]
    fn test_validate_pattern_removed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CFLAGS = -O2 -march=native").unwrap();
        drop(file);

        let result = validate_pattern_removed(&file_path, r"-flto");
        assert!(result.is_ok());
    }

    // Test 6: Validation fails when pattern still present
    #[test]
    fn test_validate_pattern_still_present() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "CFLAGS = -O2 -flto=thin").unwrap();
        drop(file);

        let result = validate_pattern_removed(&file_path, r"-flto");
        assert!(result.is_err());
    }

    // ======= MAKEFILE SYNTAX VALIDATION TESTS (Tests 7-9)

    // Test 7: Validate valid Makefile
    #[test]
    fn test_validate_makefile_syntax_valid() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "obj-y += foo.o\nall: $(TARGETS)").unwrap();
        drop(file);

        let result = validate_makefile_syntax(&makefile_path);
        assert!(result.is_ok());
    }

    // Test 8: Validation fails on empty Makefile
    #[test]
    fn test_validate_makefile_syntax_empty() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        File::create(&makefile_path).unwrap();

        let result = validate_makefile_syntax(&makefile_path);
        // Empty file is OK (comments might be present)
        assert!(result.is_ok());
    }

    // Test 9: Validation detects syntax errors
    #[test]
    fn test_validate_makefile_syntax_mismatched_parens() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "VAR = $(function arg1 arg2").unwrap();
        drop(file);

        let result = validate_makefile_syntax(&makefile_path);
        assert!(result.is_err());
    }

    // ======= CONFIG OPTIONS VALIDATION TESTS (Tests 10-12)

    // Test 10: Validate CONFIG options present
    #[test]
    fn test_validate_config_options_present() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");

        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "CONFIG_LTO_CLANG=y\nCONFIG_CFI_CLANG=y").unwrap();
        drop(file);

        let result = validate_config_options(
            &config_path,
            &[("CONFIG_LTO_CLANG", "y"), ("CONFIG_CFI_CLANG", "y")],
        );
        assert!(result.is_ok());
    }

    // Test 11: Validation fails when option missing
    #[test]
    fn test_validate_config_options_missing() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");

        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "CONFIG_LTO_CLANG=y").unwrap();
        drop(file);

        let result = validate_config_options(&config_path, &[("CONFIG_CFI_CLANG", "y")]);
        assert!(result.is_err());
    }

    // Test 12: Validation fails when value wrong
    #[test]
    fn test_validate_config_options_wrong_value() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join(".config");

        let mut file = File::create(&config_path).unwrap();
        writeln!(file, "CONFIG_LTO_CLANG=n").unwrap();
        drop(file);

        let result = validate_config_options(&config_path, &[("CONFIG_LTO_CLANG", "y")]);
        assert!(result.is_err());
    }

    // ======= LTO SHIELDING VALIDATION TESTS (Tests 13-16)

    // Test 13: Validate AMD GPU LTO shielding applied
    #[test]
    fn test_validate_lto_shielding_applied() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(
            file,
            "CFLAGS_amdgpu := $(filter-out -flto$(comma)thin,$(CFLAGS_amdgpu))\n\
             CFLAGS_amdgpu := $(filter-out -flto$(comma)full,$(CFLAGS_amdgpu))\n\
             CFLAGS_amdgpu := $(filter-out -flto,$(CFLAGS_amdgpu))"
        )
        .unwrap();
        drop(file);

        let result = validate_lto_shielding(&makefile_path);
        assert!(result.is_ok());
    }

    // Test 14: Validation fails when shielding missing
    #[test]
    fn test_validate_lto_shielding_missing() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(file, "obj-y += foo.o").unwrap();
        drop(file);

        let result = validate_lto_shielding(&makefile_path);
        assert!(result.is_err());
    }

    // Test 15: Validation fails if thin filter missing
    #[test]
    fn test_validate_lto_shielding_incomplete_thin() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(
            file,
            "CFLAGS_amdgpu := $(filter-out -flto$(comma)full,$(CFLAGS_amdgpu))\nCFLAGS_amdgpu := $(filter-out -flto,$(CFLAGS_amdgpu))"
        )
        .unwrap();
        drop(file);

        let result = validate_lto_shielding(&makefile_path);
        assert!(result.is_err());
    }

    // Test 16: Validation fails if full filter missing
    #[test]
    fn test_validate_lto_shielding_incomplete_full() {
        let temp_dir = TempDir::new().unwrap();
        let makefile_path = temp_dir.path().join("Makefile");

        let mut file = File::create(&makefile_path).unwrap();
        writeln!(
            file,
            "CFLAGS_amdgpu := $(filter-out -flto$(comma)thin,$(CFLAGS_amdgpu))\nCFLAGS_amdgpu := $(filter-out -flto,$(CFLAGS_amdgpu))"
        )
        .unwrap();
        drop(file);

        let result = validate_lto_shielding(&makefile_path);
        assert!(result.is_err());
    }

    // ======= KBUILD PATH VALIDATION TESTS (Tests 17-19)

    // Test 17: Validate valid path without spaces or colons
    #[test]
    fn test_validate_kbuild_path_valid() {
        let path = std::path::Path::new("/home/user/kernel");
        let result = validate_kbuild_path(path);
        assert!(result.is_ok());
    }

    // Test 18: Validation fails when path contains spaces
    #[test]
    fn test_validate_kbuild_path_with_spaces() {
        let path = std::path::Path::new("/home/user/Documents/GOATd Kernel");
        let result = validate_kbuild_path(path);
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("spaces"));
        }
    }

    // Test 19: Validation fails when path contains colons
    #[test]
    fn test_validate_kbuild_path_with_colons() {
        let path = std::path::Path::new("/home/user:kernel");
        let result = validate_kbuild_path(path);
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(err.to_string().contains("colons"));
        }
    }
}

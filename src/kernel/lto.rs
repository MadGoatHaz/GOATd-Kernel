//! LTO flag management and Phase A fix implementations.
//!
//! This module provides:
//! - **Phase A Fix 1**: Robust --icf flag removal from CFLAGS, CXXFLAGS, LDFLAGS
//! - **Phase A Fix 3**: AMD GPU LTO shielding for all 4 directories
//! - LTO configuration generation for kernel .config

use regex::Regex;

/// The AMD GPU directories that need LTO shielding
const AMD_SHIELD_DIRS: &[&str] = &[
    "drivers/gpu/drm/amd/",
    "drivers/gpu/drm/amd/amdgpu/",
    "drivers/gpu/drm/amd/amdkfd/",
    "drivers/gpu/drm/amd/display/",
];

/// Extract module name from directory path for Makefile variable
fn get_module_name(dir: &str) -> &str {
    match dir {
        "drivers/gpu/drm/amd/" => "amdgpu",
        "drivers/gpu/drm/amd/amdgpu/" => "amdgpu",
        "drivers/gpu/drm/amd/amdkfd/" => "amdkfd",
        "drivers/gpu/drm/amd/display/" => "amdgpu_display",
        _ => "amdgpu",
    }
}

/// **Phase A Fix 1**: Remove --icf and -flto flags from kernel compilation flags.
///
/// This is critical for GPU driver compatibility. ICF (Identical Code Folding) can
/// break GPU drivers by removing semantic distinctions they rely on.
///
/// ## Algorithm
///
/// Uses a single-pass, line-by-line approach to robustly handle all edge cases:
///
/// 1. For each line, check if it's a variable assignment (CFLAGS=, LDFLAGS=, etc.)
/// 2. If yes, extract variable name and value
/// 3. Remove ALL --icf and -flto flags from the value using one comprehensive regex
/// 4. Clean up excess whitespace within that value only
/// 5. If no, keep the line unchanged
///
/// This ensures:
/// - No cascading side effects between lines
/// - Every edge case handled in single operation
/// - No tight coupling or state mutations
/// - Clear, testable, maintainable logic
///
/// ## Handles
///
/// - `-flto=thin`, `-flto=full`, `-flto` (compiler flags)
/// - `--icf=safe`, `--icf=auto`, `--icf` (linker flags)
/// - `-Wl,--icf=*` (linker flag with Wl prefix)
/// - All quote styles (single, double, none)
/// - Variable expansion (${VAR}, $VAR)
/// - Flag concatenation and multiple flags per line
/// - Whitespace variations (spaces, tabs, multiple spaces)
/// - Quoted values with complex flag combinations
///
/// # Arguments
///
/// * `content` - The Makefile content to process
///
/// # Returns
///
/// Modified content with --icf and -flto flags removed (single pass, no side effects)
///
/// # Examples
///
/// ```
/// use goatd_kernel::kernel::lto::remove_icf_flags;
///
/// // Simple case: single flag
/// let makefile = "CFLAGS = -O2 -flto=thin --icf=safe";
/// let fixed = remove_icf_flags(makefile);
/// assert!(!fixed.contains("--icf"));
/// assert!(!fixed.contains("-flto"));
/// assert!(fixed.contains("-O2"));
///
/// // Quoted case: preserves quotes and other flags
/// let makefile = "LDFLAGS=\"-O2 -Wl,--icf=safe -march=native\"";
/// let fixed = remove_icf_flags(makefile);
/// assert!(!fixed.contains("--icf"));
/// assert!(fixed.contains("-O2"));
/// assert!(fixed.contains("-march=native"));
///
/// // Multiple lines: each processed independently
/// let makefile = "CFLAGS = -flto=thin\nLDFLAGS = --icf=auto";
/// let fixed = remove_icf_flags(makefile);
/// assert!(!fixed.contains("-flto"));
/// assert!(!fixed.contains("--icf"));
/// ```
pub fn remove_icf_flags(content: &str) -> String {
    // Pattern to match variable assignments: [indent]VAR_NAME [spaces] = [spaces] value
    // Matches: CFLAGS=, LDFLAGS=, CXXFLAGS=, KBUILD_LDFLAGS=, with optional 'export' prefix
    // Allows optional spaces around the equals sign
    let var_pattern = Regex::new(r"^(\s*)((?:export\s+)?[A-Z_]+FLAGS)\s*=\s*(.*)$")
        .expect("Invalid var_pattern regex");

    // Pattern to match ALL forms of --icf and -flto flags.
    // This handles:
    //   -flto, -flto=thin, -flto=full (compiler flags)
    //   --icf, --icf=safe, --icf=auto (linker flags)
    //   -Wl,--icf, -Wl,--icf=safe, etc. (linker flags with -Wl prefix)
    // Compound pattern (without verbose mode): match either -flto or --icf variants
    let flag_pattern = Regex::new(r"-flto(?:=[a-z]+)?|(?:-Wl,)?--icf(?:=[a-z]+)?")
        .expect("Invalid flag_pattern regex");

    // Process each line independently
    let result: Vec<String> = content
        .lines()
        .map(|line| {
            // Check if this line is a variable assignment
            if let Some(caps) = var_pattern.captures(line) {
                let indent = &caps[1];
                let var_name = &caps[2];
                let var_value = &caps[3];

                // Remove all ICF/LTO flags from the value in one pass
                // This removes the flags themselves, leaving surrounding whitespace
                let new_value = flag_pattern.replace_all(var_value, "").to_string();

                // Collapse multiple consecutive spaces into one
                // (handles cases where flag removal leaves extra whitespace)
                let new_value = Regex::new(r" {2,}")
                    .expect("Invalid space_pattern regex")
                    .replace_all(&new_value, " ")
                    .to_string();

                // Trim leading and trailing whitespace from the value
                let new_value = new_value.trim().to_string();

                // Reconstruct the assignment line with original indentation
                format!("{}{}={}", indent, var_name, new_value)
            } else {
                // Not a variable assignment, keep unchanged
                line.to_string()
            }
        })
        .collect();

    // Join lines back together
    let mut output = result.join("\n");

    // Preserve trailing newline from original if it had one
    if content.ends_with('\n') && !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

/// **Phase A Fix 3**: Shield AMD GPU drivers from LTO optimization.
///
/// Injects CFLAGS filter rules into Makefile to disable LTO for all 4 AMD GPU directories:
/// - drivers/gpu/drm/amd/
/// - drivers/gpu/drm/amd/amdgpu/
/// - drivers/gpu/drm/amd/amdkfd/
/// - drivers/gpu/drm/amd/display/
///
/// For each directory, adds:
/// ```makefile
/// CFLAGS_<module> := $(filter-out -flto$(comma)thin,$(CFLAGS_<module>))
/// CFLAGS_<module> := $(filter-out -flto$(comma)full,$(CFLAGS_<module>))
/// CFLAGS_<module> := $(filter-out -flto,$(CFLAGS_<module>))
/// ```
///
/// # Arguments
///
/// * `makefile` - The Makefile content to shield
///
/// # Returns
///
/// Modified Makefile with AMD GPU shielding applied
///
/// # Examples
///
/// ```
/// use goatd_kernel::kernel::lto::shield_amd_gpu_from_lto;
///
/// let makefile = "# Makefile\nobj-y += foo.o";
/// let shielded = shield_amd_gpu_from_lto(makefile);
/// assert!(shielded.contains("CFLAGS_amdgpu"));
/// ```
pub fn shield_amd_gpu_from_lto(makefile: &str) -> String {
    let mut result = makefile.to_string();

    // Check if shielding is already applied
    if result.contains("CFLAGS_amdgpu") && result.contains("filter-out -flto") {
        return result;
    }

    // Append shielding rules for each AMD directory
    let mut shield_lines = vec![String::new()];
    shield_lines.push("# Phase A Fix 3: AMD GPU LTO Shielding (direct source patch)".to_string());

    for dir in AMD_SHIELD_DIRS {
        let module = get_module_name(dir);
        shield_lines.push(format!(
            "CFLAGS_{} := $(filter-out -flto$(comma)thin,$(CFLAGS_{}))",
            module, module
        ));
        shield_lines.push(format!(
            "CFLAGS_{} := $(filter-out -flto$(comma)full,$(CFLAGS_{}))",
            module, module
        ));
        shield_lines.push(format!(
            "CFLAGS_{} := $(filter-out -flto,$(CFLAGS_{}))",
            module, module
        ));
    }

    result.push('\n');
    result.push_str(&shield_lines.join("\n"));

    result
}

/// Generate LTO-related kernel .config options.
///
/// Takes the desired LTO type and generates appropriate kernel configuration.
/// Supports Full, Thin, and None (disabled) LTO modes.
///
/// # Arguments
///
/// * `lto_type` - The LTO type (Full, Thin, or None)
///
/// # Returns
///
/// Configuration option string ready for .config
pub fn generate_lto_config(lto_type: crate::models::LtoType) -> String {
    match lto_type {
        crate::models::LtoType::Full => vec![
            "# LTO Configuration (Phase 3.5) - Full LTO",
            "CONFIG_LTO_CLANG=y",
            "CONFIG_LTO_CLANG_FULL=y",
            "CONFIG_HAS_LTO_CLANG=y",
            "CONFIG_CFI_CLANG=y",
            "CONFIG_CFI_PERMISSIVE=n",
        ]
        .join("\n"),
        crate::models::LtoType::Thin => vec![
            "# LTO Configuration (Phase 3.5) - Thin LTO",
            "CONFIG_LTO_CLANG=y",
            "CONFIG_LTO_CLANG_THIN=y",
            "CONFIG_HAS_LTO_CLANG=y",
            "CONFIG_CFI_CLANG=y",
            "CONFIG_CFI_PERMISSIVE=n",
        ]
        .join("\n"),
        crate::models::LtoType::None => vec![
            "# LTO Configuration (Phase 3.5) - LTO Disabled",
            "CONFIG_LTO_CLANG=n",
            "CONFIG_CFI_CLANG=n",
        ]
        .join("\n"),
    }
}

/// Generate GPU exclusion configuration for .config.
///
/// Returns GPU-related exclusions and safety flags.
///
/// # Returns
///
/// GPU exclusion configuration string
pub fn generate_gpu_exclusions() -> String {
    vec![
        "# GPU LTO Exclusions (Phase A Fix 3)",
        "CONFIG_DRM_AMDGPU=m",
        "CONFIG_DRM_AMDKFD=n",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ======= COMPREHENSIVE ICF REMOVAL TESTS (NEW SINGLE-PASS ALGORITHM)
    // These tests cover all 14+ edge cases for robust --icf flag removal

    // ========== BASIC FLAG REMOVAL TESTS (Tests 1-6) ==========

    #[test]
    fn test_icf_remove_flto_thin() {
        let makefile = "CFLAGS = -O2 -flto=thin -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto=thin"));
        assert!(!fixed.contains("-flto"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("-march=native"));
        // No double spaces
        assert!(!fixed.contains("  "));
    }

    #[test]
    fn test_icf_remove_flto_full() {
        let makefile = "CFLAGS = -O2 -flto=full -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto=full"));
        assert!(!fixed.contains("-flto"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_remove_flto_plain() {
        let makefile = "LDFLAGS = -flto -Wl,--lto-O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(fixed.contains("--lto-O2")); // Other flags preserved
    }

    #[test]
    fn test_icf_remove_icf_safe() {
        let makefile = "CFLAGS = -O2 --icf=safe -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_remove_icf_auto() {
        let makefile = "LDFLAGS = --icf=auto -Wl,--gc-sections";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("--gc-sections"));
    }

    #[test]
    fn test_icf_remove_icf_plain() {
        let makefile = "LDFLAGS = --icf -Wl,--gc-sections";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("--gc-sections"));
    }

    // ========== WHITESPACE & QUOTE HANDLING (Tests 7-10) ==========

    #[test]
    fn test_icf_with_single_quotes() {
        let makefile = "LDFLAGS='-Wl,--icf=safe -O2'";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("'"));
    }

    #[test]
    fn test_icf_with_double_quotes() {
        let makefile = "LDFLAGS=\"-Wl,--icf=safe -O2\"";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("\""));
    }

    #[test]
    fn test_icf_with_tabs() {
        let makefile = "CFLAGS\t=\t-O2\t-flto=thin\t--icf=safe";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_with_leading_space() {
        let makefile = "LDFLAGS= -Wl,--icf=safe -O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    // ========== MULTIPLE FLAGS & OCCURRENCES (Tests 11-13) ==========

    #[test]
    fn test_icf_multiple_flags_one_line() {
        let makefile = "CFLAGS = -flto=thin --icf=safe -O2 -flto=full --icf=auto -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("-march=native"));
    }

    #[test]
    fn test_icf_multiple_lines() {
        let makefile = "CFLAGS = -flto=thin\nLDFLAGS = --icf=safe\nCXXFLAGS = -O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
        assert_eq!(fixed.lines().count(), 3);
    }

    #[test]
    fn test_icf_multiple_spaces_cleanup() {
        let makefile = "CFLAGS = -O2  -flto=thin  -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        // No double spaces after removal
        assert!(!fixed.contains("  "));
        assert!(fixed.contains("-O2"));
    }

    // ========== COMPLEX REAL-WORLD CASES (Tests 14-17) ==========

    #[test]
    fn test_icf_wl_prefix() {
        let makefile = "LDFLAGS = -O2 -Wl,--icf=safe -march=native";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_concatenation() {
        let makefile = "LDFLAGS=\"${LDFLAGS} -Wl,--icf=safe\"";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("${LDFLAGS}"));
    }

    #[test]
    fn test_icf_no_quotes_required() {
        let makefile = "LDFLAGS=-Wl,--icf=safe -O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_multiple_variable_types() {
        let content = "CFLAGS = -flto=thin\nCXXFLAGS = --icf=safe\nLDFLAGS = -flto=full";
        let fixed = remove_icf_flags(content);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
    }

    // ========== EDGE CASES & ROBUSTNESS (Tests 18-22) ==========

    #[test]
    fn test_icf_empty_variable() {
        let makefile = "CFLAGS = -flto=thin";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        // Line should have CFLAGS but empty value
        assert!(fixed.contains("CFLAGS="));
    }

    #[test]
    fn test_icf_only_flag() {
        let makefile = "CFLAGS = -flto=thin";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(fixed.contains("CFLAGS="));
    }

    #[test]
    fn test_icf_no_flags_present() {
        let makefile = "CFLAGS = -O2 -march=native";
        let fixed = remove_icf_flags(makefile);
        // Spacing is normalized but content preserved
        assert!(fixed.contains("CFLAGS"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("-march=native"));
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
    }

    #[test]
    fn test_icf_non_flag_lines_preserved() {
        let makefile = "# Comment line\nobj-y += foo.o\nCFLAGS = -flto=thin";
        let fixed = remove_icf_flags(makefile);
        assert!(fixed.contains("# Comment line"));
        assert!(fixed.contains("obj-y += foo.o"));
        assert!(!fixed.contains("-flto"));
    }

    #[test]
    fn test_icf_export_prefix() {
        let makefile = "export CFLAGS = -flto=thin -O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(fixed.contains("export CFLAGS"));
        assert!(fixed.contains("-O2"));
    }

    // ========== MULTILINE & SPECIAL CONTENT (Tests 23-25) ==========

    #[test]
    fn test_icf_multiline_preserve_structure() {
        let makefile = "TARGET = -O2\nOBJS = foo.o\nCLEAN = rm";
        let fixed = remove_icf_flags(makefile);
        assert_eq!(fixed.lines().count(), 3);
        assert!(fixed.contains("TARGET = -O2"));
    }

    #[test]
    fn test_icf_empty_lines() {
        let makefile = "CFLAGS = -flto=thin\n\nLDFLAGS = --icf=safe";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        // Empty line preserved
        assert!(fixed.contains("\n\n"));
    }

    #[test]
    fn test_icf_trailing_newline_preserves() {
        let makefile = "CFLAGS = -flto=thin\n";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(fixed.ends_with('\n'));
    }

    // ========== COMPLEX REAL-WORLD SCENARIOS (Tests 26-28) ==========

    #[test]
    fn test_icf_kernel_makefile_like() {
        let makefile = r#"VERSION = 6
PATCHLEVEL = 6
SUBLEVEL = 0

CFLAGS = -O2 -flto=thin --icf=safe -march=native
CXXFLAGS = -O2 -flto=full
LDFLAGS = -Wl,--icf=auto -Wl,-z,relro

obj-y += arch/"#;
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("VERSION = 6"));
        assert!(fixed.contains("-O2"));
        assert!(fixed.contains("-march=native"));
    }

    #[test]
    fn test_icf_all_variants_together() {
        let makefile =
            "CFLAGS = -flto -flto=thin -flto=full --icf --icf=safe --icf=auto -Wl,--icf=safe -O2";
        let fixed = remove_icf_flags(makefile);
        assert!(!fixed.contains("-flto"));
        assert!(!fixed.contains("--icf"));
        assert!(fixed.contains("-O2"));
    }

    #[test]
    fn test_icf_idempotency() {
        let makefile = "CFLAGS = -flto=thin --icf=safe -O2";
        let once = remove_icf_flags(makefile);
        let twice = remove_icf_flags(&once);
        // Should be identical when applied twice
        assert_eq!(once, twice);
    }

    // ======= AMD GPU SHIELDING TESTS (Tests 11-16)

    // Test 11: Shield amdgpu directory
    #[test]
    fn test_shield_amdgpu() {
        let makefile = "obj-y += foo.o";
        let shielded = shield_amd_gpu_from_lto(makefile);
        assert!(shielded.contains("CFLAGS_amdgpu"));
        assert!(shielded.contains("filter-out -flto"));
    }

    // Test 12: Shield all 4 directories
    #[test]
    fn test_shield_all_directories() {
        let makefile = "obj-y += foo.o";
        let shielded = shield_amd_gpu_from_lto(makefile);

        // Should contain all module names
        assert!(shielded.contains("CFLAGS_amdgpu"));
        assert!(shielded.contains("CFLAGS_amdkfd"));
        assert!(shielded.contains("CFLAGS_amdgpu_display"));
    }

    // Test 13: Idempotency - applying shield twice should have same result
    #[test]
    fn test_shield_idempotent() {
        let makefile = "obj-y += foo.o";
        let shielded_once = shield_amd_gpu_from_lto(makefile);
        let shielded_twice = shield_amd_gpu_from_lto(&shielded_once);
        // Should not duplicate content
        assert_eq!(shielded_once, shielded_twice);
    }

    // Test 14: Shield preserves existing Makefile content
    #[test]
    fn test_shield_preserves_content() {
        let makefile = "obj-$(CONFIG_DRM_AMDGPU) += amdgpu/\nobj-y += foo.o";
        let shielded = shield_amd_gpu_from_lto(makefile);
        assert!(shielded.contains("CONFIG_DRM_AMDGPU"));
        assert!(shielded.contains("foo.o"));
    }

    // Test 15: Shield includes all three LTO variants
    #[test]
    fn test_shield_all_lto_variants() {
        let makefile = "obj-y += foo.o";
        let shielded = shield_amd_gpu_from_lto(makefile);

        // Should filter out -flto=thin, -flto=full, and -flto
        assert!(shielded.contains("-flto$(comma)thin"));
        assert!(shielded.contains("-flto$(comma)full"));
        assert!(shielded.contains("filter-out -flto,"));
    }

    // Test 16: Shield works on empty Makefile
    #[test]
    fn test_shield_empty_makefile() {
        let makefile = "";
        let shielded = shield_amd_gpu_from_lto(makefile);
        assert!(shielded.contains("CFLAGS_amdgpu"));
    }

    // ======= LTO CONFIG GENERATION TESTS (Tests 17-22)

    // Test 17: Generate basic LTO config (Thin)
    #[test]
    fn test_generate_lto_config() {
        let config = generate_lto_config(crate::models::LtoType::Thin);
        assert!(config.contains("CONFIG_LTO_CLANG=y"));
        assert!(config.contains("CONFIG_HAS_LTO_CLANG=y"));
        assert!(config.contains("CONFIG_CFI_CLANG=y"));
    }

    // Test 18: Generate LTO includes thin variant
    #[test]
    fn test_generate_lto_thin() {
        let config = generate_lto_config(crate::models::LtoType::Thin);
        assert!(config.contains("CONFIG_LTO_CLANG_THIN=y"));
        assert!(config.contains("CONFIG_HAS_LTO_CLANG=y"));
    }

    // Test 19: Generate LTO includes CFI permissive disabled
    #[test]
    fn test_generate_lto_cfi_permissive() {
        let config = generate_lto_config(crate::models::LtoType::Thin);
        assert!(config.contains("CONFIG_CFI_PERMISSIVE=n"));
        assert!(config.contains("CONFIG_HAS_LTO_CLANG=y"));
    }

    // Test 19b: Generate LTO Full variant
    #[test]
    fn test_generate_lto_full() {
        let config = generate_lto_config(crate::models::LtoType::Full);
        assert!(config.contains("CONFIG_LTO_CLANG_FULL=y"));
        assert!(config.contains("CONFIG_LTO_CLANG=y"));
        assert!(config.contains("CONFIG_HAS_LTO_CLANG=y"));
    }

    // Test 19c: Generate LTO None (disabled)
    #[test]
    fn test_generate_lto_none() {
        let config = generate_lto_config(crate::models::LtoType::None);
        assert!(config.contains("CONFIG_LTO_CLANG=n"));
        assert!(config.contains("CONFIG_CFI_CLANG=n"));
        // Note: CONFIG_HAS_LTO_CLANG is NOT present for None type (disabled)
    }

    // Test 20: Generate GPU exclusions
    #[test]
    fn test_generate_gpu_exclusions() {
        let config = generate_gpu_exclusions();
        assert!(config.contains("GPU"));
        assert!(config.contains("CONFIG_DRM_AMDGPU"));
    }

    // Test 21: LTO config is multiline
    #[test]
    fn test_lto_config_multiline() {
        let config = generate_lto_config(crate::models::LtoType::Thin);
        assert!(config.lines().count() >= 4);
    }

    // Test 22: GPU exclusions multiline format
    #[test]
    fn test_gpu_exclusions_multiline() {
        let config = generate_gpu_exclusions();
        assert!(config.lines().count() >= 2);
    }
}

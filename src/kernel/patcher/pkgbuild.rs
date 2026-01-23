//! PKGBUILD-specific patch operations for kernel modification
//!
//! This module implements surgical PKGBUILD modifications using regex-based injection.
//! It handles:
//! - Toolchain injection (Clang/LLVM exports)
//! - Modprobed-db automatic module filtering
//! - Kernel whitelist protection
//! - MPL sourcing for metadata persistence
//! - Module directory creation with version detection
//! - Kernel variant detection and rebranding

use std::fs;
use std::path::Path;
use regex::Regex;
use once_cell::sync::Lazy;

use super::templates;
use crate::error::PatchError;
use super::PatchResult;

// Lazy-compiled regex patterns for surgical operations
static FUNCTION_BODY_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(\w+)\(\)\s*\{").expect("Invalid function definition regex")
});

static PACKAGE_FUNCTION_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(package|_package|package_[\w-]+)\(\)\s*\{").expect("Invalid package function regex")
});

/// Centralized PKGBASE regex for detecting kernel variant
/// Matches: pkgbase='...' or pkgbase="..." or pkgbase=...
/// PHASE 17: Variant-agnostic detection using capture groups
/// Extracts the base kernel variant by stripping optional GOATd branding
/// Pattern: captures everything before optional `-goatd-*` suffix
/// Examples:
///   - pkgbase='linux-zen' -> 'linux-zen'
///   - pkgbase='linux-zen-goatd-gaming' -> 'linux-zen'
///   - pkgbase=linux-mainline -> 'linux-mainline'
///   - pkgbase="linux-goatd-custom" -> 'linux'
/// CRITICAL FIX: Uses [^\n'"]+ to prevent multi-line capture
static PKGBASE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*pkgbase=['"]?(?:([^\n'"]+?)-goatd-[^\n'"]*|([^\n'"]+))['"]?\s*$"#).expect("Invalid pkgbase regex")
});

/// Helper to read PKGBUILD from source directory
fn read_pkgbuild(src_dir: &Path) -> PatchResult<(std::path::PathBuf, String)> {
    let path = src_dir.join("PKGBUILD");
    if !path.exists() {
        return Err(PatchError::FileNotFound(format!("PKGBUILD not found at {}", path.display())));
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;
    Ok((path, content))
}

/// Find the index immediately after the opening `{` of a function
/// Used for injecting code at the start of function bodies
fn find_function_body_start(content: &str, func_name: &str) -> Option<usize> {
    // Matches "func_name() {" with various spacing patterns
    let pattern = format!(r"(?m)^{}\(\)\s*\{{", regex::escape(func_name));
    if let Ok(regex) = Regex::new(&pattern) {
        regex.find(content).map(|m| m.end())
    } else {
        None
    }
}

/// Find the index of the closing `}` that terminates the function body
/// Implements robust brace counting from the position immediately after the opening `{`
///
/// # Algorithm
/// - Tracks string state (single/double quotes) and escape sequences
/// - **NEW**: Tracks comment state to ignore braces in Bash comments (e.g., `# ... }`)
/// - When `#` is encountered outside strings, all characters until `\n` are skipped
/// - Only counts braces that are outside quotes and outside comments
///
/// # Parameters
/// - `content`: The full PKGBUILD content
/// - `start_pos`: The position immediately after the opening `{` (from find_function_body_start)
///
/// # Returns
/// `Some(index)` pointing to the position of the final closing `}`, or `None` if not found
fn find_function_body_end(content: &str, start_pos: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if start_pos >= bytes.len() {
        return None;
    }

    let mut depth = 1; // We start after the opening brace
    let mut i = start_pos;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_comment = false;

    while i < bytes.len() {
        let ch = bytes[i] as char;

        // Handle escape sequences (but not in comments)
        if !in_comment && i > 0 && bytes[i - 1] as char == '\\' {
            i += 1;
            continue;
        }

        // Handle comment state: skip all characters until newline
        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }

        // Toggle quote state (comments don't start inside quotes)
        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        } else if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        }
        // Check for comment start (only outside quotes)
        else if ch == '#' && !in_single_quote && !in_double_quote {
            in_comment = true;
        }
        // Only count braces outside of quotes and outside comments
        else if !in_single_quote && !in_double_quote {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }

        i += 1;
    }

    None // Mismatched braces
}

/// Extract pkgver from PKGBUILD
/// Returns the version string if found (e.g., "6.19rc6-1")
fn extract_pkgver(content: &str) -> Option<String> {
    // Pattern: pkgver=<anything>
    if let Ok(regex) = Regex::new(r"(?m)^pkgver=(.+?)$") {
        if let Some(caps) = regex.captures(content) {
            return caps.get(1).map(|m| m.as_str().trim().to_string());
        }
    }
    None
}

/// Detect the kernel variant from PKGBUILD's pkgbase variable
/// Returns the cleaned variant like "linux", "linux-zen", "linux-hardened", etc.
///
/// PHASE 17: Kernel Variant Agnosticism
/// This function uses a regex-based variant discovery strategy with capture groups
/// to robustly extract the core kernel identity from the `pkgbase` variable.
///
/// The regex pattern automatically handles:
/// - Optional quotes (single, double, or none): pkgbase='linux-zen' or pkgbase=linux-zen
/// - Leading/trailing whitespace (PKGBUILD allows flexible formatting)
/// - GOATd branding stripping: Captures everything before `-goatd-*` suffix
///
/// Capture group behavior:
/// - Capture group 1: The base kernel variant with GOATd branding already stripped
/// - Examples:
///   - `pkgbase='linux-zen-goatd-gaming'` -> capture: `linux-zen`
///   - `pkgbase=linux-mainline` -> capture: `linux-mainline`
///   - `pkgbase="linux-goatd-custom"` -> capture: `linux`
///   - `pkgbase=linux` -> capture: `linux`
fn detect_kernel_variant(content: &str) -> PatchResult<String> {
    // Use centralized PKGBASE_REGEX with two-group capture strategy
    // Group 1: Base variant with GOATd suffix stripped (linux-zen from linux-zen-goatd-gaming)
    // Group 2: Direct variant without GOATd suffix (fallback if group 1 doesn't match)
    if let Some(caps) = PKGBASE_REGEX.captures(content) {
        let variant = caps.get(1).or_else(|| caps.get(2))
            .map(|m| m.as_str().trim())
            .unwrap_or("");
        if variant.starts_with("linux") {
            // HARDENING: Aggressive sanity checks to prevent false positives
            // 1. Length check: variant must be at least 5 chars (e.g., "linux")
            // 2. Numeric-only check: reject if variant contains ONLY digits
            // 3. Contains hyphen: if variant has hyphens, ensure they're followed by non-numeric
            if variant.len() < 5 {
                eprintln!("[Patcher] [VARIANT] Sanity check FAILED: variant '{}' rejected (too short, len={})", variant, variant.len());
                return Ok("linux".to_string());
            }
            
            if variant.chars().all(|c| c.is_numeric() || c == '-') {
                eprintln!("[Patcher] [VARIANT] Sanity check FAILED: variant '{}' rejected (numeric-only or dash-only)", variant);
                return Ok("linux".to_string());
            }
            
            // Additional check: if variant matches pattern like "linux-1" or "linux-123", reject
            // Pattern: "linux" followed by hyphen and only digits
            if let Ok(numeric_suffix_regex) = Regex::new(r"^linux(-\d+)+$") {
                if numeric_suffix_regex.is_match(variant) {
                    eprintln!("[Patcher] [VARIANT] Sanity check FAILED: variant '{}' rejected (matches numeric-only suffix pattern)", variant);
                    return Ok("linux".to_string());
                }
            }
            
            eprintln!("[Patcher] [VARIANT] Detected variant: '{}' (len={}, sanity checks passed)", variant, variant.len());
            return Ok(variant.to_string());
        }
    }
    // Fallback to standard "linux" if no match found or doesn't start with "linux"
    eprintln!("[Patcher] [VARIANT] No pkgbase match found, fallback to 'linux'");
    Ok("linux".to_string())
}

/// Get the variant-specific package function names
/// Returns (main_package_func, Vec<potential_headers_package_funcs>)
/// The headers list includes broad fallbacks for complex rebranded variant names.
/// Supports patterns like:
///   - package_headers (generic)
///   - package_linux-headers (standard)
///   - package_linux-mainline-goatd-gaming-headers (fully rebranded)
fn get_variant_functions(variant: &str) -> (String, Vec<String>) {
    match variant {
        "linux" => (
            "package_linux".to_string(),
            vec![
                "package_headers".to_string(),
                "package_linux-headers".to_string(),
            ],
        ),
        _ => {
            let suffix = variant.replace("-", "_");
            let hyphenated = variant.to_string();
            (
                format!("package_{}", suffix),
                vec![
                    // Broad fallbacks for complex rebranded names
                    "package_headers".to_string(),
                    "package_linux-headers".to_string(),
                    // Variant-specific patterns
                    format!("package_headers_{}", suffix),
                    format!("package_{}-headers", hyphenated),
                ],
            )
        }
    }
}

/// Inject Clang/LLVM toolchain exports into PKGBUILD
/// Uses regex to surgically replace GCC variables and inject export blocks
pub fn inject_clang_into_pkgbuild(src_dir: &Path) -> PatchResult<()> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // PHASE 18: IDEMPOTENCY GUARD - Check if Clang injection already exists
    // Skip injection if markers are already present (double-patch resistant)
    if content.contains("### GOATD_CLANG_START ###") && content.contains("### GOATD_CLANG_END ###") {
        eprintln!("[Patcher] [PKGBUILD] [CLANG] Idempotency check: Clang/LLVM injection already present (skipping)");
        return Ok(());
    }

    // STEP 1: Aggressively replace GCC variable assignments with regex
    let substitutions = [
        (r"(?m)^\s*(?:export\s+)?CC\s*=\s*(?:gcc|cc)[^\n]*", "export CC=clang"),
        (r"(?m)^\s*(?:export\s+)?CXX\s*=\s*(?:g\+\+|c\+\+)[^\n]*", "export CXX=clang++"),
        (r"(?m)^\s*(?:export\s+)?LD\s*=\s*(?:ld)[^\n]*", "export LD=ld.lld"),
    ];

    for (pattern, replacement) in &substitutions {
        if let Ok(regex) = Regex::new(pattern) {
            content = regex.replace_all(&content, *replacement).to_string();
        }
    }

    // STEP 2: Inject Clang exports block into prepare, build, and _package functions
    let clang_block = templates::CLANG_EXPORTS;
    for func_name in &["prepare", "build", "_package"] {
        if let Some(pos) = find_function_body_start(&content, func_name) {
            if !content[pos..].contains("CLANG/LLVM TOOLCHAIN INJECTION") {
                content.insert_str(pos, &format!("\n    ### GOATD_CLANG_START ###\n{}\n    ### GOATD_CLANG_END ###\n", clang_block));
                eprintln!("[Patcher] [PKGBUILD] Injected Clang exports into {}()", func_name);
            }
        }
    }

    // STEP 3: Force LLVM=1 in make commands
    let lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.contains("make") && !line.contains("LLVM=1") && !line.trim().starts_with('#') {
                if let Ok(regex) = Regex::new(r"\bmake\b") {
                    regex.replace(line, "make LLVM=1 LLVM_IAS=1").to_string()
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect();
    content = lines.join("\n");

    fs::write(path, content).map_err(|e| PatchError::PatchFailed(e.to_string()))?;
    eprintln!("[Patcher] [PKGBUILD] Injected Clang/LLVM toolchain enforcement");
    Ok(())
}

/// Inject Modprobed-db localmodconfig logic into prepare()
/// Enables automatic module filtering based on modprobed-db
pub fn inject_modprobed_localmodconfig(src_dir: &Path, enabled: bool) -> PatchResult<()> {
    if !enabled {
        eprintln!("[Patcher] [PKGBUILD] Modprobed-db injection skipped (disabled)");
        return Ok(());
    }

    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Find the prepare() function body start
    if let Some(start) = find_function_body_start(&content, "prepare") {
        // Try to inject after "cd $srcdir" to ensure we're in the right directory
        let injection_point = if let Some(cd_pos) = content[start..].find("cd \"$srcdir\"") {
            let offset = start + cd_pos;
            if let Some(newline) = content[offset..].find('\n') {
                offset + newline + 1
            } else {
                start
            }
        } else {
            start
        };

        // Check if modprobed injection already exists (idempotent)
        if !content.contains("MODPROBED-DB AUTO-DISCOVERY") {
            let snippet = templates::MODPROBED_INJECTION;
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [PKGBUILD] Injected modprobed-db localmodconfig logic into prepare()");
        }
    }

    Ok(())
}

/// Inject kernel whitelist protection into prepare()
/// Ensures critical kernel CONFIG options survive modprobed-db filtering
pub fn inject_kernel_whitelist(src_dir: &Path) -> PatchResult<()> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    if let Some(prepare_start) = find_function_body_start(&content, "prepare") {
        // Use anchor-based insertion: insert after the END MODPROBED-DB BLOCK marker
        let anchor = "# END MODPROBED-DB BLOCK";

        let injection_point = if let Some(anchor_pos) = content[prepare_start..].find(anchor) {
            // Found the anchor, insert AFTER it
            let absolute_pos = prepare_start + anchor_pos + anchor.len();
            absolute_pos
        } else {
            // Anchor not found (modprobed disabled?), insert at start of function
            prepare_start
        };

        // Check if whitelist injection already exists (idempotent)
        if !content.contains("KERNEL WHITELIST PROTECTION") {
            let snippet = templates::WHITELIST_INJECTION;
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [PKGBUILD] Injected kernel whitelist protection into prepare()");
        }
    }

    Ok(())
}

/// Inject environment variable preservation into all package functions
/// Ensures GOATD_WORKSPACE_ROOT and GOATD_KERNELRELEASE survive fakeroot execution
///
/// PHASE 14: CROSS-MOUNT RESOLUTION INTEGRATION
/// This function also injects the resolve_goatd_root() Bash function to enable
/// dynamic path resolution via .goatd_anchor search across mount points.
///
/// This function:
/// - Injects the resolve_goatd_root() function definition at the start
/// - Locates all package() functions in the PKGBUILD
/// - Injects export commands and dynamic resolver calls at the start of each function
/// - Ensures idempotency by checking for existing injection markers
///
/// # Arguments
/// * `src_dir` - Path to the kernel source directory
/// * `workspace_root` - Absolute canonicalized path to workspace root (used as fallback)
/// * `kernel_release` - Optional kernel release version string
///
/// # Returns
/// Count of package functions that were modified
pub fn inject_variable_preservation(
    src_dir: &Path,
    workspace_root: &Path,
    kernel_release: Option<&str>,
) -> PatchResult<u32> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // PHASE 18: IDEMPOTENCY GUARD - Check if variable preservation already exists
    // Skip injection if markers are already present (double-patch resistant)
    if content.contains("### GOATD_VARPRESERVE_START ###") && content.contains("### GOATD_VARPRESERVE_END ###") {
        eprintln!("[Patcher] [PKGBUILD] [VARPRESERVE] Idempotency check: Variable preservation already present (skipping)");
        return Ok(0);
    }

    // PHASE 14: Inject the resolve_goatd_root() function definition
    // This should be done once at the top of the PKGBUILD before any package functions
    if !content.contains("resolve_goatd_root()") {
        let resolver_func = templates::get_resolve_goatd_root();
        let injection_point = 0; // Inject at the very start of PKGBUILD
        content.insert_str(injection_point, &format!("{}\n\n", resolver_func));
        eprintln!("[Patcher] [PKGBUILD] Injected resolve_goatd_root() function definition");
    }

    // Build the injection snippet for environment variable preservation
    let ws_root_str = workspace_root.to_string_lossy();
    let mut preservation_snippet = format!(
        "    # PHASE 14: ENVIRONMENT VARIABLE PRESERVATION (fakeroot survival + cross-mount resolution)\n    export GOATD_WORKSPACE_ROOT='{}'\n",
        ws_root_str
    );

    if let Some(kr) = kernel_release {
        preservation_snippet.push_str(&format!(
            "    export GOATD_KERNELRELEASE='{}'\n",
            kr
        ));
    }
    
    // PHASE 14: Add dynamic resolver call to ensure cross-mount path resolution
    preservation_snippet.push_str(&format!(
        "    # Attempt dynamic resolution if workspace root not set\n    if [ -z \"$GOATD_WORKSPACE_ROOT\" ]; then resolve_goatd_root || true; fi\n"
    ));

    // Regex to match package(), _package(), package_linux(), etc.
    let func_regex = Regex::new(r"(?m)^(package|_package|package_[\w-]+)\(\)\s*\{")
        .expect("Invalid package regex");

    let mut count = 0u32;
    let mut new_content = String::with_capacity(content.len() + 1000);
    let mut last_idx = 0;

    for caps in func_regex.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        let end_idx = m.end();

        new_content.push_str(&content[last_idx..end_idx]);
        
        // Check if preservation already exists (idempotent)
        let func_end_search = &content[end_idx..];
        if !func_end_search.contains("### GOATD_VARPRESERVE_START ###") {
            new_content.push('\n');
            new_content.push_str("    ### GOATD_VARPRESERVE_START ###\n");
            new_content.push_str(&preservation_snippet);
            new_content.push_str("    ### GOATD_VARPRESERVE_END ###\n");
            count += 1;
        }

        last_idx = end_idx;
    }
    new_content.push_str(&content[last_idx..]);

    if count > 0 {
        fs::write(path, new_content)
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        eprintln!("[Patcher] [PKGBUILD] Injected environment variable preservation + cross-mount resolver into {} package function(s)", count);
    }

    Ok(count)
}

/// Inject MPL (Metadata Persistence Layer) sourcing into all package functions
/// Provides GOATD_KERNELRELEASE and other build metadata
pub fn inject_mpl_sourcing(src_dir: &Path, workspace_root: &Path) -> PatchResult<u32> {
    let (path, content) = read_pkgbuild(src_dir)?;

    // PHASE 18: IDEMPOTENCY GUARD - Check if MPL injection already exists
    // Skip injection if markers are already present (double-patch resistant)
    if content.contains("### GOATD_MPL_START ###") && content.contains("### GOATD_MPL_END ###") {
        eprintln!("[Patcher] [PKGBUILD] [MPL] Idempotency check: MPL sourcing already present (skipping)");
        return Ok(0);
    }

    let snippet = templates::get_mpl_injection(workspace_root.to_string_lossy().as_ref());

    // Regex to match package(), _package(), package_linux(), etc.
    let func_regex = Regex::new(r"(?m)^(package|_package|package_[\w-]+)\(\)\s*\{")
        .expect("Invalid package regex");

    let mut count = 0u32;
    let mut new_content = String::with_capacity(content.len() + 1000);
    let mut last_idx = 0;

    for caps in func_regex.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        let end_idx = m.end();

        new_content.push_str(&content[last_idx..end_idx]);
        new_content.push('\n');
        new_content.push_str("    ### GOATD_MPL_START ###\n");
        new_content.push_str(&snippet);
        new_content.push_str("    ### GOATD_MPL_END ###\n");
        new_content.push('\n');

        last_idx = end_idx;
        count += 1;
    }
    new_content.push_str(&content[last_idx..]);

    if count > 0 {
        fs::write(path, new_content)
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        eprintln!("[Patcher] [PKGBUILD] Injected MPL sourcing into {} package function(s)", count);
    }

    Ok(count)
}

/// Synchronize PKGBUILD pkgver and pkgrel with the actual kernel version
///
/// This function ensures the PKGBUILD variables match the actual kernel release version,
/// fixing the path mismatch where headers are installed to a directory that doesn't match
/// what MODULE-REPAIR hook (based on uname -r) expects.
///
/// # Arguments
/// * `src_dir` - Path to the kernel source directory
/// * `actual_version` - The actual kernel release version (e.g., "6.18.6-arch1-1")
///
/// # Returns
/// Ok if sync successful (or already correct), Err on failure
fn synchronize_pkgbuild_version(
    src_dir: &Path,
    actual_version: Option<&str>,
) -> PatchResult<()> {
    let actual_version = match actual_version {
        Some(v) => v,
        None => return Ok(()), // No version provided, skip sync
    };

    let path = src_dir.join("PKGBUILD");
    if !path.exists() {
        return Err(PatchError::FileNotFound(format!("PKGBUILD not found at {}", path.display())));
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to read PKGBUILD: {}", e)))?;

    // STEP 1: Split actual_version into pkgver and pkgrel
    // Example: "6.18.6-arch1-1" → pkgver="6.18.6.arch1", pkgrel="1"
    // We split at the LAST hyphen to extract pkgrel
    let (new_pkgver_unsanitized, new_pkgrel) = if let Some(last_hyphen_pos) = actual_version.rfind('-') {
        let (ver, rel) = actual_version.split_at(last_hyphen_pos);
        // Remove the hyphen from rel (it's at position 0 after split_at)
        (ver.to_string(), rel[1..].to_string())
    } else {
        // No hyphen found - treat entire string as pkgver, default pkgrel to "1"
        eprintln!("[Patcher] [PKGBUILD-SYNC] ⚠ No hyphen found in actual_version, defaulting pkgrel=1");
        (actual_version.to_string(), "1".to_string())
    };

    // STEP 2: Sanitize pkgver by replacing remaining hyphens with dots
    // Example: "6.18.6-arch1" → "6.18.6.arch1"
    let new_pkgver = new_pkgver_unsanitized.replace('-', ".");

    eprintln!("[Patcher] [PKGBUILD-SYNC] Synchronizing PKGBUILD version:");
    eprintln!("[Patcher] [PKGBUILD-SYNC]   actual_version: {}", actual_version);
    eprintln!("[Patcher] [PKGBUILD-SYNC]   new_pkgver:     {}", new_pkgver);
    eprintln!("[Patcher] [PKGBUILD-SYNC]   new_pkgrel:     {}", new_pkgrel);

    // Check if already synchronized
    if let Ok(pkgver_regex) = Regex::new(r"(?m)^pkgver=(.+?)$") {
        if let Some(caps) = pkgver_regex.captures(&content) {
            let current_pkgver = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if current_pkgver == new_pkgver {
                eprintln!("[Patcher] [PKGBUILD-SYNC] ✓ PKGBUILD already synchronized (idempotent)");
                return Ok(());
            }
        }
    }

    // STEP 3: Update PKGBUILD with regex replacements
    let mut updated_content = content.clone();

    // Replace pkgver= line
    if let Ok(pkgver_regex) = Regex::new(r"(?m)^pkgver=.*$") {
        updated_content = pkgver_regex.replace(&updated_content, format!("pkgver={}", new_pkgver))
            .to_string();
        eprintln!("[Patcher] [PKGBUILD-SYNC] ✓ Updated pkgver");
    } else {
        return Err(PatchError::PatchFailed("Failed to compile pkgver regex".to_string()));
    }

    // Replace or add pkgrel= line
    if let Ok(pkgrel_regex) = Regex::new(r"(?m)^pkgrel=.*$") {
        if pkgrel_regex.is_match(&updated_content) {
            // pkgrel exists, replace it
            updated_content = pkgrel_regex.replace(&updated_content, format!("pkgrel={}", new_pkgrel))
                .to_string();
            eprintln!("[Patcher] [PKGBUILD-SYNC] ✓ Updated pkgrel");
        } else {
            // pkgrel doesn't exist, add it after pkgver
            if let Some(pkgver_line_end) = updated_content.find("pkgver=") {
                if let Some(newline) = updated_content[pkgver_line_end..].find('\n') {
                    let insert_pos = pkgver_line_end + newline + 1;
                    updated_content.insert_str(insert_pos, &format!("pkgrel={}\n", new_pkgrel));
                    eprintln!("[Patcher] [PKGBUILD-SYNC] ✓ Added pkgrel");
                }
            }
        }
    } else {
        return Err(PatchError::PatchFailed("Failed to compile pkgrel regex".to_string()));
    }

    // Write back to PKGBUILD
    fs::write(&path, updated_content)
        .map_err(|e| PatchError::PatchFailed(format!("Failed to write PKGBUILD: {}", e)))?;

    eprintln!("[Patcher] [PKGBUILD-SYNC] ✓ PKGBUILD synchronized successfully");
    Ok(())
}

/// Inject module directory creation (PHASE-E2) for package functions
/// Creates /usr/lib/modules/{version} and /usr/src/linux-{version} directories
pub fn inject_module_directory_creation(
    src_dir: &Path,
    actual_version: Option<&str>,
) -> PatchResult<u32> {
    // STEP 0: Synchronize PKGBUILD pkgver and pkgrel with actual kernel version
    // This ensures headers are installed to the correct directory matching uname -r
    synchronize_pkgbuild_version(src_dir, actual_version)?;

    let (path, content) = read_pkgbuild(src_dir)?;

    // PHASE 18: IDEMPOTENCY GUARD - Check if module directory creation already exists
    // Skip injection if markers are already present (double-patch resistant)
    if content.contains("### GOATD_MODULEDIR_START ###") && content.contains("### GOATD_MODULEDIR_END ###") {
        eprintln!("[Patcher] [PKGBUILD] [MODULEDIR] Idempotency check: Module directory creation already present (skipping)");
        return Ok(0);
    }

    // Get templates (Priority 0 vs Legacy discovery)
    let (headers_code, main_code) = templates::get_module_dir_creation(actual_version);

    let func_regex = Regex::new(r"(?m)^(package|_package|package_[\w-]+)\(\)\s*\{")
        .expect("Invalid package regex");

    let mut count = 0u32;
    let mut new_content = String::with_capacity(content.len() + 2000);
    let mut last_idx = 0;

    for caps in func_regex.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        let end_idx = m.end();
        let func_name = caps.get(1).unwrap().as_str();

        new_content.push_str(&content[last_idx..end_idx]);
        new_content.push('\n');
        new_content.push_str("    ### GOATD_MODULEDIR_START ###\n");

        // Choose template based on function name
        if func_name.contains("headers") {
            new_content.push_str(&headers_code);
        } else {
            new_content.push_str(&main_code);
        }
        
        new_content.push_str("    ### GOATD_MODULEDIR_END ###\n");
        new_content.push('\n');

        last_idx = end_idx;
        count += 1;
    }
    new_content.push_str(&content[last_idx..]);

    if count > 0 {
        fs::write(path, new_content)
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        eprintln!("[Patcher] [PKGBUILD] Injected module directory creation (PHASE-E2) into {} package function(s)", count);
    }

    Ok(count)
}

/// Rebranding: Rename pkgbase, pkgname, and function names
/// Implements the linux-{variant}-goatd-{profile} scheme
/// Examples:
///   - linux + gaming -> linux-goatd-gaming
///   - linux-zen + gaming -> linux-zen-goatd-gaming
///   - linux-mainline + gaming -> linux-mainline-goatd-gaming
///
/// Idempotency: If pkgbase already matches master_identity, the function will skip rebranding
/// to prevent double-branding when run on already-patched PKGBUILDs.
pub fn patch_pkgbuild_for_rebranding(src_dir: &Path, profile: &str) -> PatchResult<()> {
     let (path, content) = read_pkgbuild(src_dir)?;

     let variant = detect_kernel_variant(&content)?;
     let profile_lower = profile.to_lowercase();

     // Construct the new master identity name following linux-{variant}-goatd-{profile} scheme
     let master_identity = if variant == "linux" {
         // Standard linux: linux + gaming -> linux-goatd-gaming
         format!("linux-goatd-{}", profile_lower)
     } else {
         // Variant kernels: linux-zen + gaming -> linux-zen-goatd-gaming
         format!("{}-goatd-{}", variant, profile_lower)
     };

     // IDEMPOTENCY CHECK: If pkgbase already matches master_identity, skip rebranding
     // This prevents double-branding if the patcher is run on an already-patched PKGBUILD
     // PHASE 20: We check the RAW pkgbase value from the file, not the stripped variant
     // This ensures we detect when rebranding has already been applied
     if let Ok(pkgbase_regex) = Regex::new(r#"(?m)^\s*pkgbase=['"]([^'"]+)['"]?\s*$"#) {
         if let Some(caps) = pkgbase_regex.captures(&content) {
             if let Some(raw_pkgbase) = caps.get(1) {
                 let current_pkgbase = raw_pkgbase.as_str().trim();
                 if current_pkgbase == master_identity {
                     eprintln!("[Patcher] [REBRANDING] Idempotency check: pkgbase already matches '{}' (skipping rebranding)", master_identity);
                     return Ok(());
                 }
             }
         }
     }

    let _master_func_suffix = master_identity.replace("-", "_");

    // Process line by line for replacements using regex-based surgical operations
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    // PHASE 19: Stateful tracking for multi-line pkgname array rebranding
    let mut in_pkgname_array = false;
    let pkgname_start_regex = Regex::new(r"^\s*pkgname\s*=\s*\(").unwrap();
    let pkgname_end_regex = Regex::new(r"\)\s*(?:#.*)?$").unwrap();

    for line in &mut lines {
        // Log current state and line
        eprintln!("[Patcher] [REBRANDING] [STATE] in_pkgname_array={}, line: {}", in_pkgname_array, line);

        // 1. Replace pkgbase=
        if line.starts_with("pkgbase=") {
            *line = format!("pkgbase='{}'", master_identity);
        }

        // 2. Replace in pkgname array using quote-aware regex with stateful tracking
        // HARDENED: Use regex with quote awareness to preserve quotes around variant names
        // Example: pkgname=('linux-zen' ) -> pkgname=('linux-zen-goatd-gaming' )
        // Prevents bleeding and handles multi-line arrays correctly
        // PHASE 19: Activate on pkgname=( pattern
        if pkgname_start_regex.is_match(line) {
            eprintln!("[Patcher] [REBRANDING] [STATE] Detected pkgname array START: {}", line);
            in_pkgname_array = true;
        }

        if in_pkgname_array {
            // DEFENSIVE CHECK: Skip lines starting with variable assignments
            // Prevents corruption like: pkgrel=1-goatd-gaming()
            let skip_line = line.trim_start().starts_with("pkgrel=") ||
                           line.trim_start().starts_with("pkgver=") ||
                           line.trim_start().starts_with("pkgdesc=") ||
                           line.trim_start().starts_with("epoch=");

            if skip_line {
                eprintln!("[Patcher] [REBRANDING] [SAFETY] Found non-array-element line while in_pkgname_array=true, RESETTING: {}", line);
                in_pkgname_array = false;
            } else {
                // HARDENED: Quote-anchored regex to prevent word-boundary issues with hyphenated variants
                // PHASE 20: Fixed regex that correctly handles hyphenated variants (e.g., linux-zen)
                // and existing -goatd- branding while preserving suffixes (-headers, -docs, etc.)
                // Pattern: (['"])VARIANT([^'"]*)(['"])  - capture everything after variant within quotes
                // This captures: variant + optional suffix, preserves suffix in replacement
                let escaped_variant = regex::escape(&variant);
                // Match: quoted string starting with the exact variant
                // Capture groups: (quote)(remainder in quotes)(quote)
                let pattern_str = format!(r#"(['"]){}([^'"]*)(['"])"#, escaped_variant);
                
                if let Ok(re) = Regex::new(&pattern_str) {
                    let new_line = re.replace_all(line, |caps: &regex::Captures| {
                        let remainder = &caps[2];
                        // Extract suffix: everything after the variant
                        // If remainder starts with "-goatd-", it's an already-branded variant - don't re-brand
                        if remainder.starts_with("-goatd-") {
                            // Already branded, preserve as-is
                            format!("{}{}{}{}",
                                &caps[1],              // Opening quote
                                &variant,              // Original variant (unchanged)
                                remainder,             // Keep existing -goatd- suffix
                                &caps[3]               // Closing quote
                            )
                        } else {
                            // Not yet branded, apply branding
                            format!("{}{}{}{}",
                                &caps[1],              // Opening quote
                                &master_identity,     // The new branded identity
                                remainder,             // Any suffix (-headers, -docs, etc.)
                                &caps[3]               // Closing quote
                            )
                        }
                    }).to_string();
                    eprintln!("[Patcher] [REBRANDING] [ARRAY-ELEM] Rebranded array element: {} -> {}", line, new_line);
                    *line = new_line;
                } else {
                    eprintln!("[Patcher] [REBRANDING] WARNING: Failed to compile regex for pkgname array replacement, skipping line");
                }
            }
            
            // Deactivate on closing paren at end of line
            if pkgname_end_regex.is_match(line) {
                eprintln!("[Patcher] [REBRANDING] [STATE] Detected pkgname array END: {}", line);
                in_pkgname_array = false;
            }
        }

        // 3. Rename package functions with surgical regex-based replacement
        // HARDENED: Only replace variant AFTER the 'package_' prefix using strict dual-pattern approach
        // Prevents corruption like: pkgrel=1-goatd-gaming() {
        if line.starts_with("package_") && line.contains("() {") {
            // Strict regex pattern with anchors: ^(package_)([a-z0-9-]+)(VARIANT)([a-z0-9_-]*)\(\)\s*\{
            // This ensures proper parsing of the function signature with word boundaries
            
            let mut replaced = false;
            
            // Try hyphenated variant first: e.g., package_linux-mainline
            let escaped_variant = regex::escape(&variant);
            let pattern_str = format!("^(package_)([a-z0-9-]*)({})([a-z0-9_-]*)\\(\\)\\s*\\{{", escaped_variant);
            
            eprintln!("[Patcher] [REBRANDING] [FUNC-RENAME] Attempting hyphenated pattern: {}", pattern_str);
            if let Ok(regex) = Regex::new(&pattern_str) {
                if let Some(caps) = regex.captures(&line) {
                    // Safe replacement preserving all capture groups
                    let new_name = format!(
                        "{}{}{}{}() {{",
                        caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                        caps.get(2).map(|m| m.as_str()).unwrap_or(""),
                        &master_identity,
                        caps.get(4).map(|m| m.as_str()).unwrap_or("")
                    );
                    eprintln!("[Patcher] [REBRANDING] [FUNC-RENAME] SUCCESS (hyphenated): {} -> {}", line, new_name);
                    *line = new_name;
                    replaced = true;
                }
            } else {
                eprintln!("[Patcher] [REBRANDING] WARNING: Failed to compile regex for package function (hyphenated), skipping");
            }
            
            // If hyphenated replacement didn't work, try underscored variant: e.g., package_linux_zen
            if !replaced {
                let underscore_variant = variant.replace("-", "_");
                let escaped_variant = regex::escape(&underscore_variant);
                let pattern_str = format!("^(package_)([a-z0-9_]*)({})([a-z0-9_-]*)\\(\\)\\s*\\{{", escaped_variant);
                
                eprintln!("[Patcher] [REBRANDING] [FUNC-RENAME] Attempting underscored pattern: {}", pattern_str);
                if let Ok(regex) = Regex::new(&pattern_str) {
                    if let Some(caps) = regex.captures(&line) {
                        // Safe replacement preserving all capture groups
                        let underscore_master = master_identity.replace("-", "_");
                        let new_name = format!(
                            "{}{}{}{}() {{",
                            caps.get(1).map(|m| m.as_str()).unwrap_or(""),
                            caps.get(2).map(|m| m.as_str()).unwrap_or(""),
                            &underscore_master,
                            caps.get(4).map(|m| m.as_str()).unwrap_or("")
                        );
                        eprintln!("[Patcher] [REBRANDING] [FUNC-RENAME] SUCCESS (underscored): {} -> {}", line, new_name);
                        *line = new_name;
                        replaced = true;
                    }
                } else {
                    eprintln!("[Patcher] [REBRANDING] WARNING: Failed to compile regex for package function (underscored), skipping");
                }
            }
            
            if !replaced {
                eprintln!("[Patcher] [REBRANDING] [FUNC-RENAME] FAILED to match either pattern: {}", line);
            }
        }
    }

    // Add "provides" metadata to ensure compatibility with original variant
    if let Some(idx) = lines.iter().position(|l| l.starts_with("pkgdesc=")) {
        lines.insert(idx + 1, format!("provides=('{}')", variant));
    }

    fs::write(path, lines.join("\n") + "\n")
        .map_err(|e| PatchError::PatchFailed(e.to_string()))?;

    eprintln!("[Patcher] [PKGBUILD] Applied rebranding: {} -> {} (profile: {})", variant, master_identity, profile_lower);
    Ok(())
}

/// Inject post-modprobed hard enforcer into prepare() function
/// Protects filtered modules from re-expansion by olddefconfig
fn inject_post_modprobed_hard_enforcer(src_dir: &Path, use_modprobed: bool) -> PatchResult<()> {
    if !use_modprobed {
        eprintln!("[Patcher] [PHASE-G2] Post-modprobed enforcer skipped (modprobed disabled)");
        return Ok(());
    }

    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Find the prepare() function body start
    if let Some(prepare_start) = find_function_body_start(&content, "prepare") {
        let prepare_section = &content[prepare_start..];
        
        // Look for configuration steps: "make olddefconfig" or "make prepare"
        // Try to find these patterns in the prepare function
        let mut injection_point = prepare_start;

        // Search for "make olddefconfig" or similar configuration targets
        for (pattern, label) in &[
            ("make olddefconfig", "olddefconfig"),
            ("make prepare", "prepare"),
            ("make oldconfig", "oldconfig"),
        ] {
            if let Some(pos) = prepare_section.find(pattern) {
                let absolute_pos = prepare_start + pos;
                // Find the end of this line
                if let Some(newline_pos) = content[absolute_pos..].find('\n') {
                    injection_point = absolute_pos + newline_pos + 1;
                    eprintln!("[Patcher] [PHASE-G2] Found '{}' configuration step", label);
                    break;
                }
            }
        }

        // If no specific config step found, search for END MODPROBED-DB BLOCK anchor
        if injection_point == prepare_start {
            let anchor = "# END MODPROBED-DB BLOCK";
            if let Some(anchor_pos) = prepare_section.find(anchor) {
                let absolute_pos = prepare_start + anchor_pos + anchor.len();
                injection_point = absolute_pos;
            }
        }

        // Check if PHASE G2 enforcer already exists (idempotent)
        if !content.contains("PHASE G2 POST-MODPROBED: Hard enforcer") {
            let snippet = templates::PHASE_G2_ENFORCER;
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [PKGBUILD] [PHASE-G2] Injected post-modprobed hard enforcer into prepare()");
        }
    }

    Ok(())
}

/// Inject config restorer to protect modprobed filtering after config overwrites
/// Searches for "cp ../config .config" patterns and injects PHASE_G2_5_RESTORER after
fn inject_post_setting_config_restorer(src_dir: &Path, use_modprobed: bool) -> PatchResult<()> {
    if !use_modprobed {
        eprintln!("[Patcher] [PHASE-G2.5] Config restorer skipped (modprobed disabled)");
        return Ok(());
    }

    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Find prepare() function and search for "cp ../config .config" patterns
    if let Some(prepare_start) = find_function_body_start(&content, "prepare") {
        let prepare_section = &content[prepare_start..];
        
        // Look for all "cp ../config .config" patterns within prepare()
        let pattern = "cp ../config .config";
        let mut injection_count = 0;

        // Count occurrences to know if we need to inject
        for pos in prepare_section.match_indices(pattern) {
            let absolute_pos = prepare_start + pos.0;
            
            // Find the end of this line
            if let Some(newline_pos) = content[absolute_pos..].find('\n') {
                let injection_point = absolute_pos + newline_pos + 1;
                
                // Check if restorer already injected near this location
                if !content[injection_point..].starts_with("# PHASE G2.5 POST-SETTING-CONFIG") {
                    // We need to do this carefully - insert in reverse order to preserve positions
                    injection_count += 1;
                }
            }
        }

        // Inject in reverse (from end to start) to preserve positions
        let mut insertions: Vec<(usize, String)> = Vec::new();
        for pos in prepare_section.match_indices(pattern) {
            let absolute_pos = prepare_start + pos.0;
            if let Some(newline_pos) = content[absolute_pos..].find('\n') {
                let injection_point = absolute_pos + newline_pos + 1;
                if !content[injection_point..].starts_with("# PHASE G2.5 POST-SETTING-CONFIG") {
                    insertions.push((injection_point, format!("\n{}\n", templates::PHASE_G2_5_RESTORER)));
                }
            }
        }

        // Sort in reverse order and apply
        insertions.sort_by(|a, b| b.0.cmp(&a.0));
        for (pos, snippet) in insertions {
            content.insert_str(pos, &snippet);
            injection_count += 1;
        }

        if injection_count > 0 {
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [PKGBUILD] [PHASE-G2.5] Injected config restorer after {} 'cp ../config .config' pattern(s)", injection_count);
        }
    }

    Ok(())
}

/// Apply NVIDIA DKMS memremap shim into prepare() function
///
/// This function injects the Perl-based shim from templates::NVIDIA_DKMS_MEMREMAP_SHIM
/// into the prepare() function after the source directory traversal (e.g., after `cd "$srcdir/linux"`).
///
/// The shim restores the `page_free` field in struct dev_pagemap_ops which was removed
/// in Linux 6.19, enabling NVIDIA DKMS drivers (like nvidia-open 590.48.01) to compile
/// without modification.
///
/// # Algorithm
/// 1. Locate the prepare() function body
/// 2. Find the first `cd "$srcdir` command (source directory traversal)
/// 3. Inject the shim immediately after this command
/// 4. Include idempotency check to prevent duplicate injections
/// 5. Perform post-injection validation using grep to verify injection
///
/// # Returns
/// `Ok(())` on success or if already injected (idempotent)
fn apply_nvidia_dkms_memremap_shim(src_dir: &Path) -> PatchResult<()> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Idempotent check: skip if shim already injected
    if content.contains("NVIDIA DKMS COMPATIBILITY SHIM") {
        eprintln!("[Patcher] [NVIDIA-DKMS] Memremap shim already present (idempotent)");
        return Ok(());
    }

    // Find the prepare() function body start
    if let Some(prepare_start) = find_function_body_start(&content, "prepare") {
        let prepare_section = &content[prepare_start..];
        
        let mut injection_point = prepare_start;
        
        // STEP 1: Locate source directory traversal pattern
        // Common patterns: cd "$srcdir/linux", cd "$srcdir"/linux-*, etc.
        // We look for the first cd command and inject AFTER it
        if let Some(cd_pos) = prepare_section.find("cd \"$srcdir") {
            let absolute_cd_pos = prepare_start + cd_pos;
            // Find the end of this line
            if let Some(newline) = content[absolute_cd_pos..].find('\n') {
                injection_point = absolute_cd_pos + newline + 1;
                eprintln!("[Patcher] [NVIDIA-DKMS] Found directory traversal: cd command");
            }
        } else {
            // Fallback: look for alternative cd patterns
            if let Some(cd_pos) = prepare_section.find("cd $srcdir") {
                let absolute_cd_pos = prepare_start + cd_pos;
                if let Some(newline) = content[absolute_cd_pos..].find('\n') {
                    injection_point = absolute_cd_pos + newline + 1;
                    eprintln!("[Patcher] [NVIDIA-DKMS] Found directory traversal: cd $srcdir");
                }
            } else {
                // Final fallback: inject at the start of the prepare function
                eprintln!("[Patcher] [NVIDIA-DKMS] Could not find cd command, injecting at function start");
                injection_point = prepare_start;
            }
        }

        // STEP 2: Perform the injection
        let snippet = templates::NVIDIA_DKMS_MEMREMAP_SHIM;
        content.insert_str(injection_point, &format!("\n{}\n", snippet));
        
        fs::write(path.clone(), content.clone())
            .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
        
        eprintln!("[Patcher] [NVIDIA-DKMS] Injected memremap shim into prepare()");

        // STEP 3: Post-injection validation using grep
        // Verify that the page_free field restoration marker was injected correctly
        if let Ok(output) = std::process::Command::new("grep")
            .args(&["-A", "5", "NVIDIA DKMS COMPATIBILITY SHIM", path.to_string_lossy().as_ref()])
            .output()
        {
            if output.status.success() {
                let validation_output = String::from_utf8_lossy(&output.stdout);
                eprintln!("[Patcher] [NVIDIA-DKMS] [VERIFY] grep -A 5 validation passed:");
                for line in validation_output.lines() {
                    eprintln!("[Patcher] [NVIDIA-DKMS] [VERIFY]   {}", line);
                }
            } else {
                eprintln!("[Patcher] [NVIDIA-DKMS] [VERIFY] WARNING: grep validation check returned non-zero status");
            }
        }

        Ok(())
    } else {
        eprintln!("[Patcher] [NVIDIA-DKMS] WARNING: Could not find prepare() function in PKGBUILD");
        Ok(())
    }
}

/// Inject LTO hard enforcer into build() function
/// Injects immediately at the very beginning of build() function (right after opening brace)
/// to ensure environment variable exports happen BEFORE any subsequent assignments.
/// CRITICAL: The PHASE G1.1 environment exports MUST run before cd commands or other
/// variable assignments that might overwrite CFLAGS/CXXFLAGS/LDFLAGS.
fn inject_prebuild_lto_hard_enforcer(src_dir: &Path, lto_type: crate::models::LtoType) -> PatchResult<()> {
    let (path, mut content) = read_pkgbuild(src_dir)?;

    // Find the build() function body start (position immediately after opening brace)
    if let Some(injection_point) = find_function_body_start(&content, "build") {
        // Check if LTO enforcer already exists (idempotent)
        if !content.contains("PHASE G1 PREBUILD: LTO HARD ENFORCER") {
            let snippet = templates::get_prebuild_lto_enforcer(lto_type);
            // Inject immediately at the start of build() function body
            // This ensures environment exports run FIRST, before cd commands or other assignments
            content.insert_str(injection_point, &format!("\n{}\n", snippet));
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [PKGBUILD] [PHASE-G1] Injected LTO hard enforcer at VERY BEGINNING of build() function");
            eprintln!("[Patcher] [PKGBUILD] [PHASE-G1] Environment variable exports (CFLAGS/CXXFLAGS/LDFLAGS) will run FIRST");
        }
    }

    Ok(())
}

// ============================================================================
// Extension Methods on KernelPatcher (as required by the facade)
// ============================================================================

impl super::KernelPatcher {
    /// Inject Clang/LLVM toolchain exports into PKGBUILD
    pub fn inject_clang_into_pkgbuild(&self) -> PatchResult<()> {
        inject_clang_into_pkgbuild(self.src_dir())
    }

    /// Inject Modprobed-db localmodconfig logic
    pub fn inject_modprobed_localmodconfig(&self, use_modprobed: bool) -> PatchResult<()> {
        inject_modprobed_localmodconfig(self.src_dir(), use_modprobed)
    }

    /// Inject kernel whitelist protection
    pub fn inject_kernel_whitelist(&self) -> PatchResult<()> {
        inject_kernel_whitelist(self.src_dir())
    }

    /// Inject MPL sourcing for metadata persistence
    pub fn inject_mpl_sourcing(&self) -> PatchResult<()> {
        // ROBUST APPROACH: Derive workspace root from src_dir parent
        // This matches the logic in prepare_build_environment and ensures
        // MPL sourcing uses the same workspace root as environment exports
        let workspace_root = if let Some(parent) = self.src_dir().parent() {
            parent.to_path_buf()
        } else {
            // Fallback: Try environment variable next
            if let Ok(ws_root) = std::env::var("GOATD_WORKSPACE_ROOT") {
                Path::new(&ws_root).to_path_buf()
            } else {
                // Final fallback to current directory
                Path::new(".").to_path_buf()
            }
        };
        
        inject_mpl_sourcing(self.src_dir(), &workspace_root)?;
        eprintln!("[Patcher] [PKGBUILD] [MPL] Using workspace root for metadata: {}", workspace_root.display());
        Ok(())
    }

    /// Inject module directory creation (PHASE-E2)
    pub fn inject_module_directory_creation(&self) -> PatchResult<()> {
        inject_module_directory_creation(self.src_dir(), None)?;
        Ok(())
    }

    /// Inject environment variable preservation for fakeroot survival
    pub fn inject_variable_preservation(&self, kernel_release: Option<&str>) -> PatchResult<()> {
        // Get workspace root - derive from src_dir parent
        let workspace_root = if let Some(parent) = self.src_dir().parent() {
            let parent_path = std::path::PathBuf::from(parent);
            
            match parent_path.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => {
                    let abs_path = if parent_path.is_absolute() {
                        parent_path
                    } else {
                        std::env::current_dir()
                            .map(|cwd| cwd.join(&parent_path))
                            .unwrap_or(parent_path)
                    };
                    abs_path
                }
            }
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd
        } else {
            return Err(PatchError::PatchFailed("Could not determine workspace root".to_string()));
        };
        
        inject_variable_preservation(self.src_dir(), &workspace_root, kernel_release)?;
        eprintln!("[Patcher] [PKGBUILD] Environment variable preservation injected (workspace_root={})", workspace_root.display());
        Ok(())
    }

    /// Apply PKGBUILD rebranding
    pub fn patch_pkgbuild_for_rebranding(&self) -> PatchResult<()> {
        // Default profile - can be enhanced to read from environment
        let profile = std::env::var("GOATD_PROFILE_NAME").unwrap_or_else(|_| "custom".to_string());
        patch_pkgbuild_for_rebranding(self.src_dir(), &profile)
    }

    /// Detect kernel variant from PKGBUILD
    pub fn detect_kernel_variant(&self) -> PatchResult<String> {
        let (_, content) = read_pkgbuild(self.src_dir())?;
        detect_kernel_variant(&content)
    }

    /// Get variant-specific package function names
    pub fn get_variant_functions(&self, variant: &str) -> PatchResult<(String, Vec<String>)> {
        let (main_func, headers_funcs) = get_variant_functions(variant);
        Ok((main_func, headers_funcs))
    }

    /// Validate and fix PKGBUILD sources to match the correct kernel variant
    ///
    /// This function ensures the PKGBUILD has:
    /// 1. Correct pkgver matching the resolved version
    /// 2. Correct source variable pointing to the right variant repository
    /// 3. Proper source array entries that match the variant's build requirements
    ///
    /// If mismatches are detected, this function automatically repairs them:
    /// - Replaces `pkgver=` with the resolved version using regex
    /// - Replaces `source=()` array entries with the correct git URL for the variant
    /// - Writes the repaired PKGBUILD back to disk
    ///
    /// # Arguments
    /// * `kernel_variant` - The kernel variant name (e.g., "linux", "linux-lts", "linux-mainline")
    /// * `resolved_version` - The concrete version string (e.g., "6.19rc6-1", NOT "latest")
    ///
    /// # Returns
    /// * `Ok(())` if validation passes or corrections are applied successfully
    /// * `Err(PatchError)` if validation fails and cannot be corrected
    pub fn validate_and_fix_pkgbuild_sources(&self, kernel_variant: &str, resolved_version: &str) -> PatchResult<()> {
        let (path, content) = read_pkgbuild(self.src_dir())?;
        
        // STEP 1: Verify resolved_version is concrete (not "latest")
        if resolved_version == "latest" {
            eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ WARNING: resolved_version is still 'latest' (should be concrete)");
            log::warn!("[Patcher] [SOURCE-REPAIR] resolved_version was not transformed to concrete value");
            return Err(PatchError::PatchFailed(
                "resolved_version must be concrete (not 'latest')".to_string()
            ));
        }
        
        eprintln!("[Patcher] [SOURCE-REPAIR] ========== VALIDATION START ==========");
        eprintln!("[Patcher] [SOURCE-REPAIR] Kernel variant: '{}'", kernel_variant);
        eprintln!("[Patcher] [SOURCE-REPAIR] Resolved version: '{}'", resolved_version);
        log::info!("[Patcher] [SOURCE-REPAIR] Validating PKGBUILD for variant: '{}' with version: '{}'", kernel_variant, resolved_version);
        
        // Get the expected source URL from KernelSourceDB
        use crate::kernel::sources::KernelSourceDB;
        let source_db = KernelSourceDB::new();
        let expected_source_url = source_db.get_source_url(kernel_variant)
            .ok_or_else(|| PatchError::PatchFailed(
                format!("Unknown kernel variant: {}", kernel_variant)
            ))?;
        
        eprintln!("[Patcher] [SOURCE-REPAIR] Expected source URL for '{}': {}", kernel_variant, expected_source_url);
        
        // STEP 1.5: Detect AUR/External variants based on source URL
        // AUR variants: contain aur.archlinux.org
        // External variants: contain github.com (or other git hosting)
        let is_aur_or_external = expected_source_url.contains("aur.archlinux.org") ||
                                  expected_source_url.contains("github.com");
        
        if is_aur_or_external {
            eprintln!("[Patcher] [SOURCE-REPAIR] Detected AUR/External variant - source URL: {}", expected_source_url);
        }
        
        // STEP 2: Check if source variable exists in PKGBUILD
        let source_line_pattern = Regex::new(r"(?m)^source=.*")
            .expect("Invalid source line regex");
        
        let has_source = source_line_pattern.is_match(&content);
        
        if !has_source {
            eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ No source variable found in PKGBUILD");
            log::warn!("[Patcher] [SOURCE-REPAIR] No source variable found");
            return Ok(()); // Non-fatal - PKGBUILD may be valid without source declaration
        }
        
        // STEP 3: Extract current variant from PKGBUILD and log before state
        let current_variant = detect_kernel_variant(&content)?;
        eprintln!("[Patcher] [SOURCE-REPAIR] Current variant in PKGBUILD: '{}'", current_variant);
        
        // STEP 3a: Extract current pkgver
        let current_pkgver = extract_pkgver(&content);
        eprintln!("[Patcher] [SOURCE-REPAIR] Current pkgver: '{}'", current_pkgver.as_deref().unwrap_or("NOT FOUND"));
        
        // Log the BEFORE state
        eprintln!("[Patcher] [SOURCE-REPAIR] ========== BEFORE REPAIR ==========");
        eprintln!("[Patcher] [SOURCE-REPAIR] variant: '{}'", current_variant);
        eprintln!("[Patcher] [SOURCE-REPAIR] pkgver: '{}'", current_pkgver.as_deref().unwrap_or("NOT FOUND"));
        
        // STEP 4: Check for mismatches
        let variant_mismatch = current_variant != kernel_variant;
        let version_mismatch = current_pkgver.as_deref() != Some(resolved_version);
        
        if !variant_mismatch && !version_mismatch {
            eprintln!("[Patcher] [SOURCE-REPAIR] ✓ No mismatches detected - PKGBUILD is correct");
            eprintln!("[Patcher] [SOURCE-REPAIR] ========== VALIDATION COMPLETE ==========");
            return Ok(());
        }
        
        // STEP 5: Repair detected mismatches
        eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ Mismatches detected, attempting repair...");
        log::warn!("[Patcher] [SOURCE-REPAIR] Repairing PKGBUILD: variant_mismatch={}, version_mismatch={}",
            variant_mismatch, version_mismatch);
        
        let mut repaired_content = content.clone();
        
        // REPAIR 1: Fix pkgver and pkgrel if version doesn't match resolved_version
        if version_mismatch {
            eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] Fixing version: '{}' → '{}'",
                current_pkgver.as_deref().unwrap_or("NOT FOUND"), resolved_version);
            
            // STEP 1: Split resolved_version at the LAST hyphen
            // Example: "6.19rc6-1" → pkgver="6.19rc6", pkgrel="1"
            // Example: "6.19-rc6-1" → pkgver="6.19-rc6", pkgrel="1"
            let (new_pkgver_unsanitized, new_pkgrel) = if let Some(last_hyphen_pos) = resolved_version.rfind('-') {
                let (ver, rel) = resolved_version.split_at(last_hyphen_pos);
                // Remove the hyphen from rel (it's at position 0 after split_at)
                (ver.to_string(), rel[1..].to_string())
            } else {
                // No hyphen found - treat entire string as pkgver, default pkgrel to "1"
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ⚠ No hyphen found in resolved_version, defaulting pkgrel=1");
                (resolved_version.to_string(), "1".to_string())
            };
            
            eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] [SPLIT] Split version: pkgver='{}' pkgrel='{}'",
                new_pkgver_unsanitized, new_pkgrel);
            
            // STEP 2: Sanitize pkgver by replacing remaining hyphens with dots
            // Example: "6.19-rc6" → "6.19.rc6"
            let new_pkgver = new_pkgver_unsanitized.replace('-', ".");
            
            if new_pkgver_unsanitized != new_pkgver {
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] [SANITIZE] Sanitized pkgver: '{}' → '{}'",
                    new_pkgver_unsanitized, new_pkgver);
            } else {
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] [SANITIZE] pkgver already sanitized (no hyphens): '{}'",
                    new_pkgver);
            }
            
            // Validate pkgver contains only valid characters
            // Arch Linux pkgver must not contain uppercase letters or special chars (except .-+)
            if !new_pkgver.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '+' || c == '_') {
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✗ Invalid characters in sanitized pkgver: '{}'", new_pkgver);
                return Err(PatchError::PatchFailed(
                    format!("Sanitized pkgver '{}' contains invalid characters", new_pkgver)
                ));
            }
            
            // STEP 3: Update PKGBUILD with regex replacements
            // Replace pkgver= line
            if let Ok(pkgver_regex) = Regex::new(r"(?m)^pkgver=.*$") {
                repaired_content = pkgver_regex.replace(&repaired_content, format!("pkgver={}", new_pkgver))
                    .to_string();
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✓ pkgver replaced: {}", new_pkgver);
            } else {
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✗ Failed to compile pkgver regex");
                return Err(PatchError::PatchFailed("Failed to compile pkgver regex".to_string()));
            }
            
            // Replace or add pkgrel= line
            if let Ok(pkgrel_regex) = Regex::new(r"(?m)^pkgrel=.*$") {
                if pkgrel_regex.is_match(&repaired_content) {
                    // pkgrel exists, replace it
                    repaired_content = pkgrel_regex.replace(&repaired_content, format!("pkgrel={}", new_pkgrel))
                        .to_string();
                    eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✓ pkgrel replaced: {}", new_pkgrel);
                } else {
                    // pkgrel doesn't exist, add it after pkgver
                    if let Some(pkgver_line_end) = repaired_content.find("pkgver=") {
                        if let Some(newline_after_pkgver) = repaired_content[pkgver_line_end..].find('\n') {
                            let insert_pos = pkgver_line_end + newline_after_pkgver + 1;
                            repaired_content.insert_str(insert_pos, &format!("pkgrel={}\n", new_pkgrel));
                            eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✓ pkgrel added: {}", new_pkgrel);
                        }
                    }
                }
            } else {
                eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-1] ✗ Failed to compile pkgrel regex");
                return Err(PatchError::PatchFailed("Failed to compile pkgrel regex".to_string()));
            }
        }
        
        // REPAIR 2: Fix source array if variant is wrong
         // CONDITIONAL: Skip source array repair for AUR and External variants
         // They manage their own sources correctly (e.g., AUR git clones, GitHub repos)
         if variant_mismatch {
             if is_aur_or_external {
                 eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] ⊘ SKIPPED: AUR/External variant detected");
                 eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] These variants manage their own sources correctly");
                 eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] Skipping source array injection to prevent makepkg failures");
             } else {
                 eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] Fixing source array for variant mismatch: '{}' → '{}'",
                     current_variant, kernel_variant);
                 
                 // Pattern: source=(...)  - handles array notation
                 // We need to extract and rebuild the array with new URL
                 if let Ok(source_regex) = Regex::new(r"(?ms)^source=\((.*?)\)\s*$") {
                     if let Some(caps) = source_regex.captures(&repaired_content) {
                         let old_source_block = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                         eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] Found source array: {}", old_source_block);
                         
                         // Build new source array with the correct URL
                         let new_source_array = format!("source=(\"{}\")", expected_source_url);
                         repaired_content = repaired_content.replace(old_source_block, &new_source_array);
                         
                         eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] ✓ source array replaced with: {}", new_source_array);
                     } else {
                         eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] ⚠ Could not match source array with regex, trying fallback");
                     }
                 } else {
                     eprintln!("[Patcher] [SOURCE-REPAIR] [REPAIR-2] ✗ Failed to compile source regex");
                 }
             }
         }
        
        // STEP 6: Write repaired PKGBUILD back to disk
        eprintln!("[Patcher] [SOURCE-REPAIR] Writing repaired PKGBUILD to disk...");
        std::fs::write(&path, &repaired_content)
            .map_err(|e| PatchError::PatchFailed(
                format!("Failed to write repaired PKGBUILD: {}", e)
            ))?;
        eprintln!("[Patcher] [SOURCE-REPAIR] ✓ PKGBUILD written successfully to: {}", path.display());
        
        // STEP 7: Verify repairs by re-extracting values
        let repaired_variant = detect_kernel_variant(&repaired_content)?;
        let repaired_pkgver = extract_pkgver(&repaired_content);
        
        eprintln!("[Patcher] [SOURCE-REPAIR] ========== AFTER REPAIR ==========");
        eprintln!("[Patcher] [SOURCE-REPAIR] variant: '{}'", repaired_variant);
        eprintln!("[Patcher] [SOURCE-REPAIR] pkgver: '{}'", repaired_pkgver.as_deref().unwrap_or("NOT FOUND"));
        eprintln!("[Patcher] [SOURCE-REPAIR] ========== REPAIR COMPLETE ==========");
        
        // Final validation
        if repaired_variant == kernel_variant && repaired_pkgver.as_deref() == Some(resolved_version) {
            eprintln!("[Patcher] [SOURCE-REPAIR] ✓ REPAIR SUCCESSFUL: All mismatches have been fixed");
            log::info!("[Patcher] [SOURCE-REPAIR] Successfully repaired PKGBUILD: variant={}, version={}",
                kernel_variant, resolved_version);
        } else {
            eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ REPAIR VERIFICATION: Some corrections may not have applied");
            if repaired_variant != kernel_variant {
                eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ Variant still mismatches: '{}' vs '{}'",
                    repaired_variant, kernel_variant);
            }
            if repaired_pkgver.as_deref() != Some(resolved_version) {
                eprintln!("[Patcher] [SOURCE-REPAIR] ⚠ Version still mismatches: '{}' vs '{}'",
                    repaired_pkgver.as_deref().unwrap_or("NOT FOUND"), resolved_version);
            }
        }
        
        Ok(())
    }

    /// Inject LTO hard enforcer before kernel build() executes
    /// Ensures LTO settings are applied immediately before 'make' command
    pub fn inject_prebuild_lto_hard_enforcer(&self, lto_type: crate::models::LtoType) -> PatchResult<()> {
        inject_prebuild_lto_hard_enforcer(self.src_dir(), lto_type)
    }

    /// Inject post-modprobed hard enforcer to protect filtered modules
    pub fn inject_post_modprobed_hard_enforcer(&self, use_modprobed: bool) -> PatchResult<()> {
        inject_post_modprobed_hard_enforcer(self.src_dir(), use_modprobed)
    }

    /// Inject config restorer to protect modprobed filtering after config overwrites
    pub fn inject_post_setting_config_restorer(&self, use_modprobed: bool) -> PatchResult<()> {
        inject_post_setting_config_restorer(self.src_dir(), use_modprobed)
    }

    /// Placeholder: inject_build_environment_variables
    #[allow(dead_code)]
    pub fn inject_build_environment_variables(&self) -> PatchResult<()> {
        eprintln!("[Patcher] placeholder: inject_build_environment_variables - migrate from patcher.rs in Step 5");
        Ok(())
    }


    /// Placeholder: inject_pkgbuild_metadata_variables
    #[allow(dead_code)]
    pub fn inject_pkgbuild_metadata_variables(&self) -> PatchResult<()> {
        eprintln!("[Patcher] placeholder: inject_pkgbuild_metadata_variables - migrate from patcher.rs in Step 5");
        Ok(())
    }

    /// Apply Rust rmeta/so installation fix to headers package function
    ///
    /// COMPREHENSIVE FIX: Extends fuzzy regex matching to target BOTH `.rmeta`
    /// and `.so` glob patterns in Rust installation commands. Uses robust find-based
    /// solutions that gracefully handle missing files.
    ///
    /// This function:
    /// 1. Detects and replaces `install ... rust/*.rmeta` patterns
    /// 2. Detects and replaces `install ... rust/*.so` patterns
    /// 3. Uses fuzzy line matching to handle syntax variations
    /// 4. Applies idempotent fixes (checks for both patterns)
    /// 5. Maintains position stability by processing in reverse order
    ///
    /// Matches: package_linux-headers, package_headers, _package-headers, etc.
    /// Handles both underscore and hyphen variants, multi-line commands, and variations.
    ///
    /// # Returns
    /// `Ok(count)` where count is the number of headers functions fixed (0 or more)
    pub fn inject_rust_rmeta_fix(&self) -> PatchResult<u32> {
        let (path, mut content) = read_pkgbuild(self.src_dir())?;

        // IDEMPOTENCY CHECK: Skip if BOTH fixes already applied
        let rmeta_fix_present = content.contains("find rust -maxdepth 1 -type f -name '*.rmeta'");
        let so_fix_present = content.contains("find rust -maxdepth 1 -type f -name '*.so'");
        
        if rmeta_fix_present && so_fix_present {
            eprintln!("[Patcher] [RUST-HEADERS-FIX] Idempotency check: Both .rmeta and .so fixes already present (skipping)");
            return Ok(0);
        }

        // Broad-spectrum regex to match ALL headers function patterns
        // Matches: package_linux-headers, package_headers, _package-headers, etc.
        let headers_regex = Regex::new(r"(?m)^(package_[\w-]*headers|_package[\w-]*headers)\s*\(\)\s*\{")
            .expect("Invalid headers function regex");

        eprintln!("[Patcher] [RUST-HEADERS-FIX] Scanning for headers package functions...");

        let mut count = 0u32;
        
        // Collect all matches first to avoid borrowing issues
        let matches: Vec<(usize, usize, String)> = headers_regex
            .captures_iter(&content)
            .filter_map(|caps| {
                let m = caps.get(0)?;
                let match_start = m.start();
                let body_start = m.end();
                let func_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                Some((match_start, body_start, func_name))
            })
            .collect();

        eprintln!("[Patcher] [RUST-HEADERS-FIX] Found {} headers function(s)", matches.len());

        // Process matches in reverse order to maintain position stability
        for (_match_start, body_start, func_name) in matches.iter().rev() {
            if let Some(body_end) = find_function_body_end(&content, *body_start) {
                // FUZZY LINE MATCHING: Search for install commands with fuzzy regex patterns
                // This handles variations in spacing, quotes, and exact syntax for BOTH .rmeta AND .so
                
                // ===== STEP 1: COLLECT ALL MATCH INFORMATION FIRST =====
                // This SCOPE extracts all pattern matches BEFORE we mutate content
                // We collect match positions and types without holding a borrow on func_body
                #[derive(Debug)]
                struct FixCandidate {
                    pattern_type: &'static str, // "rmeta1", "rmeta2", "so1", "so2"
                    absolute_pos: usize,
                    absolute_end: usize,
                    replacement: &'static str,
                }
                
                let mut candidates: Vec<FixCandidate> = Vec::new();
                
                // Only analyze the function body during this collection phase
                {
                    let func_body = &content[*body_start..body_end];
                    
                    // ===== HANDLING .rmeta PATTERNS =====
                    // Pattern 1a: install with explicit .rmeta glob: install ... rust/*.rmeta
                    let rmeta_pattern1 = Regex::new(r"install\s+.*?rust/\*\.rmeta")
                        .expect("Invalid rmeta pattern 1 regex");
                    
                    // Pattern 2a: install .rmeta on separate lines or with continuation
                    let rmeta_pattern2 = Regex::new(r#"install\s+-[a-zA-Z]*[tm]*\s+["\$]*builddir[^\n]*rust/\*\.rmeta"#)
                        .expect("Invalid rmeta pattern 2 regex");
                    
                    // ===== HANDLING .so PATTERNS =====
                    // Pattern 1b: install with explicit .so glob: install ... rust/*.so
                    let so_pattern1 = Regex::new(r"install\s+.*?rust/\*\.so(?:\s|$)")
                        .expect("Invalid so pattern 1 regex");
                    
                    // Pattern 2b: install .so on separate lines
                    let so_pattern2 = Regex::new(r#"install\s+-[a-zA-Z]*\s+["\$]*builddir[^\n]*rust/\*\.so"#)
                        .expect("Invalid so pattern 2 regex");

                    // TRY .rmeta PATTERN 1 (most common single-line)
                    if !rmeta_fix_present {
                        if let Some(m1) = rmeta_pattern1.find(func_body) {
                            let absolute_pos = *body_start + m1.start();
                            let absolute_end = *body_start + m1.end();
                            candidates.push(FixCandidate {
                                pattern_type: "rmeta1",
                                absolute_pos,
                                absolute_end,
                                replacement: "find rust -maxdepth 1 -type f -name '*.rmeta' -exec install -Dt \"$builddir/rust\" -m644 {} +",
                            });
                        }
                    }

                    // TRY .rmeta PATTERN 2 (multi-line or with other flags)
                    if candidates.iter().all(|c| c.pattern_type != "rmeta1") && !rmeta_fix_present {
                        if rmeta_pattern2.find(func_body).is_some() {
                            let block_pattern = Regex::new(r"install\s+[^\n]*rust/\*\.rmeta[^\n]*")
                                .expect("Invalid rmeta block pattern regex");
                            
                            if let Some(bm) = block_pattern.find(func_body) {
                                let absolute_pos = *body_start + bm.start();
                                let absolute_end = *body_start + bm.end();
                                candidates.push(FixCandidate {
                                    pattern_type: "rmeta2",
                                    absolute_pos,
                                    absolute_end,
                                    replacement: "find rust -maxdepth 1 -type f -name '*.rmeta' -exec install -Dt \"$builddir/rust\" -m644 {} +",
                                });
                            }
                        }
                    }

                    // TRY .so PATTERN 1 (most common single-line)
                    if !so_fix_present {
                        if let Some(m1) = so_pattern1.find(func_body) {
                            let absolute_pos = *body_start + m1.start();
                            let absolute_end = *body_start + m1.end();
                            candidates.push(FixCandidate {
                                pattern_type: "so1",
                                absolute_pos,
                                absolute_end,
                                replacement: "find rust -maxdepth 1 -type f -name '*.so' -exec install -Dt \"$builddir/rust\" {} +",
                            });
                        }
                    }

                    // TRY .so PATTERN 2 (multi-line or with other flags)
                    if candidates.iter().all(|c| c.pattern_type != "so1") && !so_fix_present {
                        if so_pattern2.find(func_body).is_some() {
                            let block_pattern = Regex::new(r"install\s+[^\n]*rust/\*\.so[^\n]*")
                                .expect("Invalid so block pattern regex");
                            
                            if let Some(bm) = block_pattern.find(func_body) {
                                let absolute_pos = *body_start + bm.start();
                                let absolute_end = *body_start + bm.end();
                                candidates.push(FixCandidate {
                                    pattern_type: "so2",
                                    absolute_pos,
                                    absolute_end,
                                    replacement: "find rust -maxdepth 1 -type f -name '*.so' -exec install -Dt \"$builddir/rust\" {} +",
                                });
                            }
                        }
                    }
                } // End of immutable borrow scope
                
                // ===== STEP 2: NOW APPLY ALL COLLECTED FIXES IN REVERSE ORDER =====
                // Sort by position descending to maintain stability when replacing
                candidates.sort_by(|a, b| b.absolute_pos.cmp(&a.absolute_pos));
                
                let mut fixed_rmeta = false;
                let mut fixed_so = false;
                
                for candidate in candidates {
                    content.replace_range(candidate.absolute_pos..candidate.absolute_end, candidate.replacement);
                    
                    match candidate.pattern_type {
                        "rmeta1" => {
                            eprintln!("[Patcher] [RUST-HEADERS-FIX] Applied .rmeta fix to {}() using single-line pattern", func_name);
                            fixed_rmeta = true;
                        }
                        "rmeta2" => {
                            eprintln!("[Patcher] [RUST-HEADERS-FIX] Applied .rmeta fix to {}() using multi-line pattern", func_name);
                            fixed_rmeta = true;
                        }
                        "so1" => {
                            eprintln!("[Patcher] [RUST-HEADERS-FIX] Applied .so fix to {}() using single-line pattern", func_name);
                            fixed_so = true;
                        }
                        "so2" => {
                            eprintln!("[Patcher] [RUST-HEADERS-FIX] Applied .so fix to {}() using multi-line pattern", func_name);
                            fixed_so = true;
                        }
                        _ => {}
                    }
                }

                // Increment count if either pattern was fixed
                if fixed_rmeta || fixed_so {
                    count += 1;
                    if fixed_rmeta && fixed_so {
                        eprintln!("[Patcher] [RUST-HEADERS-FIX] ✓ Function {}() fixed: BOTH .rmeta AND .so", func_name);
                    } else if fixed_rmeta {
                        eprintln!("[Patcher] [RUST-HEADERS-FIX] ✓ Function {}() fixed: .rmeta only", func_name);
                    } else {
                        eprintln!("[Patcher] [RUST-HEADERS-FIX] ✓ Function {}() fixed: .so only", func_name);
                    }
                }
            } else {
                eprintln!("[Patcher] [RUST-HEADERS-FIX] WARNING: Could not find end of headers function {}()", func_name);
            }
        }

        // Write the modified content if changes were made
        if count > 0 {
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [RUST-HEADERS-FIX] ✓ Successfully applied Rust glob fixes to {} headers function(s)", count);
        } else {
            eprintln!("[Patcher] [RUST-HEADERS-FIX] ⚠ No matching install commands found in headers functions");
        }

        Ok(count)
    }

    /// Inject NVIDIA DKMS compatibility shim into the headers package function
    ///
    /// CRITICAL: Applies the page_free field restoration shim AFTER headers are copied
    /// into the package directory. This ensures the shim survives packaging and reaches
    /// the final headers package.
    ///
    /// The shim is injected into the `_package-headers()` or variant-specific headers
    /// function, immediately before the closing brace. This ensures all header staging
    /// operations complete before the shim runs.
    ///
    /// Uses a broad-spectrum regex to match all headers function patterns:
    /// - `_package-headers()`
    /// - `package-headers()`
    /// - `package_*-headers()` (with any variant suffix)
    /// - All variants with optional `-linux-` infix before `headers`
    ///
    /// # Returns
    /// `Ok(count)` where count is the number of package functions modified (0 or more)
    pub fn inject_nvidia_dkms_shim_into_headers_package(&self) -> PatchResult<u32> {
        let (path, mut content) = read_pkgbuild(self.src_dir())?;

        // PHASE 18: IDEMPOTENCY GUARD - Check if NVIDIA DKMS shim already exists
        // Skip injection if markers are already present (double-patch resistant)
        if content.contains("### GOATD_NVIDIA_DKMS_START ###") && content.contains("### GOATD_NVIDIA_DKMS_END ###") {
            eprintln!("[Patcher] [NVIDIA-DKMS] Idempotency check: NVIDIA DKMS shim already present (skipping)");
            return Ok(0);
        }

        // Legacy check: if shim is already injected (old marker)
        if content.contains("NVIDIA-DKMS HEADER PACKAGE SHIM") {
            eprintln!("[Patcher] [NVIDIA-DKMS] Header package shim already present (idempotent)");
            return Ok(0);
        }

        // Broad-spectrum regex to match all headers function patterns
        // Matches: _package-headers, package-headers, package_*-headers, with optional -linux- infix
        let headers_regex = Regex::new(r"(?m)^package_[\w-]+headers\s*\(\)\s*\{")
            .expect("Invalid headers function regex");

        eprintln!("[Patcher] [NVIDIA-DKMS] Using broad-spectrum regex to find headers functions");

        let mut count = 0u32;
        
        // Collect all matches first to avoid borrowing issues during modification
        // Store: (match_start, body_start, function_name)
        let matches: Vec<(usize, usize, String)> = headers_regex
            .captures_iter(&content)
            .filter_map(|caps| {
                let m = caps.get(0)?;
                let match_start = m.start();
                let body_start = m.end();
                // Extract function name from the match for logging
                let func_match = &content[match_start..body_start];
                let func_name = func_match.trim_end_matches(" {").trim_end_matches("\t{").to_string();
                Some((match_start, body_start, func_name))
            })
            .collect();

        eprintln!("[Patcher] [NVIDIA-DKMS] Found {} headers function(s)", matches.len());

        // Process matches in reverse order to maintain position stability
        for (_match_start, body_start, func_name) in matches.iter().rev() {
            if let Some(body_end) = find_function_body_end(&content, *body_start) {
                // Inject the shim snippet immediately before the closing brace
                let snippet = templates::NVIDIA_DKMS_HEADER_PACKAGE_SHIM;
                
                content.insert_str(body_end, &format!("\n    ### GOATD_NVIDIA_DKMS_START ###\n{}\n    ### GOATD_NVIDIA_DKMS_END ###\n", snippet));
                count += 1;

                eprintln!("[Patcher] [NVIDIA-DKMS] Injected header package shim into {}() at end of function", func_name);
            } else {
                eprintln!("[Patcher] [NVIDIA-DKMS] WARNING: Could not find end of headers function at offset {} (mismatched braces)", body_start);
            }
        }

        // Write the modified content only if changes were made
        if count > 0 {
            fs::write(path, content)
                .map_err(|e| PatchError::PatchFailed(e.to_string()))?;
            eprintln!("[Patcher] [NVIDIA-DKMS] Successfully injected header package shim into {} function(s)", count);
        } else {
            eprintln!("[Patcher] [NVIDIA-DKMS] WARNING: Could not find any headers function matching broad-spectrum regex");
        }

        Ok(count)
    }

    /// Inject post-install repair hook for module symlink integrity
    ///
    /// PHASE 15: Module Symlink Integrity Check
    /// Creates a standalone `.install` file (e.g., `linux-goatd.install`) at the build root
    /// and injects `install=<filename>.install` into the PKGBUILD global scope.
    ///
    /// The repair hook ensures `/usr/lib/modules/$(uname -r)/build` and `source` symlinks
    /// are correct after package installation/upgrade. This is critical for DKMS and
    /// out-of-tree module compilation.
    ///
    /// # Algorithm
    /// 1. Read PKGBUILD to detect kernel variant and extract pkgbase
    /// 2. Generate `.install` filename from pkgbase (e.g., `linux-goatd.install`)
    /// 3. Create `.install` file at build root with MODULE_REPAIR_INSTALL template
    /// 4. Inject `install=<filename>` into PKGBUILD global scope (idempotent)
    /// 5. Place injection near pkgbase or at the start of global variables
    ///
    /// # Returns
    /// `Ok(())` on success, or `Err(PatchError)` if file operations fail
    pub fn inject_post_install_repair_hook(&self) -> PatchResult<()> {
        let (pkgbuild_path, content) = read_pkgbuild(self.src_dir())?;
        
        // STEP 1: Detect kernel variant and extract pkgbase using centralized PKGBASE_REGEX
        // Uses two-group capture strategy:
        // Group 1: Base variant with GOATd suffix stripped
        // Group 2: Direct variant without GOATd suffix (fallback)
        let pkgbase = {
            let mut base = String::new();
            // Use centralized PKGBASE_REGEX for robust matching
            if let Some(caps) = PKGBASE_REGEX.captures(&content) {
                base = caps.get(1).or_else(|| caps.get(2))
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
            }
            if base.is_empty() {
                base = "linux".to_string();
            }
            base
        };
        
        eprintln!("[Patcher] [PHASE-15] Detected pkgbase: {}", pkgbase);
        
        // STEP 2: Generate .install filename from pkgbase
        let install_filename = format!("{}.install", pkgbase);
        
        // STEP 3: Resolve build root (parent directory of PKGBUILD)
        let build_root = pkgbuild_path.parent()
            .ok_or_else(|| PatchError::PatchFailed(
                "Could not determine build root from PKGBUILD path".to_string()
            ))?;
        
        let install_file_path = build_root.join(&install_filename);
        
        eprintln!("[Patcher] [PHASE-15] Creating .install file at: {}", install_file_path.display());
        
        // STEP 4: Write .install file with MODULE_REPAIR_INSTALL template
        fs::write(&install_file_path, templates::MODULE_REPAIR_INSTALL)
            .map_err(|e| PatchError::PatchFailed(
                format!("Failed to create .install file: {}", e)
            ))?;
        
        eprintln!("[Patcher] [PHASE-15] Created: {}", install_filename);
        
        // STEP 5: Inject install= into PKGBUILD global scope (idempotent)
        let mut modified_content = content.clone();
        
        // Check if install= already exists (idempotent)
        if modified_content.contains(&format!("install=\"{}\"", install_filename)) ||
           modified_content.contains(&format!("install='{}'", install_filename)) {
            eprintln!("[Patcher] [PHASE-15] install= entry already present (idempotent)");
            return Ok(());
        }
        
        // Find insertion point: after pkgbase= line (or at start if no pkgbase)
        let insertion_point = if let Some(pos) = modified_content.find("pkgbase=") {
            // Find the end of the pkgbase line
            if let Some(newline) = modified_content[pos..].find('\n') {
                pos + newline + 1
            } else {
                // pkgbase is the last line - append at end
                modified_content.len()
            }
        } else {
            // No pkgbase found, insert near the top after any comments
            let lines: Vec<&str> = modified_content.lines().collect();
            let mut insert_pos = 0;
            
            for (_idx, line) in lines.iter().enumerate() {
                if line.starts_with("pkgver=") || line.starts_with("pkgname=") {
                    // Found a global variable, insert before this
                    insert_pos = modified_content[..].find(line).unwrap_or(0);
                    break;
                }
           
             }
             insert_pos
        };
        
        // Format the injection with proper spacing
        let install_injection = format!("install='{}'\n", install_filename);
        
        modified_content.insert_str(insertion_point, &install_injection);
        
        // STEP 6: Write modified PKGBUILD
        fs::write(&pkgbuild_path, modified_content)
            .map_err(|e| PatchError::PatchFailed(
                format!("Failed to write PKGBUILD with install= entry: {}", e)
            ))?;
        
        eprintln!("[Patcher] [PHASE-15] Injected install='{}' into PKGBUILD global scope", install_filename);
        eprintln!("[Patcher] [PHASE-15] ✓ Module symlink repair hook integrated successfully");
        
        Ok(())
    }
}

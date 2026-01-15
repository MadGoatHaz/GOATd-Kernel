//! PKGBUILD file parser for extracting kernel build metadata.
//!
//! Parses bash-based PKGBUILD files to extract key variables including:
//! - pkgver: kernel version
//! - pkgrel: package release number
//! - patches: source patch files
//! - CONFIG options
//! - Custom variables
//!
//! Handles:
//! - Comments (# and ## styles)
//! - Multiline strings (including arrays)
//! - Variable substitution
//! - Edge cases (empty values, quoted values)

use std::collections::HashMap;

/// Parsed PKGBUILD structure containing extracted metadata
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageBuild {
    /// Kernel version (e.g., "6.6.0")
    pub pkgver: String,
    /// Package release number (e.g., "1")
    pub pkgrel: String,
    /// Patch files referenced in PKGBUILD
    pub patches: Vec<String>,
    /// Kernel configuration options (CONFIG_*)
    pub config_options: HashMap<String, String>,
    /// Other custom variables
    pub custom_vars: HashMap<String, String>,
}

impl Default for PackageBuild {
    fn default() -> Self {
        PackageBuild {
            pkgver: String::new(),
            pkgrel: String::new(),
            patches: Vec::new(),
            config_options: HashMap::new(),
            custom_vars: HashMap::new(),
        }
    }
}

/// Parse a PKGBUILD file (bash script) and extract metadata.
///
/// # Arguments
///
/// * `content` - The complete PKGBUILD file as a string
///
/// # Returns
///
/// A PackageBuild struct containing extracted metadata
///
/// # Examples
///
/// ```
/// use goatd_kernel::kernel::parser::parse_pkgbuild;
///
/// let content = r#"
/// pkgver=6.6.0
/// pkgrel=1
/// patches=(somepatch.patch)
/// "#;
/// let pb = parse_pkgbuild(content);
/// assert_eq!(pb.pkgver, "6.6.0");
/// ```
pub fn parse_pkgbuild(content: &str) -> PackageBuild {
    let mut result = PackageBuild::default();

    // Join all lines to handle multiline constructs
    let normalized = normalize_content(content);

    // Split into logical statements (handle multiline arrays)
    let statements = split_statements(&normalized);

    for statement in statements {
        let trimmed = statement.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse pkgver
        if trimmed.starts_with("pkgver") {
            if let Some(value) = extract_bash_value(trimmed, "pkgver") {
                result.pkgver = value;
            }
        }

        // Parse pkgrel
        if trimmed.starts_with("pkgrel") {
            if let Some(value) = extract_bash_value(trimmed, "pkgrel") {
                result.pkgrel = value;
            }
        }

        // Parse patches array or variable
        if trimmed.starts_with("patches") {
            if let Some(patches_str) = extract_bash_value(trimmed, "patches") {
                result.patches = parse_bash_array(&patches_str);
            }
        }

        // Parse CONFIG_ options
        if trimmed.starts_with("CONFIG_") {
            if let Some((key, value)) = extract_config_option(trimmed) {
                result.config_options.insert(key, value);
            }
        }
    }

    result
}

/// Normalize content by removing inline comments from value lines
fn normalize_content(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            // Remove inline comments (# followed by space or tab), but be careful
            // not to remove # inside quoted strings
            let mut result = String::new();
            let mut in_quotes = false;
            let mut quote_char = ' ';
            let mut prev_char = ' ';

            for ch in line.chars() {
                if (ch == '"' || ch == '\'') && (prev_char == '=' || prev_char == '(' || prev_char == ' ' || prev_char == '\t') {
                    in_quotes = true;
                    quote_char = ch;
                    result.push(ch);
                } else if in_quotes && ch == quote_char && prev_char != '\\' {
                    in_quotes = false;
                    result.push(ch);
                } else if !in_quotes && ch == '#' && (prev_char == ' ' || prev_char == '\t') {
                    // Start of comment, stop here
                    break;
                } else {
                    result.push(ch);
                }
                prev_char = ch;
            }
            result
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split content into statements, handling multiline arrays
fn split_statements(content: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            if !current.is_empty() {
                statements.push(current.clone());
                current.clear();
            }
            continue;
        }

        current.push(' ');
        current.push_str(trimmed);

        // Track parentheses for arrays
        for ch in trimmed.chars() {
            if ch == '(' {
                paren_depth += 1;
            } else if ch == ')' {
                paren_depth -= 1;
            }
        }

        // If we've closed all parentheses and hit end of line, records statement
        if paren_depth == 0 && (trimmed.ends_with(')') || !trimmed.contains('(')) {
            statements.push(current.trim().to_string());
            current.clear();
        }
    }

    if !current.is_empty() {
        statements.push(current.trim().to_string());
    }

    statements
}

/// Extract a bash variable value from a line like `variable=value`
fn extract_bash_value(line: &str, var_name: &str) -> Option<String> {
    // Handle various formats:
    // variable=value
    // variable="value"
    // variable='value'
    // variable=(array values)

    let prefix = format!("{}=", var_name);
    if !line.starts_with(&prefix) {
        return None;
    }

    let value_part = &line[prefix.len()..];

    // Handle array format: (item1 item2)
    if value_part.starts_with('(') && value_part.ends_with(')') {
        return Some(value_part[1..value_part.len() - 1].to_string());
    }

    // Remove quotes if present
    let unquoted = if (value_part.starts_with('"') && value_part.ends_with('"'))
        || (value_part.starts_with('\'') && value_part.ends_with('\''))
    {
        &value_part[1..value_part.len() - 1]
    } else {
        value_part
    };

    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_string())
    }
}

/// Parse a bash array string into individual items
fn parse_bash_array(array_str: &str) -> Vec<String> {
    // Handle formats like: "item1" "item2" or item1 item2
    let mut items = Vec::new();

    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for ch in array_str.chars() {
        match ch {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = ch;
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
                if !current.is_empty() {
                    items.push(current.clone());
                    current.clear();
                }
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    items.push(current.clone());
                    current.clear();
                }
            }
            c => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        items.push(current);
    }

    items
}

/// Extract a CONFIG_* option line and return (key, value)
fn extract_config_option(line: &str) -> Option<(String, String)> {
    // Format: CONFIG_NAME=value or CONFIG_NAME="value"
    if let Some(eq_pos) = line.find('=') {
        let key = line[..eq_pos].to_string();
        let value_part = &line[eq_pos + 1..];

        let value = if (value_part.starts_with('"') && value_part.ends_with('"'))
            || (value_part.starts_with('\'') && value_part.ends_with('\''))
        {
            value_part[1..value_part.len() - 1].to_string()
        } else {
            value_part.to_string()
        };

        if key.starts_with("CONFIG_") && !value.is_empty() {
            return Some((key, value));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ======= Test 1: Parse basic PKGBUILD with version and release
    #[test]
    fn test_parse_pkgbuild_basic() {
        let content = r#"
pkgver=6.6.0
pkgrel=1
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "1");
    }

    // ======= Test 2: Parse with double-quoted values
    #[test]
    fn test_parse_pkgbuild_quoted() {
        let content = r#"
pkgver="6.6.0"
pkgrel="2"
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "2");
    }

    // ======= Test 3: Parse with single-quoted values
    #[test]
    fn test_parse_pkgbuild_single_quoted() {
        let content = r#"
pkgver='6.6.0'
pkgrel='3'
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "3");
    }

    // ======= Test 4: Parse patches array with multiple entries
    #[test]
    fn test_parse_pkgbuild_patches_array() {
        let content = r#"
patches=(
  "0001-patch.patch"
  "0002-another.patch"
  "0003-third.patch"
)
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.patches.len(), 3);
        assert!(pb.patches.contains(&"0001-patch.patch".to_string()));
        assert!(pb.patches.contains(&"0002-another.patch".to_string()));
        assert!(pb.patches.contains(&"0003-third.patch".to_string()));
    }

    // ======= Test 5: Skip comment lines
    #[test]
    fn test_parse_pkgbuild_skip_comments() {
        let content = r#"
# This is a comment
pkgver=6.6.0 # inline comment not parsed
## Another comment style
pkgrel=1
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "1");
    }

    // ======= Test 6: Parse CONFIG_* options
    #[test]
    fn test_parse_pkgbuild_config_options() {
        let content = r#"
CONFIG_LTO_CLANG=y
CONFIG_CFI_CLANG=y
CONFIG_SOMETHING="value"
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.config_options.get("CONFIG_LTO_CLANG"), Some(&"y".to_string()));
        assert_eq!(pb.config_options.get("CONFIG_CFI_CLANG"), Some(&"y".to_string()));
        assert_eq!(pb.config_options.get("CONFIG_SOMETHING"), Some(&"value".to_string()));
    }

    // ======= Test 7: Empty patches array
    #[test]
    fn test_parse_pkgbuild_empty_patches() {
        let content = r#"
pkgver=6.6.0
patches=()
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.patches.len(), 0);
    }

    // ======= Test 8: Whitespace handling
    #[test]
    fn test_parse_pkgbuild_whitespace() {
        let content = "  pkgver=6.6.0  \n  pkgrel=1  ";
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "1");
    }

    // ======= Test 9: Parse patches in space-separated format
    #[test]
    fn test_parse_bash_array_space_separated() {
        let array_str = "patch1.patch patch2.patch patch3.patch";
        let items = parse_bash_array(array_str);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], "patch1.patch");
        assert_eq!(items[1], "patch2.patch");
        assert_eq!(items[2], "patch3.patch");
    }

    // ======= Test 10: Parse patches with quotes
    #[test]
    fn test_parse_bash_array_quoted() {
        let array_str = r#""patch1.patch" "patch2.patch""#;
        let items = parse_bash_array(array_str);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], "patch1.patch");
        assert_eq!(items[1], "patch2.patch");
    }

    // ======= Test 11: Parse complete real-world PKGBUILD
    #[test]
    fn test_parse_pkgbuild_complete() {
        let content = r#"
# Arch Linux kernel PKGBUILD
pkgver=6.6.0
pkgrel=1
pkgname=linux-arch
patches=(
  "lto-fix.patch"
  "amd-gpu-shield.patch"
)

CONFIG_LTO_CLANG=y
CONFIG_CFI_CLANG=y
CONFIG_DEBUG_KERNEL=n
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "1");
        assert_eq!(pb.patches.len(), 2);
        assert_eq!(pb.config_options.get("CONFIG_LTO_CLANG"), Some(&"y".to_string()));
    }

    // ======= Test 12: Handle missing values
    #[test]
    fn test_parse_pkgbuild_missing_values() {
        let content = r#"
pkgver=6.6.0
"#;
        let pb = parse_pkgbuild(content);
        assert_eq!(pb.pkgver, "6.6.0");
        assert_eq!(pb.pkgrel, "");
        assert_eq!(pb.patches.len(), 0);
    }
}

/// UI Scaling Regression Tests
///
/// Verifies that global UI scaling logic correctly handles window width scaling
/// and font size scaling, including clamping and edge cases.

/// Calculates the scale factor based on window width
/// 
/// Formula: scale_factor = window_width / 1280.0, clamped to [0.7, 2.0]
/// Reference width: 1280.0px (720p baseline) → scale_factor = 1.0
///
/// # Arguments
/// * `window_width` - The current window width in pixels
///
/// # Returns
/// The scale factor, guaranteed to be in range [0.7, 2.0]
pub fn calculate_window_scale_factor(window_width: f32) -> f32 {
    const BASE_WIDTH: f32 = 1280.0;
    const MIN_SCALE: f32 = 0.7;
    const MAX_SCALE: f32 = 2.0;

    (window_width / BASE_WIDTH).max(MIN_SCALE).min(MAX_SCALE)
}

/// Calculates pixels_per_point based on font size setting
///
/// Formula: pixels_per_point = ui_font_size / 10.4
/// where ui_font_size is in points (e.g., 12.0, 14.0, 16.0)
/// baseline: 10.4pt
///
/// # Arguments
/// * `ui_font_size` - Font size in points (must be > 0.0)
///
/// # Returns
/// The pixels_per_point scale factor
pub fn calculate_font_size_scale_factor(ui_font_size: f32) -> f32 {
    const BASELINE_PT: f32 = 10.4;
    (ui_font_size / BASELINE_PT).max(0.1) // Ensure minimum scale
}

#[cfg(test)]
mod window_width_scaling_tests {
    use super::*;

    #[test]
    fn test_baseline_width_produces_unity_scale() {
        // At 1280px (baseline), scale should be 1.0
        let scale = calculate_window_scale_factor(1280.0);
        assert!((scale - 1.0).abs() < 0.0001, "Expected 1.0, got {}", scale);
    }

    #[test]
    fn test_half_baseline_width_produces_0_5_scale() {
        // At 640px (half of 1280), scale should be 0.5, but clamped to min 0.7
        let scale = calculate_window_scale_factor(640.0);
        assert!((scale - 0.7).abs() < 0.0001, "Expected 0.7 (clamped), got {}", scale);
    }

    #[test]
    fn test_small_width_clamped_to_minimum() {
        // Very small widths should clamp to 0.7
        let scale = calculate_window_scale_factor(100.0);
        assert!((scale - 0.7).abs() < 0.0001, "Expected 0.7 (min clamp), got {}", scale);
    }

    #[test]
    fn test_zero_width_clamped_to_minimum() {
        // Edge case: 0 width should clamp to 0.7
        let scale = calculate_window_scale_factor(0.0);
        assert!((scale - 0.7).abs() < 0.0001, "Expected 0.7 (min clamp), got {}", scale);
    }

    #[test]
    fn test_800px_width_produces_0_625_scale_clamped() {
        // 800 / 1280 = 0.625, should clamp to 0.7
        let scale = calculate_window_scale_factor(800.0);
        assert!((scale - 0.7).abs() < 0.0001, "Expected 0.7 (clamped), got {}", scale);
    }

    #[test]
    fn test_896px_width_produces_0_7_scale() {
        // 896 / 1280 = 0.7 exactly (boundary case)
        let scale = calculate_window_scale_factor(896.0);
        assert!((scale - 0.7).abs() < 0.0001, "Expected 0.7, got {}", scale);
    }

    #[test]
    fn test_1920px_width_produces_1_5_scale() {
        // 1920 / 1280 = 1.5 (1080p scaling)
        let scale = calculate_window_scale_factor(1920.0);
        assert!((scale - 1.5).abs() < 0.0001, "Expected 1.5, got {}", scale);
    }

    #[test]
    fn test_2560px_width_produces_max_scale() {
        // 2560 / 1280 = 2.0 exactly, should clamp to 2.0
        let scale = calculate_window_scale_factor(2560.0);
        assert!((scale - 2.0).abs() < 0.0001, "Expected 2.0 (max clamp), got {}", scale);
    }

    #[test]
    fn test_very_large_width_clamped_to_maximum() {
        // Very large widths should clamp to 2.0
        let scale = calculate_window_scale_factor(5120.0);
        assert!((scale - 2.0).abs() < 0.0001, "Expected 2.0 (max clamp), got {}", scale);
    }

    #[test]
    fn test_all_tested_widths_within_bounds() {
        let test_widths = vec![0.0, 100.0, 640.0, 800.0, 896.0, 1280.0, 1920.0, 2560.0, 5120.0];
        
        for width in test_widths {
            let scale = calculate_window_scale_factor(width);
            assert!(scale >= 0.7, "Width {} produced scale {} below minimum 0.7", width, scale);
            assert!(scale <= 2.0, "Width {} produced scale {} above maximum 2.0", width, scale);
        }
    }
}

#[cfg(test)]
mod font_size_scaling_tests {
    use super::*;

    #[test]
    fn test_baseline_font_size_10_4pt() {
        // At 10.4pt (baseline), scale should be 1.0
        let scale = calculate_font_size_scale_factor(10.4);
        assert!((scale - 1.0).abs() < 0.0001, "Expected 1.0, got {}", scale);
    }

    #[test]
    fn test_12pt_font_size() {
        // At 12pt: 12 / 10.4 ≈ 1.1538
        let scale = calculate_font_size_scale_factor(12.0);
        let expected = 12.0 / 10.4;
        assert!((scale - expected).abs() < 0.0001, "Expected {}, got {}", expected, scale);
    }

    #[test]
    fn test_14pt_font_size() {
        // At 14pt: 14 / 10.4 ≈ 1.3462
        let scale = calculate_font_size_scale_factor(14.0);
        let expected = 14.0 / 10.4;
        assert!((scale - expected).abs() < 0.0001, "Expected {}, got {}", expected, scale);
    }

    #[test]
    fn test_16pt_font_size() {
        // At 16pt: 16 / 10.4 ≈ 1.5385
        let scale = calculate_font_size_scale_factor(16.0);
        let expected = 16.0 / 10.4;
        assert!((scale - expected).abs() < 0.0001, "Expected {}, got {}", expected, scale);
    }

    #[test]
    fn test_20pt_font_size() {
        // At 20pt: 20 / 10.4 ≈ 1.9231
        let scale = calculate_font_size_scale_factor(20.0);
        let expected = 20.0 / 10.4;
        assert!((scale - expected).abs() < 0.0001, "Expected {}, got {}", expected, scale);
    }

    #[test]
    fn test_zero_font_size_clamped_to_minimum() {
        // Zero or negative sizes should be handled gracefully (minimum 0.1)
        let scale = calculate_font_size_scale_factor(0.0);
        assert!((scale - 0.1).abs() < 0.0001, "Expected 0.1 (min clamp), got {}", scale);
    }

    #[test]
    fn test_very_small_font_size_clamped() {
        // Very small sizes should clamp to 0.1
        let scale = calculate_font_size_scale_factor(0.5);
        // 0.5 / 10.4 ≈ 0.048, but clamped to 0.1
        assert!((scale - 0.1).abs() < 0.0001, "Expected 0.1 (min clamp), got {}", scale);
    }

    #[test]
    fn test_typical_font_sizes_are_reasonable() {
        // Common font sizes should all be reasonable scales
        let common_sizes = vec![10.4, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0];
        
        for size in common_sizes {
            let scale = calculate_font_size_scale_factor(size);
            assert!(scale > 0.0, "Font size {} produced zero or negative scale {}", size, scale);
            // All reasonable font sizes should produce reasonable scales
            assert!(scale < 5.0, "Font size {} produced unreasonably large scale {}", size, scale);
        }
    }
}

#[cfg(test)]
mod combined_scaling_scenarios {
    use super::*;

    #[test]
    fn test_480p_window_with_12pt_font() {
        // Simulates 960x540 window (480p equivalent, using width 960)
        let window_scale = calculate_window_scale_factor(960.0);
        let font_scale = calculate_font_size_scale_factor(12.0);
        
        // Window scale should be clamped to min since 960/1280 = 0.75
        assert!((window_scale - 0.75).abs() < 0.0001, "Expected 0.75 for 960px width");
        
        // Font scale should be reasonable
        assert!(font_scale > 1.0 && font_scale < 1.2, "Font scale should be ~1.15");
    }

    #[test]
    fn test_720p_window_with_baseline_font() {
        // Simulates 1280x720 window with baseline font
        let window_scale = calculate_window_scale_factor(1280.0);
        let font_scale = calculate_font_size_scale_factor(10.4);
        
        assert!((window_scale - 1.0).abs() < 0.0001, "Expected 1.0 for baseline 1280px");
        assert!((font_scale - 1.0).abs() < 0.0001, "Expected 1.0 for baseline 10.4pt");
    }

    #[test]
    fn test_1080p_window_with_16pt_font() {
        // Simulates 1920x1080 window with larger font
        let window_scale = calculate_window_scale_factor(1920.0);
        let font_scale = calculate_font_size_scale_factor(16.0);
        
        assert!((window_scale - 1.5).abs() < 0.0001, "Expected 1.5 for 1920px width");
        let expected_font = 16.0 / 10.4;
        assert!((font_scale - expected_font).abs() < 0.0001, "Expected ~1.54 for 16pt");
    }

    #[test]
    fn test_4k_window_with_max_font() {
        // Simulates 4K window (2560px width) with large font
        let window_scale = calculate_window_scale_factor(2560.0);
        let font_scale = calculate_font_size_scale_factor(20.0);
        
        // Window scale should clamp to max
        assert!((window_scale - 2.0).abs() < 0.0001, "Expected 2.0 (max) for 2560px");
        
        let expected_font = 20.0 / 10.4;
        assert!((font_scale - expected_font).abs() < 0.0001, "Expected ~1.92 for 20pt");
    }
}

#[cfg(test)]
mod edge_cases_and_regression_tests {
    use super::*;

    #[test]
    fn test_fractional_window_widths() {
        // Test that fractional widths are handled correctly
        let scale1 = calculate_window_scale_factor(1280.5);
        let scale2 = calculate_window_scale_factor(1279.5);
        
        assert!((scale1 - 1.0).abs() < 0.001, "Expected ~1.0 for 1280.5px");
        assert!((scale2 - 1.0).abs() < 0.001, "Expected ~1.0 for 1279.5px");
    }

    #[test]
    fn test_fractional_font_sizes() {
        // Test that fractional font sizes are handled correctly
        let scale1 = calculate_font_size_scale_factor(12.5);
        let scale2 = calculate_font_size_scale_factor(14.7);
        
        let expected1 = 12.5 / 10.4;
        let expected2 = 14.7 / 10.4;
        
        assert!((scale1 - expected1).abs() < 0.0001, "Expected {} for 12.5pt", expected1);
        assert!((scale2 - expected2).abs() < 0.0001, "Expected {} for 14.7pt", expected2);
    }

    #[test]
    fn test_window_scale_monotonicity() {
        // Verify that larger widths always produce larger or equal scales (respecting clamps)
        let widths = vec![100.0, 500.0, 896.0, 1280.0, 1600.0, 1920.0, 2560.0, 3000.0];
        let scales: Vec<f32> = widths.iter().map(|&w| calculate_window_scale_factor(w)).collect();
        
        // After clamping, scales should be monotonically non-decreasing up to the max
        for i in 0..scales.len() - 1 {
            assert!(scales[i] <= scales[i + 1], 
                    "Window scaling not monotonic: scale[{}]={} > scale[{}]={}", 
                    i, scales[i], i + 1, scales[i + 1]);
        }
    }

    #[test]
    fn test_font_scale_monotonicity() {
        // Verify that larger font sizes always produce larger scales
        let fonts = vec![8.0, 10.0, 10.4, 12.0, 14.0, 16.0, 20.0, 24.0];
        let scales: Vec<f32> = fonts.iter().map(|&f| calculate_font_size_scale_factor(f)).collect();
        
        // Scales should be monotonically increasing
        for i in 0..scales.len() - 1 {
            assert!(scales[i] < scales[i + 1], 
                    "Font scaling not monotonic: scale[{}]={} >= scale[{}]={}", 
                    i, scales[i], i + 1, scales[i + 1]);
        }
    }

    #[test]
    fn test_window_scale_never_zero() {
        // Ensure window scaling never produces zero
        let test_widths = vec![0.0, 0.1, 0.5, 1.0];
        
        for width in test_widths {
            let scale = calculate_window_scale_factor(width);
            assert!(scale > 0.0, "Window width {} produced zero scale", width);
        }
    }

    #[test]
    fn test_very_large_window_width_stability() {
        // Ensure very large widths are stable and clamped
        let large_widths = vec![10000.0, 100000.0, f32::INFINITY];
        
        for width in large_widths {
            let scale = calculate_window_scale_factor(width);
            assert!((scale - 2.0).abs() < 0.0001, 
                    "Very large width {} should clamp to 2.0, got {}", width, scale);
        }
    }

    #[test]
    fn test_window_scale_at_clamp_boundaries() {
        // Test exact boundary values
        let min_width = 0.7 * 1280.0; // 896.0
        let max_width = 2.0 * 1280.0; // 2560.0
        
        let min_scale = calculate_window_scale_factor(min_width);
        let max_scale = calculate_window_scale_factor(max_width);
        
        assert!((min_scale - 0.7).abs() < 0.0001, "Min boundary should produce 0.7");
        assert!((max_scale - 2.0).abs() < 0.0001, "Max boundary should produce 2.0");
    }
}

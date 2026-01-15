/// Custom egui Widgets
/// 
/// Implementations of specialized widgets using egui::Painter:
/// - RadialGauge: Circular progress indicator for KPIs
/// - Sparkline: Compact line chart for historical data
/// - CPUHeatmap: Temperature distribution across cores
/// - TerminalViewport: Monospace log viewer

use eframe::egui;
use egui::{Pos2, Rect, Stroke, Color32, Vec2};

/// Draws a radial gauge (circular progress indicator) with descriptive label
///
/// # Arguments
/// * `ui` - egui Ui context
/// * `value` - Current value
/// * `range` - Value range (min..max)
/// * `label` - Display label (e.g., "Max Latency (µs)")
///
/// The gauge displays:
/// - A circular arc showing progress (0-100% normalized)
/// - The numeric value in microunits at the center
/// - The descriptive label below the gauge
pub fn radial_gauge(
    ui: &mut egui::Ui,
    value: f32,
    range: std::ops::Range<f32>,
    label: &str,
) {
    // Reserve space for gauge + label below (140px total height for proper spacing)
    let (response, painter) = ui.allocate_painter(
        Vec2::new(120.0, 140.0),
        egui::Sense::hover(),
    );
    
    let gauge_rect = response.rect;
    let center = Pos2::new(gauge_rect.center().x, gauge_rect.top() + 60.0); // Centered in upper 120px
    let radius = 55.0;
    
    // Normalize value to 0..1 for both fill and color
    let normalized = ((value - range.start) / (range.end - range.start)).clamp(0.0, 1.0);
    
    // Determine fill color based on value (green -> yellow -> red)
    let fill_color = if normalized < 0.5 {
        // Green to yellow: 0.0-0.5
        let t = normalized * 2.0; // 0-1
        Color32::from_rgb(
            (255.0 * t) as u8,
            255,
            0,
        )
    } else {
        // Yellow to red: 0.5-1.0
        let t = (normalized - 0.5) * 2.0; // 0-1
        Color32::from_rgb(
            255,
            (255.0 * (1.0 - t)) as u8,
            0,
        )
    };
    
    // Draw background circle (unfilled gauge area)
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(3.0, Color32::from_gray(100)),
    );
    
    // Draw filled arc using line segments for smooth appearance
    let start_angle = -std::f32::consts::PI * 0.75; // -135°
    let end_angle = start_angle + std::f32::consts::PI * 1.5 * normalized; // Arc length based on normalized value
    
    let segments = (50.0 * normalized) as usize + 2;
    let arc_radius = radius - 2.0;
    
    for i in 0..segments {
        let t0 = i as f32 / (segments - 1).max(1) as f32;
        let t1 = (i + 1) as f32 / (segments - 1).max(1) as f32;
        
        let angle0 = start_angle + (end_angle - start_angle) * t0;
        let angle1 = start_angle + (end_angle - start_angle) * t1;
        
        let p0 = center + Vec2::new(angle0.cos() * arc_radius, angle0.sin() * arc_radius);
        let p1 = center + Vec2::new(angle1.cos() * arc_radius, angle1.sin() * arc_radius);
        
        painter.line_segment(
            [Pos2::from(p0), Pos2::from(p1)],
            Stroke::new(6.0, fill_color),
        );
    }
    
    // Draw outer circle outline
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(2.0, Color32::DARK_GRAY),
    );
    
    // Draw numeric value text in center (showing actual value, not just percentage)
    let display_text = format!("{:.1}", value);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        display_text,
        egui::FontId::new(14.0, egui::FontFamily::Monospace),
        Color32::WHITE,
    );
    
    // Draw label below gauge using ui.label for proper layout spacing
    // This reserves space and prevents overlap with subsequent elements
    painter.text(
        Pos2::new(gauge_rect.center().x, center.y + radius + 10.0),
        egui::Align2::CENTER_TOP,
        label,
        egui::FontId::new(10.0, egui::FontFamily::Proportional),
        Color32::LIGHT_GRAY,
    );
}

/// Draws a sparkline (compact line chart)
///
/// # Arguments
/// * `ui` - egui Ui context
/// * `samples` - Historical data points
/// * `label` - Display label
pub fn sparkline(
    ui: &mut egui::Ui,
    samples: &[f32],
    label: &str,
) {
    if samples.is_empty() {
        ui.label(label);
        return;
    }
    
    // Draw label before chart
    ui.label(label);
    
    // Use fixed maximum height to prevent vertical overflow
    // Chart stays compact within container, leaves room for other controls below
    let max_chart_height = 100.0; // Fixed max height
    let chart_height = max_chart_height;
    
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width(), chart_height),
        egui::Sense::hover(),
    );
    
    let rect = response.rect;
    let width = rect.width();
    let height = rect.height();
    
    // Find min/max for scaling with robustness
    let (min_val, max_val) = samples.iter().fold(
        (f32::INFINITY, f32::NEG_INFINITY),
        |(min, max), &v| (min.min(v), max.max(v)),
    );
    let range = (max_val - min_val).max(0.001);
    
    // Draw background
    painter.rect_filled(rect, 0.0, Color32::from_rgb(46, 52, 64));
    
    // Helper function to map sample value to Y coordinate
    let value_to_y = |val: f32| -> f32 {
        rect.bottom() - ((val - min_val) / range) * height
    };
    
    // Build filled area polygon (area chart effect)
    let mut area_points = Vec::new();
    
    // Top edge of area chart (line through data points)
    for (i, &sample) in samples.iter().enumerate() {
        let x = rect.left() + i as f32 / (samples.len() - 1).max(1) as f32 * width;
        let y = value_to_y(sample);
        area_points.push(Pos2::new(x, y));
    }
    
    // Bottom edge (baseline at bottom of chart)
    for i in (0..samples.len()).rev() {
        let x = rect.left() + i as f32 / (samples.len() - 1).max(1) as f32 * width;
        let y = rect.bottom();
        area_points.push(Pos2::new(x, y));
    }
    
    // Draw filled area using polygon with transparency
    if area_points.len() >= 3 {
        painter.add(egui::Shape::convex_polygon(
            area_points,
            Color32::from_rgba_unmultiplied(136, 192, 208, 80),
            Stroke::NONE,
        ));
    }
    
    // Draw line on top of filled area with improved stroke
    let line_stroke = Stroke::new(2.0, Color32::from_rgb(136, 192, 208));
    for i in 1..samples.len() {
        let x0 = rect.left() + (i - 1) as f32 / (samples.len() - 1).max(1) as f32 * width;
        let y0 = value_to_y(samples[i - 1]);
        let x1 = rect.left() + i as f32 / (samples.len() - 1).max(1) as f32 * width;
        let y1 = value_to_y(samples[i]);
        
        painter.line_segment(
            [Pos2::new(x0, y0), Pos2::new(x1, y1)],
            line_stroke,
        );
    }
    
    // Draw subtle grid lines
    let grid_color = Color32::from_gray(70);
    for tick in 0..=4 {
        let y = rect.top() + tick as f32 / 4.0 * height;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(0.5, grid_color),
        );
    }
    
    // Draw border around chart
    painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(90)));
}

/// Draws a CPU heatmap grid with temperature-based color gradients and core labels
///
/// Layout:
/// - Spreads cells horizontally to utilize full window width
/// - Each cell labeled with core/thread number (0...N-1)
/// - Temperature displayed as color gradient: Blue (cold) -> Yellow (warm) -> Red (hot)
/// - Dynamic sizing based on physical core count
///
/// The heatmap adapts to any core count:
/// - 4-8 cores: 1 row, 4-8 columns
/// - 16 cores: 2 rows, 8 columns
/// - 32 cores: 4 rows, 8 columns
pub fn cpu_heatmap(
    ui: &mut egui::Ui,
    core_temps: &[f32],
    _label: &str,
) {
    if core_temps.is_empty() {
        ui.label("No core temperature data available");
        return;
    }
    
    // Calculate heat statistics
    let max_temp = core_temps.iter().fold(0.0_f32, |a, &b| a.max(b));
    let min_temp = core_temps.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let avg_temp = core_temps.iter().sum::<f32>() / core_temps.len().max(1) as f32;
    
    // Use a reasonable temperature range (0-100°C as baseline)
    let temp_min = min_temp.min(20.0);
    let temp_max = max_temp.max(80.0);
    let temp_range = (temp_max - temp_min).max(1.0);
    
    // Helper function to map temperature to color (blue -> yellow -> red)
    let temp_to_color = |temp: f32| -> Color32 {
        let normalized = ((temp - temp_min) / temp_range).clamp(0.0, 1.0);
        
        if normalized < 0.5 {
            // Blue to yellow: 0.0-0.5
            let t = normalized * 2.0;
            Color32::from_rgb(
                (255.0 * t) as u8,
                (255.0 * t) as u8,
                (255.0 * (1.0 - t)) as u8,
            )
        } else {
            // Yellow to red: 0.5-1.0
            let t = (normalized - 0.5) * 2.0;
            Color32::from_rgb(
                255,
                (255.0 * (1.0 - t)) as u8,
                0,
            )
        }
    };
    
    // Determine grid layout - spread horizontally across available width
    // Maximum 8 columns to keep cells reasonably sized
    let cols = (core_temps.len() as f32).sqrt().ceil().min(8.0) as usize;
    let rows = ((core_temps.len() + cols - 1) / cols).max(1);
    
    // Use full available width, scale height proportionally (cells ~50px wide, ~45px tall)
    let available_width = ui.available_width();
    let heatmap_width = available_width - 10.0; // Small margin
    let cell_width = (heatmap_width / cols as f32).max(40.0); // Minimum 40px width
    let cell_height = 45.0;
    let heatmap_height = (rows as f32) * cell_height + 10.0;
    
    let (response, painter) = ui.allocate_painter(
        Vec2::new(heatmap_width, heatmap_height),
        egui::Sense::hover(),
    );
    
    let rect = response.rect;
    
    // Draw background
    painter.rect_filled(rect, 2.0, Color32::from_gray(40));
    
    let cell_padding = 2.0;
    
    // Draw temperature cells with core labels
    for (i, &temp) in core_temps.iter().enumerate() {
        let row = i / cols;
        let col = i % cols;
        
        let cell_left = rect.left() + col as f32 * cell_width + cell_padding;
        let cell_top = rect.top() + row as f32 * cell_height + cell_padding;
        let cell_rect = Rect::from_min_size(
            Pos2::new(cell_left, cell_top),
            Vec2::new(cell_width - 2.0 * cell_padding, cell_height - 2.0 * cell_padding),
        );
        
        let color = temp_to_color(temp);
        
        // Draw filled cell
        painter.rect_filled(cell_rect, 1.0, color);
        
        // Draw border
        painter.rect_stroke(cell_rect, 1.0, Stroke::new(1.0, Color32::from_gray(80)));
        
        // Draw temperature value in upper half
        painter.text(
            Pos2::new(cell_rect.center().x, cell_rect.top() + 12.0),
            egui::Align2::CENTER_CENTER,
            format!("{:.0}°C", temp),
            egui::FontId::new(11.0, egui::FontFamily::Monospace),
            Color32::BLACK,
        );
        
        // Draw core label in lower half (e.g., "Core 0", "Core 15")
        painter.text(
            Pos2::new(cell_rect.center().x, cell_rect.bottom() - 8.0),
            egui::Align2::CENTER_CENTER,
            format!("Core {}", i),
            egui::FontId::new(8.0, egui::FontFamily::Proportional),
            Color32::BLACK,
        );
    }
    
    // Draw border around entire heatmap
    painter.rect_stroke(rect, 2.0, Stroke::new(2.0, Color32::from_gray(120)));
    
    // Statistics below the heatmap using ui.label for proper layout spacing
    ui.separator();
    ui.label(
        egui::RichText::new(format!(
            "Temperature Range: {:.1}°C - {:.1}°C | Average: {:.1}°C | Cores: {}",
            min_temp, max_temp, avg_temp, core_temps.len()
        ))
        .monospace()
        .color(Color32::LIGHT_GRAY)
        .small()
    );
}

/// Terminal-style log viewer with monospace font
pub fn terminal_viewport(
    ui: &mut egui::Ui,
    log_content: &str,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.monospace(if log_content.is_empty() {
                "Awaiting output..."
            } else {
                log_content
            });
        });
}

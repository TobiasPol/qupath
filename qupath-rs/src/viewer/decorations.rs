//! Overview, scale bar, location text and grid, drawn over the viewer as in QuPath.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};

use super::transform::Transform;
use crate::objects::{Calibration, Pt};

/// Size of the overview's longest side, in points.
pub const OVERVIEW_SIZE: f32 = 180.0;

/// Where the overview sits for an image of a given aspect.
pub fn overview_rect(viewer: Rect, width: u32, height: u32) -> Rect {
    let aspect = width as f32 / height.max(1) as f32;
    let size = if aspect >= 1.0 { vec2(OVERVIEW_SIZE, OVERVIEW_SIZE / aspect) } else { vec2(OVERVIEW_SIZE * aspect, OVERVIEW_SIZE) };
    Rect::from_min_size(pos2(viewer.right() - size.x - 10.0, viewer.top() + 10.0), size)
}

/// Draws the overview thumbnail with the visible region outlined.
pub fn draw_overview(painter: &Painter, t: &Transform, tex: egui::TextureId, width: u32, height: u32) -> Rect {
    let r = overview_rect(t.rect, width, height);
    painter.rect_filled(r.expand(2.0), 2.0, Color32::from_black_alpha(140));
    painter.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    let v = t.visible_bounds();
    let sx = r.width() as f64 / f64::from(width);
    let sy = r.height() as f64 / f64::from(height);
    let vr = Rect::from_min_max(
        pos2(r.left() + (v.x0 * sx) as f32, r.top() + (v.y0 * sy) as f32),
        pos2(r.left() + (v.x1 * sx) as f32, r.top() + (v.y1 * sy) as f32),
    )
    .intersect(r);
    painter.rect_stroke(vr, 0.0, Stroke::new(1.5, Color32::from_rgb(255, 60, 60)), StrokeKind::Middle);
    r
}

/// The image point an overview click refers to.
pub fn overview_to_image(r: Rect, p: Pos2, width: u32, height: u32) -> Pt {
    Pt { x: f64::from((p.x - r.left()) / r.width()) * f64::from(width), y: f64::from((p.y - r.top()) / r.height()) * f64::from(height) }
}

/// A "nice" length (1, 2, 5 × 10ⁿ) close to `target`.
pub fn nice_length(target: f64) -> f64 {
    if target <= 0.0 {
        return 1.0;
    }
    let p = 10f64.powf(target.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * p).rfind(|v| *v <= target).unwrap_or(p)
}

/// Draws a scale bar about a sixth of the viewer wide.
pub fn draw_scalebar(painter: &Painter, t: &Transform, cal: &Calibration, text: Color32) {
    let units_per_point = t.downsample * cal.average();
    let len = nice_length(f64::from(t.rect.width()) / 6.0 * units_per_point);
    let px = (len / units_per_point) as f32;
    let y = t.rect.bottom() - 18.0;
    let x0 = t.rect.left() + 18.0;
    let (a, b) = (pos2(x0, y), pos2(x0 + px, y));
    let shadow = Stroke::new(5.0, Color32::from_black_alpha(140));
    let bar = Stroke::new(3.0, Color32::WHITE);
    for s in [shadow, bar] {
        painter.line_segment([a, b], s);
        painter.line_segment([a - vec2(0.0, 5.0), a + vec2(0.0, 2.0)], s);
        painter.line_segment([b - vec2(0.0, 5.0), b + vec2(0.0, 2.0)], s);
    }
    let label = format_length(len, cal.unit());
    outlined_text(painter, pos2((a.x + b.x) / 2.0, y - 6.0), Align2::CENTER_BOTTOM, &label, text);
}

fn format_length(v: f64, unit: &str) -> String {
    if unit == "µm" && v >= 1000.0 { format!("{} mm", trim(v / 1000.0)) } else { format!("{} {unit}", trim(v)) }
}

fn trim(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// White text with a dark outline, readable over any image.
pub fn outlined_text(painter: &Painter, at: Pos2, align: Align2, text: &str, color: Color32) {
    let font = FontId::proportional(12.5);
    for d in [vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0)] {
        painter.text(at + d, align, text, font.clone(), Color32::from_black_alpha(170));
    }
    painter.text(at, align, text, font, color);
}

/// Draws the cursor location (and pixel values) at the bottom right.
pub fn draw_location(painter: &Painter, t: &Transform, p: Pt, cal: &Calibration, values: Option<String>) {
    let (x, y) = (p.x * cal.pixel_width, p.y * cal.pixel_height);
    let mut s = format!("x = {x:.2} {u}, y = {y:.2} {u}", u = cal.unit());
    if let Some(v) = values {
        s.push('\n');
        s.push_str(&v);
    }
    outlined_text(painter, t.rect.right_bottom() - vec2(12.0, 10.0), Align2::RIGHT_BOTTOM, &s, Color32::WHITE);
}

/// Draws grid lines every `spacing` (calibrated units).
pub fn draw_grid(painter: &Painter, t: &Transform, spacing: f64, cal: &Calibration, width: u32, height: u32) {
    let step = spacing / cal.average();
    if step / t.downsample < 4.0 {
        return;
    }
    let v = t.visible_bounds();
    let stroke = Stroke::new(1.0, Color32::from_white_alpha(110));
    let mut x = (v.x0.max(0.0) / step).floor() * step;
    while x <= v.x1.min(f64::from(width)) {
        let a = t.to_screen(Pt { x, y: v.y0.max(0.0) });
        let b = t.to_screen(Pt { x, y: v.y1.min(f64::from(height)) });
        painter.line_segment([a, b], stroke);
        x += step;
    }
    let mut y = (v.y0.max(0.0) / step).floor() * step;
    while y <= v.y1.min(f64::from(height)) {
        let a = t.to_screen(Pt { x: v.x0.max(0.0), y });
        let b = t.to_screen(Pt { x: v.x1.min(f64::from(width)), y });
        painter.line_segment([a, b], stroke);
        y += step;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nice_lengths() {
        assert_eq!(super::nice_length(730.0), 500.0);
        assert_eq!(super::nice_length(0.27), 0.2);
        assert_eq!(super::nice_length(10.0), 10.0);
    }
}

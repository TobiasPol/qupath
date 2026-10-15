//! ROIs as screen shapes that egui tessellates safely.

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Shape, Stroke};

use super::transform::Transform;
use crate::objects::{Pt, Roi};

/// Draws one ROI.
pub fn draw_roi(painter: &Painter, t: &Transform, roi: &Roi, stroke: Stroke, fill: Option<Color32>, point_radius: f32) {
    painter.extend(roi_shapes(t, roi, stroke, fill, point_radius));
}

/// Shapes for a ROI, with vertices less than a point apart dropped.
pub fn roi_shapes(t: &Transform, roi: &Roi, stroke: Stroke, fill: Option<Color32>, point_radius: f32) -> Vec<Shape> {
    match roi {
        Roi::Points(pts) => pts
            .iter()
            .map(|p| Shape::circle_filled(t.to_screen(*p), point_radius, stroke.color))
            .chain(
                pts.iter().map(|p| Shape::circle_stroke(t.to_screen(*p), point_radius, Stroke::new(1.0, Color32::from_black_alpha(160)))),
            )
            .collect(),
        Roi::Line { .. } | Roi::Polyline(_) => vec![Shape::line(screen_path(t, &roi.outline(), 0.0), stroke)],
        Roi::Area(mp) => {
            mp.0.iter()
                .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
                .map(|ring| closed(screen_path(t, &ring.0, 0.75), stroke, None))
                .collect()
        }
        _ => vec![closed(screen_path(t, &roi.outline(), 0.75), stroke, fill)],
    }
}

fn closed(mut points: Vec<Pos2>, stroke: Stroke, fill: Option<Color32>) -> Shape {
    // Rings close themselves; a repeated end point would make a zero-length edge.
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    remove_spikes(&mut points, true);
    // A shape shrunk below a pixel can decimate to two points: draw a dot.
    if points.len() < 3 {
        let c = points.iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2()) / points.len().max(1) as f32;
        return Shape::circle_filled(c.to_pos2(), stroke.width.max(1.0), stroke.color);
    }
    let Some(fill) = fill else {
        // egui joins closed loops with unlimited miters (sharp corners become long
        // spikes); open paths cut sharp corners, so outlines are drawn open.
        points.push(points[0]);
        return Shape::line(points, stroke);
    };
    let mut path = PathShape::closed_line(points, PathStroke::new(stroke.width, stroke.color));
    path.fill = fill;
    Shape::Path(path)
}

/// Screen points of a path, skipping vertices closer than `min_dist` to the last kept one.
pub fn screen_path(t: &Transform, pts: &[Pt], min_dist: f32) -> Vec<Pos2> {
    let mut out: Vec<Pos2> = Vec::with_capacity(pts.len());
    for p in pts {
        let s = t.to_screen(*p);
        if out.last().is_none_or(|l| l.distance(s) >= min_dist) {
            out.push(s);
        }
    }
    if out.len() < 2
        && let Some(last) = pts.last()
    {
        out.push(t.to_screen(*last));
    }
    remove_spikes(&mut out, false);
    out
}

/// Removes vertices where a path turns back on itself (A→B→A), including across the
/// start of a closed ring. Anti-parallel edges give egui's tessellator a zero join
/// normal, which turns into NaN or an endless miter.
pub fn remove_spikes(pts: &mut Vec<Pos2>, closed: bool) {
    let reverses = |a: Pos2, b: Pos2, c: Pos2| {
        let (u, v) = ((b - a).normalized(), (c - b).normalized());
        !u.x.is_finite() || !v.x.is_finite() || u.dot(v) < -0.99
    };
    let mut changed = true;
    while changed && pts.len() >= 3 {
        changed = false;
        let n = pts.len();
        let range = if closed { 0..n } else { 1..n - 1 };
        if let Some(i) = range.into_iter().find(|&i| reverses(pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n])) {
            pts.remove(i);
            changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    #[test]
    fn spikes_are_removed_even_across_the_ring_start() {
        // A square whose first vertex is a hair: 0 → 1 → back towards 0.
        let mut ring =
            vec![pos2(5.0, -20.0), pos2(5.0, 0.0), pos2(10.0, 0.0), pos2(10.0, 10.0), pos2(0.0, 10.0), pos2(0.0, 0.0), pos2(5.0, 0.0)];
        super::remove_spikes(&mut ring, true);
        assert!(!ring.contains(&pos2(5.0, -20.0)), "{ring:?}");
        assert!(ring.len() >= 4);
    }
}

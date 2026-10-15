//! Handles for editing a selected annotation: box handles for rectangles and
//! ellipses, vertex handles for polygons and lines (QuPath's ROI editor).

use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind, vec2};

use crate::objects::{Bounds, Pt, Roi};
use crate::viewer::transform::Transform;

/// Which handle is dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// Box handle 0..8, clockwise from the top left.
    Box(u8),
    /// Polygon, polyline or line vertex.
    Vertex(usize),
}

const HIT_RADIUS: f32 = 7.0;

fn box_points(b: &Bounds) -> [Pt; 8] {
    let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
    [
        Pt { x: b.x0, y: b.y0 },
        Pt { x: cx, y: b.y0 },
        Pt { x: b.x1, y: b.y0 },
        Pt { x: b.x1, y: cy },
        Pt { x: b.x1, y: b.y1 },
        Pt { x: cx, y: b.y1 },
        Pt { x: b.x0, y: b.y1 },
        Pt { x: b.x0, y: cy },
    ]
}

/// Handles of an editable ROI.
pub fn handle_points(roi: &Roi) -> Vec<(Handle, Pt)> {
    match roi {
        Roi::Rectangle { .. } | Roi::Ellipse { .. } => {
            box_points(&roi.bounds()).iter().enumerate().map(|(i, p)| (Handle::Box(i as u8), *p)).collect()
        }
        Roi::Polygon(_) | Roi::Polyline(_) | Roi::Line { .. } => {
            let pts = roi.outline();
            // Very detailed shapes (brush strokes) are not edited vertex by vertex.
            if pts.len() > 400 {
                return vec![];
            }
            pts.into_iter().enumerate().map(|(i, p)| (Handle::Vertex(i), p)).collect()
        }
        _ => vec![],
    }
}

/// The handle under a screen position.
pub fn hit_handle(roi: &Roi, t: &Transform, s: Pos2) -> Option<Handle> {
    handle_points(roi).into_iter().find(|(_, p)| t.to_screen(*p).distance(s) <= HIT_RADIUS).map(|(h, _)| h)
}

/// The ROI with a handle moved to `p`.
pub fn apply(roi: &Roi, handle: Handle, p: Pt) -> Roi {
    match (roi, handle) {
        (Roi::Rectangle { .. } | Roi::Ellipse { .. }, Handle::Box(i)) => {
            let mut b = roi.bounds();
            if matches!(i, 0 | 6 | 7) {
                b.x0 = p.x;
            }
            if matches!(i, 2..=4) {
                b.x1 = p.x;
            }
            if matches!(i, 0..=2) {
                b.y0 = p.y;
            }
            if matches!(i, 4..=6) {
                b.y1 = p.y;
            }
            let (a, c) = (Pt { x: b.x0, y: b.y0 }, Pt { x: b.x1, y: b.y1 });
            if matches!(roi, Roi::Rectangle { .. }) { Roi::rect_from_corners(a, c) } else { Roi::ellipse_from_corners(a, c) }
        }
        (Roi::Line { x1, y1, x2, y2 }, Handle::Vertex(i)) => {
            if i == 0 {
                Roi::Line { x1: p.x, y1: p.y, x2: *x2, y2: *y2 }
            } else {
                Roi::Line { x1: *x1, y1: *y1, x2: p.x, y2: p.y }
            }
        }
        (Roi::Polygon(pts), Handle::Vertex(i)) => Roi::Polygon(replace(pts, i, p)),
        (Roi::Polyline(pts), Handle::Vertex(i)) => Roi::Polyline(replace(pts, i, p)),
        (Roi::Points(pts), Handle::Vertex(i)) => Roi::Points(replace(pts, i, p)),
        _ => roi.clone(),
    }
}

fn replace(pts: &[Pt], i: usize, p: Pt) -> Vec<Pt> {
    let mut v = pts.to_vec();
    if let Some(slot) = v.get_mut(i) {
        *slot = p;
    }
    v
}

/// Draws the handles of the selected annotation.
pub fn draw_handles(painter: &Painter, t: &Transform, roi: &Roi) {
    for (_, p) in handle_points(roi) {
        let s = t.to_screen(p);
        let r = Rect::from_center_size(s, vec2(7.0, 7.0));
        painter.rect_filled(r, 1.0, Color32::from_white_alpha(220));
        painter.rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_black_alpha(200)), StrokeKind::Middle);
    }
}

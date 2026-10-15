//! Rectangle, ellipse, line, polygon and polyline tools.

use super::{Gesture, Pointer, Tool, ToolCtx};
use crate::objects::{Pt, Roi};

/// The ROI spanned by a drag (shift makes squares and circles).
pub fn shape_roi(tool: Tool, a: Pt, mut b: Pt, constrain: bool) -> Roi {
    if constrain {
        let s = (b.x - a.x).abs().max((b.y - a.y).abs());
        b = Pt { x: a.x + s * (b.x - a.x).signum(), y: a.y + s * (b.y - a.y).signum() };
    }
    match tool {
        Tool::Ellipse => Roi::ellipse_from_corners(a, b),
        Tool::Line => Roi::Line { x1: a.x, y1: a.y, x2: b.x, y2: b.y },
        _ => Roi::rect_from_corners(a, b),
    }
}

/// Rectangle, ellipse and line: drag out a shape; a click selects.
pub fn shape(tool: Tool, g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    if p.clicked {
        ctx.click_select(p.pos, p.command);
        return;
    }
    if p.drag_started {
        *g = Gesture::Shape { start: p.pos, current: p.pos };
    }
    if let Gesture::Shape { start, current } = g {
        if p.dragging {
            *current = p.pos;
        }
        if p.drag_stopped {
            let roi = shape_roi(tool, *start, p.pos, p.shift);
            let b = roi.bounds();
            let min = 2.0 * ctx.t.downsample;
            *g = Gesture::None;
            if b.width().max(b.height()) >= min {
                ctx.create(roi);
            }
        }
    }
}

/// Polygon and polyline: click to add vertices, double-click (or click the
/// first vertex) to finish; or drag to draw freehand, finishing on release.
pub fn poly(tool: Tool, g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    let screen = ctx.t.to_screen(p.pos);
    if p.double_clicked {
        if let Gesture::Poly { points, .. } = std::mem::take(g) {
            finish(tool, points, ctx);
        }
        return;
    }
    if p.clicked {
        match g {
            Gesture::Poly { points, freehand: false } => {
                let closes = tool == Tool::Polygon
                    && points.len() >= 3
                    && points.first().is_some_and(|f| ctx.t.to_screen(*f).distance(screen) < 7.0);
                if closes {
                    let pts = std::mem::take(points);
                    *g = Gesture::None;
                    finish(tool, pts, ctx);
                } else {
                    points.push(p.pos);
                }
            }
            _ => *g = Gesture::Poly { points: vec![p.pos], freehand: false },
        }
        return;
    }
    if p.drag_started && !matches!(g, Gesture::Poly { .. }) {
        *g = Gesture::Poly { points: vec![p.pos], freehand: true };
    }
    if let Gesture::Poly { points, freehand } = g {
        if p.dragging && points.last().is_none_or(|l| ctx.t.to_screen(*l).distance(screen) > 2.5) {
            points.push(p.pos);
        }
        if p.drag_stopped && *freehand {
            let pts = std::mem::take(points);
            *g = Gesture::None;
            finish(tool, pts, ctx);
        }
    }
}

fn finish(tool: Tool, mut points: Vec<Pt>, ctx: &mut ToolCtx<'_>) {
    points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
    let roi = match (tool, points.len()) {
        (Tool::Polygon, n) if n >= 3 => Roi::Polygon(points),
        (Tool::Polyline, 2) => Roi::Line { x1: points[0].x, y1: points[0].y, x2: points[1].x, y2: points[1].y },
        (Tool::Polyline, n) if n >= 3 => Roi::Polyline(points),
        _ => return,
    };
    ctx.create(roi);
}

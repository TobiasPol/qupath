//! Brush and wand: paint an area by strokes, adding to (or with Alt, removing
//! from) the selected annotation when the stroke starts inside it.

use geo::{BooleanOps, Buffer, LineString, MultiPolygon};

use super::{Gesture, Pointer, Tool, ToolCtx};
use crate::analysis::contours::{flood_fill, mask_to_polygons};
use crate::image::{RegionRequest, read_region};
use crate::objects::{ObjKey, Pt, Roi, ops};

/// One frame of brush or wand input.
pub fn handle(tool: Tool, g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    if p.drag_started || p.clicked {
        *g = start(tool, ctx, p);
    }
    if p.dragging
        && let Gesture::Paint { shape, last, .. } = g
        && ctx.t.to_screen(*last).distance(ctx.t.to_screen(p.pos)) > 2.0
    {
        let add = stroke(tool, ctx, *last, p.pos);
        *shape = shape.union(&add);
        *last = p.pos;
    }
    if (p.drag_stopped || p.clicked)
        && let Gesture::Paint { shape, target, subtract, .. } = std::mem::take(g)
    {
        finish(ctx, shape, target, subtract);
    }
}

fn radius(ctx: &ToolCtx<'_>) -> f64 {
    f64::from(ctx.brush_diameter) / 2.0 * ctx.t.downsample
}

fn start(tool: Tool, ctx: &mut ToolCtx<'_>, p: &Pointer) -> Gesture {
    let r = radius(ctx);
    let target = ctx
        .h
        .primary_selected()
        .filter(|k| ctx.h.get(*k).is_some_and(|o| o.is_annotation() && !o.locked && o.roi.is_area() && (p.alt || near(&o.roi, p.pos, r))));
    let subtract = p.alt && target.is_some();
    Gesture::Paint { shape: stamp(tool, ctx, p.pos), target, subtract, last: p.pos }
}

fn near(roi: &Roi, p: Pt, r: f64) -> bool {
    roi.contains(p) || roi.outline().iter().any(|q| (q.x - p.x).hypot(q.y - p.y) <= r)
}

fn circle(c: Pt, r: f64) -> MultiPolygon<f64> {
    Roi::Ellipse { x: c.x - r, y: c.y - r, w: 2.0 * r, h: 2.0 * r }.to_multipolygon().unwrap_or(MultiPolygon::new(vec![]))
}

fn stamp(tool: Tool, ctx: &ToolCtx<'_>, p: Pt) -> MultiPolygon<f64> {
    match tool {
        Tool::Wand => wand_region(ctx, p),
        _ => circle(p, radius(ctx)),
    }
}

fn stroke(tool: Tool, ctx: &ToolCtx<'_>, a: Pt, b: Pt) -> MultiPolygon<f64> {
    match tool {
        Tool::Wand => wand_region(ctx, b),
        _ => LineString::new(vec![a, b]).buffer(radius(ctx)),
    }
}

fn finish(ctx: &mut ToolCtx<'_>, shape: MultiPolygon<f64>, target: Option<ObjKey>, subtract: bool) {
    let Some(k) = target else {
        if let Some(roi) = Roi::from_multipolygon(shape) {
            ctx.create(roi);
        }
        return;
    };
    let Some(base) = ctx.h.get(k).and_then(|o| o.roi.to_multipolygon()) else { return };
    let merged = if subtract { base.difference(&shape) } else { base.union(&shape) };
    ctx.checkpoint();
    match Roi::from_multipolygon(merged) {
        Some(roi) => {
            ops::replace_roi(ctx.h, k, roi);
        }
        None => ctx.h.remove(k, true),
    }
}

/// The region of similar color around a point, limited to the brush circle.
fn wand_region(ctx: &ToolCtx<'_>, p: Pt) -> MultiPolygon<f64> {
    let r = radius(ctx);
    let ds = ctx.t.downsample.max(1.0);
    let req = RegionRequest { downsample: ds, x: p.x - r, y: p.y - r, w: 2.0 * r, h: 2.0 * r };
    let Ok(tile) = read_region(ctx.server.as_ref(), &req) else { return MultiPolygon::new(vec![]) };
    let (x0, y0) = ((p.x - r).max(0.0), (p.y - r).max(0.0));
    let scale = (2.0 * r).min(f64::from(ctx.server.metadata().width()) - x0) / f64::from(tile.width.max(1));
    let seed = (((p.x - x0) / scale) as u32, ((p.y - y0) / scale) as u32);
    if seed.0 >= tile.width || seed.1 >= tile.height {
        return MultiPolygon::new(vec![]);
    }
    let channels = tile.channels.min(3);
    let mean = |x: u32, y: u32| -> Vec<f32> {
        (0..channels)
            .map(|c| {
                let mut s = 0.0;
                let mut n = 0.0;
                for (dx, dy) in [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (xx, yy) = ((x as i32 + dx).clamp(0, tile.width as i32 - 1), (y as i32 + dy).clamp(0, tile.height as i32 - 1));
                    s += tile.get(xx as u32, yy as u32, c);
                    n += 1.0;
                }
                s / n
            })
            .collect()
    };
    let seed_color = mean(seed.0, seed.1);
    let dist = |x: u32, y: u32| -> f32 { mean(x, y).iter().zip(&seed_color).map(|(a, b)| (a - b) * (a - b)).sum::<f32>().sqrt() };
    let tol = adaptive_tolerance(&tile, &dist);
    let mask = flood_fill(|x, y| dist(x, y) <= tol, tile.width, tile.height, seed, r / scale);
    mask_to_polygons(&mask, (x0, y0), scale, scale * 0.5)
}

/// QuPath's wand adapts to local contrast: a fraction of the spread of color distances.
fn adaptive_tolerance(tile: &crate::image::Tile, dist: &impl Fn(u32, u32) -> f32) -> f32 {
    let mut d: Vec<f32> = Vec::new();
    for y in (0..tile.height).step_by(3) {
        for x in (0..tile.width).step_by(3) {
            d.push(dist(x, y));
        }
    }
    if d.is_empty() {
        return 20.0;
    }
    d.sort_by(f32::total_cmp);
    (d[d.len() / 2] * 0.6).clamp(8.0, 70.0)
}

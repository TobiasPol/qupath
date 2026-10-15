//! Drawing the image: coarse levels first, then sharper ones on top as they arrive.

use std::sync::Arc;

use egui::{Color32, Painter, Rect, pos2};

use super::tiles::{TileKey, TileManager};
use super::transform::Transform;
use crate::image::ImageServer;
use crate::image::display::ImageDisplay;
use crate::objects::{Bounds, Pt};

/// Draws the visible tiles; returns whether any tile is still missing.
pub fn draw_image(
    ctx: &egui::Context,
    painter: &Painter,
    t: &Transform,
    server: &Arc<dyn ImageServer>,
    image_id: u64,
    tiles: &mut TileManager,
    display: &ImageDisplay,
) -> bool {
    let md = server.metadata();
    let target = md.preferred_level(t.downsample);
    let coarsest = md.levels.len() - 1;
    let visible = t.visible_bounds();
    let full = Bounds { x0: 0.0, y0: 0.0, x1: f64::from(md.width()), y1: f64::from(md.height()) };
    if !visible.intersects(&full) {
        return false;
    }
    let mut missing = false;
    for level in (target..=coarsest).rev() {
        // Only the target and the coarsest level are read; others are drawn if cached.
        let fetch = level == target || level == coarsest;
        missing |= draw_level(ctx, painter, t, server, image_id, tiles, display, level, &visible, fetch);
    }
    missing
}

#[allow(clippy::too_many_arguments)]
fn draw_level(
    ctx: &egui::Context,
    painter: &Painter,
    t: &Transform,
    server: &Arc<dyn ImageServer>,
    image_id: u64,
    tiles: &mut TileManager,
    display: &ImageDisplay,
    level: usize,
    visible: &Bounds,
    fetch: bool,
) -> bool {
    let md = server.metadata();
    let l = md.levels[level];
    let (sx, sy) = (f64::from(md.width()) / f64::from(l.width), f64::from(md.height()) / f64::from(l.height));
    let (tw, th) = (f64::from(md.tile_width) * sx, f64::from(md.tile_height) * sy);
    let cols = l.width.div_ceil(md.tile_width);
    let rows = l.height.div_ceil(md.tile_height);
    let c0 = (visible.x0 / tw).floor().max(0.0) as u32;
    let r0 = (visible.y0 / th).floor().max(0.0) as u32;
    let c1 = ((visible.x1 / tw).ceil() as u32).min(cols);
    let r1 = ((visible.y1 / th).ceil() as u32).min(rows);
    let mut missing = false;
    // Center-out order so the middle of the view loads first.
    let mut cells: Vec<(u32, u32)> = (r0..r1).flat_map(|r| (c0..c1).map(move |c| (c, r))).collect();
    let (mc, mr) = (f64::from(c0 + c1) / 2.0, f64::from(r0 + r1) / 2.0);
    cells.sort_by(|a, b| dist(*a, mc, mr).total_cmp(&dist(*b, mc, mr)));
    for (tx, ty) in cells {
        let key = TileKey { image: image_id, level, tx, ty };
        if fetch {
            tiles.request(key, server);
        }
        let Some(tex) = tiles.texture(ctx, key, display) else {
            missing |= fetch;
            continue;
        };
        let Some(raw) = tiles.raw(&key) else { continue };
        let x0 = f64::from(tx) * tw;
        let y0 = f64::from(ty) * th;
        let b = Bounds { x0, y0, x1: x0 + f64::from(raw.width) * sx, y1: y0 + f64::from(raw.height) * sy };
        let r = t.rect_to_screen(&b);
        painter.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    missing
}

fn dist((c, r): (u32, u32), mc: f64, mr: f64) -> f64 {
    (f64::from(c) + 0.5 - mc).hypot(f64::from(r) + 0.5 - mr)
}

/// The full-resolution image point of a screen position, if it lies in the image.
pub fn image_point(t: &Transform, s: egui::Pos2, width: u32, height: u32) -> Option<Pt> {
    let p = t.to_image(s);
    (p.x >= 0.0 && p.y >= 0.0 && p.x < f64::from(width) && p.y < f64::from(height)).then_some(p)
}

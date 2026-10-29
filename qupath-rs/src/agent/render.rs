//! Renders a region with its objects drawn on top, so agents can look at the slide.

use anyhow::Result;
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_hollow_circle_mut, draw_line_segment_mut};

use super::session::OpenImage;
use crate::image::display::ImageDisplay;
use crate::image::{RegionRequest, read_region};
use crate::objects::{ObjectKind, PathObject, Pt, Roi};

/// Detections drawn at most; beyond this only annotations are drawn.
const MAX_DETECTIONS: usize = 30_000;

/// A rendered region.
pub struct View {
    /// PNG bytes.
    pub png: Vec<u8>,
    /// Output size.
    pub width: u32,
    /// Output height.
    pub height: u32,
    /// Objects drawn.
    pub drawn: usize,
    /// Detections were left out because there were too many.
    pub detections_skipped: bool,
}

/// The region `[x, y, w, h]` clamped to the image, with the downsample that fits `max_size`.
pub fn request(img: &OpenImage, region: Option<[f64; 4]>, downsample: Option<f64>, max_size: u32) -> RegionRequest {
    let md = img.server.metadata();
    let (iw, ih) = (f64::from(md.width()), f64::from(md.height()));
    let [x, y, w, h] = region.unwrap_or([0.0, 0.0, iw, ih]);
    let (x0, y0) = (x.clamp(0.0, iw), y.clamp(0.0, ih));
    let (w, h) = ((x + w).clamp(0.0, iw) - x0, (y + h).clamp(0.0, ih) - y0);
    let fit = w.max(h) / f64::from(max_size.max(16));
    let ds = downsample.unwrap_or(1.0).max(fit).max(1.0);
    RegionRequest { downsample: ds, x: x0, y: y0, w, h }
}

/// Reads and renders a region as the viewer shows it, optionally with objects.
pub fn render(img: &OpenImage, req: &RegionRequest, overlay: bool) -> Result<View> {
    let md = img.server.metadata();
    let tile = read_region(img.server.as_ref(), req)?;
    let mut display = ImageDisplay::new(md, img.settings.image_type, img.settings.stains.clone());
    if !md.rgb {
        for c in 0..tile.channels {
            display.auto_range(&tile, c, 0.1);
        }
    }
    let mut canvas = RgbaImage::from_raw(tile.width, tile.height, display.to_rgba(&tile)).unwrap_or_else(|| RgbaImage::new(1, 1));
    let (mut drawn, mut skipped) = (0, false);
    if overlay {
        (drawn, skipped) = draw_objects(img, req, &mut canvas);
    }
    let mut png = Vec::new();
    canvas.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    Ok(View { png, width: canvas.width(), height: canvas.height(), drawn, detections_skipped: skipped })
}

fn draw_objects(img: &OpenImage, req: &RegionRequest, canvas: &mut RgbaImage) -> (usize, bool) {
    let view = crate::objects::Bounds { x0: req.x, y0: req.y, x1: req.x + req.w, y1: req.y + req.h };
    let visible: Vec<&PathObject> =
        img.hierarchy.iter().map(|(_, o)| o).filter(|o| o.kind != ObjectKind::Root && view.intersects(&o.roi.bounds())).collect();
    let n_det = visible.iter().filter(|o| o.is_detection()).count();
    let skip = n_det > MAX_DETECTIONS;
    let mut drawn = 0;
    // Detections first, so annotation outlines stay on top.
    for o in visible.iter().filter(|o| o.is_detection() && !skip) {
        let c = rgba(o);
        stroke(canvas, req, &o.roi, c, 1);
        if let Some(n) = &o.nucleus {
            stroke(canvas, req, n, c, 1);
        }
        drawn += 1;
    }
    for o in visible.iter().filter(|o| !o.is_detection()) {
        stroke(canvas, req, &o.roi, rgba(o), 2);
        drawn += 1;
    }
    (drawn, skip)
}

fn rgba(o: &PathObject) -> Rgba<u8> {
    let c = o.display_color();
    Rgba([c.0, c.1, c.2, 255])
}

/// The rings of a ROI, each with whether it is closed.
fn rings(roi: &Roi) -> Vec<(Vec<Pt>, bool)> {
    match roi {
        Roi::Area(mp) => {
            mp.0.iter().flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors())).map(|r| (r.0.clone(), true)).collect()
        }
        Roi::Line { .. } | Roi::Polyline(_) => vec![(roi.outline(), false)],
        Roi::Points(_) => vec![],
        _ => vec![(roi.outline(), true)],
    }
}

fn stroke(canvas: &mut RgbaImage, req: &RegionRequest, roi: &Roi, c: Rgba<u8>, width: i32) {
    let to = |p: &Pt| (((p.x - req.x) / req.downsample) as f32, ((p.y - req.y) / req.downsample) as f32);
    if let Roi::Points(pts) = roi {
        for p in pts {
            let (x, y) = to(p);
            draw_hollow_circle_mut(canvas, (x as i32, y as i32), 4, c);
        }
        return;
    }
    for (pts, closed) in rings(roi) {
        let segs = pts.len().saturating_sub(usize::from(!closed));
        for i in 0..segs {
            let (a, b) = (to(&pts[i]), to(&pts[(i + 1) % pts.len()]));
            for d in 0..width {
                let o = d as f32;
                draw_line_segment_mut(canvas, (a.0 + o, a.1), (b.0 + o, b.1), c);
                draw_line_segment_mut(canvas, (a.0, a.1 + o), (b.0, b.1 + o), c);
            }
        }
    }
}

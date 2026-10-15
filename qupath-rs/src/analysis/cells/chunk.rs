//! Detection in one chunk: channels, background, smoothing, seeds, watershed, expansion.

use anyhow::Result;
use image::GrayImage;

use super::measure::{Channels, measure_cell};
use super::{CellParams, DetectionImage, Source};
use crate::analysis::contours::mask_to_polygons;
use crate::analysis::morphology::{FImage, connected_components, expand_labels, find_seeds, gaussian, opening, watershed};
use crate::image::{RegionRequest, Tile, read_region};
use crate::objects::{PathObject, Pt, Roi};

/// Pixels of each label, and their bounding boxes.
pub struct Labels {
    pub pixels: Vec<Vec<usize>>,
    pub bbox: Vec<[usize; 4]>,
}

fn group(labels: &[u32], w: usize, n: usize) -> Labels {
    let mut pixels = vec![Vec::new(); n + 1];
    let mut bbox = vec![[usize::MAX, usize::MAX, 0, 0]; n + 1];
    for (i, l) in labels.iter().enumerate() {
        let l = *l as usize;
        if l == 0 || l > n {
            continue;
        }
        pixels[l].push(i);
        let (x, y) = (i % w, i / w);
        let b = &mut bbox[l];
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
    Labels { pixels, bbox }
}

/// The detection image and the stain images used for measurements.
fn channels(src: &Source, t: &Tile, p: &CellParams) -> (FImage, Channels) {
    let (w, h) = (t.width as usize, t.height as usize);
    let mut det = FImage::new(w, h);
    let mut ch = Channels::new(w, h, src);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let vals = ch.fill(src, t, x as u32, y as u32, i);
            det.data[i] = match (p.detection_image, &src.stains) {
                (DetectionImage::Hematoxylin, Some(_)) => vals[0],
                (DetectionImage::OpticalDensitySum, Some(s)) => {
                    s.od_sum([t.get(x as u32, y as u32, 0), t.get(x as u32, y as u32, 1), t.get(x as u32, y as u32, 2)]) as f32
                }
                (DetectionImage::Channel(c), _) => t.get(x as u32, y as u32, c.min(t.channels - 1)),
                _ => vals[0],
            };
        }
    }
    (det, ch)
}

/// Detects cells in `core` (full-resolution bounds), reading `pad` beyond it.
pub fn process(src: &Source, roi: &Roi, p: &CellParams, core: [f64; 4], pad: f64, ds: f64) -> Result<Vec<PathObject>> {
    let md = src.server.metadata();
    let rx0 = (core[0] - pad).max(0.0);
    let ry0 = (core[1] - pad).max(0.0);
    let rx1 = (core[2] + pad).min(f64::from(md.width()));
    let ry1 = (core[3] + pad).min(f64::from(md.height()));
    let t = read_region(src.server.as_ref(), &RegionRequest { downsample: ds, x: rx0, y: ry0, w: rx1 - rx0, h: ry1 - ry0 })?;
    let (w, h) = (t.width as usize, t.height as usize);
    let scale = (rx1 - rx0) / w as f64;
    let out_px = src.cal.average() * scale;
    let (det, ch) = channels(src, &t, p);
    let (smooth, mask) = nucleus_mask(&det, p, out_px);
    let labels = if p.watershed {
        let min_dist = ((p.min_area / std::f64::consts::PI).sqrt() / out_px).max(2.0) as f32;
        watershed(&smooth, &mask, &find_seeds(&smooth, &mask, min_dist))
    } else {
        connected_components(&mask, w, h)
    };
    let n = labels.iter().copied().max().unwrap_or(0) as usize;
    let nuclei = group(&labels, w, n);
    let area_px = out_px * out_px;
    let keep: Vec<bool> =
        (0..=n).map(|l| l > 0 && (p.min_area..=p.max_area).contains(&(nuclei.pixels[l].len() as f64 * area_px))).collect();
    let kept: Vec<u32> = labels.iter().map(|l| if keep[*l as usize] { *l } else { 0 }).collect();
    let cell_labels = expand_labels(&kept, w, h, (p.cell_expansion / out_px) as f32);
    let cells = group(&cell_labels, w, n);
    let to_full = |x: f64, y: f64| Pt { x: rx0 + (x + 0.5) * scale, y: ry0 + (y + 0.5) * scale };
    let mut out = Vec::new();
    for l in (1..=n).filter(|l| keep[*l]) {
        let px = &nuclei.pixels[l];
        let (sx, sy) = px.iter().fold((0.0, 0.0), |(a, b), i| (a + (i % w) as f64, b + (i / w) as f64));
        let c = to_full(sx / px.len() as f64, sy / px.len() as f64);
        let in_core = c.x >= core[0] && c.x < core[2] && c.y >= core[1] && c.y < core[3];
        if !in_core || !roi.contains(c) {
            continue;
        }
        let eps = if p.smooth_boundaries { scale * 0.75 } else { scale * 0.25 };
        let Some(nucleus) = trace(&labels, w, l as u32, nuclei.bbox[l], (rx0, ry0), scale, eps) else { continue };
        let cell = trace(&cell_labels, w, l as u32, cells.bbox[l], (rx0, ry0), scale, eps).unwrap_or_else(|| nucleus.clone());
        let mut obj = PathObject::cell(cell, Some(nucleus), None);
        if p.make_measurements {
            measure_cell(&mut obj, &ch, &nuclei.pixels[l], &cells.pixels[l], &src.cal);
        }
        out.push(obj);
    }
    Ok(out)
}

/// Background-subtracted, smoothed detection image and the pixels above threshold.
fn nucleus_mask(det: &FImage, p: &CellParams, out_px: f64) -> (FImage, Vec<bool>) {
    let mut sub = det.clone();
    let mut allowed = vec![true; det.data.len()];
    let r = (p.background_radius / out_px).round() as usize;
    if r > 0 {
        let bg = opening(det, r);
        for (i, v) in sub.data.iter_mut().enumerate() {
            *v -= bg.data[i];
            allowed[i] = f64::from(bg.data[i]) <= p.max_background;
        }
    }
    let smooth = gaussian(&sub, (p.sigma / out_px) as f32);
    let mask = smooth.data.iter().zip(&allowed).map(|(v, a)| *a && f64::from(*v) > p.threshold).collect();
    (smooth, mask)
}

/// The outline of one label, in full-resolution coordinates.
fn trace(labels: &[u32], w: usize, l: u32, bb: [usize; 4], origin: (f64, f64), scale: f64, eps: f64) -> Option<Roi> {
    let (bw, bh) = (bb[2] - bb[0] + 1, bb[3] - bb[1] + 1);
    let mut m = GrayImage::new(bw as u32, bh as u32);
    for y in 0..bh {
        for x in 0..bw {
            if labels[(bb[1] + y) * w + bb[0] + x] == l {
                m.put_pixel(x as u32, y as u32, image::Luma([255]));
            }
        }
    }
    let o = (origin.0 + bb[0] as f64 * scale, origin.1 + bb[1] as f64 * scale);
    let mp = mask_to_polygons(&m, o, scale, eps);
    // Keep the largest part (watershed lines can leave crumbs) and drop holes.
    let biggest = mp.0.into_iter().max_by(|a, b| geo::Area::unsigned_area(a).total_cmp(&geo::Area::unsigned_area(b)))?;
    let mut pts = biggest.exterior().0.clone();
    pts.pop();
    (pts.len() >= 3).then_some(Roi::Polygon(pts))
}

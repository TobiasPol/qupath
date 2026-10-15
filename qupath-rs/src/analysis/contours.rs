//! Binary masks to polygons (with holes), in image coordinates.

use geo::{Coord, LineString, MultiPolygon, Polygon, Simplify};
use image::GrayImage;
use imageproc::contours::{BorderType, find_contours};

/// Traces a mask (non-zero = inside) into polygons, mapping pixel `(i, j)` to
/// `origin + (i + 0.5, j + 0.5) * downsample`. Rings are simplified by `epsilon` image pixels.
pub fn mask_to_polygons(mask: &GrayImage, origin: (f64, f64), downsample: f64, epsilon: f64) -> MultiPolygon<f64> {
    // Pad by one pixel so regions touching the border close properly.
    let (w, h) = mask.dimensions();
    let mut padded = GrayImage::new(w + 2, h + 2);
    for (x, y, p) in mask.enumerate_pixels() {
        if p.0[0] > 0 {
            padded.put_pixel(x + 1, y + 1, image::Luma([255]));
        }
    }
    let contours = find_contours::<i32>(&padded);
    let to_ring = |pts: &[imageproc::point::Point<i32>]| -> LineString<f64> {
        let coords: Vec<Coord<f64>> = pts
            .iter()
            .map(|p| Coord { x: origin.0 + (f64::from(p.x - 1) + 0.5) * downsample, y: origin.1 + (f64::from(p.y - 1) + 0.5) * downsample })
            .collect();
        let mut ls = LineString::new(despike(coords));
        ls.close();
        if epsilon > 0.0 { ls.simplify(epsilon) } else { ls }
    };
    let mut polys: Vec<(usize, Polygon<f64>)> = Vec::new();
    for (i, c) in contours.iter().enumerate() {
        if c.border_type == BorderType::Outer && c.points.len() >= 3 {
            polys.push((i, Polygon::new(to_ring(&c.points), vec![])));
        }
    }
    for c in contours.iter().filter(|c| c.border_type == BorderType::Hole && c.points.len() >= 3) {
        if let Some(parent) = c.parent
            && let Some((_, p)) = polys.iter_mut().find(|(i, _)| *i == parent)
        {
            p.interiors_push(to_ring(&c.points));
        }
    }
    let single_pixel = |p: &Polygon<f64>| p.exterior().0.len() < 4;
    MultiPolygon::new(polys.into_iter().map(|(_, p)| p).filter(|p| !single_pixel(p)).collect())
}

/// Removes out-and-back vertices (one-pixel-wide hairs traced A→B→A) and repeats.
fn despike(mut pts: Vec<Coord<f64>>) -> Vec<Coord<f64>> {
    let mut changed = true;
    while changed && pts.len() > 3 {
        changed = false;
        let n = pts.len();
        let keep: Vec<bool> = (0..n)
            .map(|i| {
                let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
                let (ux, uy, vx, vy) = (b.x - a.x, b.y - a.y, c.x - b.x, c.y - b.y);
                let (lu, lv) = (ux.hypot(uy), vx.hypot(vy));
                lu > 0.0 && lv > 0.0 && (ux * vx + uy * vy) / (lu * lv) > -0.999
            })
            .collect();
        if keep.iter().any(|k| !k) {
            pts = pts.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| p).collect();
            changed = true;
        }
    }
    pts
}

/// Fills a disk-limited region of pixels similar to the seed (4-connected flood fill).
pub fn flood_fill(similar: impl Fn(u32, u32) -> bool, w: u32, h: u32, seed: (u32, u32), radius: f64) -> GrayImage {
    let mut mask = GrayImage::new(w, h);
    if seed.0 >= w || seed.1 >= h || !similar(seed.0, seed.1) {
        return mask;
    }
    let r2 = radius * radius;
    let inside = |x: u32, y: u32| {
        let (dx, dy) = (f64::from(x) - f64::from(seed.0), f64::from(y) - f64::from(seed.1));
        dx * dx + dy * dy <= r2
    };
    let mut stack = vec![seed];
    mask.put_pixel(seed.0, seed.1, image::Luma([255]));
    while let Some((x, y)) = stack.pop() {
        let neighbors = [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)];
        for (nx, ny) in neighbors {
            if nx < w && ny < h && mask.get_pixel(nx, ny).0[0] == 0 && inside(nx, ny) && similar(nx, ny) {
                mask.put_pixel(nx, ny, image::Luma([255]));
                stack.push((nx, ny));
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use geo::Area;

    #[test]
    fn square_with_hole() {
        let mut m = image::GrayImage::new(20, 20);
        for y in 2..18 {
            for x in 2..18 {
                if !(8..12).contains(&x) || !(8..12).contains(&y) {
                    m.put_pixel(x, y, image::Luma([1]));
                }
            }
        }
        let mp = super::mask_to_polygons(&m, (0.0, 0.0), 1.0, 0.0);
        assert_eq!(mp.0.len(), 1);
        assert_eq!(mp.0[0].interiors().len(), 1);
        // Contours run through boundary pixel centers: 15×15 outside; the hole's ring is
        // traced 8-connected, so its four corners are cut diagonally (25 − 4 × 0.5 = 23).
        assert!((mp.unsigned_area() - (225.0 - 23.0)).abs() < 1e-9, "{}", mp.unsigned_area());
    }
}

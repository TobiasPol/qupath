//! A procedural H-DAB "slide" for tests and demos: tissue islands, stroma,
//! hematoxylin nuclei and a fraction of DAB-positive nuclei, with known truth.

use anyhow::Result;

use crate::image::server::{Tile, TileData};
use crate::image::stains::{DAB, HEMATOXYLIN};
use crate::image::tiff::writer::PixelSource;

/// A synthetic slide of any size; nuclei sit on a jittered grid.
pub struct SyntheticSlide {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Grid spacing between nuclei, in pixels.
    pub cell_spacing: f64,
    /// Nucleus radius, in pixels.
    pub nucleus_radius: f64,
    /// Fraction of nuclei that are DAB positive.
    pub positive_fraction: f64,
}

impl SyntheticSlide {
    /// Proportions of a 40× scan (0.25 µm/px).
    pub fn new(width: u32, height: u32) -> Self {
        SyntheticSlide { width, height, cell_spacing: 44.0, nucleus_radius: 13.0, positive_fraction: 0.3 }
    }

    /// The color at a full-resolution position: stain amounts mixed by Beer–Lambert,
    /// so color deconvolution recovers them (positive nuclei keep hematoxylin counterstain).
    pub fn sample(&self, x: f64, y: f64) -> [f32; 3] {
        let tissue = f64::from(self.tissue(x, y).clamp(0.0, 0.25) * 4.0);
        let t = f64::from(value_noise(x / 9.0, y / 9.0, 7)) * 0.5 + f64::from(value_noise(x / 40.0, y / 40.0, 3)) * 0.5;
        // Background stroma: faint hematoxylin with a little DAB.
        let (mut h, mut dab) = (tissue * (0.06 + 0.08 * t), tissue * (0.02 + 0.03 * t));
        if let Some((d, positive)) = self.nearest_nucleus(x, y)
            && tissue > 0.6
        {
            let edge = ((self.nucleus_radius - d) / 2.0).clamp(0.0, 1.0);
            let grain = f64::from(value_noise(x / 3.0, y / 3.0, 11)) * 0.15;
            let (nh, nd) = if positive { (0.30 + grain, 0.55 + grain) } else { (0.55 + grain, 0.02) };
            h += (nh - h) * edge;
            dab += (nd - dab) * edge;
        }
        let od = |c: usize| h * HEMATOXYLIN[c] + dab * DAB[c];
        [0, 1, 2].map(|c| (245.0 * 10f64.powf(-od(c))) as f32)
    }

    /// > 0 inside tissue; smooth islands from low-frequency noise.
    fn tissue(&self, x: f64, y: f64) -> f32 {
        let (w, h) = (f64::from(self.width), f64::from(self.height));
        let border = ((x.min(w - x).min(y).min(h - y)) / (w.min(h) * 0.08)).clamp(0.0, 1.0) as f32;
        let n = value_noise(x / 1500.0, y / 1500.0, 1) * 0.7 + value_noise(x / 400.0, y / 400.0, 2) * 0.3;
        (n - 0.42) * border
    }

    fn nearest_nucleus(&self, x: f64, y: f64) -> Option<(f64, bool)> {
        let s = self.cell_spacing;
        let (gx, gy) = ((x / s).floor() as i64, (y / s).floor() as i64);
        let mut best: Option<(f64, bool)> = None;
        for j in gy - 1..=gy + 1 {
            for i in gx - 1..=gx + 1 {
                let h = hash(i, j, 99);
                if h % 100 < 12 {
                    continue; // some grid cells have no nucleus
                }
                let jx = (f64::from((h >> 8) % 1000) / 1000.0 - 0.5) * s * 0.5;
                let jy = (f64::from((h >> 18) % 1000) / 1000.0 - 0.5) * s * 0.5;
                let (cx, cy) = ((i as f64 + 0.5) * s + jx, (j as f64 + 0.5) * s + jy);
                // Slightly elliptical nuclei.
                let d = ((x - cx) * 1.1).hypot((y - cy) * 0.92);
                let positive = f64::from((h >> 4) % 1000) / 1000.0 < self.positive_fraction;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, positive));
                }
            }
        }
        best.filter(|(d, _)| *d < self.nucleus_radius + 2.0)
    }
}

impl PixelSource for SyntheticSlide {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    fn read(&self, ds: f64, x: u32, y: u32, w: u32, h: u32) -> Result<Tile> {
        // Supersample when downsampling so low resolutions look like real scans.
        let n = (ds.ceil() as u32).clamp(1, 3);
        let mut out = Vec::with_capacity((w * h * 3) as usize);
        for oy in 0..h {
            for ox in 0..w {
                let mut acc = [0f32; 3];
                for sy in 0..n {
                    for sx in 0..n {
                        let fx = f64::from(x) + (f64::from(ox) + (f64::from(sx) + 0.5) / f64::from(n)) * ds;
                        let fy = f64::from(y) + (f64::from(oy) + (f64::from(sy) + 0.5) / f64::from(n)) * ds;
                        let p = self.sample(fx, fy);
                        acc = [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]];
                    }
                }
                let k = (n * n) as f32;
                out.extend(acc.map(|v| (v / k).clamp(0.0, 255.0) as u8));
            }
        }
        Ok(Tile { width: w, height: h, channels: 3, data: TileData::U8(out) })
    }
}

fn hash(x: i64, y: i64, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^ (h >> 15)
}

/// Smooth value noise in 0..1.
fn value_noise(x: f64, y: f64, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let v = |i: i64, j: i64| (hash(i, j, seed) % 10_000) as f32 / 10_000.0;
    let (i, j) = (x0 as i64, y0 as i64);
    let a = v(i, j) + (v(i + 1, j) - v(i, j)) * sx;
    let b = v(i, j + 1) + (v(i + 1, j + 1) - v(i, j + 1)) * sx;
    a + (b - a) * sy
}

//! Reading regions at any downsample (QuPath's `RegionRequest`).

use anyhow::{Result, bail};

use super::server::{ImageServer, Tile, TileData};

/// A region in full-resolution coordinates, to be read at a downsample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegionRequest {
    /// Full-resolution pixels per output pixel.
    pub downsample: f64,
    /// Left, in full-resolution pixels.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

impl RegionRequest {
    /// Output width in pixels.
    pub fn out_width(&self) -> u32 {
        (self.w / self.downsample).round().max(1.0) as u32
    }
    /// Output height in pixels.
    pub fn out_height(&self) -> u32 {
        (self.h / self.downsample).round().max(1.0) as u32
    }
}

/// Reads a region, choosing the best pyramid level and resampling (area average when shrinking).
pub fn read_region(server: &dyn ImageServer, req: &RegionRequest) -> Result<Tile> {
    let md = server.metadata();
    let (iw, ih) = (f64::from(md.width()), f64::from(md.height()));
    let x0 = req.x.clamp(0.0, iw);
    let y0 = req.y.clamp(0.0, ih);
    let x1 = (req.x + req.w).clamp(0.0, iw);
    let y1 = (req.y + req.h).clamp(0.0, ih);
    if x1 <= x0 || y1 <= y0 {
        bail!("region outside the image");
    }
    let level = md.preferred_level(req.downsample);
    let l = md.levels[level];
    let (sx, sy) = (iw / f64::from(l.width), ih / f64::from(l.height));
    let lx = (x0 / sx).floor() as u32;
    let ly = (y0 / sy).floor() as u32;
    let lw = ((x1 / sx).ceil() as u32).min(l.width).saturating_sub(lx).max(1);
    let lh = ((y1 / sy).ceil() as u32).min(l.height).saturating_sub(ly).max(1);
    let src = server.read_level_region(level, lx, ly, lw, lh)?;
    let ow = ((x1 - x0) / req.downsample).round().max(1.0) as u32;
    let oh = ((y1 - y0) / req.downsample).round().max(1.0) as u32;
    if ow == lw && oh == lh {
        return Ok(src);
    }
    Ok(resample(&src, ow, oh))
}

/// Resizes by averaging source pixels that fall in each output pixel.
pub fn resample(src: &Tile, ow: u32, oh: u32) -> Tile {
    let c = src.channels;
    let fx = f64::from(src.width) / f64::from(ow);
    let fy = f64::from(src.height) / f64::from(oh);
    let mut out = vec![0f32; (ow * oh) as usize * c];
    for oy in 0..oh {
        let (y0, y1) = span(oy, fy, src.height);
        for ox in 0..ow {
            let (x0, x1) = span(ox, fx, src.width);
            let n = ((y1 - y0) * (x1 - x0)) as f32;
            for ch in 0..c {
                let mut s = 0.0;
                for y in y0..y1 {
                    for x in x0..x1 {
                        s += src.get(x, y, ch);
                    }
                }
                out[((oy * ow + ox) as usize) * c + ch] = s / n;
            }
        }
    }
    let data = match src.data {
        TileData::U8(_) => TileData::U8(out.iter().map(|v| v.round() as u8).collect()),
        TileData::U16(_) => TileData::U16(out.iter().map(|v| v.round() as u16).collect()),
        TileData::F32(_) => TileData::F32(out),
    };
    Tile { width: ow, height: oh, channels: c, data }
}

fn span(o: u32, f: f64, max: u32) -> (u32, u32) {
    let a = ((f64::from(o) * f).floor() as u32).min(max - 1);
    let b = ((f64::from(o + 1) * f).ceil() as u32).clamp(a + 1, max);
    (a, b)
}

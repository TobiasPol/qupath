//! How pixels become colors on screen (QuPath's `ImageDisplay` and Brightness/Contrast).

use super::server::{ImageMetadata, Tile};
use super::stains::{ImageType, Stains};
use crate::objects::Rgb;

/// What a display channel shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChannelKind {
    /// One channel of the image.
    Raw(usize),
    /// One deconvolved stain of an RGB brightfield image.
    Stain(usize),
    /// Sum of RGB optical densities.
    OdSum,
}

/// A channel with its display range.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayChannel {
    /// Name.
    pub name: String,
    /// What it shows.
    pub kind: ChannelKind,
    /// Color in composites.
    pub color: Rgb,
    /// Value shown black.
    pub min: f32,
    /// Value shown at full intensity.
    pub max: f32,
    /// Included in the composite.
    pub visible: bool,
}

/// What the viewer draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    /// RGB images as they are (with the RGB ranges), others as a composite of visible channels.
    Original,
    /// One channel alone.
    Single(usize),
}

/// Display settings for an image.
#[derive(Clone, Debug)]
pub struct ImageDisplay {
    /// Channels that can be shown.
    pub channels: Vec<DisplayChannel>,
    /// What is drawn.
    pub selection: Selection,
    /// Gamma applied after the range.
    pub gamma: f32,
    /// Draw a single channel in gray rather than its color.
    pub grayscale: bool,
    /// Stains used for deconvolved channels.
    pub stains: Option<Stains>,
    /// Bumped on every change, so tile textures know to refresh.
    pub version: u64,
    rgb: bool,
}

impl ImageDisplay {
    /// Defaults for an image of a type.
    pub fn new(md: &ImageMetadata, image_type: ImageType, stains: Option<Stains>) -> Self {
        let max = md.pixel_type.max_value();
        let mut channels: Vec<DisplayChannel> = md
            .channels
            .iter()
            .enumerate()
            .map(|(i, c)| DisplayChannel { name: c.name.clone(), kind: ChannelKind::Raw(i), color: c.color, min: 0.0, max, visible: true })
            .collect();
        if md.rgb && image_type.is_brightfield() {
            if let Some(s) = &stains {
                for (i, sv) in s.stains.iter().enumerate() {
                    let c = stain_color(sv.od);
                    channels.push(DisplayChannel {
                        name: sv.name.clone(),
                        kind: ChannelKind::Stain(i),
                        color: c,
                        min: 0.0,
                        max: 1.5,
                        visible: false,
                    });
                }
            }
            channels.push(DisplayChannel {
                name: "Optical density sum".into(),
                kind: ChannelKind::OdSum,
                color: Rgb(255, 255, 255),
                min: 0.0,
                max: 2.5,
                visible: false,
            });
        }
        ImageDisplay { channels, selection: Selection::Original, gamma: 1.0, grayscale: false, stains, version: 1, rgb: md.rgb }
    }

    /// Records a change.
    pub fn changed(&mut self) {
        self.version += 1;
    }

    /// The value a channel shows at one pixel.
    #[inline]
    pub fn value(&self, kind: ChannelKind, t: &Tile, x: u32, y: u32) -> f32 {
        match kind {
            ChannelKind::Raw(c) => t.get(x, y, c.min(t.channels - 1)),
            ChannelKind::Stain(s) => {
                let rgb = [t.get(x, y, 0), t.get(x, y, 1), t.get(x, y, 2)];
                self.stains.as_ref().map_or(0.0, |st| st.deconvolve(rgb)[s] as f32)
            }
            ChannelKind::OdSum => {
                let rgb = [t.get(x, y, 0), t.get(x, y, 1), t.get(x, y, 2)];
                self.stains.as_ref().map_or(0.0, |st| st.od_sum(rgb) as f32)
            }
        }
    }

    fn scale(&self, c: &DisplayChannel, v: f32) -> f32 {
        let s = ((v - c.min) / (c.max - c.min).max(1e-6)).clamp(0.0, 1.0);
        if (self.gamma - 1.0).abs() < 1e-3 { s } else { s.powf(self.gamma) }
    }

    /// RGBA bytes for a tile.
    pub fn to_rgba(&self, t: &Tile) -> Vec<u8> {
        let n = (t.width * t.height) as usize;
        let mut out = vec![255u8; n * 4];
        match self.selection {
            Selection::Original if self.rgb => self.rgb_original(t, &mut out),
            Selection::Original => self.composite(t, &mut out),
            Selection::Single(i) => self.single(t, i, &mut out),
        }
        out
    }

    fn rgb_original(&self, t: &Tile, out: &mut [u8]) {
        // Lookup tables: 8-bit RGB is the common case and must be fast.
        let luts: Vec<[u8; 256]> = (0..3)
            .map(|c| {
                let ch = &self.channels[c];
                std::array::from_fn(|v| if ch.visible { (self.scale(ch, v as f32) * 255.0) as u8 } else { 0 })
            })
            .collect();
        if let super::server::TileData::U8(px) = &t.data {
            for (o, p) in out.as_chunks_mut::<4>().0.iter_mut().zip(px.as_chunks::<3>().0) {
                o[0] = luts[0][p[0] as usize];
                o[1] = luts[1][p[1] as usize];
                o[2] = luts[2][p[2] as usize];
            }
        }
    }

    fn composite(&self, t: &Tile, out: &mut [u8]) {
        let visible: Vec<&DisplayChannel> = self.channels.iter().filter(|c| c.visible).collect();
        for y in 0..t.height {
            for x in 0..t.width {
                let mut acc = [0f32; 3];
                for c in &visible {
                    let s = self.scale(c, self.value(c.kind, t, x, y));
                    acc[0] += s * f32::from(c.color.0);
                    acc[1] += s * f32::from(c.color.1);
                    acc[2] += s * f32::from(c.color.2);
                }
                let i = ((y * t.width + x) * 4) as usize;
                out[i..i + 3].copy_from_slice(&acc.map(|v| v.min(255.0) as u8));
            }
        }
    }

    fn single(&self, t: &Tile, i: usize, out: &mut [u8]) {
        let Some(c) = self.channels.get(i) else { return };
        // Brightfield stains look natural inverted: white background, colored stain.
        let invert = matches!(c.kind, ChannelKind::Stain(_) | ChannelKind::OdSum) && !self.grayscale;
        for y in 0..t.height {
            for x in 0..t.width {
                let s = self.scale(c, self.value(c.kind, t, x, y));
                let px = if self.grayscale {
                    let g = (s * 255.0) as u8;
                    [g, g, g]
                } else if invert {
                    [c.color.0, c.color.1, c.color.2].map(|k| (255.0 - s * (255.0 - f32::from(k))) as u8)
                } else {
                    [c.color.0, c.color.1, c.color.2].map(|k| (s * f32::from(k)) as u8)
                };
                let o = ((y * t.width + x) * 4) as usize;
                out[o..o + 3].copy_from_slice(&px);
            }
        }
    }

    /// Histogram of a channel over a tile: counts and the range they span.
    pub fn histogram(&self, t: &Tile, channel: usize, bins: usize) -> (Vec<u32>, f32, f32) {
        let Some(c) = self.channels.get(channel) else { return (vec![], 0.0, 1.0) };
        let step = ((t.width * t.height) as usize / 200_000).max(1) as u32;
        let mut vals = Vec::new();
        for y in (0..t.height).step_by(step as usize) {
            for x in 0..t.width {
                vals.push(self.value(c.kind, t, x, y));
            }
        }
        let lo = vals.iter().copied().fold(f32::MAX, f32::min).min(0.0);
        let hi = vals.iter().copied().fold(f32::MIN, f32::max).max(c.max);
        let mut h = vec![0u32; bins];
        for v in vals {
            let b = (((v - lo) / (hi - lo).max(1e-6)) * (bins - 1) as f32) as usize;
            h[b.min(bins - 1)] += 1;
        }
        (h, lo, hi)
    }

    /// Sets a channel's range to cover all but `saturation` % at each end.
    pub fn auto_range(&mut self, t: &Tile, channel: usize, saturation: f32) {
        let Some(c) = self.channels.get(channel) else { return };
        let mut vals: Vec<f32> = Vec::with_capacity((t.width * t.height) as usize);
        for y in 0..t.height {
            for x in 0..t.width {
                vals.push(self.value(c.kind, t, x, y));
            }
        }
        if vals.is_empty() {
            return;
        }
        vals.sort_by(f32::total_cmp);
        let k = ((vals.len() as f32 * saturation / 100.0) as usize).min(vals.len() - 1);
        let (lo, hi) = (vals[k], vals[vals.len() - 1 - k]);
        let c = &mut self.channels[channel];
        c.min = lo;
        c.max = if hi > lo { hi } else { lo + 1.0 };
        self.changed();
    }
}

/// The color a stain looks like (QuPath shows stains in their own color).
pub fn stain_color(od: [f64; 3]) -> Rgb {
    let c = |o: f64| (255.0 * 10f64.powf(-o.max(0.0) * 1.2)).clamp(0.0, 255.0) as u8;
    Rgb(c(od[0]), c(od[1]), c(od[2]))
}

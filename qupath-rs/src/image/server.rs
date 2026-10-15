//! The `ImageServer` abstraction: multi-resolution pixel access, as in QuPath.

use std::path::PathBuf;

use anyhow::Result;

use crate::objects::Rgb;

/// One resolution of the pyramid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    /// Width in pixels at this level.
    pub width: u32,
    /// Height in pixels at this level.
    pub height: u32,
    /// Full-resolution pixels per pixel at this level.
    pub downsample: f64,
}

/// A channel's name and display color.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelInfo {
    /// Name.
    pub name: String,
    /// Display color.
    pub color: Rgb,
}

/// Sample storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelType {
    /// 8-bit unsigned.
    U8,
    /// 16-bit unsigned.
    U16,
    /// 32-bit float.
    F32,
}

impl PixelType {
    /// QuPath's name for the type.
    pub fn name(self) -> &'static str {
        match self {
            PixelType::U8 => "uint8",
            PixelType::U16 => "uint16",
            PixelType::F32 => "float32",
        }
    }
    /// Largest value for display defaults.
    pub fn max_value(self) -> f32 {
        match self {
            PixelType::U8 => 255.0,
            PixelType::U16 => 65535.0,
            PixelType::F32 => 1.0,
        }
    }
}

/// What an image is.
#[derive(Clone, Debug)]
pub struct ImageMetadata {
    /// Display name (usually the file name).
    pub name: String,
    /// The file.
    pub path: PathBuf,
    /// Which reader serves it.
    pub server_type: String,
    /// Levels, highest resolution first.
    pub levels: Vec<Level>,
    /// Preferred tile width.
    pub tile_width: u32,
    /// Preferred tile height.
    pub tile_height: u32,
    /// Pixel width in µm, if known.
    pub pixel_width_um: Option<f64>,
    /// Pixel height in µm, if known.
    pub pixel_height_um: Option<f64>,
    /// Objective magnification, if known.
    pub magnification: Option<f64>,
    /// Channels.
    pub channels: Vec<ChannelInfo>,
    /// Interleaved 8-bit RGB.
    pub rgb: bool,
    /// Sample type.
    pub pixel_type: PixelType,
    /// Names of extra images (label, macro, thumbnail).
    pub associated: Vec<String>,
    /// Raw metadata key/value pairs.
    pub properties: Vec<(String, String)>,
}

impl ImageMetadata {
    /// Full-resolution width.
    pub fn width(&self) -> u32 {
        self.levels[0].width
    }
    /// Full-resolution height.
    pub fn height(&self) -> u32 {
        self.levels[0].height
    }
    /// Number of channels.
    pub fn n_channels(&self) -> usize {
        self.channels.len()
    }
    /// The highest-resolution level whose downsample does not exceed `downsample`
    /// (with a small tolerance, as QuPath's `ServerTools.getPreferredResolutionLevel`).
    pub fn preferred_level(&self, downsample: f64) -> usize {
        let mut best = 0;
        for (i, l) in self.levels.iter().enumerate() {
            if l.downsample <= downsample * 1.0001 {
                best = i;
            }
        }
        best
    }
    /// Default channels for RGB images.
    pub fn rgb_channels() -> Vec<ChannelInfo> {
        vec![
            ChannelInfo { name: "Red".into(), color: Rgb(255, 0, 0) },
            ChannelInfo { name: "Green".into(), color: Rgb(0, 255, 0) },
            ChannelInfo { name: "Blue".into(), color: Rgb(0, 0, 255) },
        ]
    }
    /// Default channel names/colors for `n` fluorescence channels (QuPath's cycle).
    pub fn default_channels(n: usize) -> Vec<ChannelInfo> {
        const COLORS: [Rgb; 6] = [Rgb(255, 0, 0), Rgb(0, 255, 0), Rgb(0, 0, 255), Rgb(255, 255, 0), Rgb(0, 255, 255), Rgb(255, 0, 255)];
        if n == 1 {
            return vec![ChannelInfo { name: "Channel 1".into(), color: Rgb(255, 255, 255) }];
        }
        (0..n).map(|i| ChannelInfo { name: format!("Channel {}", i + 1), color: COLORS[i % COLORS.len()] }).collect()
    }
}

/// Decoded pixels, channels interleaved.
#[derive(Clone, Debug)]
pub enum TileData {
    /// 8-bit samples.
    U8(Vec<u8>),
    /// 16-bit samples.
    U16(Vec<u16>),
    /// Float samples.
    F32(Vec<f32>),
}

/// A block of pixels.
#[derive(Clone, Debug)]
pub struct Tile {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Interleaved channels.
    pub channels: usize,
    /// Samples.
    pub data: TileData,
}

impl Tile {
    /// A sample as f32.
    #[inline]
    pub fn get(&self, x: u32, y: u32, c: usize) -> f32 {
        let i = (y as usize * self.width as usize + x as usize) * self.channels + c;
        match &self.data {
            TileData::U8(v) => f32::from(v[i]),
            TileData::U16(v) => f32::from(v[i]),
            TileData::F32(v) => v[i],
        }
    }

    /// Approximate memory use in bytes.
    pub fn byte_size(&self) -> usize {
        match &self.data {
            TileData::U8(v) => v.len(),
            TileData::U16(v) => v.len() * 2,
            TileData::F32(v) => v.len() * 4,
        }
    }

    /// 8-bit RGB pixels (channels beyond the third ignored; one channel repeated).
    pub fn to_rgb8(&self) -> Vec<u8> {
        let n = (self.width * self.height) as usize;
        let mut out = Vec::with_capacity(n * 3);
        let scale = match self.data {
            TileData::U16(_) => 1.0 / 257.0,
            TileData::F32(_) => 255.0,
            TileData::U8(_) => 1.0,
        };
        for i in 0..n {
            let (x, y) = ((i % self.width as usize) as u32, (i / self.width as usize) as u32);
            for c in 0..3 {
                let cc = c.min(self.channels - 1);
                out.push((self.get(x, y, cc) * scale).clamp(0.0, 255.0) as u8);
            }
        }
        out
    }
}

/// Multi-resolution pixel access. Implementations must be thread safe: tiles are read in parallel.
pub trait ImageServer: Send + Sync {
    /// What the image is.
    fn metadata(&self) -> &ImageMetadata;

    /// Reads a region of one pyramid level, in that level's pixel coordinates.
    fn read_level_region(&self, level: usize, x: u32, y: u32, w: u32, h: u32) -> Result<Tile>;

    /// Reads an associated image (label, macro) as RGBA.
    fn read_associated(&self, _name: &str) -> Result<image::RgbaImage> {
        anyhow::bail!("no associated images")
    }
}

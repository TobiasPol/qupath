//! Ordinary images (PNG, JPEG, BMP, small TIFF...), held in memory with a computed pyramid.

use std::path::Path;

use anyhow::{Context, Result, bail};

use super::region::resample;
use super::server::{ImageMetadata, ImageServer, Level, PixelType, Tile, TileData};

/// An image decoded entirely into memory.
pub struct PlainServer {
    levels: Vec<Tile>,
    metadata: ImageMetadata,
}

impl PlainServer {
    /// Decodes an image file.
    pub fn open(path: &Path) -> Result<Self> {
        let img =
            image::ImageReader::open(path)?.with_guessed_format()?.decode().with_context(|| format!("cannot decode {}", path.display()))?;
        let (w, h) = (img.width(), img.height());
        let base = match img {
            image::DynamicImage::ImageLuma8(i) => Tile { width: w, height: h, channels: 1, data: TileData::U8(i.into_raw()) },
            image::DynamicImage::ImageLuma16(i) => Tile { width: w, height: h, channels: 1, data: TileData::U16(i.into_raw()) },
            image::DynamicImage::ImageRgb16(i) => Tile { width: w, height: h, channels: 3, data: TileData::U16(i.into_raw()) },
            other => Tile { width: w, height: h, channels: 3, data: TileData::U8(other.into_rgb8().into_raw()) },
        };
        Self::from_tile(base, path)
    }

    /// Serves pixels already in memory.
    pub fn from_tile(base: Tile, path: &Path) -> Result<Self> {
        if base.width == 0 || base.height == 0 {
            bail!("empty image");
        }
        let rgb = base.channels == 3 && matches!(base.data, TileData::U8(_));
        let pixel_type = match base.data {
            TileData::U8(_) => PixelType::U8,
            TileData::U16(_) => PixelType::U16,
            TileData::F32(_) => PixelType::F32,
        };
        let channels = if rgb { ImageMetadata::rgb_channels() } else { ImageMetadata::default_channels(base.channels) };
        let mut levels = vec![base];
        while let Some(last) = levels.last().filter(|l| l.width.max(l.height) > 1024) {
            let next = resample(last, last.width.div_ceil(2), last.height.div_ceil(2));
            levels.push(next);
        }
        let (w0, h0) = (f64::from(levels[0].width), f64::from(levels[0].height));
        let lv = levels
            .iter()
            .map(|t| Level { width: t.width, height: t.height, downsample: (w0 / f64::from(t.width) + h0 / f64::from(t.height)) / 2.0 })
            .collect();
        let metadata = ImageMetadata {
            name: path.file_name().map_or_else(|| "Image".into(), |n| n.to_string_lossy().into_owned()),
            path: path.to_path_buf(),
            server_type: "Image (in memory)".into(),
            levels: lv,
            tile_width: 512,
            tile_height: 512,
            pixel_width_um: None,
            pixel_height_um: None,
            magnification: None,
            channels,
            rgb,
            pixel_type,
            associated: vec![],
            properties: vec![],
        };
        Ok(PlainServer { levels, metadata })
    }
}

impl ImageServer for PlainServer {
    fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }

    fn read_level_region(&self, level: usize, x: u32, y: u32, w: u32, h: u32) -> Result<Tile> {
        let src = self.levels.get(level).context("no such level")?;
        let (w, h) = (w.min(src.width.saturating_sub(x)), h.min(src.height.saturating_sub(y)));
        if w == 0 || h == 0 {
            bail!("empty region");
        }
        let c = src.channels;
        let row = |yy: u32| ((yy * src.width + x) as usize * c, ((yy * src.width + x + w) as usize) * c);
        let data = match &src.data {
            TileData::U8(v) => TileData::U8(
                (y..y + h)
                    .flat_map(|yy| {
                        let (a, b) = row(yy);
                        v[a..b].to_vec()
                    })
                    .collect(),
            ),
            TileData::U16(v) => TileData::U16(
                (y..y + h)
                    .flat_map(|yy| {
                        let (a, b) = row(yy);
                        v[a..b].to_vec()
                    })
                    .collect(),
            ),
            TileData::F32(v) => TileData::F32(
                (y..y + h)
                    .flat_map(|yy| {
                        let (a, b) = row(yy);
                        v[a..b].to_vec()
                    })
                    .collect(),
            ),
        };
        Ok(Tile { width: w, height: h, channels: c, data })
    }
}

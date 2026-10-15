//! OpenSlide, loaded at runtime, for vendor formats (NDPI, MRXS, SCN, VMS, BIF, JPEG 2000 SVS...).

mod loader;

use std::ffi::{CStr, CString, c_char};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};

use super::server::{ImageMetadata, ImageServer, Level, PixelType, Tile, TileData};
use loader::{API, Api, Osr, load};
pub use loader::{availability, preload, status};

/// A slide opened through OpenSlide.
pub struct OpenSlideServer {
    api: Arc<Api>,
    osr: Osr,
    metadata: ImageMetadata,
}

// SAFETY: OpenSlide handles are documented as thread safe for reading.
unsafe impl Send for OpenSlideServer {}
// SAFETY: as above.
unsafe impl Sync for OpenSlideServer {}

fn strings(list: *const *const c_char) -> Vec<String> {
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    // SAFETY: OpenSlide returns a NULL-terminated array of C strings it owns.
    unsafe {
        let mut p = list;
        while !(*p).is_null() {
            out.push(CStr::from_ptr(*p).to_string_lossy().into_owned());
            p = p.add(1);
        }
    }
    out
}

impl OpenSlideServer {
    /// Opens a slide.
    pub fn open(path: &Path) -> Result<Self> {
        let api = API.get_or_init(load).clone().map_err(|e| anyhow!(e))?;
        let c = CString::new(path.to_string_lossy().as_bytes())?;
        // SAFETY: valid C string; the handle is checked before use.
        let osr = unsafe { (api.open)(c.as_ptr()) };
        if osr.is_null() {
            bail!("OpenSlide cannot read {}", path.display());
        }
        let mut s = OpenSlideServer { api, osr, metadata: placeholder(path) };
        s.check()?;
        s.metadata = s.read_metadata(path);
        Ok(s)
    }

    fn check(&self) -> Result<()> {
        // SAFETY: valid handle.
        let e = unsafe { (self.api.get_error)(self.osr) };
        if e.is_null() {
            return Ok(());
        }
        // SAFETY: OpenSlide returns a C string it owns.
        bail!("OpenSlide: {}", unsafe { CStr::from_ptr(e) }.to_string_lossy())
    }

    fn property(&self, name: &str) -> Option<String> {
        let c = CString::new(name).ok()?;
        // SAFETY: valid handle and C string.
        let v = unsafe { (self.api.property_value)(self.osr, c.as_ptr()) };
        // SAFETY: OpenSlide returns a C string it owns, or NULL.
        (!v.is_null()).then(|| unsafe { CStr::from_ptr(v) }.to_string_lossy().into_owned())
    }

    fn read_metadata(&self, path: &Path) -> ImageMetadata {
        // SAFETY: valid handle.
        let n = unsafe { (self.api.level_count)(self.osr) }.max(1);
        let levels = (0..n)
            .map(|i| {
                let (mut w, mut h) = (0i64, 0i64);
                // SAFETY: valid handle and level index.
                let ds = unsafe {
                    (self.api.level_dimensions)(self.osr, i, &raw mut w, &raw mut h);
                    (self.api.level_downsample)(self.osr, i)
                };
                Level { width: w as u32, height: h as u32, downsample: ds }
            })
            .collect();
        // SAFETY: valid handle.
        let names = strings(unsafe { (self.api.property_names)(self.osr) });
        let properties: Vec<(String, String)> = names.iter().filter_map(|k| Some((k.clone(), self.property(k)?))).collect();
        let num = |k: &str| self.property(k).and_then(|v| v.parse::<f64>().ok());
        let tile = |k: &str| num(k).map_or(512, |v| (v as u32).clamp(256, 1024));
        ImageMetadata {
            levels,
            pixel_width_um: num("openslide.mpp-x"),
            pixel_height_um: num("openslide.mpp-y"),
            magnification: num("openslide.objective-power"),
            tile_width: tile("openslide.level[0].tile-width"),
            tile_height: tile("openslide.level[0].tile-height"),
            server_type: format!("OpenSlide ({})", self.property("openslide.vendor").unwrap_or_default()),
            // SAFETY: valid handle.
            associated: strings(unsafe { (self.api.associated_names)(self.osr) }),
            properties,
            ..placeholder(path)
        }
    }
}

fn placeholder(path: &Path) -> ImageMetadata {
    ImageMetadata {
        name: path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        path: path.to_path_buf(),
        server_type: "OpenSlide".into(),
        levels: vec![Level { width: 1, height: 1, downsample: 1.0 }],
        tile_width: 512,
        tile_height: 512,
        pixel_width_um: None,
        pixel_height_um: None,
        magnification: None,
        channels: ImageMetadata::rgb_channels(),
        rgb: true,
        pixel_type: PixelType::U8,
        associated: vec![],
        properties: vec![],
    }
}

/// Premultiplied ARGB to RGB over white, as QuPath's OpenSlide reader shows transparency.
fn argb_to_rgb(buf: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len() * 3);
    for p in buf {
        let a = (p >> 24) & 0xFF;
        let bg = 255 - a;
        for shift in [16, 8, 0] {
            out.push((((p >> shift) & 0xFF) + bg).min(255) as u8);
        }
    }
    out
}

impl ImageServer for OpenSlideServer {
    fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }

    fn read_level_region(&self, level: usize, x: u32, y: u32, w: u32, h: u32) -> Result<Tile> {
        let l = self.metadata.levels.get(level).context("no such level")?;
        let mut buf = vec![0u32; (w * h) as usize];
        let (fx, fy) = ((f64::from(x) * l.downsample) as i64, (f64::from(y) * l.downsample) as i64);
        // SAFETY: the buffer holds w*h pixels, as OpenSlide requires.
        unsafe { (self.api.read_region)(self.osr, buf.as_mut_ptr(), fx, fy, level as i32, i64::from(w), i64::from(h)) };
        self.check()?;
        Ok(Tile { width: w, height: h, channels: 3, data: TileData::U8(argb_to_rgb(&buf)) })
    }

    fn read_associated(&self, name: &str) -> Result<image::RgbaImage> {
        let c = CString::new(name)?;
        let (mut w, mut h) = (0i64, 0i64);
        // SAFETY: valid handle and name; buffer sized from the reported dimensions.
        unsafe { (self.api.associated_dimensions)(self.osr, c.as_ptr(), &raw mut w, &raw mut h) };
        if w <= 0 || h <= 0 {
            bail!("no associated image {name}");
        }
        let mut buf = vec![0u32; (w * h) as usize];
        // SAFETY: as above.
        unsafe { (self.api.read_associated)(self.osr, c.as_ptr(), buf.as_mut_ptr()) };
        self.check()?;
        let rgba = argb_to_rgb(&buf).as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        image::RgbaImage::from_raw(w as u32, h as u32, rgba).context("bad associated image")
    }
}

impl Drop for OpenSlideServer {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and closed once.
        unsafe { (self.api.close)(self.osr) };
    }
}

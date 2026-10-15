//! Image reading, display and writing.

pub mod display;
pub mod openslide;
pub mod plain;
pub mod region;
pub mod server;
pub mod stains;
pub mod tiff;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};

pub use region::{RegionRequest, read_region};
pub use server::{ImageMetadata, ImageServer, Tile, TileData};

/// Extensions of the formats this app can try to open.
pub const EXTENSIONS: &[&str] = &[
    "svs", "tif", "tiff", "btf", "qptiff", "ndpi", "vms", "vmu", "mrxs", "scn", "svslide", "bif", "dcm", "png", "jpg", "jpeg", "bmp",
    "gif", "webp",
];

/// Opens an image with the most suitable reader, as QuPath does by trying its servers in turn.
pub fn open(path: &Path) -> Result<Arc<dyn ImageServer>> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let vendor = matches!(ext.as_str(), "ndpi" | "vms" | "vmu" | "mrxs" | "scn" | "svslide" | "bif" | "dcm");
    let tiff_like = matches!(ext.as_str(), "svs" | "tif" | "tiff" | "btf" | "qptiff");
    let mut errors = Vec::new();
    if tiff_like {
        match tiff::TiffServer::open(path).and_then(probe) {
            Ok(s) => return Ok(s),
            Err(e) => errors.push(format!("TIFF reader: {e:#}")),
        }
    }
    if vendor || tiff_like {
        match openslide::OpenSlideServer::open(path) {
            Ok(s) => return Ok(Arc::new(s)),
            Err(e) => errors.push(format!("OpenSlide: {e:#}")),
        }
    }
    match plain::PlainServer::open(path) {
        Ok(s) => Ok(Arc::new(s)),
        Err(e) => {
            errors.push(format!("Image reader: {e:#}"));
            Err(anyhow::anyhow!(errors.join("\n"))).with_context(|| format!("cannot open {}", path.display()))
        }
    }
}

/// Reads the smallest level's first tile, so unsupported compression is found at open time.
fn probe(s: tiff::TiffServer) -> Result<Arc<dyn ImageServer>> {
    let last = s.metadata().levels.len() - 1;
    s.read_level_region(last, 0, 0, 16, 16)?;
    Ok(Arc::new(s))
}

/// A thumbnail of at most `max` pixels on its longest side, as RGB.
pub fn thumbnail(server: &dyn ImageServer, max: u32) -> Result<image::RgbImage> {
    let md = server.metadata();
    let ds = (f64::from(md.width().max(md.height())) / f64::from(max)).max(1.0);
    let req = RegionRequest { downsample: ds, x: 0.0, y: 0.0, w: f64::from(md.width()), h: f64::from(md.height()) };
    let t = read_region(server, &req)?;
    let disp = display::ImageDisplay::new(md, stains::ImageType::Unset, None);
    let rgba = disp.to_rgba(&t);
    let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    image::RgbImage::from_raw(t.width, t.height, rgb).context("thumbnail size mismatch")
}

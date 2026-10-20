//! Writes a synthetic H-DAB whole-slide image as a pyramidal OME-TIFF.
//!
//! Usage: `make-test-slide OUTPUT.ome.tif [WIDTH HEIGHT]`

use std::path::PathBuf;

use qupath::image::tiff::writer::{Compression, WriteOptions, write_ome_tiff};
use qupath::synthetic::SyntheticSlide;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "test-slide.ome.tif".into()));
    let width: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(24_000);
    let height: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(18_000);
    let slide = SyntheticSlide::new(width, height);
    let levels = (1..).take_while(|l| (width.max(height) >> l) >= 512).count() + 1;
    let opts = WriteOptions {
        tile_size: 512,
        compression: Compression::Jpeg(90),
        pixel_size_um: Some(0.25),
        name: "Synthetic H-DAB".into(),
        levels,
    };
    let start = std::time::Instant::now();
    write_ome_tiff(&out, &slide, &opts)?;
    let size = std::fs::metadata(&out)?.len();
    eprintln!("Wrote {} ({width}×{height}, {levels} levels, {:.1} MB) in {:.1?}", out.display(), size as f64 / 1e6, start.elapsed());
    Ok(())
}

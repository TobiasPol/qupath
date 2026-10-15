//! Pyramidal OME-TIFF written by qupath-rs reads back with the right pixels and metadata.

use qupath::image::ImageServer;
use qupath::image::server::{Tile, TileData};
use qupath::image::tiff::TiffServer;
use qupath::image::tiff::writer::{Compression, PixelSource, WriteOptions, write_ome_tiff};
use qupath::synthetic::SyntheticSlide;

/// A gradient whose values can be predicted exactly.
struct Gradient;

impl PixelSource for Gradient {
    fn width(&self) -> u32 {
        1000
    }
    fn height(&self) -> u32 {
        700
    }
    fn read(&self, ds: f64, x: u32, y: u32, w: u32, h: u32) -> anyhow::Result<Tile> {
        let mut v = Vec::new();
        for yy in 0..h {
            for xx in 0..w {
                let (fx, fy) = ((f64::from(x) + f64::from(xx) * ds) as u32, (f64::from(y) + f64::from(yy) * ds) as u32);
                v.extend([(fx % 256) as u8, (fy % 256) as u8, ((fx + fy) % 251) as u8]);
            }
        }
        Ok(Tile { width: w, height: h, channels: 3, data: TileData::U8(v) })
    }
}

fn opts(compression: Compression) -> WriteOptions {
    WriteOptions { tile_size: 256, compression, pixel_size_um: Some(0.5), name: "test".into(), levels: 3 }
}

#[test]
fn deflate_is_lossless_and_metadata_survives() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gradient.ome.tif");
    write_ome_tiff(&path, &Gradient, &opts(Compression::Deflate)).expect("write");
    let s = TiffServer::open(&path).expect("open");
    let md = s.metadata();
    assert_eq!((md.width(), md.height()), (1000, 700));
    assert_eq!(md.levels.len(), 3);
    assert_eq!(md.pixel_width_um, Some(0.5));
    assert!(md.rgb);
    assert!((md.levels[1].downsample - 2.0).abs() < 0.01);
    // A region crossing tile boundaries.
    let t = s.read_level_region(0, 250, 250, 20, 20).expect("read");
    let expected = Gradient.read(1.0, 250, 250, 20, 20).expect("source");
    assert_eq!(t.to_rgb8(), expected.to_rgb8());
}

#[test]
fn jpeg_tiles_decode_close_to_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("slide.ome.tif");
    let slide = SyntheticSlide::new(1500, 1200);
    write_ome_tiff(&path, &slide, &opts(Compression::Jpeg(95))).expect("write");
    let s = TiffServer::open(&path).expect("open");
    let t = s.read_level_region(0, 600, 500, 64, 64).expect("read");
    let src = slide.read(1.0, 600, 500, 64, 64).expect("source");
    let (a, b) = (t.to_rgb8(), src.to_rgb8());
    let mean_err = a.iter().zip(&b).map(|(x, y)| (f64::from(*x) - f64::from(*y)).abs()).sum::<f64>() / a.len() as f64;
    assert!(mean_err < 4.0, "mean error {mean_err}");
}

#[test]
fn opening_by_extension_picks_the_tiff_reader() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("g.tif");
    write_ome_tiff(&path, &Gradient, &opts(Compression::Deflate)).expect("write");
    let s = qupath::image::open(&path).expect("open");
    assert!(s.metadata().server_type.contains("OME-TIFF"));
    let thumb = qupath::image::thumbnail(s.as_ref(), 100).expect("thumbnail");
    assert_eq!(thumb.width(), 100);
}

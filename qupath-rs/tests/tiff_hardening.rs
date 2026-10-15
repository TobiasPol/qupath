//! Damaged or hostile TIFFs fail cleanly, and exported JPEG tiles are tagged so Bio-Formats reads true colors.

use qupath::image::server::{Tile, TileData};
use qupath::image::tiff::TiffServer;
use qupath::image::tiff::ifd::{TiffReader, tag};
use qupath::image::tiff::writer::{Compression, PixelSource, WriteOptions, write_ome_tiff};

/// A little-endian classic TIFF with one IFD built from (tag, type, count, value).
fn tiff(entries: &[(u16, u16, u32, u32)]) -> Vec<u8> {
    let mut b = b"II*\0".to_vec();
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (t, typ, count, value) in entries {
        b.extend_from_slice(&t.to_le_bytes());
        b.extend_from_slice(&typ.to_le_bytes());
        b.extend_from_slice(&count.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
    }
    b.extend_from_slice(&0u32.to_le_bytes());
    b
}

fn open(bytes: &[u8]) -> anyhow::Result<TiffServer> {
    let dir = tempfile::tempdir()?;
    let p = dir.path().join("bad.tif");
    std::fs::write(&p, bytes)?;
    TiffServer::open(&p)
}

#[test]
fn huge_dimensions_are_rejected() {
    let bytes = tiff(&[
        (tag::IMAGE_WIDTH, 4, 1, u32::MAX),
        (tag::IMAGE_LENGTH, 4, 1, u32::MAX),
        (tag::BITS_PER_SAMPLE, 3, 1, 8),
        (tag::TILE_WIDTH, 3, 1, 256),
        (tag::TILE_LENGTH, 3, 1, 256),
    ]);
    let err = open(&bytes).err().expect("must fail");
    assert!(format!("{err:#}").contains("size"), "{err:#}");
}

#[test]
fn missing_tile_offsets_are_rejected() {
    let bytes = tiff(&[
        (tag::IMAGE_WIDTH, 4, 1, 4096),
        (tag::IMAGE_LENGTH, 4, 1, 4096),
        (tag::BITS_PER_SAMPLE, 3, 1, 8),
        (tag::TILE_WIDTH, 3, 1, 256),
        (tag::TILE_LENGTH, 3, 1, 256),
        (tag::TILE_OFFSETS, 4, 1, 0),
        (tag::TILE_BYTE_COUNTS, 4, 1, 0),
    ]);
    assert!(open(&bytes).is_err());
}

#[test]
fn absurd_tag_counts_do_not_allocate() {
    // An ImageDescription claiming 4 billion bytes.
    let bytes = tiff(&[(tag::IMAGE_WIDTH, 4, 1, 16), (tag::IMAGE_DESCRIPTION, 2, u32::MAX, 8)]);
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("tag.tif");
    std::fs::write(&p, bytes).expect("write");
    let (mut r, first) = TiffReader::open(&p).expect("header");
    let (ifd, _) = r.read_ifd(first).expect("directory");
    assert!(ifd.ascii(tag::IMAGE_DESCRIPTION).is_none(), "oversized tag skipped");
}

struct Flat;

impl PixelSource for Flat {
    fn width(&self) -> u32 {
        300
    }
    fn height(&self) -> u32 {
        200
    }
    fn read(&self, _ds: f64, _x: u32, _y: u32, w: u32, h: u32) -> anyhow::Result<Tile> {
        Ok(Tile { width: w, height: h, channels: 3, data: TileData::U8([214u8, 208, 220].repeat((w * h) as usize)) })
    }
}

#[test]
fn jpeg_tiles_are_tagged_rgb_for_bio_formats() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = dir.path().join("flat.ome.tif");
    let opts = WriteOptions { tile_size: 256, compression: Compression::Jpeg(95), pixel_size_um: None, name: "flat".into(), levels: 1 };
    write_ome_tiff(&p, &Flat, &opts).expect("write");
    let (mut r, first) = TiffReader::open(&p).expect("header");
    let (ifd, _) = r.read_ifd(first).expect("directory");
    // Photometric YCbCr made Bio-Formats convert twice: stroma turned magenta.
    assert_eq!(ifd.int(tag::PHOTOMETRIC), Some(2));
    assert!(ifd.ints(tag::YCBCR_SUBSAMPLING).is_none());
    let s = TiffServer::open(&p).expect("open");
    let t = qupath::image::ImageServer::read_level_region(&s, 0, 10, 10, 1, 1).expect("read");
    let px = t.to_rgb8();
    assert!(px.iter().zip([214u8, 208, 220]).all(|(a, b)| a.abs_diff(b) <= 3), "{px:?}");
}

#[test]
fn self_referencing_subifds_are_bounded() {
    // IFD0 lists 5000 SubIFDs, all pointing back at IFD0 itself.
    let n = 5000u32;
    let entries: [(u16, u16, u32, u32); 3] = [(tag::IMAGE_WIDTH, 4, 1, 64), (tag::IMAGE_LENGTH, 4, 1, 64), (tag::SUB_IFDS, 4, n, 0)];
    let ifd_len = 2 + entries.len() as u32 * 12 + 4;
    let data_at = 8 + ifd_len;
    let mut b = tiff(&entries.map(|e| if e.0 == tag::SUB_IFDS { (e.0, e.1, e.2, data_at) } else { e }));
    for _ in 0..n {
        b.extend_from_slice(&8u32.to_le_bytes());
    }
    let started = std::time::Instant::now();
    assert!(open(&b).is_err(), "too many directories must be refused");
    assert!(started.elapsed().as_secs() < 10, "refused promptly");
}

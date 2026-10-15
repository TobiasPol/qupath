//! Writing pyramidal OME-TIFF (BigTIFF, tiled, SubIFDs), readable by QuPath, Bio-Formats and this app.

use std::fs::File;
use std::io::{BufWriter, Seek, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::ifd::tag;
use crate::image::server::{Tile, TileData};

/// Tile compression.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Compression {
    /// Lossy JPEG (8-bit RGB or grayscale only).
    Jpeg(u8),
    /// Lossless zlib.
    Deflate,
}

/// What to write.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// Tile size in pixels.
    pub tile_size: u32,
    /// Compression.
    pub compression: Compression,
    /// Pixel size in µm.
    pub pixel_size_um: Option<f64>,
    /// Image name for the OME-XML.
    pub name: String,
    /// Number of pyramid levels (each half the size of the last), at least 1.
    pub levels: usize,
}

/// Something to read full-resolution-scaled regions from.
pub trait PixelSource: Sync {
    /// Full-resolution width.
    fn width(&self) -> u32;
    /// Full-resolution height.
    fn height(&self) -> u32;
    /// Reads `w`×`h` output pixels starting at full-resolution `(x, y)`, at a downsample.
    fn read(&self, downsample: f64, x: u32, y: u32, w: u32, h: u32) -> Result<Tile>;
}

struct Out {
    w: BufWriter<File>,
}

impl Out {
    fn pos(&mut self) -> Result<u64> {
        Ok(self.w.stream_position()?)
    }
    fn bytes(&mut self, b: &[u8]) -> Result<u64> {
        let p = self.pos()?;
        self.w.write_all(b)?;
        if self.pos()? % 2 == 1 {
            self.w.write_all(&[0])?;
        }
        Ok(p)
    }
}

/// One IFD entry: tag, type, count, and the encoded values.
struct Entry(u16, u16, u64, Vec<u8>);

fn shorts(t: u16, v: &[u16]) -> Entry {
    Entry(t, 3, v.len() as u64, v.iter().flat_map(|x| x.to_le_bytes()).collect())
}
fn longs8(t: u16, v: &[u64]) -> Entry {
    Entry(t, 16, v.len() as u64, v.iter().flat_map(|x| x.to_le_bytes()).collect())
}
fn ascii(t: u16, s: &str) -> Entry {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    Entry(t, 2, b.len() as u64, b)
}

/// Writes an image as a pyramidal OME-TIFF.
pub fn write_ome_tiff(path: &Path, src: &dyn PixelSource, opts: &WriteOptions) -> Result<()> {
    let probe = src.read(1.0, 0, 0, 1, 1)?;
    let channels = probe.channels;
    let bits: u16 = match probe.data {
        TileData::U8(_) => 8,
        TileData::U16(_) => 16,
        TileData::F32(_) => bail!("float images cannot be written yet"),
    };
    if bits != 8 && matches!(opts.compression, Compression::Jpeg(_)) {
        bail!("JPEG needs 8-bit pixels; use Deflate");
    }
    let mut out = Out { w: BufWriter::new(File::create(path).with_context(|| format!("cannot create {}", path.display()))?) };
    out.w.write_all(b"II\x2B\x00\x08\x00\x00\x00")?;
    out.w.write_all(&[0u8; 8])?; // first IFD offset, patched at the end
    let mut sub_offsets = Vec::new();
    for level in 1..opts.levels.max(1) {
        let ds = f64::from(1u32 << level);
        let ifd = write_level(&mut out, src, opts, ds, channels, bits, None, true)?;
        sub_offsets.push(ifd);
    }
    let ifd0 = write_level(&mut out, src, opts, 1.0, channels, bits, Some(&sub_offsets), false)?;
    let mut f = out.w.into_inner().map_err(|e| anyhow::anyhow!("flush failed: {e}"))?;
    f.seek(std::io::SeekFrom::Start(8))?;
    f.write_all(&ifd0.to_le_bytes())?;
    f.sync_all()?;
    Ok(())
}

/// One pyramid level: its downsample, size and sample layout.
struct LevelSpec {
    ds: f64,
    w: u32,
    h: u32,
    channels: usize,
    bits: u16,
}

#[allow(clippy::too_many_arguments)]
fn write_level(
    out: &mut Out,
    src: &dyn PixelSource,
    opts: &WriteOptions,
    ds: f64,
    channels: usize,
    bits: u16,
    sub_ifds: Option<&[u64]>,
    reduced: bool,
) -> Result<u64> {
    let (w, h) = ((f64::from(src.width()) / ds).ceil() as u32, (f64::from(src.height()) / ds).ceil() as u32);
    let spec = LevelSpec { ds, w, h, channels, bits };
    let (offsets, counts) = write_tiles(out, src, opts, &spec)?;
    let mut e = level_entries(opts, &spec, reduced, &offsets, &counts);
    if let Some(subs) = sub_ifds {
        e.push(ascii(tag::IMAGE_DESCRIPTION, &ome_xml(opts, w, h, channels, bits)));
        if !subs.is_empty() {
            e.push(longs8(tag::SUB_IFDS, subs));
        }
    }
    write_ifd(out, e)
}

/// Encodes a row of tiles at a time in parallel and writes them in order.
fn write_tiles(out: &mut Out, src: &dyn PixelSource, opts: &WriteOptions, l: &LevelSpec) -> Result<(Vec<u64>, Vec<u64>)> {
    use rayon::prelude::*;
    let ts = opts.tile_size;
    let (across, down) = (l.w.div_ceil(ts), l.h.div_ceil(ts));
    let (mut offsets, mut counts) = (Vec::new(), Vec::new());
    for ty in 0..down {
        let row: Vec<Result<Vec<u8>>> =
            (0..across).into_par_iter().map(|tx| encode_tile(src, opts, l.ds, [tx * ts, ty * ts], [l.w, l.h], l.channels)).collect();
        for t in row {
            let bytes = t?;
            counts.push(bytes.len() as u64);
            offsets.push(out.bytes(&bytes)?);
        }
    }
    Ok((offsets, counts))
}

fn level_entries(opts: &WriteOptions, l: &LevelSpec, reduced: bool, offsets: &[u64], counts: &[u64]) -> Vec<Entry> {
    let jpeg = matches!(opts.compression, Compression::Jpeg(_));
    // RGB even for JPEG: each tile is a JFIF stream that says it holds YCbCr, and
    // Bio-Formats converts JFIF to RGB and then again when Photometric says YCbCr.
    let photometric = if l.channels == 3 { 2 } else { 1 };
    let ts = opts.tile_size as u16;
    let e = vec![
        Entry(tag::NEW_SUBFILE_TYPE, 4, 1, u32::from(reduced).to_le_bytes().to_vec()),
        Entry(tag::IMAGE_WIDTH, 4, 1, l.w.to_le_bytes().to_vec()),
        Entry(tag::IMAGE_LENGTH, 4, 1, l.h.to_le_bytes().to_vec()),
        shorts(tag::BITS_PER_SAMPLE, &vec![l.bits; l.channels]),
        shorts(tag::COMPRESSION, &[if jpeg { 7 } else { 8 }]),
        shorts(tag::PHOTOMETRIC, &[photometric]),
        shorts(tag::SAMPLES_PER_PIXEL, &[l.channels as u16]),
        shorts(tag::PLANAR_CONFIG, &[1]),
        ascii(tag::SOFTWARE, concat!("qupath-rs ", env!("CARGO_PKG_VERSION"))),
        shorts(tag::TILE_WIDTH, &[ts]),
        shorts(tag::TILE_LENGTH, &[ts]),
        longs8(tag::TILE_OFFSETS, offsets),
        longs8(tag::TILE_BYTE_COUNTS, counts),
    ];
    e
}

fn encode_tile(src: &dyn PixelSource, opts: &WriteOptions, ds: f64, at: [u32; 2], size: [u32; 2], channels: usize) -> Result<Vec<u8>> {
    let ts = opts.tile_size;
    let (rw, rh) = ((size[0] - at[0]).min(ts), (size[1] - at[1]).min(ts));
    let fx = (f64::from(at[0]) * ds).round() as u32;
    let fy = (f64::from(at[1]) * ds).round() as u32;
    let tile = src.read(ds, fx, fy, rw, rh)?;
    let raw = padded(&tile, ts, channels);
    match opts.compression {
        Compression::Jpeg(q) => {
            let mut buf = Vec::new();
            let color = if channels == 3 { image::ExtendedColorType::Rgb8 } else { image::ExtendedColorType::L8 };
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q).encode(&raw, ts, ts, color)?;
            Ok(buf)
        }
        Compression::Deflate => {
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
            z.write_all(&raw)?;
            Ok(z.finish()?)
        }
    }
}

/// A full-size tile's bytes, padding the edges by repeating the last row/column.
fn padded(t: &Tile, ts: u32, channels: usize) -> Vec<u8> {
    let bps = match t.data {
        TileData::U16(_) => 2,
        _ => 1,
    };
    let mut out = Vec::with_capacity((ts * ts) as usize * channels * bps);
    for y in 0..ts {
        for x in 0..ts {
            let (sx, sy) = (x.min(t.width - 1), y.min(t.height - 1));
            for c in 0..channels {
                let v = t.get(sx, sy, c);
                if bps == 2 {
                    out.extend_from_slice(&(v as u16).to_le_bytes());
                } else {
                    out.push(v as u8);
                }
            }
        }
    }
    out
}

fn write_ifd(out: &mut Out, mut entries: Vec<Entry>) -> Result<u64> {
    entries.sort_by_key(|e| e.0);
    // Values longer than 8 bytes go before the IFD.
    let mut slots = Vec::with_capacity(entries.len());
    for e in &entries {
        if e.3.len() > 8 {
            slots.push(out.bytes(&e.3)?.to_le_bytes().to_vec());
        } else {
            let mut v = e.3.clone();
            v.resize(8, 0);
            slots.push(v);
        }
    }
    let at = out.pos()?;
    out.w.write_all(&(entries.len() as u64).to_le_bytes())?;
    for (e, slot) in entries.iter().zip(slots) {
        out.w.write_all(&e.0.to_le_bytes())?;
        out.w.write_all(&e.1.to_le_bytes())?;
        out.w.write_all(&e.2.to_le_bytes())?;
        out.w.write_all(&slot)?;
    }
    out.w.write_all(&0u64.to_le_bytes())?;
    Ok(at)
}

fn ome_xml(opts: &WriteOptions, w: u32, h: u32, channels: usize, bits: u16) -> String {
    let size = opts
        .pixel_size_um
        .map_or(String::new(), |s| format!(r#" PhysicalSizeX="{s}" PhysicalSizeXUnit="µm" PhysicalSizeY="{s}" PhysicalSizeYUnit="µm""#));
    let ty = if bits == 16 { "uint16" } else { "uint8" };
    let name = opts.name.replace(['"', '<', '>', '&'], "_");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><OME xmlns="http://www.openmicroscopy.org/Schemas/OME/2016-06" Creator="qupath-rs"><Image ID="Image:0" Name="{name}"><Pixels BigEndian="false" DimensionOrder="XYCZT" ID="Pixels:0" Interleaved="true"{size} SizeC="{channels}" SizeT="1" SizeX="{w}" SizeY="{h}" SizeZ="1" Type="{ty}"><Channel ID="Channel:0:0" SamplesPerPixel="{channels}"/><TiffData IFD="0" PlaneCount="1"/></Pixels></Image></OME>"#
    )
}

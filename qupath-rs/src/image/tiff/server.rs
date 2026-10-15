//! A pure-Rust reader for pyramidal TIFF: SVS, OME-TIFF and generic tiled TIFF.

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};

use super::decode::{ChunkFormat, decompress};
use super::ifd::{Ifd, TiffReader, tag};
use super::meta::{self, DescribedMetadata};
use crate::image::server::{ImageMetadata, ImageServer, Level, PixelType, Tile, TileData};

type ChunkCache = lru::LruCache<(usize, usize), Arc<Vec<u8>>>;

/// Serves pixels from a TIFF file.
pub struct TiffServer {
    reader: Mutex<TiffReader>,
    ifds: Vec<Ifd>,
    /// For each level, the IFD of each channel plane.
    levels: Vec<Vec<usize>>,
    associated: Vec<(String, usize)>,
    little_endian: bool,
    cache: Mutex<ChunkCache>,
    metadata: ImageMetadata,
}

/// The IFDs in the main chain, followed by every SubIFD, with each SubIFD list per main IFD.
fn read_all_ifds(r: &mut TiffReader, first: u64) -> Result<(Vec<Ifd>, Vec<Vec<usize>>)> {
    let mut main = Vec::new();
    let mut off = first;
    while off != 0 && main.len() < 10_000 {
        let (ifd, next) = r.read_ifd(off)?;
        main.push(ifd);
        off = next;
    }
    let n_main = main.len();
    let mut all = main;
    let mut subs = vec![Vec::new(); n_main];
    for (i, sub) in subs.iter_mut().enumerate() {
        let offsets: Vec<u64> = all[i].ints(tag::SUB_IFDS).map(<[u64]>::to_vec).unwrap_or_default();
        for o in offsets {
            let (ifd, _) = r.read_ifd(o)?;
            sub.push(all.len());
            all.push(ifd);
        }
    }
    Ok((all, subs))
}

/// Works out which IFDs make up the pyramid and which are extras.
fn layout(ifds: &[Ifd], subs: &[Vec<usize>], meta: &DescribedMetadata) -> (Vec<Vec<usize>>, Vec<(String, usize)>) {
    let n_main = subs.len();
    if !subs[0].is_empty() {
        // OME-TIFF: SubIFDs hold the lower resolutions, per plane.
        let planes: Vec<usize> = (0..meta.ome_channels.unwrap_or(1).min(n_main)).collect();
        let mut levels = vec![planes.clone()];
        for l in 0..subs[0].len() {
            levels.push(planes.iter().filter_map(|p| subs[*p].get(l).copied()).collect());
        }
        return (levels, Vec::new());
    }
    if let Some(c) = meta.ome_channels.filter(|c| *c <= n_main) {
        return (vec![(0..c).collect()], Vec::new());
    }
    let tiled = ifds[0].is_tiled();
    let mut levels = vec![vec![0]];
    let mut assoc = Vec::new();
    let spp = ifds[0].int(tag::SAMPLES_PER_PIXEL);
    for i in 1..n_main {
        let last = &ifds[*levels.last().and_then(|l| l.first()).unwrap_or(&0)];
        let is_level = ifds[i].is_tiled() == tiled
            && ifds[i].int(tag::SAMPLES_PER_PIXEL) == spp
            && ifds[i].width() < last.width()
            && ifds[i].height() < last.height()
            && (tiled || ifds[i].int(tag::NEW_SUBFILE_TYPE).unwrap_or(0) & 1 == 1)
            && !meta::associated_name(&ifds[i], i).starts_with(['l', 'm']);
        if is_level {
            levels.push(vec![i]);
        } else {
            assoc.push((meta::associated_name(&ifds[i], i), i));
        }
    }
    (levels, assoc)
}

impl TiffServer {
    /// Opens a TIFF file.
    pub fn open(path: &Path) -> Result<Self> {
        let (mut reader, first) = TiffReader::open(path)?;
        let little_endian = reader_is_little(path)?;
        let (ifds, subs) = read_all_ifds(&mut reader, first)?;
        if ifds.is_empty() {
            bail!("TIFF without images");
        }
        let described = meta::describe(&ifds[0]);
        let (levels, associated) = layout(&ifds, &subs, &described);
        let metadata = build_metadata(path, &ifds, &levels, &associated, described)?;
        let cache = Mutex::new(lru::LruCache::new(NonZeroUsize::new(256).context("cache size")?));
        Ok(TiffServer { reader: Mutex::new(reader), ifds, levels, associated, little_endian, cache, metadata })
    }

    fn format(&self, ifd: &Ifd) -> ChunkFormat {
        ChunkFormat {
            compression: ifd.int(tag::COMPRESSION).unwrap_or(1) as u16,
            photometric: ifd.int(tag::PHOTOMETRIC).unwrap_or(1) as u16,
            samples: ifd.int(tag::SAMPLES_PER_PIXEL).unwrap_or(1) as usize,
            bits: ifd.int(tag::BITS_PER_SAMPLE).unwrap_or(8) as u16,
            predictor: ifd.int(tag::PREDICTOR).unwrap_or(1) as u16,
            jpeg_tables: ifd.bytes(tag::JPEG_TABLES),
            little_endian: self.little_endian,
        }
    }

    /// Chunk grid of an IFD: (chunk width, chunk height, chunks across).
    fn grid(ifd: &Ifd) -> (u32, u32, u32) {
        let (cw, ch) = if ifd.is_tiled() {
            (ifd.int(tag::TILE_WIDTH).unwrap_or(256) as u32, ifd.int(tag::TILE_LENGTH).unwrap_or(256) as u32)
        } else {
            (ifd.width(), (ifd.int(tag::ROWS_PER_STRIP).unwrap_or(u64::from(ifd.height())) as u32).min(ifd.height()))
        };
        (cw.max(1), ch.max(1), ifd.width().div_ceil(cw.max(1)))
    }

    /// Decoded bytes of one chunk, cached.
    fn chunk(&self, ifd_index: usize, chunk: usize) -> Result<Arc<Vec<u8>>> {
        if let Some(c) = self.cache.lock().ok().and_then(|mut c| c.get(&(ifd_index, chunk)).cloned()) {
            return Ok(c);
        }
        let ifd = &self.ifds[ifd_index];
        let (offsets, counts) = if ifd.is_tiled() {
            (ifd.ints(tag::TILE_OFFSETS), ifd.ints(tag::TILE_BYTE_COUNTS))
        } else {
            (ifd.ints(tag::STRIP_OFFSETS), ifd.ints(tag::STRIP_BYTE_COUNTS))
        };
        let (off, len) = (offsets.context("no offsets")?, counts.context("no byte counts")?);
        let (o, n) = (*off.get(chunk).context("chunk out of range")?, *len.get(chunk).context("chunk out of range")?);
        let (cw, ch, _) = Self::grid(ifd);
        let rows = if ifd.is_tiled() { ch } else { ch.min(ifd.height() - (chunk as u32 * ch).min(ifd.height())) };
        let decoded = if n == 0 {
            vec![0u8; (cw * rows) as usize * self.format(ifd).samples * (self.format(ifd).bits as usize / 8)]
        } else {
            let raw = self.reader.lock().map_err(|_| anyhow::anyhow!("reader poisoned"))?.read_at(o, n as usize)?;
            decompress(&raw, &self.format(ifd), cw as usize, rows as usize)?
        };
        let arc = Arc::new(decoded);
        if let Ok(mut c) = self.cache.lock() {
            c.put((ifd_index, chunk), arc.clone());
        }
        Ok(arc)
    }

    /// Copies one plane's region into an interleaved output buffer.
    fn read_plane(&self, ifd_index: usize, rect: [u32; 4], out: &mut [u8], out_channels: usize, c0: usize) -> Result<()> {
        let [x, y, w, h] = rect;
        let ifd = &self.ifds[ifd_index];
        let fmt = self.format(ifd);
        let bps = usize::from(fmt.bits / 8);
        let (cw, ch, across) = Self::grid(ifd);
        for cy in (y / ch)..=((y + h - 1) / ch) {
            for cx in (x / cw)..=((x + w - 1) / cw) {
                let data = self.chunk(ifd_index, (cy * across + cx) as usize)?;
                let (ox, oy) = (cx * cw, cy * ch);
                let (ix0, ix1) = (x.max(ox), (x + w).min(ox + cw));
                let (iy0, iy1) = (y.max(oy), (y + h).min(oy + ch));
                for yy in iy0..iy1 {
                    for xx in ix0..ix1 {
                        let src = (((yy - oy) * cw + (xx - ox)) as usize) * fmt.samples * bps;
                        let dst = (((yy - y) * w + (xx - x)) as usize * out_channels + c0) * bps;
                        let n = fmt.samples * bps;
                        if src + n <= data.len() {
                            out[dst..dst + n].copy_from_slice(&data[src..src + n]);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn reader_is_little(path: &Path) -> Result<bool> {
    let mut b = [0u8; 2];
    std::io::Read::read_exact(&mut std::fs::File::open(path)?, &mut b)?;
    Ok(&b == b"II")
}

fn level_sizes(ifds: &[Ifd], levels: &[Vec<usize>]) -> Vec<Level> {
    let base = &ifds[levels[0][0]];
    let (w0, h0) = (f64::from(base.width()), f64::from(base.height()));
    levels
        .iter()
        .map(|l| {
            let i = &ifds[l[0]];
            let ds = ((w0 / f64::from(i.width())) + (h0 / f64::from(i.height()))) / 2.0;
            Level { width: i.width(), height: i.height(), downsample: ds }
        })
        .collect()
}

fn pixel_type(base: &Ifd) -> Result<PixelType> {
    let bits = base.int(tag::BITS_PER_SAMPLE).unwrap_or(8);
    match (bits, base.int(tag::SAMPLE_FORMAT).unwrap_or(1)) {
        (8, _) => Ok(PixelType::U8),
        (16, _) => Ok(PixelType::U16),
        (32, 3) => bail!("32-bit float TIFF is not supported yet"),
        (b, f) => bail!("unsupported TIFF samples: {b} bits, format {f}"),
    }
}

fn build_metadata(
    path: &Path,
    ifds: &[Ifd],
    levels: &[Vec<usize>],
    associated: &[(String, usize)],
    d: DescribedMetadata,
) -> Result<ImageMetadata> {
    let base = &ifds[levels[0][0]];
    let lv = level_sizes(ifds, levels);
    let spp = base.int(tag::SAMPLES_PER_PIXEL).unwrap_or(1) as usize;
    let pixel_type = pixel_type(base)?;
    let n_channels = spp * levels[0].len();
    let rgb = n_channels == 3 && pixel_type == PixelType::U8 && levels[0].len() == 1;
    let mut channels = if rgb { ImageMetadata::rgb_channels() } else { ImageMetadata::default_channels(n_channels) };
    for (c, name) in channels.iter_mut().zip(&d.channel_names) {
        if !name.is_empty() {
            c.name.clone_from(name);
        }
    }
    let tiled = base.is_tiled();
    let tile = |t: u16, def: u32| if tiled { (base.int(t).unwrap_or(256) as u32).max(64) } else { def };
    Ok(ImageMetadata {
        name: path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        path: path.to_path_buf(),
        server_type: format!("{} (qupath-rs)", d.flavor),
        levels: lv,
        tile_width: tile(tag::TILE_WIDTH, 512),
        tile_height: tile(tag::TILE_LENGTH, 512),
        pixel_width_um: d.pixel_width_um,
        pixel_height_um: d.pixel_height_um,
        magnification: d.magnification,
        channels,
        rgb,
        pixel_type,
        associated: associated.iter().map(|(n, _)| n.clone()).collect(),
        properties: d.properties,
    })
}

impl ImageServer for TiffServer {
    fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }

    fn read_level_region(&self, level: usize, x: u32, y: u32, w: u32, h: u32) -> Result<Tile> {
        let planes = self.levels.get(level).context("no such level")?;
        let base = &self.ifds[planes[0]];
        let (w, h) = (w.min(base.width().saturating_sub(x)), h.min(base.height().saturating_sub(y)));
        if w == 0 || h == 0 {
            bail!("empty region");
        }
        let spp = self.format(base).samples;
        let channels = spp * planes.len();
        let bps = usize::from(self.format(base).bits / 8);
        let mut buf = vec![0u8; (w * h) as usize * channels * bps];
        for (p, ifd) in planes.iter().enumerate() {
            self.read_plane(*ifd, [x, y, w, h], &mut buf, channels, p * spp)?;
        }
        let data = if bps == 2 {
            let le = self.little_endian;
            TileData::U16(
                buf.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
                    .collect(),
            )
        } else {
            TileData::U8(buf)
        };
        Ok(Tile { width: w, height: h, channels, data })
    }

    fn read_associated(&self, name: &str) -> Result<image::RgbaImage> {
        let (_, i) = self.associated.iter().find(|(n, _)| n == name).context("no such associated image")?;
        let ifd = &self.ifds[*i];
        let fmt = self.format(ifd);
        let (w, h) = (ifd.width(), ifd.height());
        let mut buf = vec![0u8; (w * h) as usize * fmt.samples * usize::from(fmt.bits / 8)];
        self.read_plane(*i, [0, 0, w, h], &mut buf, fmt.samples, 0)?;
        let t = Tile { width: w, height: h, channels: fmt.samples, data: TileData::U8(buf) };
        let rgba: Vec<u8> = t.to_rgb8().as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        image::RgbaImage::from_raw(w, h, rgba).context("bad associated image")
    }
}

//! Limits that keep a damaged or hostile TIFF from exhausting memory or
//! overflowing arithmetic. Real whole-slide images stay far below them.

use anyhow::{Result, bail};

use super::ifd::{Ifd, tag};

/// Largest width or height accepted (16.7 million pixels).
pub const MAX_DIMENSION: u32 = 1 << 24;
/// Largest tile side accepted.
pub const MAX_TILE: u32 = 1 << 16;
/// Largest decoded tile or strip.
pub const MAX_CHUNK_BYTES: usize = 512 << 20;
/// Largest region read in one call.
pub const MAX_REGION_BYTES: usize = 2 << 30;
/// Largest tag value read from the file, measured as decoded (8 bytes per number).
pub const MAX_TAG_BYTES: usize = 64 << 20;
/// Decoded tag data allowed for the whole file (all directories together).
pub const MAX_FILE_TAG_BYTES: usize = 256 << 20;
/// Directories allowed in a file, SubIFDs included.
pub const MAX_IFDS: usize = 4096;
/// Largest JPEGTables tag (real tables are a few hundred bytes).
pub const MAX_JPEG_TABLES: usize = 64 << 10;
/// Decoded chunks kept in memory per open image.
pub const CHUNK_CACHE_BYTES: usize = 256 << 20;

/// Product of sizes, failing instead of overflowing or exceeding `limit`.
pub fn bytes(factors: &[usize], limit: usize) -> Result<usize> {
    let mut n: usize = 1;
    for f in factors {
        n = match n.checked_mul(*f) {
            Some(v) => v,
            None => bail!("TIFF sizes overflow"),
        };
    }
    if n > limit {
        bail!("TIFF region of {n} bytes exceeds the {limit}-byte limit");
    }
    Ok(n)
}

/// Rejects IFDs whose sizes or layout cannot be read safely.
pub fn validate(ifd: &Ifd) -> Result<()> {
    let (w, h) = (ifd.width(), ifd.height());
    if w == 0 || h == 0 || w > MAX_DIMENSION || h > MAX_DIMENSION {
        bail!("unsupported TIFF size {w}×{h}");
    }
    let samples = ifd.int(tag::SAMPLES_PER_PIXEL).unwrap_or(1);
    if !(1..=16).contains(&samples) {
        bail!("unsupported TIFF samples per pixel: {samples}");
    }
    let bits = ifd.int(tag::BITS_PER_SAMPLE).unwrap_or(8);
    if !matches!(bits, 8 | 16 | 32) {
        bail!("unsupported TIFF bits per sample: {bits}");
    }
    let (cw, ch, across, down, offsets, counts) = if ifd.is_tiled() {
        let tw = ifd.int(tag::TILE_WIDTH).unwrap_or(0);
        let th = ifd.int(tag::TILE_LENGTH).unwrap_or(0);
        if tw == 0 || th == 0 || tw > u64::from(MAX_TILE) || th > u64::from(MAX_TILE) {
            bail!("unsupported TIFF tile size {tw}×{th}");
        }
        (tw, th, u64::from(w).div_ceil(tw), u64::from(h).div_ceil(th), tag::TILE_OFFSETS, tag::TILE_BYTE_COUNTS)
    } else {
        let rows = ifd.int(tag::ROWS_PER_STRIP).unwrap_or(u64::from(h)).clamp(1, u64::from(h));
        (u64::from(w), rows, 1, u64::from(h).div_ceil(rows), tag::STRIP_OFFSETS, tag::STRIP_BYTE_COUNTS)
    };
    bytes(&[cw as usize, ch as usize, samples as usize, (bits / 8) as usize], MAX_CHUNK_BYTES)?;
    let needed = across.saturating_mul(down);
    let (n_off, n_cnt) = (ifd.ints(offsets).map_or(0, <[u64]>::len), ifd.ints(counts).map_or(0, <[u64]>::len));
    if (n_off as u64) < needed || (n_cnt as u64) < needed {
        bail!("TIFF lists {n_off} chunk offsets and {n_cnt} sizes for {needed} chunks");
    }
    if ifd.ints(counts).is_some_and(|c| c.iter().any(|n| *n as usize > MAX_CHUNK_BYTES)) {
        bail!("TIFF chunk larger than {MAX_CHUNK_BYTES} bytes");
    }
    Ok(())
}

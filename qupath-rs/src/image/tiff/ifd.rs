//! TIFF and BigTIFF directory (IFD) parsing.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use anyhow::{Context, Result, bail};

/// Tag numbers used by the reader and writer.
#[allow(missing_docs)]
pub mod tag {
    pub const NEW_SUBFILE_TYPE: u16 = 254;
    pub const IMAGE_WIDTH: u16 = 256;
    pub const IMAGE_LENGTH: u16 = 257;
    pub const BITS_PER_SAMPLE: u16 = 258;
    pub const COMPRESSION: u16 = 259;
    pub const PHOTOMETRIC: u16 = 262;
    pub const IMAGE_DESCRIPTION: u16 = 270;
    pub const STRIP_OFFSETS: u16 = 273;
    pub const SAMPLES_PER_PIXEL: u16 = 277;
    pub const ROWS_PER_STRIP: u16 = 278;
    pub const STRIP_BYTE_COUNTS: u16 = 279;
    pub const X_RESOLUTION: u16 = 282;
    pub const Y_RESOLUTION: u16 = 283;
    pub const PLANAR_CONFIG: u16 = 284;
    pub const RESOLUTION_UNIT: u16 = 296;
    pub const SOFTWARE: u16 = 305;
    pub const PREDICTOR: u16 = 317;
    pub const TILE_WIDTH: u16 = 322;
    pub const TILE_LENGTH: u16 = 323;
    pub const TILE_OFFSETS: u16 = 324;
    pub const TILE_BYTE_COUNTS: u16 = 325;
    pub const SUB_IFDS: u16 = 330;
    pub const SAMPLE_FORMAT: u16 = 339;
    pub const JPEG_TABLES: u16 = 347;
    pub const YCBCR_SUBSAMPLING: u16 = 530;
}

/// A tag's values, widened.
#[derive(Clone, Debug)]
pub enum TagValue {
    /// Integer types.
    Ints(Vec<u64>),
    /// Rational and floating point types.
    Floats(Vec<f64>),
    /// ASCII text.
    Ascii(String),
    /// Undefined bytes.
    Bytes(Vec<u8>),
}

/// One image file directory.
#[derive(Clone, Debug, Default)]
pub struct Ifd {
    /// The tags, by number.
    pub tags: HashMap<u16, TagValue>,
}

impl Ifd {
    /// All integer values of a tag.
    pub fn ints(&self, t: u16) -> Option<&[u64]> {
        match self.tags.get(&t)? {
            TagValue::Ints(v) => Some(v),
            _ => None,
        }
    }
    /// The first integer value of a tag.
    pub fn int(&self, t: u16) -> Option<u64> {
        self.ints(t)?.first().copied()
    }
    /// The first value of a rational or float tag.
    pub fn float(&self, t: u16) -> Option<f64> {
        match self.tags.get(&t)? {
            TagValue::Floats(v) => v.first().copied(),
            TagValue::Ints(v) => v.first().map(|x| *x as f64),
            _ => None,
        }
    }
    /// Text of an ASCII tag.
    pub fn ascii(&self, t: u16) -> Option<&str> {
        match self.tags.get(&t)? {
            TagValue::Ascii(s) => Some(s),
            _ => None,
        }
    }
    /// Raw bytes of an UNDEFINED (or BYTE) tag.
    pub fn bytes(&self, t: u16) -> Option<Vec<u8>> {
        match self.tags.get(&t)? {
            TagValue::Bytes(b) => Some(b.clone()),
            TagValue::Ints(v) => Some(v.iter().map(|x| *x as u8).collect()),
            _ => None,
        }
    }
    /// Width.
    pub fn width(&self) -> u32 {
        self.int(tag::IMAGE_WIDTH).unwrap_or(0) as u32
    }
    /// Height.
    pub fn height(&self) -> u32 {
        self.int(tag::IMAGE_LENGTH).unwrap_or(0) as u32
    }
    /// Tiled (rather than stripped).
    pub fn is_tiled(&self) -> bool {
        self.tags.contains_key(&tag::TILE_WIDTH)
    }
}

/// Reads values with the file's byte order.
pub struct TiffReader {
    file: File,
    little: bool,
    /// Whether this is a BigTIFF.
    pub big: bool,
    /// Decoded tag bytes still allowed for this file.
    tag_budget: usize,
}

impl TiffReader {
    /// Opens a file and checks the header; returns the reader and the first IFD offset.
    pub fn open(path: &std::path::Path) -> Result<(Self, u64)> {
        let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
        let mut hdr = [0u8; 16];
        file.read_exact(&mut hdr[..8]).context("file too short for TIFF")?;
        let little = match &hdr[..2] {
            b"II" => true,
            b"MM" => false,
            _ => bail!("not a TIFF file"),
        };
        let mut r = TiffReader { file, little, big: false, tag_budget: super::limits::MAX_FILE_TAG_BYTES };
        let magic = r.u16_from(&hdr[2..4]);
        let first = match magic {
            42 => u64::from(r.u32_from(&hdr[4..8])),
            43 => {
                r.big = true;
                r.file.read_exact(&mut hdr[8..16])?;
                r.u64_from(&hdr[8..16])
            }
            _ => bail!("not a TIFF file (magic {magic})"),
        };
        Ok((r, first))
    }

    fn u16_from(&self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        if self.little { u16::from_le_bytes(a) } else { u16::from_be_bytes(a) }
    }
    fn u32_from(&self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        if self.little { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
    }
    fn u64_from(&self, b: &[u8]) -> u64 {
        let mut a = [0u8; 8];
        a.copy_from_slice(&b[..8]);
        if self.little { u64::from_le_bytes(a) } else { u64::from_be_bytes(a) }
    }

    /// Reads bytes at an offset.
    pub fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Reads the IFD at an offset; returns it and the next IFD's offset (0 at the end).
    pub fn read_ifd(&mut self, offset: u64) -> Result<(Ifd, u64)> {
        let (count_len, entry_len, next_len) = if self.big { (8, 20, 8) } else { (2, 12, 4) };
        let cb = self.read_at(offset, count_len)?;
        let n = if self.big { self.u64_from(&cb) } else { u64::from(self.u16_from(&cb)) };
        if n > 100_000 {
            bail!("corrupt TIFF directory ({n} entries)");
        }
        let body = self.read_at(offset + count_len as u64, n as usize * entry_len + next_len)?;
        let mut ifd = Ifd::default();
        for i in 0..n as usize {
            let e = &body[i * entry_len..(i + 1) * entry_len];
            if let Ok((t, v)) = self.read_entry(e) {
                ifd.tags.insert(t, v);
            }
        }
        let nb = &body[n as usize * entry_len..];
        let next = if self.big { self.u64_from(nb) } else { u64::from(self.u32_from(nb)) };
        Ok((ifd, next))
    }

    fn read_entry(&mut self, e: &[u8]) -> Result<(u16, TagValue)> {
        let t = self.u16_from(&e[0..2]);
        let typ = self.u16_from(&e[2..4]);
        let (count, inline) =
            if self.big { (self.u64_from(&e[4..12]), &e[12..20]) } else { (u64::from(self.u32_from(&e[4..8])), &e[8..12]) };
        let size = type_size(typ).context("unknown TIFF type")?;
        let count = usize::try_from(count).unwrap_or(usize::MAX);
        let total = size.checked_mul(count).context("TIFF tag value too large")?;
        // Numbers are held as 8-byte values whatever their size on disk, so charge
        // what they take in memory, against both this tag and the whole file.
        let held = if matches!(typ, 2 | 7) { total } else { count.saturating_mul(8) };
        if held > super::limits::MAX_TAG_BYTES || held > self.tag_budget {
            bail!("TIFF tag data exceeds the reader's limits");
        }
        self.tag_budget -= held;
        let data = if total <= inline.len() { inline[..total].to_vec() } else { self.read_at(self.offset_of(inline), total)? };
        Ok((t, self.decode_values(typ, count, &data)))
    }

    fn offset_of(&self, inline: &[u8]) -> u64 {
        if self.big { self.u64_from(inline) } else { u64::from(self.u32_from(inline)) }
    }

    fn decode_values(&self, typ: u16, n: usize, d: &[u8]) -> TagValue {
        match typ {
            1 | 6 => TagValue::Ints(d.iter().map(|b| u64::from(*b)).collect()),
            2 => TagValue::Ascii(String::from_utf8_lossy(d).trim_end_matches('\0').to_owned()),
            7 => TagValue::Bytes(d.to_vec()),
            3 | 8 => TagValue::Ints((0..n).map(|i| u64::from(self.u16_from(&d[i * 2..]))).collect()),
            4 | 9 | 13 => TagValue::Ints((0..n).map(|i| u64::from(self.u32_from(&d[i * 4..]))).collect()),
            16..=18 => TagValue::Ints((0..n).map(|i| self.u64_from(&d[i * 8..])).collect()),
            5 | 10 => TagValue::Floats(
                (0..n)
                    .map(|i| {
                        let (a, b) = (self.u32_from(&d[i * 8..]), self.u32_from(&d[i * 8 + 4..]));
                        if b == 0 { 0.0 } else { f64::from(a) / f64::from(b) }
                    })
                    .collect(),
            ),
            11 => TagValue::Floats((0..n).map(|i| f64::from(f32::from_bits(self.u32_from(&d[i * 4..])))).collect()),
            12 => TagValue::Floats((0..n).map(|i| f64::from_bits(self.u64_from(&d[i * 8..]))).collect()),
            _ => TagValue::Bytes(d.to_vec()),
        }
    }
}

/// Bytes per value of a TIFF field type.
pub fn type_size(typ: u16) -> Option<usize> {
    Some(match typ {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => return None,
    })
}

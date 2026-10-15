//! Decompressing TIFF tiles and strips.

use std::io::Read;

use anyhow::{Result, bail};
use jpeg_decoder::{ColorTransform, Decoder};

/// What the reader needs to decode one chunk.
#[derive(Clone, Debug)]
pub struct ChunkFormat {
    /// TIFF compression code.
    pub compression: u16,
    /// TIFF photometric interpretation.
    pub photometric: u16,
    /// Samples per pixel.
    pub samples: usize,
    /// Bits per sample (8, 16 or 32).
    pub bits: u16,
    /// Predictor (1 none, 2 horizontal differencing).
    pub predictor: u16,
    /// Shared JPEG tables, if any.
    pub jpeg_tables: Option<Vec<u8>>,
    /// Whether multi-byte samples are little endian.
    pub little_endian: bool,
}

/// Decompresses one chunk to interleaved samples (bytes; multi-byte samples in file order).
pub fn decompress(raw: &[u8], fmt: &ChunkFormat, width: usize, height: usize) -> Result<Vec<u8>> {
    let expected = width * height * fmt.samples * (usize::from(fmt.bits) / 8);
    let mut out = match fmt.compression {
        1 => raw.to_vec(),
        5 => weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8).decode(raw).or_else(|_| lzw_partial(raw))?,
        8 | 32946 => {
            let mut v = Vec::with_capacity(expected);
            flate2::read::ZlibDecoder::new(raw).read_to_end(&mut v)?;
            v
        }
        32773 => packbits(raw, expected),
        7 | 6 => return decode_jpeg(raw, fmt, expected),
        c => bail!("unsupported TIFF compression {c} (try OpenSlide for this file)"),
    };
    out.resize(expected, 0);
    if fmt.predictor == 2 {
        undo_horizontal_predictor(&mut out, fmt, width);
    }
    Ok(out)
}

/// LZW streams are sometimes truncated without an end code; keep what decoded.
fn lzw_partial(raw: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut dec = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
    let r = dec.into_stream(&mut out).decode_all(raw);
    if out.is_empty() {
        r.status?;
    }
    Ok(out)
}

fn packbits(raw: &[u8], expected: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < raw.len() && out.len() < expected {
        let n = raw[i] as i8;
        i += 1;
        if n >= 0 {
            let len = n as usize + 1;
            out.extend_from_slice(&raw[i..(i + len).min(raw.len())]);
            i += len;
        } else if n != -128 {
            if let Some(b) = raw.get(i) {
                out.extend(std::iter::repeat_n(*b, (1 - n as isize) as usize));
            }
            i += 1;
        }
    }
    out
}

fn undo_horizontal_predictor(buf: &mut [u8], fmt: &ChunkFormat, width: usize) {
    let s = fmt.samples;
    match fmt.bits {
        8 => {
            for row in buf.chunks_mut(width * s) {
                for i in s..row.len() {
                    row[i] = row[i].wrapping_add(row[i - s]);
                }
            }
        }
        16 => {
            let rd = |b: &[u8]| if fmt.little_endian { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
            for row in buf.chunks_mut(width * s * 2) {
                for i in s..row.len() / 2 {
                    let v = rd(&row[i * 2..]).wrapping_add(rd(&row[(i - s) * 2..]));
                    let b = if fmt.little_endian { v.to_le_bytes() } else { v.to_be_bytes() };
                    row[i * 2..i * 2 + 2].copy_from_slice(&b);
                }
            }
        }
        _ => {}
    }
}

/// Joins shared tables with an abbreviated stream: tables without EOI, data without SOI.
fn with_tables(raw: &[u8], tables: Option<&[u8]>) -> Vec<u8> {
    match tables {
        Some(t) if t.len() > 4 && raw.len() > 2 => {
            let mut v = Vec::with_capacity(t.len() + raw.len());
            v.extend_from_slice(&t[..t.len() - 2]);
            v.extend_from_slice(&raw[2..]);
            v
        }
        _ => raw.to_vec(),
    }
}

/// Markers that tell decoders the color space: (JFIF APP0, Adobe APP14).
fn color_markers(data: &[u8]) -> (bool, bool) {
    let (mut jfif, mut adobe) = (false, false);
    let mut i = 2;
    while i + 4 <= data.len() && data[i] == 0xFF {
        let m = data[i + 1];
        if m == 0xDA {
            break;
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        let seg = &data[(i + 4).min(data.len())..(i + 2 + len).min(data.len())];
        jfif |= m == 0xE0 && seg.starts_with(b"JFIF");
        adobe |= m == 0xEE && seg.starts_with(b"Adobe");
        i += 2 + len;
    }
    (jfif, adobe)
}

fn decode_jpeg(raw: &[u8], fmt: &ChunkFormat, expected: usize) -> Result<Vec<u8>> {
    let data = with_tables(raw, fmt.jpeg_tables.as_deref());
    let mut dec = Decoder::new(data.as_slice());
    let (jfif, adobe) = color_markers(&data);
    // Without JFIF or Adobe markers, the TIFF photometric says what the samples are:
    // Aperio and others store unconverted RGB with photometric RGB.
    if !jfif && !adobe && fmt.samples == 3 {
        dec.set_color_transform(if fmt.photometric == 6 { ColorTransform::YCbCr } else { ColorTransform::RGB });
    }
    let mut px = dec.decode()?;
    px.resize(expected, 0);
    Ok(px)
}

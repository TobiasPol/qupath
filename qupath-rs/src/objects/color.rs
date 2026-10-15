//! Packed colors, compatible with QuPath's Java `int` ARGB colors.

use serde::{Deserialize, Serialize};

/// An opaque RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// QuPath stores colors as Java ints: `0xAARRGGBB`, so opaque colors are negative.
    pub fn to_packed(self) -> i32 {
        let v = 0xFF00_0000u32 | (u32::from(self.0) << 16) | (u32::from(self.1) << 8) | u32::from(self.2);
        v as i32
    }

    /// Reads a Java packed color, ignoring alpha.
    pub fn from_packed(v: i32) -> Self {
        let v = v as u32;
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Scales each component, clamped to 0..=255 (QuPath's `ColorTools.makeScaledRGB`).
    pub fn scaled(self, factor: f64) -> Self {
        let s = |c: u8| (f64::from(c) * factor).round().clamp(0.0, 255.0) as u8;
        Rgb(s(self.0), s(self.1), s(self.2))
    }

    /// `#RRGGBB`.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// The color egui draws with.
    pub fn to_color32(self) -> egui::Color32 {
        egui::Color32::from_rgb(self.0, self.1, self.2)
    }

    /// Reads an egui color, dropping alpha.
    pub fn from_color32(c: egui::Color32) -> Self {
        Rgb(c.r(), c.g(), c.b())
    }

    /// A stable, reasonably saturated color derived from a name.
    pub fn from_name_hash(name: &str) -> Self {
        // FNV-1a, then pick a hue so the same class always gets the same color.
        let mut h: u32 = 0x811c_9dc5;
        for b in name.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(0x0100_0193);
        }
        let hue = f64::from(h % 360);
        hsv_to_rgb(hue, 0.65, 0.85)
    }
}

/// HSV (hue in degrees, saturation and value in 0..1) to RGB.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let hp = (h / 60.0) % 6.0;
    let x = c * (1.0 - ((hp % 2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f64| ((f + m) * 255.0).round() as u8;
    Rgb(to(r), to(g), to(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_roundtrip_matches_java() {
        // QuPath's Tumor class: ColorTools.packRGB(200, 0, 0) == -3670016
        assert_eq!(Rgb(200, 0, 0).to_packed(), -3_670_016);
        assert_eq!(Rgb::from_packed(-3_670_016), Rgb(200, 0, 0));
    }
}

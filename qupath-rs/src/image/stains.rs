//! Image types and color deconvolution (Ruifrok & Johnston), as in QuPath's `ColorDeconvolutionStains`.

use serde::{Deserialize, Serialize};

/// QuPath's image types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ImageType {
    /// Hematoxylin & eosin.
    #[serde(rename = "BRIGHTFIELD_H_E")]
    BrightfieldHE,
    /// Hematoxylin & DAB.
    #[serde(rename = "BRIGHTFIELD_H_DAB")]
    BrightfieldHDab,
    /// Other brightfield stains.
    #[serde(rename = "BRIGHTFIELD_OTHER")]
    BrightfieldOther,
    /// Fluorescence.
    #[serde(rename = "FLUORESCENCE")]
    Fluorescence,
    /// Anything else.
    #[serde(rename = "OTHER")]
    Other,
    /// Not set yet.
    #[default]
    #[serde(rename = "UNSET")]
    Unset,
}

impl ImageType {
    /// Every type, in QuPath's menu order.
    pub const ALL: [ImageType; 6] = [
        ImageType::BrightfieldHE,
        ImageType::BrightfieldHDab,
        ImageType::BrightfieldOther,
        ImageType::Fluorescence,
        ImageType::Other,
        ImageType::Unset,
    ];

    /// The label QuPath shows.
    pub fn label(self) -> &'static str {
        match self {
            ImageType::BrightfieldHE => "Brightfield (H&E)",
            ImageType::BrightfieldHDab => "Brightfield (H-DAB)",
            ImageType::BrightfieldOther => "Brightfield (other)",
            ImageType::Fluorescence => "Fluorescence",
            ImageType::Other => "Other",
            ImageType::Unset => "Not set",
        }
    }

    /// The name in scripts, e.g. `setImageType('BRIGHTFIELD_H_DAB')`.
    pub fn script_name(self) -> &'static str {
        match self {
            ImageType::BrightfieldHE => "BRIGHTFIELD_H_E",
            ImageType::BrightfieldHDab => "BRIGHTFIELD_H_DAB",
            ImageType::BrightfieldOther => "BRIGHTFIELD_OTHER",
            ImageType::Fluorescence => "FLUORESCENCE",
            ImageType::Other => "OTHER",
            ImageType::Unset => "UNSET",
        }
    }

    /// Brightfield images get stain vectors.
    pub fn is_brightfield(self) -> bool {
        matches!(self, ImageType::BrightfieldHE | ImageType::BrightfieldHDab | ImageType::BrightfieldOther)
    }
}

/// A stain's name and its optical density vector (normalized).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StainVector {
    /// Name, e.g. "Hematoxylin".
    pub name: String,
    /// Red, green and blue optical densities.
    pub od: [f64; 3],
}

impl StainVector {
    /// A named stain, normalized.
    pub fn new(name: &str, od: [f64; 3]) -> Self {
        let n = (od[0] * od[0] + od[1] * od[1] + od[2] * od[2]).sqrt().max(1e-12);
        StainVector { name: name.into(), od: [od[0] / n, od[1] / n, od[2] / n] }
    }
}

/// Three stains plus the background (white) values, and the inverse matrix used to separate them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stains {
    /// Name, e.g. "H-DAB default".
    pub name: String,
    /// The stains; the third is the residual when only two are given.
    pub stains: [StainVector; 3],
    /// Background RGB values (usually 255).
    pub max_rgb: [f64; 3],
    #[serde(skip)]
    inverse: Option<[[f64; 3]; 3]>,
}

/// QuPath's default hematoxylin vector.
pub const HEMATOXYLIN: [f64; 3] = [0.65, 0.70, 0.29];
/// QuPath's default DAB vector.
pub const DAB: [f64; 3] = [0.27, 0.57, 0.78];
/// QuPath's default eosin vector.
pub const EOSIN: [f64; 3] = [0.2159, 0.8012, 0.5581];

impl Stains {
    /// Two stains and the residual: their cross product, as QuPath's `makeResidualStainVector`
    /// (taking absolute values would make it nearly parallel to hematoxylin).
    pub fn from_two(name: &str, a: StainVector, b: StainVector) -> Self {
        let (u, v) = (a.od, b.od);
        let r = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let mut s = Stains { name: name.into(), stains: [a, b, StainVector::new("Residual", r)], max_rgb: [255.0; 3], inverse: None };
        s.update_inverse();
        s
    }

    /// The defaults for an image type, if it is brightfield.
    pub fn default_for(t: ImageType) -> Option<Self> {
        let h = StainVector::new("Hematoxylin", HEMATOXYLIN);
        match t {
            ImageType::BrightfieldHE => Some(Self::from_two("H&E default", h, StainVector::new("Eosin", EOSIN))),
            ImageType::BrightfieldHDab | ImageType::BrightfieldOther => {
                Some(Self::from_two("H-DAB default", h, StainVector::new("DAB", DAB)))
            }
            _ => None,
        }
    }

    /// Recomputes the matrix after stains or background change.
    pub fn update_inverse(&mut self) {
        let m = [self.stains[0].od, self.stains[1].od, self.stains[2].od];
        self.inverse = invert3(m);
    }

    /// Optical density of an 8-bit value against the background.
    #[inline]
    pub fn od(&self, value: f32, channel: usize) -> f64 {
        let max = self.max_rgb[channel];
        let v = f64::from(value).clamp(1.0, max);
        -(v / max).log10()
    }

    /// Deconvolved stain amounts for one RGB pixel.
    #[inline]
    pub fn deconvolve(&self, rgb: [f32; 3]) -> [f64; 3] {
        let od = [self.od(rgb[0], 0), self.od(rgb[1], 1), self.od(rgb[2], 2)];
        let Some(inv) = self.inverse.or_else(|| invert3([self.stains[0].od, self.stains[1].od, self.stains[2].od])) else {
            return [0.0; 3];
        };
        let mut out = [0.0; 3];
        for (s, o) in out.iter_mut().enumerate() {
            *o = od[0] * inv[0][s] + od[1] * inv[1][s] + od[2] * inv[2][s];
        }
        out
    }

    /// Sum of the RGB optical densities.
    #[inline]
    pub fn od_sum(&self, rgb: [f32; 3]) -> f64 {
        self.od(rgb[0], 0) + self.od(rgb[1], 1) + self.od(rgb[2], 2)
    }

    /// QuPath's script text for these stains.
    pub fn script(&self) -> String {
        let v = |s: &StainVector| format!("\"{}\", \"values\": \"{:.5} {:.5} {:.5}\"", s.name, s.od[0], s.od[1], s.od[2]);
        format!(
            "setColorDeconvolutionStains('{{\"Name\" : \"{}\", \"Stain 1\" : {}, \"Stain 2\" : {}, \"Background\" : \" {:.0} {:.0} {:.0}\"}}');",
            self.name,
            v(&self.stains[0]),
            v(&self.stains[1]),
            self.max_rgb[0],
            self.max_rgb[1],
            self.max_rgb[2]
        )
    }
}

/// Inverse of a 3×3 matrix whose rows are stain vectors (`od = amounts · M`).
fn invert3(m: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let c = |r1: usize, c1: usize, r2: usize, c2: usize| m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1];
    let adj = [
        [c(1, 1, 2, 2), -c(0, 1, 2, 2), c(0, 1, 1, 2)],
        [-c(1, 0, 2, 2), c(0, 0, 2, 2), -c(0, 0, 1, 2)],
        [c(1, 0, 2, 1), -c(0, 0, 2, 1), c(0, 0, 1, 1)],
    ];
    Some(adj.map(|row| row.map(|v| v / det)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_hematoxylin_deconvolves_to_first_stain() {
        let s = Stains::default_for(ImageType::BrightfieldHDab).expect("brightfield");
        let h = &s.stains[0].od;
        // A pixel with OD = 0.5 * hematoxylin.
        let rgb = h.map(|o| (255.0 * 10f64.powf(-0.5 * o)) as f32);
        let d = s.deconvolve(rgb);
        assert!((d[0] - 0.5).abs() < 0.02, "{d:?}");
        assert!(d[1].abs() < 0.02, "{d:?}");
    }
}

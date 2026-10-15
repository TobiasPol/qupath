//! Cell measurements, named as QuPath names them.

use geo::{Area, ConvexHull};

use super::Source;
use crate::image::Tile;
use crate::objects::{Calibration, PathObject, Roi};

/// Per-pixel stain (or channel) values, for intensity measurements.
pub struct Channels {
    /// Channel names, e.g. "Hematoxylin OD", "DAB OD".
    pub names: Vec<String>,
    /// Values, one image per channel.
    pub data: Vec<Vec<f32>>,
}

impl Channels {
    /// Empty channels for an image of this size.
    pub fn new(w: usize, h: usize, src: &Source) -> Self {
        let names: Vec<String> = match &src.stains {
            Some(s) => s.stains[..2].iter().map(|v| format!("{} OD", v.name)).collect(),
            None => src.server.metadata().channels.iter().map(|c| c.name.clone()).collect(),
        };
        let data = vec![vec![0.0; w * h]; names.len()];
        Channels { names, data }
    }

    /// Stores the values at one pixel and returns them.
    pub fn fill(&mut self, src: &Source, t: &Tile, x: u32, y: u32, i: usize) -> Vec<f32> {
        let vals: Vec<f32> = match &src.stains {
            Some(s) => {
                let d = s.deconvolve([t.get(x, y, 0), t.get(x, y, 1), t.get(x, y, 2)]);
                vec![d[0] as f32, d[1] as f32]
            }
            None => (0..self.names.len()).map(|c| t.get(x, y, c.min(t.channels - 1))).collect(),
        };
        for (c, v) in vals.iter().enumerate() {
            self.data[c][i] = *v;
        }
        vals
    }
}

struct Stats {
    mean: f64,
    sum: f64,
    std: f64,
    min: f64,
    max: f64,
}

fn stats(values: &[f32], pixels: &[usize]) -> Stats {
    let n = pixels.len().max(1) as f64;
    let (mut sum, mut sq, mut min, mut max) = (0.0, 0.0, f64::MAX, f64::MIN);
    for &i in pixels {
        let v = f64::from(values[i]);
        sum += v;
        sq += v * v;
        min = min.min(v);
        max = max.max(v);
    }
    let mean = sum / n;
    Stats { mean, sum, std: (sq / n - mean * mean).max(0.0).sqrt(), min, max }
}

fn shape(obj: &mut PathObject, prefix: &str, roi: &Roi, cal: &Calibration) {
    let u = cal.unit();
    let area = roi.area(cal.pixel_width, cal.pixel_height);
    let perimeter = roi.length(cal.pixel_width, cal.pixel_height);
    let m = &mut obj.measurements;
    m.insert(format!("{prefix}: Area {u}^2"), area);
    m.insert(format!("{prefix}: Length {u}"), perimeter);
    let circ = if perimeter > 0.0 { (4.0 * std::f64::consts::PI * area / (perimeter * perimeter)).min(1.0) } else { 0.0 };
    m.insert(format!("{prefix}: Circularity"), circ);
    if let Some(mp) = roi.to_multipolygon() {
        let hull = mp.convex_hull().unsigned_area();
        m.insert(format!("{prefix}: Solidity"), if hull > 0.0 { mp.unsigned_area() / hull } else { 0.0 });
    }
}

/// Adds shape and intensity measurements for nucleus, cell and cytoplasm.
pub fn measure_cell(obj: &mut PathObject, ch: &Channels, nucleus: &[usize], cell: &[usize], cal: &Calibration) {
    if let Some(n) = obj.nucleus.clone() {
        shape(obj, "Nucleus", &n, cal);
    }
    for (c, name) in ch.names.iter().enumerate() {
        let s = stats(&ch.data[c], nucleus);
        let m = &mut obj.measurements;
        m.insert(format!("Nucleus: {name} mean"), s.mean);
        m.insert(format!("Nucleus: {name} sum"), s.sum);
        m.insert(format!("Nucleus: {name} std dev"), s.std);
        m.insert(format!("Nucleus: {name} max"), s.max);
        m.insert(format!("Nucleus: {name} min"), s.min);
        m.insert(format!("Nucleus: {name} range"), s.max - s.min);
    }
    let roi = obj.roi.clone();
    shape(obj, "Cell", &roi, cal);
    let nuc: std::collections::HashSet<usize> = nucleus.iter().copied().collect();
    let cyto: Vec<usize> = cell.iter().copied().filter(|i| !nuc.contains(i)).collect();
    for (c, name) in ch.names.iter().enumerate() {
        let s = stats(&ch.data[c], cell);
        obj.measurements.insert(format!("Cell: {name} mean"), s.mean);
        obj.measurements.insert(format!("Cell: {name} std dev"), s.std);
        obj.measurements.insert(format!("Cell: {name} max"), s.max);
        obj.measurements.insert(format!("Cell: {name} min"), s.min);
        let cy = stats(&ch.data[c], &cyto);
        obj.measurements.insert(format!("Cytoplasm: {name} mean"), if cyto.is_empty() { f64::NAN } else { cy.mean });
    }
    let ratio = if cell.is_empty() { f64::NAN } else { nucleus.len() as f64 / cell.len() as f64 };
    obj.measurements.insert("Nucleus/Cell area ratio".into(), ratio);
}

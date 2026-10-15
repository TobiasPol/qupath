//! Measurements shown in tables, computed the way QuPath names them.

use indexmap::IndexMap;

use super::classes::{self, PathClass};
use super::hierarchy::Hierarchy;
use super::object::{ObjKey, ObjectKind};

/// Pixel size, for calibrated measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Calibration {
    /// Width of a pixel (µm when calibrated).
    pub pixel_width: f64,
    /// Height of a pixel (µm when calibrated).
    pub pixel_height: f64,
    /// Whether the size is in µm (else pixels).
    pub calibrated: bool,
}

impl Default for Calibration {
    fn default() -> Self {
        Calibration { pixel_width: 1.0, pixel_height: 1.0, calibrated: false }
    }
}

impl Calibration {
    /// From an optional pixel size in µm.
    pub fn from_microns(pw: Option<f64>, ph: Option<f64>) -> Self {
        match (pw, ph) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Calibration { pixel_width: w, pixel_height: h, calibrated: true },
            _ => Calibration::default(),
        }
    }

    /// `µm` or `px`.
    pub fn unit(&self) -> &'static str {
        if self.calibrated { "µm" } else { "px" }
    }

    /// Average of width and height.
    pub fn average(&self) -> f64 {
        (self.pixel_width + self.pixel_height) / 2.0
    }
}

/// A table cell: text or a number.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Text.
    Text(String),
    /// A number (NaN when missing).
    Number(f64),
}

impl Value {
    /// The number, if any.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(v) => Some(*v),
            Value::Text(_) => None,
        }
    }

    /// Text for tables and exports.
    pub fn format(&self, decimals: usize) -> String {
        match self {
            Value::Text(s) => s.clone(),
            Value::Number(v) if v.is_nan() => "NaN".into(),
            Value::Number(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{v:.0}"),
            Value::Number(v) => format!("{v:.decimals$}"),
        }
    }
}

/// Every measurement of one object, keyed by column name.
pub fn object_row(h: &Hierarchy, k: ObjKey, cal: &Calibration) -> IndexMap<String, Value> {
    let mut row = IndexMap::new();
    let Some(o) = h.get(k) else { return row };
    let text = |s: &str| Value::Text(s.to_owned());
    row.insert("Object ID".into(), text(&o.id.to_string()));
    row.insert("Object type".into(), text(o.kind.display_name()));
    row.insert("Name".into(), text(o.name.as_deref().unwrap_or("")));
    row.insert("Classification".into(), text(o.class.as_ref().map_or("", PathClass::name)));
    let parent = o.parent().and_then(|p| h.get(p)).map_or("".into(), parent_label);
    row.insert("Parent".into(), Value::Text(parent));
    row.insert("ROI".into(), text(o.roi.type_name()));
    let unit = cal.unit();
    let c = o.roi.centroid();
    row.insert(format!("Centroid X {unit}"), Value::Number(c.x * cal.pixel_width));
    row.insert(format!("Centroid Y {unit}"), Value::Number(c.y * cal.pixel_height));
    if o.kind != ObjectKind::Cell {
        shape_measurements(&o.roi, cal, &mut row);
    }
    if !o.is_detection() {
        count_measurements(h, k, cal, &mut row);
    }
    for (name, v) in &o.measurements {
        row.insert(name.clone(), Value::Number(*v));
    }
    row
}

fn parent_label(p: &super::object::PathObject) -> String {
    if p.kind == ObjectKind::Root { "Image".into() } else { p.label() }
}

/// Area/perimeter, length or point count.
fn shape_measurements(roi: &super::roi::Roi, cal: &Calibration, row: &mut IndexMap<String, Value>) {
    let unit = cal.unit();
    let (pw, ph) = (cal.pixel_width, cal.pixel_height);
    if roi.is_area() {
        row.insert(format!("Area {unit}^2"), Value::Number(roi.area(pw, ph)));
        row.insert(format!("Perimeter {unit}"), Value::Number(roi.length(pw, ph)));
    } else if roi.is_line() {
        row.insert(format!("Length {unit}"), Value::Number(roi.length(pw, ph)));
    } else if roi.is_points() {
        row.insert("Num points".into(), Value::Number(roi.num_points() as f64));
    }
}

/// Detection counts, per class, and positive percentages for an annotation (or the image).
fn count_measurements(h: &Hierarchy, k: ObjKey, cal: &Calibration, row: &mut IndexMap<String, Value>) {
    let mut total = 0usize;
    let mut per_class: IndexMap<String, usize> = IndexMap::new();
    let (mut pos, mut neg) = (0usize, 0usize);
    for d in h.descendants(k) {
        let Some(o) = h.get(d) else { continue };
        if !o.is_detection() || o.kind == ObjectKind::Tile {
            continue;
        }
        total += 1;
        if let Some(c) = &o.class {
            if c.is_ignored() {
                continue;
            }
            *per_class.entry(c.name().to_owned()).or_default() += 1;
            for n in c.names().iter().skip(1) {
                *per_class.entry((*n).to_owned()).or_default() += 0;
            }
            pos += usize::from(c.is_positive());
            neg += usize::from(c.is_negative());
        }
    }
    row.insert("Num Detections".into(), Value::Number(total as f64));
    for (name, n) in per_class.iter().filter(|(_, n)| **n > 0) {
        row.insert(format!("Num {name}"), Value::Number(*n as f64));
    }
    if pos + neg > 0 {
        let classified = (pos + neg) as f64;
        row.insert(format!("Num {}", classes::POSITIVE), Value::Number(pos as f64));
        row.insert(format!("Num {}", classes::NEGATIVE), Value::Number(neg as f64));
        row.insert("Positive %".into(), Value::Number(pos as f64 / classified * 100.0));
        if let Some(Value::Number(area)) = row.get(&format!("Area {}^2", cal.unit())).cloned() {
            let mm2 = if cal.calibrated { area / 1e6 } else { f64::NAN };
            row.insert("Num Positive per mm^2".into(), Value::Number(pos as f64 / mm2));
        }
    }
}

/// Rows for many objects, with the union of their columns (in first-seen order).
pub fn table(h: &Hierarchy, keys: &[ObjKey], cal: &Calibration) -> (Vec<String>, Vec<IndexMap<String, Value>>) {
    let rows: Vec<_> = keys.iter().map(|k| object_row(h, *k, cal)).collect();
    let mut cols: indexmap::IndexSet<String> = indexmap::IndexSet::new();
    for r in &rows {
        for c in r.keys() {
            if !cols.contains(c) {
                cols.insert(c.clone());
            }
        }
    }
    (cols.into_iter().collect(), rows)
}

/// Tab-separated text, as QuPath's measurement export writes it.
pub fn to_tsv(cols: &[String], rows: &[IndexMap<String, Value>], separator: char) -> String {
    let mut out = String::new();
    let sep = separator.to_string();
    out.push_str(&cols.join(&sep));
    out.push('\n');
    for r in rows {
        let cells: Vec<String> = cols.iter().map(|c| r.get(c).map_or_else(|| "NaN".into(), |v| escape(&v.format(4), separator))).collect();
        out.push_str(&cells.join(&sep));
        out.push('\n');
    }
    out
}

fn escape(s: &str, separator: char) -> String {
    if s.contains(separator) || s.contains('"') || s.contains('\n') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_owned() }
}

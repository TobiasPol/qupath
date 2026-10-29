//! Reading tool arguments (JSON objects) with errors an agent can act on.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::objects::{Pt, Rgb, Roi};

/// A required string.
pub fn text<'a>(a: &'a Value, key: &str) -> Result<&'a str> {
    a.get(key).and_then(Value::as_str).with_context(|| format!("missing string argument `{key}`"))
}

/// An optional string.
pub fn opt_str<'a>(a: &'a Value, key: &str) -> Option<&'a str> {
    a.get(key).and_then(Value::as_str)
}

/// An optional number.
pub fn opt_f64(a: &Value, key: &str) -> Option<f64> {
    a.get(key).and_then(Value::as_f64)
}

/// A number with a default.
pub fn f64_or(a: &Value, key: &str, default: f64) -> f64 {
    opt_f64(a, key).unwrap_or(default)
}

/// A required number.
pub fn num(a: &Value, key: &str) -> Result<f64> {
    opt_f64(a, key).with_context(|| format!("missing number argument `{key}`"))
}

/// A flag with a default.
pub fn bool_or(a: &Value, key: &str, default: bool) -> bool {
    a.get(key).and_then(Value::as_bool).unwrap_or(default)
}

/// A count with a default.
pub fn usize_or(a: &Value, key: &str, default: usize) -> usize {
    a.get(key).and_then(Value::as_u64).map_or(default, |v| v as usize)
}

/// An optional list of strings (a single string counts as a list of one).
pub fn opt_strings(a: &Value, key: &str) -> Option<Vec<String>> {
    match a.get(key)? {
        Value::String(s) => Some(vec![s.clone()]),
        Value::Array(v) => Some(v.iter().filter_map(Value::as_str).map(str::to_owned).collect()),
        _ => None,
    }
}

/// `#RRGGBB` or `[r, g, b]`.
pub fn color(v: &Value) -> Result<Rgb> {
    match v {
        Value::String(s) if s.len() == 7 && s.starts_with('#') => {
            let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).context("bad hex color");
            Ok(Rgb(c(1)?, c(3)?, c(5)?))
        }
        Value::Array(a) if a.len() == 3 => {
            let c = |i: usize| a[i].as_u64().map(|x| x.min(255) as u8).context("bad color component");
            Ok(Rgb(c(0)?, c(1)?, c(2)?))
        }
        _ => bail!("colors are \"#RRGGBB\" or [r, g, b]"),
    }
}

fn points(v: &Value, scale: (f64, f64)) -> Result<Vec<Pt>> {
    let arr = v.get("points").and_then(Value::as_array).context("this shape needs `points`: [[x, y], …]")?;
    arr.iter()
        .map(|p| {
            let x = p.get(0).and_then(Value::as_f64).context("points are [x, y] pairs")?;
            let y = p.get(1).and_then(Value::as_f64).context("points are [x, y] pairs")?;
            Ok(Pt { x: x * scale.0, y: y * scale.1 })
        })
        .collect()
}

/// A ROI from a shape description. `scale` converts the shape's units to pixels.
///
/// Shapes: `{"type":"rectangle"|"ellipse", x, y, width, height}`, `{"type":"line", x1, y1, x2, y2}`,
/// `{"type":"polygon"|"polyline"|"points", points:[[x,y],…]}`, or a GeoJSON geometry (pixels).
pub fn shape(v: &Value, scale: (f64, f64)) -> Result<Roi> {
    let kind = text(v, "type")?.to_ascii_lowercase();
    let (sx, sy) = scale;
    let n = |k: &str| num(v, k);
    let roi = match kind.as_str() {
        _ if v.get("coordinates").is_some() => crate::io::geojson::geometry_to_roi(v)?,
        "rectangle" | "ellipse" => {
            let (x, y, w, h) = (n("x")? * sx, n("y")? * sy, n("width")? * sx, n("height")? * sy);
            if w <= 0.0 || h <= 0.0 {
                bail!("width and height must be positive");
            }
            if kind == "rectangle" { Roi::Rectangle { x, y, w, h } } else { Roi::Ellipse { x, y, w, h } }
        }
        "line" => Roi::Line { x1: n("x1")? * sx, y1: n("y1")? * sy, x2: n("x2")? * sx, y2: n("y2")? * sy },
        "polygon" => Roi::Polygon(points(v, scale)?),
        "polyline" => Roi::Polyline(points(v, scale)?),
        "points" => Roi::Points(points(v, scale)?),
        other => bail!("unknown shape type `{other}`: use rectangle, ellipse, line, polygon, polyline, points or a GeoJSON geometry"),
    };
    if matches!(&roi, Roi::Polygon(p) if p.len() < 3) {
        bail!("a polygon needs at least 3 points");
    }
    Ok(roi)
}

/// `[x, y, width, height]` of a ROI's bounding box, rounded for display.
pub fn bounds_json(roi: &Roi) -> Value {
    let b = roi.bounds();
    let r = |v: f64| (v * 10.0).round() / 10.0;
    json!([r(b.x0), r(b.y0), r(b.width()), r(b.height())])
}

/// Rounds for compact output.
pub fn round(v: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (v * f).round() / f
}

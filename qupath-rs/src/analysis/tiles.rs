//! Analyze → Tiles & superpixels → Create tiles.

use geo::BooleanOps;

use crate::objects::{PathClass, PathObject, Roi};

/// Tile parameters (µm when calibrated, else pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct TileParams {
    /// Tile width.
    pub width: f64,
    /// Tile height.
    pub height: f64,
    /// Clip tiles to the parent shape.
    pub trim_to_roi: bool,
    /// Create annotations instead of tile objects.
    pub make_annotations: bool,
}

impl Default for TileParams {
    fn default() -> Self {
        TileParams { width: 100.0, height: 100.0, trim_to_roi: true, make_annotations: false }
    }
}

impl TileParams {
    /// The equivalent QuPath script line.
    pub fn script(&self) -> String {
        let params = serde_json::json!({
            "tileSizeMicrons": self.width,
            "trimToROI": self.trim_to_roi,
            "makeAnnotations": self.make_annotations,
            "removeParentAnnotation": false,
        });
        crate::io::script::run_plugin("qupath.lib.algorithms.TilerPlugin", &params)
    }
}

/// Tiles covering a ROI. `px_w`/`px_h` convert the parameters to pixels.
pub fn create_tiles(roi: &Roi, p: &TileParams, px_w: f64, px_h: f64) -> Vec<PathObject> {
    let (tw, th) = (p.width / px_w, p.height / px_h);
    if tw < 1.0 || th < 1.0 {
        return vec![];
    }
    let b = roi.bounds();
    let geom = roi.to_multipolygon();
    let mut out = Vec::new();
    let mut y = b.y0;
    while y < b.y1 {
        let mut x = b.x0;
        while x < b.x1 {
            let rect = Roi::Rectangle { x, y, w: tw, h: th };
            let tile = match (&geom, p.trim_to_roi) {
                (Some(g), true) => rect.to_multipolygon().and_then(|r| Roi::from_multipolygon(r.intersection(g))),
                (Some(_), false) if !roi.contains(rect.centroid()) => None,
                _ => Some(rect),
            };
            if let Some(t) = tile.filter(|t| t.area(1.0, 1.0) > 1.0) {
                let mut o = if p.make_annotations { PathObject::annotation(t, None) } else { PathObject::tile(t) };
                o.class = None::<PathClass>;
                out.push(o);
            }
            x += tw;
        }
        y += th;
    }
    out
}

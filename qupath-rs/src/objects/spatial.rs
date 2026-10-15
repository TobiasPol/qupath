//! An R-tree over object bounds, rebuilt when the hierarchy changes.

use rstar::{AABB, RTree, RTreeObject};

use super::hierarchy::Hierarchy;
use super::object::ObjKey;
use super::roi::{Bounds, Pt};

#[derive(Clone, Debug)]
struct Entry {
    key: ObjKey,
    envelope: AABB<[f64; 2]>,
    area: f64,
    detection: bool,
}

impl RTreeObject for Entry {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Finds objects by location quickly.
#[derive(Default)]
pub struct SpatialIndex {
    tree: RTree<Entry>,
    version: Option<u64>,
}

impl SpatialIndex {
    /// Rebuilds if the hierarchy changed since the last call.
    pub fn update(&mut self, h: &Hierarchy) {
        if self.version == Some(h.version()) {
            return;
        }
        let entries = h
            .iter()
            .map(|(key, o)| {
                let b = o.roi.bounds();
                Entry {
                    key,
                    envelope: AABB::from_corners([b.x0, b.y0], [b.x1, b.y1]),
                    area: o.roi.area(1.0, 1.0),
                    detection: o.is_detection(),
                }
            })
            .collect();
        self.tree = RTree::bulk_load(entries);
        self.version = Some(h.version());
    }

    /// Objects whose bounds intersect a region.
    pub fn query(&self, b: &Bounds) -> impl Iterator<Item = ObjKey> + '_ {
        let env = AABB::from_corners([b.x0, b.y0], [b.x1, b.y1]);
        self.tree.locate_in_envelope_intersecting(env).map(|e| e.key)
    }

    /// The object under a point that QuPath would select: the smallest one
    /// that contains it (detections only when `detections` is set), or a
    /// line/point within `tolerance` pixels.
    pub fn hit(&self, h: &Hierarchy, p: Pt, tolerance: f64, detections: bool) -> Option<ObjKey> {
        let env = AABB::from_corners([p.x - tolerance, p.y - tolerance], [p.x + tolerance, p.y + tolerance]);
        let mut best: Option<(ObjKey, f64)> = None;
        for e in self.tree.locate_in_envelope_intersecting(env) {
            if e.detection && !detections {
                continue;
            }
            let Some(o) = h.get(e.key) else { continue };
            let hit = if o.roi.is_area() { o.roi.contains(p) } else { near_outline(&o.roi.outline(), p, tolerance) };
            // Prefer lines and points (area 0), then the smallest area.
            if hit && best.is_none_or(|(_, a)| e.area < a) {
                best = Some((e.key, e.area));
            }
        }
        best.map(|(k, _)| k)
    }
}

fn near_outline(pts: &[Pt], p: Pt, tol: f64) -> bool {
    if pts.len() == 1 {
        return (pts[0].x - p.x).hypot(pts[0].y - p.y) <= tol;
    }
    pts.windows(2).any(|w| segment_distance(w[0], w[1], p) <= tol) || pts.iter().any(|q| (q.x - p.x).hypot(q.y - p.y) <= tol)
}

/// Distance from a point to a segment.
pub fn segment_distance(a: Pt, b: Pt, p: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    (a.x + t * dx - p.x).hypot(a.y + t * dy - p.y)
}

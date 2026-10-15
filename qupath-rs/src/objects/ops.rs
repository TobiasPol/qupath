//! Geometry operations behind the Objects → Annotations menu.

use geo::{BooleanOps, Buffer, MultiPolygon, Simplify};

use super::hierarchy::Hierarchy;
use super::object::{ObjKey, PathObject};
use super::roi::Roi;

/// Union of area ROIs.
pub fn union_all<'a>(rois: impl IntoIterator<Item = &'a Roi>) -> Option<MultiPolygon<f64>> {
    let mut acc: Option<MultiPolygon<f64>> = None;
    for r in rois {
        let Some(mp) = r.to_multipolygon() else { continue };
        acc = Some(match acc {
            None => mp,
            Some(a) => a.union(&mp),
        });
    }
    acc
}

/// Merges the selected annotations into one (QuPath: "Merge selected").
pub fn merge_selected(h: &mut Hierarchy) -> Option<ObjKey> {
    let sel: Vec<ObjKey> = h.selected().into_iter().filter(|k| h.get(*k).is_some_and(PathObject::is_annotation)).collect();
    if sel.len() < 2 {
        return None;
    }
    let first = h.get(sel[0])?.clone();
    let rois: Vec<Roi> = sel.iter().filter_map(|k| h.get(*k).map(|o| o.roi.clone())).collect();
    let roi = Roi::from_multipolygon(union_all(&rois)?)?;
    h.remove_many(&sel, true);
    let merged = PathObject { roi, measurements: Default::default(), ..first };
    let k = h.insert(merged);
    h.select(Some(k));
    Some(k)
}

/// Subtracts the other selected annotations from the main selection.
pub fn subtract_from_primary(h: &mut Hierarchy) -> Option<ObjKey> {
    let primary = h.primary_selected()?;
    let others: Vec<Roi> = h
        .selected()
        .into_iter()
        .filter(|k| *k != primary)
        .filter_map(|k| h.get(k).filter(|o| o.is_annotation()).map(|o| o.roi.clone()))
        .collect();
    let base = h.get(primary)?.roi.to_multipolygon()?;
    let cut = union_all(&others)?;
    let roi = Roi::from_multipolygon(base.difference(&cut))?;
    replace_roi(h, primary, roi)
}

/// Inverts the selected annotation within the image bounds (QuPath: "Make inverse").
pub fn make_inverse(h: &mut Hierarchy, width: f64, height: f64) -> Option<ObjKey> {
    let k = h.primary_selected()?;
    let o = h.get(k)?.clone();
    let full = Roi::Rectangle { x: 0.0, y: 0.0, w: width, h: height }.to_multipolygon()?;
    let roi = Roi::from_multipolygon(full.difference(&o.roi.to_multipolygon()?))?;
    let inverse = PathObject { roi, measurements: Default::default(), ..PathObject::annotation(o.roi.clone(), o.class) };
    let nk = h.insert(inverse);
    h.select(Some(nk));
    Some(nk)
}

/// Expands (or shrinks, with negative distance) selected annotations; returns new keys.
pub fn expand_selected(h: &mut Hierarchy, distance_px: f64, remove_interior: bool) -> Vec<ObjKey> {
    let sel: Vec<ObjKey> = h.selected().into_iter().filter(|k| h.get(*k).is_some_and(|o| o.is_annotation())).collect();
    let mut created = Vec::new();
    for k in sel {
        let Some(o) = h.get(k).cloned() else { continue };
        let grown = match o.roi.to_multipolygon() {
            Some(mp) => mp.buffer(distance_px),
            None => line_buffer(&o.roi, distance_px),
        };
        let shape = if remove_interior { o.roi.to_multipolygon().map_or(grown.clone(), |mp| grown.difference(&mp)) } else { grown };
        if let Some(roi) = Roi::from_multipolygon(shape) {
            created.push(h.insert(PathObject::annotation(roi, o.class.clone())));
        }
    }
    h.clear_selection();
    h.add_to_selection(created.iter().copied());
    created
}

fn line_buffer(roi: &Roi, d: f64) -> MultiPolygon<f64> {
    let pts = roi.outline();
    if pts.len() == 1 {
        let p = pts[0];
        return Roi::Ellipse { x: p.x - d, y: p.y - d, w: 2.0 * d, h: 2.0 * d }.to_multipolygon().unwrap_or(MultiPolygon::new(vec![]));
    }
    geo::LineString::new(pts).buffer(d.abs())
}

/// Simplifies selected annotation shapes (Douglas-Peucker, altitude in pixels).
pub fn simplify_selected(h: &mut Hierarchy, altitude: f64) {
    for k in h.selected() {
        let Some(o) = h.get(k) else { continue };
        if !o.is_annotation() || o.locked {
            continue;
        }
        let roi = match &o.roi {
            Roi::Polyline(p) => Some(Roi::Polyline(geo::LineString::new(p.clone()).simplify(altitude).0)),
            r => r.to_multipolygon().and_then(|mp| Roi::from_multipolygon(mp.simplify(altitude))),
        };
        if let Some(roi) = roi
            && let Some(o) = h.get_mut(k)
        {
            o.roi = roi;
        }
    }
}

/// Splits multi-part annotations into one annotation per part.
pub fn split_selected(h: &mut Hierarchy) -> Vec<ObjKey> {
    let mut created = Vec::new();
    for k in h.selected() {
        let Some(o) = h.get(k).cloned() else { continue };
        let Roi::Area(mp) = &o.roi else { continue };
        if mp.0.len() < 2 {
            continue;
        }
        h.remove(k, true);
        for p in &mp.0 {
            if let Some(roi) = Roi::from_multipolygon(MultiPolygon::new(vec![p.clone()])) {
                created.push(h.insert(PathObject::annotation(roi, o.class.clone())));
            }
        }
    }
    created
}

/// Replaces an object's ROI, re-inserting it so its parent stays correct.
pub fn replace_roi(h: &mut Hierarchy, k: ObjKey, roi: Roi) -> Option<ObjKey> {
    let mut o = h.get(k)?.clone();
    o.roi = roi;
    h.remove(k, true);
    let nk = h.insert(o);
    h.select(Some(nk));
    Some(nk)
}

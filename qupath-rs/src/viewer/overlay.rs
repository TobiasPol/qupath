//! Drawing objects over the image (QuPath's `HierarchyOverlay`).

use std::collections::{HashMap, HashSet};

use egui::epaint::Mesh;
use egui::{Align2, Color32, FontId, Painter, Shape, Stroke};
use geo::TriangulateEarcut;

pub use super::paths::{draw_roi, remove_spikes, roi_shapes, screen_path};
use super::transform::Transform;
use crate::objects::spatial::SpatialIndex;
use crate::objects::{Hierarchy, ObjKey, PathObject, Pt, Roi};

/// How cells are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellDisplay {
    /// Cell boundaries only.
    Boundaries,
    /// Nuclei only.
    Nuclei,
    /// Both.
    Both,
    /// A dot per cell.
    Centroids,
}

/// What to draw (QuPath's `OverlayOptions`).
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayOptions {
    /// Draw annotations.
    pub show_annotations: bool,
    /// Fill annotations.
    pub fill_annotations: bool,
    /// Draw annotation names.
    pub show_names: bool,
    /// Draw detections.
    pub show_detections: bool,
    /// Fill detections.
    pub fill_detections: bool,
    /// How cells look.
    pub cell_display: CellDisplay,
    /// Overview thumbnail.
    pub show_overview: bool,
    /// Cursor location.
    pub show_location: bool,
    /// Scale bar.
    pub show_scalebar: bool,
    /// Counting grid.
    pub show_grid: bool,
    /// Overlay opacity.
    pub opacity: f32,
    /// Class names (or "Unclassified") not drawn.
    pub hidden_classes: HashSet<String>,
}

impl Default for OverlayOptions {
    fn default() -> Self {
        OverlayOptions {
            show_annotations: true,
            fill_annotations: false,
            show_names: true,
            show_detections: true,
            fill_detections: false,
            cell_display: CellDisplay::Both,
            show_overview: true,
            show_location: true,
            show_scalebar: true,
            show_grid: false,
            opacity: 1.0,
            hidden_classes: HashSet::new(),
        }
    }
}

/// Line widths in screen points.
#[derive(Clone, Copy, Debug)]
pub struct Strokes {
    /// Annotations.
    pub annotation: f32,
    /// Detections.
    pub detection: f32,
    /// Point marker radius.
    pub point_radius: f32,
}

/// QuPath's selected-object color.
pub const SELECTED: Color32 = Color32::YELLOW;
/// Above this many visible detections, draw dots instead of shapes.
const DETAIL_LIMIT: usize = 40_000;

/// Triangulated annotation fills, cached per hierarchy version.
#[derive(Default)]
pub struct FillCache {
    version: u64,
    meshes: HashMap<ObjKey, (Vec<Pt>, Vec<u32>)>,
}

/// Draws visible objects; returns how many detections were in view.
pub fn draw_objects(
    painter: &Painter,
    t: &Transform,
    h: &Hierarchy,
    index: &SpatialIndex,
    o: &OverlayOptions,
    s: Strokes,
    fills: &mut FillCache,
) -> usize {
    if fills.version != h.version() {
        fills.meshes.clear();
        fills.version = h.version();
    }
    let visible = t.visible_bounds();
    let (mut anns, mut dets) = (Vec::new(), Vec::new());
    for k in index.query(&visible) {
        let Some(obj) = h.get(k) else { continue };
        let hidden = o.hidden_classes.contains(crate::objects::classes::display_name(obj.class.as_ref()));
        if hidden {
            continue;
        }
        if obj.is_detection() { dets.push(k) } else { anns.push(k) }
    }
    if o.show_detections {
        draw_detections(painter, t, h, &dets, o, s);
    }
    if o.show_annotations {
        // Big ones first so small ones stay on top.
        anns.sort_by(|a, b| area(h, *b).total_cmp(&area(h, *a)));
        for k in &anns {
            if let Some(obj) = h.get(*k) {
                draw_annotation(painter, t, obj, h.is_selected(*k), o, s, fills, *k);
            }
        }
    }
    // Selected detections are drawn last, highlighted.
    for k in h.selected().into_iter().filter(|k| h.get(*k).is_some_and(PathObject::is_detection)) {
        if let Some(obj) = h.get(k) {
            draw_roi(painter, t, &obj.roi, Stroke::new(s.detection + 1.0, SELECTED), None, s.point_radius);
        }
    }
    dets.len()
}

fn area(h: &Hierarchy, k: ObjKey) -> f64 {
    h.get(k).map_or(0.0, |o| o.roi.area(1.0, 1.0))
}

fn color(obj: &PathObject, opacity: f32) -> Color32 {
    obj.display_color().to_color32().gamma_multiply(opacity)
}

fn draw_detections(painter: &Painter, t: &Transform, h: &Hierarchy, dets: &[ObjKey], o: &OverlayOptions, s: Strokes) {
    let many = dets.len() > DETAIL_LIMIT || t.downsample > 16.0;
    let mut shapes = Vec::with_capacity(dets.len());
    for k in dets {
        let Some(obj) = h.get(*k) else { continue };
        let c = color(obj, o.opacity);
        if many || o.cell_display == CellDisplay::Centroids {
            let p = t.to_screen(obj.roi.centroid());
            shapes.push(Shape::circle_filled(p, if many { 1.5 } else { 2.5 }, c));
            continue;
        }
        let fill = o.fill_detections.then(|| c.gamma_multiply(0.5));
        let stroke = Stroke::new(s.detection, c);
        let show_cell = obj.nucleus.is_none() || matches!(o.cell_display, CellDisplay::Boundaries | CellDisplay::Both);
        if show_cell {
            shapes.extend(roi_shapes(t, &obj.roi, stroke, fill, s.point_radius));
        }
        if let Some(n) = &obj.nucleus
            && matches!(o.cell_display, CellDisplay::Nuclei | CellDisplay::Both)
        {
            let nfill = if show_cell { None } else { fill };
            shapes.extend(roi_shapes(t, n, stroke, nfill, s.point_radius));
        }
    }
    painter.extend(shapes);
}

#[allow(clippy::too_many_arguments)]
fn draw_annotation(
    painter: &Painter,
    t: &Transform,
    obj: &PathObject,
    selected: bool,
    o: &OverlayOptions,
    s: Strokes,
    fills: &mut FillCache,
    k: ObjKey,
) {
    let c = color(obj, o.opacity);
    let stroke = Stroke::new(s.annotation, if selected { SELECTED } else { c });
    if o.fill_annotations && obj.roi.is_area() {
        let (verts, idx) = fills.meshes.entry(k).or_insert_with(|| triangulate(&obj.roi));
        let mut mesh = Mesh::default();
        let fill = c.gamma_multiply(0.25);
        for v in verts.iter() {
            mesh.colored_vertex(t.to_screen(*v), fill);
        }
        mesh.indices.clone_from(idx);
        painter.add(Shape::mesh(mesh));
    }
    draw_roi(painter, t, &obj.roi, stroke, None, s.point_radius);
    if o.show_names
        && let Some(name) = &obj.name
    {
        let b = t.rect_to_screen(&obj.roi.bounds());
        let at = egui::pos2(b.center().x, b.top() - 4.0);
        painter.text(at + egui::vec2(1.0, 1.0), Align2::CENTER_BOTTOM, name, FontId::proportional(13.0), Color32::from_black_alpha(160));
        painter.text(at, Align2::CENTER_BOTTOM, name, FontId::proportional(13.0), Color32::WHITE);
    }
}

fn triangulate(roi: &Roi) -> (Vec<Pt>, Vec<u32>) {
    let (mut verts, mut idx) = (Vec::new(), Vec::new());
    for poly in roi.to_multipolygon().map(|m| m.0).unwrap_or_default() {
        let raw = poly.earcut_triangles_raw();
        let base = verts.len() as u32;
        verts.extend(raw.vertices.iter().map(|v| Pt { x: v[0], y: v[1] }));
        idx.extend(raw.triangle_indices.iter().map(|i| base + *i as u32));
    }
    (verts, idx)
}

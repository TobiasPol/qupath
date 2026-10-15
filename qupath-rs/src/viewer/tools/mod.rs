//! Drawing and editing tools (QuPath's `PathTools`).

mod draw;
mod handles;
mod move_tool;
mod paint;
mod points;

use std::sync::Arc;

use geo::MultiPolygon;

use super::transform::Transform;
use crate::app::data::UndoStack;
use crate::image::ImageServer;
use crate::objects::spatial::SpatialIndex;
use crate::objects::{Hierarchy, ObjKey, PathClass, PathObject, Pt, Roi};
use crate::ui::icons::Icon;

pub use handles::draw_handles;

/// The tools in QuPath's toolbar, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    /// Pan and select.
    Move,
    /// Rectangle annotations.
    Rectangle,
    /// Ellipse annotations.
    Ellipse,
    /// Line annotations.
    Line,
    /// Polygon annotations (click or drag).
    Polygon,
    /// Polyline annotations.
    Polyline,
    /// Paint annotations.
    Brush,
    /// Paint by similar color.
    Wand,
    /// Point annotations.
    Points,
}

impl Tool {
    /// Every tool, as in the toolbar.
    pub const ALL: [Tool; 9] =
        [Tool::Move, Tool::Rectangle, Tool::Ellipse, Tool::Line, Tool::Polygon, Tool::Polyline, Tool::Brush, Tool::Wand, Tool::Points];

    /// Name.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line",
            Tool::Polygon => "Polygon",
            Tool::Polyline => "Polyline",
            Tool::Brush => "Brush",
            Tool::Wand => "Wand",
            Tool::Points => "Points",
        }
    }

    /// Toolbar icon.
    pub fn icon(self) -> Icon {
        match self {
            Tool::Move => Icon::Move,
            Tool::Rectangle => Icon::Rectangle,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Line => Icon::Line,
            Tool::Polygon => Icon::Polygon,
            Tool::Polyline => Icon::Polyline,
            Tool::Brush => Icon::Brush,
            Tool::Wand => Icon::Wand,
            Tool::Points => Icon::Points,
        }
    }

    /// QuPath's single-key shortcut.
    pub fn key(self) -> egui::Key {
        use egui::Key;
        match self {
            Tool::Move => Key::M,
            Tool::Rectangle => Key::R,
            Tool::Ellipse => Key::O,
            Tool::Line => Key::L,
            Tool::Polygon => Key::P,
            Tool::Polyline => Key::V,
            Tool::Brush => Key::B,
            Tool::Wand => Key::W,
            Tool::Points => Key::Period,
        }
    }

    /// Shapes drawn in one go, after which QuPath returns to the Move tool.
    pub fn returns_to_move(self) -> bool {
        matches!(self, Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Polygon | Tool::Polyline)
    }
}

/// An interaction in progress.
#[derive(Clone, Debug, Default)]
pub enum Gesture {
    /// Nothing.
    #[default]
    None,
    /// Dragging the image.
    Pan,
    /// Dragging selected objects.
    MoveObjects { keys: Vec<ObjKey>, last: Pt },
    /// Dragging a handle or vertex of an annotation.
    Handle { key: ObjKey, handle: handles::Handle },
    /// Rectangle, ellipse or line being dragged out.
    Shape { start: Pt, current: Pt },
    /// Polygon or polyline vertices; `freehand` when started by dragging.
    Poly { points: Vec<Pt>, freehand: bool },
    /// Brush or wand strokes.
    Paint { shape: MultiPolygon<f64>, target: Option<ObjKey>, subtract: bool, last: Pt },
}

/// What a tool can see and change.
pub struct ToolCtx<'a> {
    /// Objects.
    pub h: &'a mut Hierarchy,
    /// Undo history.
    pub undo: &'a mut UndoStack,
    /// Spatial index (may be stale during a gesture).
    pub index: &'a SpatialIndex,
    /// The view.
    pub t: &'a mut Transform,
    /// Pixels, for the wand.
    pub server: &'a Arc<dyn ImageServer>,
    /// Class for new annotations.
    pub auto_class: Option<PathClass>,
    /// Draw shapes to select objects rather than create annotations.
    pub selection_mode: bool,
    /// Whether detections can be clicked.
    pub detections_selectable: bool,
    /// Brush diameter in screen points.
    pub brush_diameter: f32,
    /// Set when a shape was completed.
    pub finished: bool,
    checkpointed: bool,
}

impl<'a> ToolCtx<'a> {
    /// Bundles what a tool needs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        h: &'a mut Hierarchy,
        undo: &'a mut UndoStack,
        index: &'a SpatialIndex,
        t: &'a mut Transform,
        server: &'a Arc<dyn ImageServer>,
        auto_class: Option<PathClass>,
        selection_mode: bool,
        detections_selectable: bool,
        brush_diameter: f32,
    ) -> Self {
        ToolCtx {
            h,
            undo,
            index,
            t,
            server,
            auto_class,
            selection_mode,
            detections_selectable,
            brush_diameter,
            finished: false,
            checkpointed: false,
        }
    }

    /// Saves an undo step, once per gesture.
    pub fn checkpoint(&mut self) {
        if !self.checkpointed {
            self.undo.checkpoint(self.h);
            self.checkpointed = true;
        }
    }

    /// Adds an annotation (or, in selection mode, selects what it contains).
    pub fn create(&mut self, roi: Roi) {
        self.finished = true;
        if self.selection_mode {
            select_inside(self.h, self.index, &roi);
            return;
        }
        self.checkpoint();
        let k = self.h.insert(PathObject::annotation(roi, self.auto_class.clone()));
        self.h.select(Some(k));
    }

    /// Selects the object under a point (toggling with the platform modifier).
    pub fn click_select(&mut self, p: Pt, toggle: bool) {
        let tol = 4.0 * self.t.downsample;
        let hit = self.index.hit(self.h, p, tol, self.detections_selectable);
        match (hit, toggle) {
            (Some(k), true) => self.h.toggle_selected(k),
            (hit, false) => self.h.select(hit),
            (None, true) => {}
        }
    }
}

/// Selects every object whose centroid lies inside an area.
fn select_inside(h: &mut Hierarchy, index: &SpatialIndex, roi: &Roi) {
    if !roi.is_area() {
        return;
    }
    let keys: Vec<ObjKey> = index.query(&roi.bounds()).filter(|k| h.get(*k).is_some_and(|o| roi.contains(o.roi.centroid()))).collect();
    h.clear_selection();
    h.add_to_selection(keys);
}

/// Pointer state for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    /// Image position.
    pub pos: Pt,
    /// Primary drag began this frame.
    pub drag_started: bool,
    /// Primary drag continues.
    pub dragging: bool,
    /// Primary drag ended this frame.
    pub drag_stopped: bool,
    /// A click (press and release without moving).
    pub clicked: bool,
    /// A double click.
    pub double_clicked: bool,
    /// Shift held.
    pub shift: bool,
    /// Alt/Option held.
    pub alt: bool,
    /// Cmd (macOS) or Ctrl held.
    pub command: bool,
    /// Screen movement this frame.
    pub delta: egui::Vec2,
}

/// Runs the active tool for one frame of input.
pub fn handle(tool: Tool, g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    match tool {
        Tool::Move => move_tool::handle(g, ctx, p),
        Tool::Rectangle | Tool::Ellipse | Tool::Line => draw::shape(tool, g, ctx, p),
        Tool::Polygon | Tool::Polyline => draw::poly(tool, g, ctx, p),
        Tool::Brush | Tool::Wand => paint::handle(tool, g, ctx, p),
        Tool::Points => points::handle(g, ctx, p),
    }
}

/// The shape being drawn, for the preview.
pub fn preview(tool: Tool, g: &Gesture, cursor: Option<Pt>) -> Option<Roi> {
    match g {
        Gesture::Shape { start, current } => Some(draw::shape_roi(tool, *start, *current, false)),
        Gesture::Poly { points, .. } => {
            let mut pts = points.clone();
            pts.extend(cursor);
            Some(Roi::Polyline(pts))
        }
        Gesture::Paint { shape, .. } => Some(Roi::Area(shape.clone())),
        _ => None,
    }
}

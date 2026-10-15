//! The analysis pane: Project, Image, Annotations, Hierarchy and Workflow tabs.

pub mod annotations;
pub mod hierarchy;
pub mod image;
pub mod project;
pub mod workflow;

use crate::app::QuPathApp;
use crate::objects::ObjKey;

/// The tabs, as in QuPath.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    /// Project images.
    #[default]
    Project,
    /// Image properties.
    Image,
    /// Annotations, classes and measurements.
    Annotations,
    /// Object hierarchy.
    Hierarchy,
    /// Command history.
    Workflow,
}

/// Draws the pane.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for (p, name) in [
            (Pane::Project, "Project"),
            (Pane::Image, "Image"),
            (Pane::Annotations, "Annotations"),
            (Pane::Hierarchy, "Hierarchy"),
            (Pane::Workflow, "Workflow"),
        ] {
            ui.selectable_value(&mut app.ui.pane, p, name);
        }
    });
    ui.separator();
    match app.ui.pane {
        Pane::Project => project::show(ui, app),
        Pane::Image => image::show(ui, app),
        Pane::Annotations => annotations::show(ui, app),
        Pane::Hierarchy => hierarchy::show(ui, app),
        Pane::Workflow => workflow::show(ui, app),
    }
}

/// Selects an object from a list, centering the viewer on it when it is out of view.
pub fn select_and_show(app: &mut QuPathApp, k: ObjKey, toggle: bool) {
    let Some(d) = &mut app.s.image else { return };
    if toggle {
        d.hierarchy.toggle_selected(k);
    } else {
        d.hierarchy.select(Some(k));
    }
    if let Some(b) = d.hierarchy.get(k).map(|o| o.roi.bounds())
        && !app.viewer.t.visible_bounds().contains_bounds(&b)
    {
        app.viewer.center_on(&b);
    }
}

/// A small color square.
pub fn swatch(ui: &mut egui::Ui, c: egui::Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, c);
    resp
}

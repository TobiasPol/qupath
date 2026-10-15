//! The Hierarchy tab: objects as a tree, as QuPath shows them.

use crate::app::QuPathApp;
use crate::objects::{Hierarchy, ObjKey};

/// Children shown per node before "… more".
const MAX_CHILDREN: usize = 200;

/// Draws the Hierarchy tab.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &app.s.image else {
        ui.label("No image open.");
        return;
    };
    let h = &d.hierarchy;
    let mut clicked = None;
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        let root = h.root();
        let label = format!("Image ({} objects)", h.len());
        egui::CollapsingHeader::new(label).id_salt(("node", root)).default_open(true).show(ui, |ui| {
            children(ui, h, root, &mut clicked);
        });
    });
    if let Some((k, toggle)) = clicked {
        super::select_and_show(app, k, toggle);
    }
}

fn children(ui: &mut egui::Ui, h: &Hierarchy, k: ObjKey, clicked: &mut Option<(ObjKey, bool)>) {
    let kids = h.children(k);
    for c in kids.iter().take(MAX_CHILDREN) {
        node(ui, h, *c, clicked);
    }
    if kids.len() > MAX_CHILDREN {
        ui.weak(format!("… {} more", kids.len() - MAX_CHILDREN));
    }
}

fn node(ui: &mut egui::Ui, h: &Hierarchy, k: ObjKey, clicked: &mut Option<(ObjKey, bool)>) {
    let Some(o) = h.get(k) else { return };
    let n = o.children().len();
    let mut label = format!("{} — {}", o.kind.display_name(), o.label());
    if n > 0 {
        label.push_str(&format!(" ({n})"));
    }
    let selected = h.is_selected(k);
    if n == 0 {
        ui.horizontal(|ui| {
            super::swatch(ui, o.display_color().to_color32());
            if ui.selectable_label(selected, label).clicked() {
                *clicked = Some((k, ui.input(|i| i.modifiers.command)));
            }
        });
        return;
    }
    let header = egui::CollapsingHeader::new(egui::RichText::new(label).color(if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().text_color()
    }))
    .id_salt(("node", k))
    .show(ui, |ui| children(ui, h, k, clicked));
    if header.header_response.clicked() {
        *clicked = Some((k, ui.input(|i| i.modifiers.command)));
    }
}

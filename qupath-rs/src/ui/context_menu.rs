//! The viewer's right-click menu (QuPath's viewer context menu).

use crate::app::commands::Command as C;
use crate::app::state::AppState;
use crate::objects::PathClass;

/// Draws the context menu for the current selection.
pub fn show(ui: &mut egui::Ui, s: &mut AppState) {
    let has_selection = s.image.as_ref().is_some_and(|d| d.hierarchy.primary_selected().is_some());
    if has_selection {
        ui.menu_button("Set classification", |ui| {
            if ui.button("Unclassified").clicked() {
                set_class(s, None);
                ui.close();
            }
            for c in s.classes.clone() {
                let swatch = egui::RichText::new("■").color(c.color().to_color32());
                if ui.add(egui::Button::new((swatch, c.name()))).clicked() {
                    set_class(s, Some(c));
                    ui.close();
                }
            }
        });
        ui.separator();
        for cmd in [C::LockSelected, C::UnlockSelected, C::DuplicateAnnotation, C::MergeSelected, C::InsertIntoHierarchy] {
            item(ui, s, cmd);
        }
        ui.separator();
        item(ui, s, C::DeleteSelected);
        ui.separator();
    }
    ui.menu_button("Zoom", |ui| {
        for cmd in [C::ZoomToFit, C::Zoom400, C::Zoom100, C::Zoom10] {
            item(ui, s, cmd);
        }
    });
    ui.menu_button("Overlays", |ui| {
        for cmd in [C::ShowAnnotations, C::FillAnnotations, C::ShowDetections, C::FillDetections, C::ShowGrid] {
            item(ui, s, cmd);
        }
    });
    item(ui, s, C::BrightnessContrast);
}

fn item(ui: &mut egui::Ui, s: &mut AppState, cmd: C) {
    if ui.button(cmd.label()).clicked() {
        s.queued.push(cmd);
        ui.close();
    }
}

/// Sets the class of every selected object (one undo step).
pub fn set_class(s: &mut AppState, class: Option<PathClass>) {
    let Some(d) = &mut s.image else { return };
    let sel = d.hierarchy.selected();
    if sel.is_empty() {
        return;
    }
    d.undo.checkpoint(&d.hierarchy);
    for k in &sel {
        if let Some(o) = d.hierarchy.get_mut(*k) {
            o.class.clone_from(&class);
        }
    }
    let name = crate::objects::classes::display_name(class.as_ref()).to_owned();
    d.record("Set classification", format!("getSelectedObjects().each {{ it.setPathClass(getPathClass('{name}')) }}"));
}

//! The Annotations tab: annotation list, class list, and the selected object's measurements.

use crate::app::QuPathApp;
use crate::objects::{ObjKey, PathClass, Rgb, measurements};
use crate::ui::dialogs::Dialog;
use crate::ui::icons::Icon;

/// Draws the Annotations tab.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    if app.s.image.is_none() {
        ui.label("No image open.");
        return;
    }
    let h = ui.available_height();
    egui::Panel::top("annotation-list").resizable(true).default_size(h * 0.35).show(ui, |ui| annotation_list(ui, app));
    egui::Panel::top("class-list").resizable(true).default_size(h * 0.32).show(ui, |ui| class_list(ui, app));
    selected_measurements(ui, app);
}

fn annotation_list(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &app.s.image else { return };
    let h = &d.hierarchy;
    let rows: Vec<(ObjKey, String, egui::Color32, bool, bool)> =
        h.annotations().map(|(k, o)| (k, o.label(), o.display_color().to_color32(), h.is_selected(k), o.locked)).collect();
    ui.weak(format!("{} annotations", rows.len()));
    let mut clicked = None;
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
    egui::ScrollArea::vertical().id_salt("ann-scroll").auto_shrink(false).show_rows(ui, row_h, rows.len(), |ui, range| {
        for (k, label, color, selected, locked) in &rows[range] {
            ui.horizontal(|ui| {
                super::swatch(ui, *color);
                let text = if *locked { format!("{label} (locked)") } else { label.clone() };
                if ui.selectable_label(*selected, text).clicked() {
                    clicked = Some((*k, ui.input(|i| i.modifiers.command)));
                }
            });
        }
    });
    if let Some((k, toggle)) = clicked {
        super::select_and_show(app, k, toggle);
    }
}

fn class_list(ui: &mut egui::Ui, app: &mut QuPathApp) {
    ui.horizontal(|ui| {
        let sel = app.ui.class_index;
        let class = if sel == 0 { None } else { app.s.classes.get(sel - 1).cloned() };
        if ui.button("Set selected").on_hover_text("Assign the class to selected objects").clicked() {
            crate::ui::context_menu::set_class(&mut app.s, class.clone());
        }
        let mut auto = app.s.auto_class.is_some() && app.s.auto_class == class;
        if ui.toggle_value(&mut auto, "Auto set").on_hover_text("Give new annotations this class").changed() {
            app.s.auto_class = if auto { class } else { None };
        }
    });
    let n = app.s.classes.len() + 1;
    egui::ScrollArea::vertical().id_salt("class-scroll").auto_shrink(false).max_height(ui.available_height() - 28.0).show(ui, |ui| {
        for i in 0..n {
            class_row(ui, app, i);
        }
    });
    class_buttons(ui, app);
}

fn class_row(ui: &mut egui::Ui, app: &mut QuPathApp, i: usize) {
    let class: Option<PathClass> = if i == 0 { None } else { app.s.classes.get(i - 1).cloned() };
    let name = crate::objects::classes::display_name(class.as_ref()).to_owned();
    let hidden = app.s.overlay.hidden_classes.contains(&name);
    ui.horizontal(|ui| {
        let tint = ui.visuals().weak_text_color();
        let eye = if hidden { Icon::EyeOff } else { Icon::Eye };
        if ui.add(egui::Button::image(eye.image(tint, 14.0)).frame(false)).on_hover_text("Show/hide objects of this class").clicked() {
            if hidden {
                app.s.overlay.hidden_classes.remove(&name);
            } else {
                app.s.overlay.hidden_classes.insert(name.clone());
            }
        }
        let color = class.as_ref().map_or(egui::Color32::from_gray(160), |c| c.color().to_color32());
        super::swatch(ui, color);
        let auto = app.s.auto_class.is_some() && app.s.auto_class == class;
        let label = if auto { format!("{name}  (auto)") } else { name.clone() };
        let r = ui.selectable_label(app.ui.class_index == i, label);
        if r.clicked() {
            app.ui.class_index = i;
        }
        if r.double_clicked()
            && let Some(c) = class
        {
            app.s.dialog = Some(Dialog::EditClass { original: Some(c.clone()), name: c.name().to_owned(), color: c.color() });
        }
    });
}

fn class_buttons(ui: &mut egui::Ui, app: &mut QuPathApp) {
    ui.horizontal(|ui| {
        if ui.button("+").on_hover_text("Add class").clicked() {
            app.s.dialog = Some(Dialog::EditClass { original: None, name: String::new(), color: Rgb(200, 120, 40) });
        }
        if ui.button("−").on_hover_text("Remove class").clicked() && app.ui.class_index > 0 {
            app.s.classes.remove(app.ui.class_index - 1);
            app.ui.class_index = 0;
            app.save_project();
        }
        if ui.button("Populate from existing").on_hover_text("Add classes used by objects in this image").clicked() {
            populate(app);
        }
    });
}

fn populate(app: &mut QuPathApp) {
    let Some(d) = &app.s.image else { return };
    let mut found: Vec<PathClass> = d.hierarchy.iter().filter_map(|(_, o)| o.class.clone()).collect();
    found.sort();
    found.dedup();
    for c in found {
        if !app.s.classes.contains(&c) {
            app.s.classes.push(c);
        }
    }
    app.save_project();
}

fn selected_measurements(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &app.s.image else { return };
    let Some(k) = d.hierarchy.primary_selected() else {
        ui.weak("Select an object to see its measurements");
        return;
    };
    let cal = d.calibration(&app.s.prefs);
    let row = measurements::object_row(&d.hierarchy, k, &cal);
    egui::ScrollArea::vertical().id_salt("meas-scroll").auto_shrink(false).show(ui, |ui| {
        egui::Grid::new("selected-meas").num_columns(2).striped(true).show(ui, |ui| {
            for (name, v) in row.iter().filter(|(n, _)| *n != "Object ID") {
                ui.label(name);
                ui.label(v.format(4));
                ui.end_row();
            }
        });
    });
}

//! Forms for annotations, classes, export and renaming.

use egui::Grid;

use super::analysis::class_combo;
use super::{DialogEnv, ExportFormat, Outcome, Specify, SpecifyShape, buttons, number};
use crate::objects::Rgb;

/// Specify annotation.
pub fn specify(ui: &mut egui::Ui, s: &mut Specify, env: &DialogEnv<'_>) -> Outcome {
    Grid::new("specify").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Type");
        ui.horizontal(|ui| {
            ui.radio_value(&mut s.shape, SpecifyShape::Rectangle, "Rectangle");
            ui.radio_value(&mut s.shape, SpecifyShape::Ellipse, "Ellipse");
        });
        ui.end_row();
        ui.label("Classification");
        class_combo(ui, "specify-class", &mut s.class, env.classes);
        ui.end_row();
        ui.label("Units");
        ui.add_enabled_ui(env.calibrated, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.microns, true, "µm");
                ui.radio_value(&mut s.microns, false, "Pixels");
            });
        });
        ui.end_row();
        number(ui, "Width", &mut s.w, 1.0, 0.0..=1e7, "");
        number(ui, "Height", &mut s.h, 1.0, 0.0..=1e7, "");
        ui.label("Centered in viewer");
        ui.checkbox(&mut s.centered, "");
        ui.end_row();
        if !s.centered {
            number(ui, "X origin", &mut s.x, 1.0, 0.0..=1e7, "");
            number(ui, "Y origin", &mut s.y, 1.0, 0.0..=1e7, "");
        }
        ui.label("Name");
        ui.text_edit_singleline(&mut s.name);
        ui.end_row();
    });
    buttons(ui, "Add annotation")
}

/// Expand annotations.
pub fn expand(ui: &mut egui::Ui, distance: &mut f64, remove_interior: &mut bool, env: &DialogEnv<'_>) -> Outcome {
    ui.label("Negative values shrink annotations.");
    Grid::new("expand").num_columns(2).show(ui, |ui| {
        number(ui, "Expansion radius", distance, 0.5, -1e5..=1e5, if env.calibrated { " µm" } else { " px" });
        ui.label("Remove interior");
        ui.checkbox(remove_interior, "");
        ui.end_row();
    });
    buttons(ui, "Expand")
}

/// Simplify shape.
pub fn simplify(ui: &mut egui::Ui, altitude: &mut f64) -> Outcome {
    ui.label("Removes vertices closer than the altitude to the simplified outline.");
    Grid::new("simplify").num_columns(2).show(ui, |ui| number(ui, "Altitude", altitude, 0.1, 0.0..=1000.0, " px"));
    buttons(ui, "Simplify")
}

/// Export image.
pub fn export(ui: &mut egui::Ui, downsample: &mut f64, format: &mut ExportFormat, rendered: &mut bool, selection: &mut bool) -> Outcome {
    Grid::new("export").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Format");
        ui.horizontal(|ui| {
            ui.radio_value(format, ExportFormat::OmeTiff, "OME-TIFF (pyramidal)");
            ui.radio_value(format, ExportFormat::Png, "PNG");
            ui.radio_value(format, ExportFormat::Jpeg, "JPEG");
        });
        ui.end_row();
        number(ui, "Downsample", downsample, 0.5, 1.0..=512.0, "×");
        ui.label("Rendered (display settings)");
        ui.checkbox(rendered, "");
        ui.end_row();
        ui.label("Selected object only");
        ui.checkbox(selection, "");
        ui.end_row();
    });
    buttons(ui, "Export")
}

/// Add or edit a class.
pub fn edit_class(ui: &mut egui::Ui, name: &mut String, color: &mut Rgb) -> Outcome {
    Grid::new("class").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Name");
        ui.text_edit_singleline(name);
        ui.end_row();
        ui.label("Color");
        let mut c = [color.0, color.1, color.2];
        ui.color_edit_button_srgb(&mut c);
        *color = Rgb(c[0], c[1], c[2]);
        ui.end_row();
    });
    if name.trim().is_empty() {
        ui.colored_label(ui.visuals().warn_fg_color, "Enter a name");
    }
    match buttons(ui, "OK") {
        Outcome::Accept if name.trim().is_empty() => Outcome::Open,
        o => o,
    }
}

/// Rename a project entry.
pub fn rename(ui: &mut egui::Ui, name: &mut String) -> Outcome {
    let r = ui.text_edit_singleline(name);
    r.request_focus();
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    match buttons(ui, "Rename") {
        Outcome::Open if enter => Outcome::Accept,
        o => o,
    }
}

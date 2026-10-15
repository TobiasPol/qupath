//! Edit → Preferences.

use egui::Grid;

use crate::app::QuPathApp;
use crate::io::prefs::ThemeChoice;

fn theme_label(t: &ThemeChoice) -> String {
    match t {
        ThemeChoice::System => "System".into(),
        ThemeChoice::Light => "Light (QuPath)".into(),
        ThemeChoice::Dark => "Dark (QuPath)".into(),
        ThemeChoice::Palette(f) => fastframe_theme::display_name(f).to_owned(),
    }
}

/// Draws the preferences.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let before = app.s.prefs.clone();
    appearance(ui, &mut app.s.prefs);
    viewer(ui, &mut app.s.prefs);
    readers(ui);
    if app.s.prefs != before {
        apply(ui.ctx(), app, &before);
    }
}

fn appearance(ui: &mut egui::Ui, p: &mut crate::io::prefs::Prefs) {
    egui::CollapsingHeader::new("Appearance").default_open(true).show(ui, |ui| {
        Grid::new("prefs-appearance").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Theme");
            egui::ComboBox::from_id_salt("theme").selected_text(theme_label(&p.theme)).show_ui(ui, |ui| {
                for t in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
                    let l = theme_label(&t);
                    ui.selectable_value(&mut p.theme, t, l);
                }
                ui.separator();
                for (file, _) in crate::ui::style::shared_palettes() {
                    let t = ThemeChoice::Palette(file);
                    let l = theme_label(&t);
                    ui.selectable_value(&mut p.theme, t, l);
                }
            });
            ui.end_row();
            ui.label("Interface font");
            ui.checkbox(&mut p.system_font, "Use the system font (instead of Inter)");
            ui.end_row();
        });
    });
}

fn viewer(ui: &mut egui::Ui, p: &mut crate::io::prefs::Prefs) {
    egui::CollapsingHeader::new("Viewer").default_open(true).show(ui, |ui| {
        Grid::new("prefs-viewer").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            drag(ui, "Tile cache (MB)", &mut p.tile_cache_mb, 64..=16384);
            slider(ui, "Annotation line width", &mut p.annotation_stroke, 0.5..=8.0);
            slider(ui, "Detection line width", &mut p.detection_stroke, 0.5..=8.0);
            slider(ui, "Point radius", &mut p.point_radius, 1.0..=20.0);
            ui.label("Grid spacing");
            ui.add(egui::DragValue::new(&mut p.grid_spacing).range(1.0..=100_000.0).suffix(" µm"));
            ui.end_row();
        });
    });
    egui::CollapsingHeader::new("Drawing tools").default_open(true).show(ui, |ui| {
        Grid::new("prefs-tools").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            slider(ui, "Brush diameter (screen px)", &mut p.brush_diameter, 2.0..=400.0);
            ui.label("Return to Move tool");
            ui.checkbox(&mut p.return_to_move_tool, "after drawing a shape");
            ui.end_row();
            ui.label("Units");
            ui.checkbox(&mut p.use_calibrated_units, "Measure in µm when the pixel size is known");
            ui.end_row();
        });
    });
}

fn readers(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Image readers").show(ui, |ui| match crate::image::openslide::status() {
        None => ui.label("OpenSlide: loading…"),
        Some(Ok(())) => ui.label("OpenSlide: available"),
        Some(Err(e)) => {
            ui.label(format!("OpenSlide: {e}\nSet QUPATH_RS_OPENSLIDE to libopenslide's path, or install QuPath or openslide."))
        }
    });
}

fn apply(ctx: &egui::Context, app: &mut QuPathApp, before: &crate::io::prefs::Prefs) {
    let p = &app.s.prefs;
    if p.system_font != before.system_font {
        crate::ui::style::install_fonts(ctx, p);
    }
    if p.theme != before.theme || p.system_font != before.system_font {
        app.s.palette = crate::ui::style::apply_theme(ctx, p);
    }
    if p.tile_cache_mb != before.tile_cache_mb {
        app.viewer.tiles.clear(p.tile_cache_mb);
    }
    app.s.save_prefs();
}

fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, r: std::ops::RangeInclusive<f32>) {
    ui.label(label);
    ui.add(egui::Slider::new(v, r));
    ui.end_row();
}

fn drag(ui: &mut egui::Ui, label: &str, v: &mut usize, r: std::ops::RangeInclusive<usize>) {
    ui.label(label);
    ui.add(egui::DragValue::new(v).range(r).speed(16));
    ui.end_row();
}

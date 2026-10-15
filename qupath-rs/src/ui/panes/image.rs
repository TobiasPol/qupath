//! The Image tab: properties, with editable type and pixel size.

use egui::Grid;

use crate::app::QuPathApp;
use crate::image::stains::ImageType;

/// Draws the Image tab.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &mut app.s.image else {
        ui.label("No image open.");
        return;
    };
    let md = d.server.metadata().clone();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        Grid::new("image-props").num_columns(2).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            row(ui, "Name", &md.name);
            row(ui, "URI", &md.path.display().to_string());
            row(ui, "Pixel type", md.pixel_type.name());
            row(ui, "Server type", &md.server_type);
            row(ui, "Width", &format!("{} px", md.width()));
            row(ui, "Height", &format!("{} px", md.height()));
            pixel_size(ui, d, md.pixel_width_um, md.pixel_height_um);
            row(ui, "Magnification", &md.magnification.map_or("Unknown".into(), |m| format!("{m}")));
            row(ui, "Number of channels", &md.n_channels().to_string());
            row(ui, "Channels", &md.channels.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
            image_type(ui, d);
            if let Some(s) = &d.settings.stains {
                for (i, v) in s.stains.iter().enumerate().take(2) {
                    row(ui, &format!("Stain {}", i + 1), &format!("{}: {:.3} {:.3} {:.3}", v.name, v.od[0], v.od[1], v.od[2]));
                }
                row(ui, "Background", &format!("{:.0} {:.0} {:.0}", s.max_rgb[0], s.max_rgb[1], s.max_rgb[2]));
            }
            let levels: Vec<String> = md.levels.iter().map(|l| format!("{:.0}", l.downsample)).collect();
            row(ui, "Pyramid", &levels.join(", "));
            row(ui, "Tile size", &format!("{} × {}", md.tile_width, md.tile_height));
            if !md.associated.is_empty() {
                row(ui, "Associated images", &md.associated.join(", "));
            }
        });
        if !md.properties.is_empty() {
            ui.collapsing(format!("Metadata ({})", md.properties.len()), |ui| {
                Grid::new("image-meta").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, v) in &md.properties {
                        row(ui, k, v);
                    }
                });
            });
        }
    });
}

fn row(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.label(k);
    ui.add(egui::Label::new(v).truncate()).on_hover_text(v);
    ui.end_row();
}

fn pixel_size(ui: &mut egui::Ui, d: &mut crate::app::data::ImageData, w: Option<f64>, h: Option<f64>) {
    for (label, field, default) in [("Pixel width", 0, w), ("Pixel height", 1, h)] {
        ui.label(label);
        let slot = if field == 0 { &mut d.settings.pixel_width_microns } else { &mut d.settings.pixel_height_microns };
        let mut v = slot.or(default).unwrap_or(0.0);
        let resp = ui.add(egui::DragValue::new(&mut v).speed(0.001).range(0.0..=1000.0).suffix(" µm").max_decimals(4));
        if resp.changed() {
            *slot = (v > 0.0).then_some(v);
            d.settings_changed();
        }
        ui.end_row();
    }
}

fn image_type(ui: &mut egui::Ui, d: &mut crate::app::data::ImageData) {
    ui.label("Image type");
    let mut t = d.settings.image_type;
    egui::ComboBox::from_id_salt("image-type").selected_text(t.label()).show_ui(ui, |ui| {
        for opt in ImageType::ALL {
            ui.selectable_value(&mut t, opt, opt.label());
        }
    });
    if t != d.settings.image_type {
        d.set_image_type(t);
    }
    ui.end_row();
}

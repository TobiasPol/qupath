//! What the viewer area shows before an image is open.

use crate::app::commands::Command;
use crate::app::state::AppState;

/// The empty viewer: what to do next.
pub fn show(ui: &mut egui::Ui, s: &mut AppState) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(24));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(40.0)), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(rect.height() * 0.22);
            ui.label(egui::RichText::new("QuPath").size(40.0).strong().color(egui::Color32::from_gray(230)));
            ui.label(egui::RichText::new("Rust · egui · fastframe").color(egui::Color32::from_gray(150)));
            ui.add_space(24.0);
            for (cmd, text) in [
                (Command::OpenImage, "Open an image…"),
                (Command::ProjectNew, "Create a project…"),
                (Command::ProjectOpen, "Open a project…"),
            ] {
                if ui.add_sized([220.0, 30.0], egui::Button::new(text)).clicked() {
                    s.queued.push(cmd);
                }
            }
            ui.add_space(16.0);
            ui.label(egui::RichText::new("Or drop a slide, a project.qpproj or a GeoJSON file here").color(egui::Color32::from_gray(140)));
            if let Some(Err(e)) = crate::image::openslide::status() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("OpenSlide not found: vendor formats (NDPI, MRXS…) are unavailable")
                        .color(egui::Color32::from_gray(120)),
                )
                .on_hover_text(e);
            }
        });
    });
}

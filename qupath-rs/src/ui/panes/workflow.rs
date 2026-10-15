//! The Workflow tab: commands run on this image, as QuPath script lines.

use crate::app::QuPathApp;

/// Draws the Workflow tab.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &app.s.image else {
        ui.label("No image open.");
        return;
    };
    let steps = &d.settings.workflow;
    ui.horizontal(|ui| {
        ui.weak(format!("{} steps", steps.len()));
        if ui.button("Copy script").on_hover_text("Copy the steps as a QuPath (Groovy) script").clicked() {
            let script: Vec<String> = steps.iter().map(|s| s.script.clone()).collect();
            ui.ctx().copy_text(script.join("\n"));
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, s) in steps.iter().enumerate() {
            egui::CollapsingHeader::new(format!("{}. {}", i + 1, s.name)).id_salt(("step", i)).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(&s.script).monospace()).wrap());
            });
        }
    });
}

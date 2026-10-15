//! The status bar: running tasks with progress, and recent messages.

use super::QuPathApp;

/// Seconds a message stays visible.
const NOTICE_SECONDS: f64 = 6.0;

impl QuPathApp {
    /// Draws the status bar.
    pub(super) fn status_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        self.s.notices.retain(|n| now - n.at < NOTICE_SECONDS * if n.error { 2.0 } else { 1.0 });
        ui.horizontal(|ui| {
            for t in &self.s.tasks.running {
                ui.spinner();
                ui.add(egui::ProgressBar::new(t.fraction()).desired_width(140.0).text(t.name.clone()));
                if ui.small_button("Cancel").clicked() {
                    t.cancel();
                }
            }
            if let Some(n) = self.s.notices.last() {
                let color = if n.error { ui.visuals().error_fg_color } else { ui.visuals().weak_text_color() };
                ui.colored_label(color, &n.text);
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(d) = &self.s.image {
                    let h = &d.hierarchy;
                    let n_ann = h.annotations().count();
                    let n_det = h.len() - n_ann;
                    ui.weak(format!("{n_ann} annotations · {n_det} detections · {} selected", h.selected().len()));
                }
            });
        });
    }
}

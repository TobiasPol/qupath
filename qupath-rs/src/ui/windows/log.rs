//! View → Show log: the per-run log file written by fastframe-log.

use crate::app::QuPathApp;

/// The log window's state.
pub struct LogView {
    text: String,
    read_at: f64,
    level: usize,
}

impl Default for LogView {
    fn default() -> Self {
        LogView { text: String::new(), read_at: f64::MIN, level: 2 }
    }
}

const LEVELS: [&str; 5] = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

/// Draws the log.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let now = ui.input(|i| i.time);
    let v = &mut app.ui.log;
    if now - v.read_at > 1.0 {
        v.read_at = now;
        if let Some(f) = &app.s.log_file {
            v.text = std::fs::read_to_string(f).unwrap_or_default();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
    ui.horizontal(|ui| {
        ui.label("Show");
        egui::ComboBox::from_id_salt("log-level").selected_text(LEVELS[v.level]).show_ui(ui, |ui| {
            for (i, l) in LEVELS.iter().enumerate() {
                ui.selectable_value(&mut v.level, i, *l);
            }
        });
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(v.text.clone());
        }
        match &app.s.log_file {
            Some(f) => ui.weak(f.display().to_string()),
            None => ui.weak("Logging to stderr only"),
        };
    });
    ui.separator();
    let allowed = &LEVELS[..=v.level];
    egui::ScrollArea::both().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
        for line in v
            .text
            .lines()
            .filter(|l| allowed.iter().any(|lv| l.contains(&format!(" {lv} "))) || !LEVELS.iter().any(|lv| l.contains(&format!(" {lv} "))))
        {
            let color = if line.contains(" ERROR ") {
                ui.visuals().error_fg_color
            } else if line.contains(" WARN ") {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().text_color()
            };
            ui.label(egui::RichText::new(line).monospace().color(color));
        }
    });
}

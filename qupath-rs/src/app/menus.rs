//! The menu bar, built from the command table.

use egui::{Button, MenuBar};

use super::QuPathApp;
use super::menu_table::{Item, MENUS};

impl QuPathApp {
    /// Draws the menu bar.
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        MenuBar::new().ui(ui, |ui| {
            for (name, items) in MENUS {
                ui.menu_button(*name, |ui| {
                    self.menu_items(ui, items);
                    if *name == "File" {
                        self.recent_projects(ui);
                    }
                });
            }
        });
    }

    fn menu_items(&mut self, ui: &mut egui::Ui, items: &[Item]) {
        for item in items {
            match item {
                Item::Sep => {
                    ui.separator();
                }
                Item::Sub(name, sub) => {
                    ui.menu_button(*name, |ui| self.menu_items(ui, sub));
                }
                Item::Cmd(cmd) => {
                    let mut label = cmd.label().to_owned();
                    if let Some(on) = self.checked(*cmd) {
                        label = format!("{} {label}", if on { "✔" } else { "    " });
                    }
                    let mut b = Button::new(label);
                    if let Some(sc) = cmd.shortcut() {
                        b = b.shortcut_text(ui.ctx().format_shortcut(&sc));
                    }
                    if ui.add_enabled(self.enabled(*cmd), b).clicked() {
                        self.s.queued.push(*cmd);
                        ui.close();
                    }
                }
            }
        }
    }

    fn recent_projects(&mut self, ui: &mut egui::Ui) {
        let recent = self.s.prefs.recent_projects.clone();
        if recent.is_empty() {
            return;
        }
        ui.separator();
        ui.menu_button("Recent projects", |ui| {
            for p in recent {
                let label =
                    p.parent().and_then(|d| d.file_name()).map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
                if ui.button(label).on_hover_text(p.display().to_string()).clicked() {
                    let ctx = ui.ctx().clone();
                    self.open_project(&ctx, &p);
                    ui.close();
                }
            }
        });
    }
}

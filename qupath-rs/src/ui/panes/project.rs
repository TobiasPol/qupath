//! The Project tab: images with thumbnails, filter, open, rename and remove.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::app::QuPathApp;
use crate::app::commands::Command;
use crate::ui::dialogs::{Dialog, PendingOpen};

/// Thumbnails and the filter.
#[derive(Default)]
pub struct ProjectPane {
    thumbs: HashMap<u64, Option<egui::TextureHandle>>,
    filter: String,
}

impl ProjectPane {
    /// Reloads the thumbnails just written.
    pub fn refresh_thumbnails(&mut self, list: &[(u64, PathBuf)]) {
        for (id, _) in list {
            self.thumbs.remove(id);
        }
    }

    /// Forgets everything (another project).
    pub fn clear(&mut self) {
        self.thumbs.clear();
        self.filter.clear();
    }

    fn thumb(&mut self, ctx: &egui::Context, id: u64, file: PathBuf) -> Option<egui::TextureId> {
        self.thumbs
            .entry(id)
            .or_insert_with(|| {
                let img = image::open(&file).ok()?.into_rgb8();
                let ci = egui::ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw());
                Some(ctx.load_texture(format!("thumb-{id}"), ci, egui::TextureOptions::LINEAR))
            })
            .as_ref()
            .map(egui::TextureHandle::id)
    }
}

/// Draws the Project tab.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(project) = &app.s.project else {
        ui.label("No project open.");
        ui.horizontal(|ui| {
            if ui.button("Create project").clicked() {
                app.s.queued.push(Command::ProjectNew);
            }
            if ui.button("Open project").clicked() {
                app.s.queued.push(Command::ProjectOpen);
            }
        });
        return;
    };
    ui.horizontal(|ui| {
        ui.strong(project.name());
        ui.weak(format!("{} images", project.entries.len()));
    });
    ui.horizontal(|ui| {
        if ui.button("Add images").clicked() {
            app.s.queued.push(Command::ProjectAddImages);
        }
        ui.add(egui::TextEdit::singleline(&mut app.ui.project.filter).hint_text("Filter images").desired_width(f32::INFINITY));
    });
    ui.separator();
    let filter = app.ui.project.filter.to_lowercase();
    let entries: Vec<(u64, String, PathBuf)> = project
        .entries
        .iter()
        .filter(|e| {
            filter.is_empty() || e.name.to_lowercase().contains(&filter) || e.tags.iter().any(|t| t.to_lowercase().contains(&filter))
        })
        .map(|e| (e.id, e.name.clone(), project.entry_dir(e.id).join("thumbnail.jpg")))
        .collect();
    let current = app.s.image.as_ref().and_then(|d| d.entry);
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (id, name, thumb) in entries {
            entry_row(ui, app, id, &name, thumb, current == Some(id));
        }
    });
}

fn entry_row(ui: &mut egui::Ui, app: &mut QuPathApp, id: u64, name: &str, thumb: PathBuf, current: bool) {
    let ctx = ui.ctx().clone();
    let resp = ui
        .horizontal(|ui| {
            match app.ui.project.thumb(&ctx, id, thumb) {
                Some(tex) => {
                    ui.add(egui::Image::new((tex, egui::vec2(64.0, 48.0))).maintain_aspect_ratio(true).max_size(egui::vec2(64.0, 48.0)));
                }
                None => {
                    ui.add_sized([64.0, 48.0], egui::Label::new("—"));
                }
            }
            let text = if current { egui::RichText::new(name).strong() } else { egui::RichText::new(name) };
            ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()))
        })
        .inner;
    let row = resp.on_hover_text("Double-click to open");
    if row.double_clicked() {
        app.request_open(&ctx, PendingOpen::Entry(id));
    }
    row.context_menu(|ui| {
        if ui.button("Open image").clicked() {
            app.request_open(&ctx, PendingOpen::Entry(id));
            ui.close();
        }
        if ui.button("Rename image").clicked() {
            app.s.dialog = Some(Dialog::Rename { id, name: name.to_owned() });
            ui.close();
        }
        if ui.button("Remove image").clicked() {
            if let Some(p) = &mut app.s.project {
                p.remove(id);
            }
            app.save_project();
            ui.close();
        }
    });
}

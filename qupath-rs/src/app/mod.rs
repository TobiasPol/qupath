//! The application: menus, toolbar, analysis pane, viewer and tool windows.

mod automation;
pub mod commands;
pub mod data;
mod dialogs;
mod export;
mod files;
pub mod menu_table;
mod menus;
mod objects;
mod run;
pub mod state;
mod statusbar;
pub mod tasks;
mod toolbar;

use std::path::PathBuf;

use egui::{CentralPanel, Panel};

use crate::io::prefs::Prefs;
use crate::ui::style::Palette;
use crate::viewer::{Viewer, ViewerArgs};
use state::AppState;
use tasks::TaskResult;

/// The QuPath window.
pub struct QuPathApp {
    /// Everything the interface works on.
    pub s: AppState,
    /// The image viewer.
    pub viewer: Viewer,
    /// Per-window state of tool windows (tables, log, panes).
    pub ui: crate::ui::UiState,
    auto: automation::Automation,
}

impl QuPathApp {
    /// Creates the app; `open` is a file or project given on the command line.
    pub fn new(ctx: &egui::Context, prefs: Prefs, palette: Palette, log_file: Option<PathBuf>, open: Option<PathBuf>) -> Self {
        let viewer = Viewer::new(ctx, prefs.tile_cache_mb);
        let mut app = QuPathApp {
            s: AppState::new(ctx, prefs, palette, log_file),
            viewer,
            ui: crate::ui::UiState::default(),
            auto: automation::Automation::from_env(),
        };
        if let Some(p) = open {
            app.open_path(ctx, &p);
        }
        app
    }

    fn handle_results(&mut self, ctx: &egui::Context) {
        for r in self.s.tasks.poll() {
            match r {
                TaskResult::ImageOpened(data) => self.install_image(ctx, *data),
                TaskResult::Objects { image_id, groups, replace, insert, step } => {
                    self.add_objects(ctx, image_id, groups, replace, insert, step);
                }
                TaskResult::ProjectThumbnails(list) => self.ui.project.refresh_thumbnails(&list),
                TaskResult::Message(m) => self.s.notify(ctx, m),
                TaskResult::Error(e) => self.s.error(ctx, e),
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        for cmd in commands::Command::ALL {
            let Some(sc) = cmd.shortcut() else { continue };
            if typing && cmd.is_plain_key() {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                self.s.queued.push(*cmd);
            }
        }
        if !typing && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            self.s.queued.push(commands::Command::DeleteSelected);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        for p in dropped {
            self.open_path(ctx, &p);
        }
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let Some(data) = self.s.image.as_mut() else {
            crate::ui::welcome::show(ui, &mut self.s);
            return;
        };
        let args = ViewerArgs {
            data,
            overlay: &self.s.overlay,
            tool: self.s.tool,
            prefs: &self.s.prefs,
            auto_class: self.s.auto_class.clone(),
            selection_mode: self.s.selection_mode,
        };
        let (response, finished) = self.viewer.show(ui, args);
        if finished && self.s.prefs.return_to_move_tool && self.s.tool.returns_to_move() {
            self.s.tool = crate::viewer::tools::Tool::Move;
        }
        response.context_menu(|ui| crate::ui::context_menu::show(ui, &mut self.s));
    }
}

impl eframe::App for QuPathApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_results(&ctx);
        self.handle_shortcuts(&ctx);
        self.handle_drops(&ctx);
        Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        if self.s.show_analysis_pane {
            Panel::left("analysis").resizable(true).default_size(330.0).min_size(240.0).show(ui, |ui| {
                crate::ui::panes::show(ui, self);
            });
        }
        CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.central(ui));
        crate::ui::windows::show(&ctx, self);
        self.show_dialog(&ctx);
        let queued = std::mem::take(&mut self.s.queued);
        for cmd in queued {
            self.run(&ctx, cmd);
        }
        self.handle_screenshot(&ctx);
        self.automate(&ctx);
        self.update_title(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_on_exit();
        self.s.writer.flush(fastframe_file::FLUSH_TIMEOUT);
    }
}

impl QuPathApp {
    fn update_title(&mut self, ctx: &egui::Context) {
        let mut title = String::from("QuPath (qupath-rs)");
        if let Some(p) = &self.s.project {
            title = format!("{title} — {}", p.name());
        }
        if let Some(d) = &self.s.image {
            title = format!("{title} — {}{}", d.name(), if d.is_changed() { " *" } else { "" });
        }
        if self.ui.title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.ui.title = title;
        }
    }
}

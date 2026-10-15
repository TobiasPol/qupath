//! Everything the interface works on.

use std::path::PathBuf;

use super::data::ImageData;
use super::tasks::Tasks;
use crate::io::prefs::Prefs;
use crate::io::project::Project;
use crate::objects::PathClass;
use crate::ui::dialogs::Dialog;
use crate::ui::style::Palette;
use crate::viewer::overlay::OverlayOptions;
use crate::viewer::tools::Tool;

/// A message shown briefly at the bottom of the window.
pub struct Notice {
    /// Text.
    pub text: String,
    /// Shown as an error.
    pub error: bool,
    /// When it appeared (egui time).
    pub at: f64,
}

/// Which tool windows are open.
#[derive(Default)]
pub struct Windows {
    /// Brightness/Contrast.
    pub brightness: bool,
    /// Preferences.
    pub prefs: bool,
    /// Log.
    pub log: bool,
    /// Command list.
    pub command_list: bool,
    /// About.
    pub about: bool,
    /// Annotation measurement table.
    pub annotation_table: bool,
    /// Detection measurement table.
    pub detection_table: bool,
    /// Slide label.
    pub slide_label: bool,
}

/// The application's state, shared by menus, panes and the viewer.
pub struct AppState {
    /// Preferences.
    pub prefs: Prefs,
    /// The open project.
    pub project: Option<Project>,
    /// The open image.
    pub image: Option<ImageData>,
    /// Active tool.
    pub tool: Tool,
    /// Shapes select rather than create.
    pub selection_mode: bool,
    /// What the viewer draws.
    pub overlay: OverlayOptions,
    /// Classes offered in the Annotations tab (Unclassified is implicit).
    pub classes: Vec<PathClass>,
    /// Class given to new annotations ("Auto set").
    pub auto_class: Option<PathClass>,
    /// Saves files off the interface thread.
    pub writer: fastframe_file::Writer,
    /// Background work.
    pub tasks: Tasks,
    /// Messages for the status bar.
    pub notices: Vec<Notice>,
    /// Open tool windows.
    pub windows: Windows,
    /// The modal dialog, if any.
    pub dialog: Option<Dialog>,
    /// Left pane visible.
    pub show_analysis_pane: bool,
    /// This run's log file.
    pub log_file: Option<PathBuf>,
    /// Colors in use.
    pub palette: Palette,
    /// Counter for image ids.
    pub next_image_id: u64,
    /// Command queued by a pane or dialog, run after the frame.
    pub queued: Vec<super::commands::Command>,
    /// A screenshot of the viewer was requested.
    pub snapshot_requested: Option<PathBuf>,
}

impl AppState {
    /// Fresh state.
    pub fn new(ctx: &egui::Context, prefs: Prefs, palette: Palette, log_file: Option<PathBuf>) -> Self {
        let mut classes: Vec<PathClass> = crate::objects::classes::default_classes().into_iter().flatten().collect();
        for name in &prefs.default_classes {
            let c = PathClass::new(name);
            if !classes.contains(&c) {
                classes.push(c);
            }
        }
        AppState {
            prefs,
            project: None,
            image: None,
            tool: Tool::Move,
            selection_mode: false,
            overlay: OverlayOptions::default(),
            classes,
            auto_class: None,
            writer: fastframe_file::Writer::new(),
            tasks: Tasks::new(ctx),
            notices: Vec::new(),
            windows: Windows::default(),
            dialog: None,
            show_analysis_pane: true,
            log_file,
            palette,
            next_image_id: 1,
            queued: Vec::new(),
            snapshot_requested: None,
        }
    }

    /// Shows a message in the status bar (and logs it).
    pub fn notify(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let text = text.into();
        log::info!("{text}");
        self.notices.push(Notice { text, error: false, at: ctx.input(|i| i.time) });
    }

    /// Shows an error in the status bar (and logs it).
    pub fn error(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let text = text.into();
        log::error!("{text}");
        self.notices.push(Notice { text, error: true, at: ctx.input(|i| i.time) });
    }

    /// Saves preferences in the background.
    pub fn save_prefs(&self) {
        if let Some(p) = crate::io::prefs::prefs_file() {
            self.writer.write(&p, self.prefs.to_json(), fastframe_file::Access::Default);
        }
    }

    /// A new id for an opened image.
    pub fn new_image_id(&mut self) -> u64 {
        self.next_image_id += 1;
        self.next_image_id
    }

    /// Where data for an image outside a project is kept.
    pub fn sidecar_dir(path: &std::path::Path) -> Option<PathBuf> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        path.hash(&mut h);
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        crate::io::prefs::dirs().map(|d| d.data_dir().join("images").join(format!("{name}-{:016x}", h.finish())))
    }
}

//! Interface pieces: style, icons, the analysis pane, tool windows and dialogs.

pub mod context_menu;
pub mod dialogs;
pub mod icons;
pub mod panes;
pub mod style;
pub mod welcome;
pub mod windows;

/// State of panes and windows that is not part of the document.
#[derive(Default)]
pub struct UiState {
    /// Window title last sent.
    pub title: String,
    /// A screenshot was asked for and has not arrived.
    pub screenshot_pending: bool,
    /// Which analysis tab is shown.
    pub pane: panes::Pane,
    /// Project tab state.
    pub project: panes::project::ProjectPane,
    /// Annotation measurement table.
    pub annotation_table: windows::measurements::Table,
    /// Detection measurement table.
    pub detection_table: windows::measurements::Table,
    /// Log window state.
    pub log: windows::log::LogView,
    /// Command list filter.
    pub command_filter: String,
    /// Selected row in the class list (0 = Unclassified).
    pub class_index: usize,
    /// Brightness/Contrast selected channel.
    pub bc_channel: usize,
    /// Cached Brightness/Contrast histogram: (image, channel, stain) → (counts, low, high).
    #[allow(clippy::type_complexity)]
    pub bc_hist: Option<((u64, usize, Option<[f64; 3]>), (Vec<u32>, f32, f32))>,
    /// Slide label texture.
    pub label: Option<(u64, egui::TextureHandle)>,
    /// Hierarchy tab: expanded nodes are remembered by egui; this is the filter.
    pub hierarchy_filter: String,
}

impl UiState {
    /// Forgets per-image state when another image opens.
    pub fn reset_for_image(&mut self) {
        self.annotation_table = Default::default();
        self.detection_table = Default::default();
        self.bc_channel = 0;
        self.bc_hist = None;
        self.label = None;
    }
}

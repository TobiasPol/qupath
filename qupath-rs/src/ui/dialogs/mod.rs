//! Modal dialogs. Each holds its own parameters; the app runs the accepted action.

mod analysis;
mod objects;

use egui::{Id, Modal};

use crate::analysis::cells::CellParams;
use crate::analysis::threshold::ThresholdParams;
use crate::analysis::tiles::TileParams;
use crate::objects::{PathClass, Rgb};

/// Export formats for File → Export images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// Pyramidal OME-TIFF (JPEG tiles for RGB, deflate otherwise).
    OmeTiff,
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
}

/// Shape for Specify annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecifyShape {
    /// Rectangle.
    Rectangle,
    /// Ellipse.
    Ellipse,
}

/// Parameters of Objects → Annotations → Specify annotation.
#[derive(Clone, Debug, PartialEq)]
pub struct Specify {
    /// Shape.
    pub shape: SpecifyShape,
    /// Left (or center x when centered).
    pub x: f64,
    /// Top (or center y).
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
    /// Values are in µm.
    pub microns: bool,
    /// Place at the center of the viewer.
    pub centered: bool,
    /// Class.
    pub class: Option<PathClass>,
    /// Name.
    pub name: String,
}

/// An open dialog.
#[derive(Clone, Debug)]
pub enum Dialog {
    /// Cell detection (positive when `params.positive` is set).
    Cells(CellParams),
    /// Create tiles.
    Tiles(TileParams),
    /// Create thresholder.
    Threshold(ThresholdParams),
    /// Specify annotation.
    Specify(Specify),
    /// Expand annotations.
    Expand {
        /// Distance (µm or px).
        distance: f64,
        /// Keep only the ring.
        remove_interior: bool,
    },
    /// Simplify shape.
    Simplify {
        /// Altitude in pixels.
        altitude: f64,
    },
    /// Export images.
    Export {
        /// Downsample.
        downsample: f64,
        /// Format.
        format: ExportFormat,
        /// Apply the display (brightness/contrast, channels).
        rendered: bool,
        /// Only the selected object's bounds.
        selection_only: bool,
    },
    /// Add or edit a class.
    EditClass {
        /// The class being edited (None to add).
        original: Option<PathClass>,
        /// Name.
        name: String,
        /// Color.
        color: Rgb,
    },
    /// Rename a project entry.
    Rename {
        /// Entry id.
        id: u64,
        /// New name.
        name: String,
    },
    /// Unsaved changes before switching images.
    ConfirmDiscard {
        /// What to open afterwards.
        then: PendingOpen,
    },
}

/// What waits behind a "save changes?" question.
#[derive(Clone, Debug)]
pub enum PendingOpen {
    /// An image file.
    Image(std::path::PathBuf),
    /// A project entry.
    Entry(u64),
    /// Close the image.
    Close,
    /// Quit.
    Quit,
}

/// What the user did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    /// Closed without action.
    Cancel,
    /// Run the action.
    Accept,
    /// Alternative action (e.g. "Don't save").
    Alternative,
}

/// Context the dialogs need.
pub struct DialogEnv<'a> {
    /// µm available.
    pub calibrated: bool,
    /// Brightfield stains available.
    pub brightfield: bool,
    /// Image channel names.
    pub channels: Vec<String>,
    /// Classes to choose from.
    pub classes: &'a [PathClass],
}

/// Shows the dialog as a modal.
pub fn show(ctx: &egui::Context, d: &mut Dialog, env: &DialogEnv<'_>) -> Outcome {
    let title = title(d);
    let mut outcome = Outcome::Open;
    let resp = Modal::new(Id::new("qupath-dialog")).show(ctx, |ui| {
        ui.set_min_width(360.0);
        ui.heading(title);
        ui.separator();
        outcome = body(ui, d, env);
    });
    if resp.should_close() && outcome == Outcome::Open {
        outcome = Outcome::Cancel;
    }
    outcome
}

fn title(d: &Dialog) -> &'static str {
    match d {
        Dialog::Cells(p) if p.positive.is_some() => "Positive cell detection",
        Dialog::Cells(_) => "Cell detection",
        Dialog::Tiles(_) => "Create tiles",
        Dialog::Threshold(_) => "Create thresholder",
        Dialog::Specify(_) => "Specify annotation",
        Dialog::Expand { .. } => "Expand annotations",
        Dialog::Simplify { .. } => "Simplify shape",
        Dialog::Export { .. } => "Export image",
        Dialog::EditClass { original: None, .. } => "Add class",
        Dialog::EditClass { .. } => "Edit class",
        Dialog::Rename { .. } => "Rename image",
        Dialog::ConfirmDiscard { .. } => "Save changes?",
    }
}

fn body(ui: &mut egui::Ui, d: &mut Dialog, env: &DialogEnv<'_>) -> Outcome {
    match d {
        Dialog::Cells(p) => analysis::cells(ui, p, env),
        Dialog::Tiles(p) => analysis::tiles(ui, p, env),
        Dialog::Threshold(p) => analysis::threshold(ui, p, env),
        Dialog::Specify(s) => objects::specify(ui, s, env),
        Dialog::Expand { distance, remove_interior } => objects::expand(ui, distance, remove_interior, env),
        Dialog::Simplify { altitude } => objects::simplify(ui, altitude),
        Dialog::Export { downsample, format, rendered, selection_only } => {
            objects::export(ui, downsample, format, rendered, selection_only)
        }
        Dialog::EditClass { name, color, .. } => objects::edit_class(ui, name, color),
        Dialog::Rename { name, .. } => objects::rename(ui, name),
        Dialog::ConfirmDiscard { .. } => confirm_discard(ui),
    }
}

fn confirm_discard(ui: &mut egui::Ui) -> Outcome {
    ui.label("The current image has unsaved changes.");
    ui.add_space(8.0);
    let mut o = Outcome::Open;
    ui.horizontal(|ui| {
        if ui.button("Save").clicked() {
            o = Outcome::Accept;
        }
        if ui.button("Don't save").clicked() {
            o = Outcome::Alternative;
        }
        if ui.button("Cancel").clicked() {
            o = Outcome::Cancel;
        }
    });
    o
}

/// OK / Cancel buttons at the bottom of a form.
pub fn buttons(ui: &mut egui::Ui, ok: &str) -> Outcome {
    ui.add_space(8.0);
    ui.separator();
    let mut o = Outcome::Open;
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            o = Outcome::Cancel;
        }
        if ui.add(egui::Button::new(ok).fill(ui.visuals().selection.bg_fill)).clicked() {
            o = Outcome::Accept;
        }
    });
    o
}

/// A labelled number field.
pub fn number(ui: &mut egui::Ui, label: &str, v: &mut f64, speed: f64, range: std::ops::RangeInclusive<f64>, suffix: &str) {
    ui.label(label);
    ui.add(egui::DragValue::new(v).speed(speed).range(range).suffix(suffix));
    ui.end_row();
}

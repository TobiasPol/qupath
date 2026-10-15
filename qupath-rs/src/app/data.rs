//! An open image with its objects, settings, display and undo history (QuPath's `ImageData`).

use std::sync::Arc;

use crate::image::ImageServer;
use crate::image::display::ImageDisplay;
use crate::image::stains::{ImageType, Stains};
use crate::io::image_data::{ImageSettings, WorkflowStep};
use crate::io::prefs::Prefs;
use crate::objects::{Calibration, Hierarchy};

/// Hierarchies larger than this are not copied for undo.
const UNDO_MAX_OBJECTS: usize = 250_000;
/// Undo steps kept.
const UNDO_STEPS: usize = 20;

/// Snapshots of the hierarchy for undo and redo.
#[derive(Default)]
pub struct UndoStack {
    undo: Vec<Hierarchy>,
    redo: Vec<Hierarchy>,
}

impl UndoStack {
    /// Remembers the hierarchy before a change.
    pub fn checkpoint(&mut self, h: &Hierarchy) {
        if h.len() > UNDO_MAX_OBJECTS {
            log::info!("Too many objects ({}) to keep an undo step", h.len());
            self.undo.clear();
            return;
        }
        self.undo.push(h.clone());
        if self.undo.len() > UNDO_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Restores the previous state.
    pub fn undo(&mut self, h: &mut Hierarchy) -> bool {
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(h, prev));
        h.touch();
        true
    }

    /// Re-applies an undone change.
    pub fn redo(&mut self, h: &mut Hierarchy) -> bool {
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(h, next));
        h.touch();
        true
    }

    /// Whether undo is possible.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

/// An image open in the viewer.
pub struct ImageData {
    /// Unique per opening, for tile caches.
    pub id: u64,
    /// Pixels.
    pub server: Arc<dyn ImageServer>,
    /// Objects.
    pub hierarchy: Hierarchy,
    /// Type, stains, calibration overrides, workflow.
    pub settings: ImageSettings,
    /// How it is shown.
    pub display: ImageDisplay,
    /// Project entry, when opened from a project.
    pub entry: Option<u64>,
    /// Undo history.
    pub undo: UndoStack,
    /// Small RGB image for the overview.
    pub overview: Option<image::RgbImage>,
    /// A small level of the image, for histograms and auto contrast.
    pub preview: Option<crate::image::Tile>,
    saved_version: u64,
    settings_dirty: bool,
}

impl ImageData {
    /// Wraps a freshly opened image.
    pub fn new(id: u64, server: Arc<dyn ImageServer>, hierarchy: Hierarchy, settings: ImageSettings, entry: Option<u64>) -> Self {
        let display = ImageDisplay::new(server.metadata(), settings.image_type, settings.stains.clone());
        let saved_version = hierarchy.version();
        ImageData {
            id,
            server,
            hierarchy,
            settings,
            display,
            entry,
            undo: UndoStack::default(),
            overview: None,
            preview: None,
            saved_version,
            settings_dirty: false,
        }
    }

    /// Pixel size used for measurements.
    pub fn calibration(&self, prefs: &Prefs) -> Calibration {
        if !prefs.use_calibrated_units {
            return Calibration::default();
        }
        let md = self.server.metadata();
        Calibration::from_microns(
            self.settings.pixel_width_microns.or(md.pixel_width_um),
            self.settings.pixel_height_microns.or(md.pixel_height_um),
        )
    }

    /// Unsaved changes?
    pub fn is_changed(&self) -> bool {
        self.settings_dirty || self.hierarchy.version() != self.saved_version
    }

    /// Records that everything was saved.
    pub fn mark_saved(&mut self) {
        self.saved_version = self.hierarchy.version();
        self.settings_dirty = false;
    }

    /// Adds a workflow step (with the equivalent QuPath script line).
    pub fn record(&mut self, name: &str, script: impl Into<String>) {
        self.settings.workflow.push(WorkflowStep { name: name.into(), script: script.into() });
        self.settings_dirty = true;
    }

    /// Changes the image type, resetting stains to that type's defaults.
    pub fn set_image_type(&mut self, t: ImageType) {
        if self.settings.image_type == t {
            return;
        }
        self.settings.image_type = t;
        self.settings.stains = Stains::default_for(t);
        self.rebuild_display();
        self.record("Set image type", format!("setImageType('{}');", t.script_name()));
        if let Some(s) = &self.settings.stains {
            let script = s.script();
            self.record("Set color deconvolution stains", script);
        }
    }

    /// Recreates the display after the type or stains change.
    pub fn rebuild_display(&mut self) {
        let version = self.display.version;
        self.display = ImageDisplay::new(self.server.metadata(), self.settings.image_type, self.settings.stains.clone());
        self.display.version = version + 1;
        self.settings_dirty = true;
    }

    /// Marks settings as changed (pixel size edits, etc.).
    pub fn settings_changed(&mut self) {
        self.settings_dirty = true;
    }

    /// The name to show.
    pub fn name(&self) -> &str {
        &self.server.metadata().name
    }
}

/// Guesses the type of an image from a thumbnail, as QuPath's "estimate" option does.
pub fn estimate_image_type(server: &dyn ImageServer, thumb: Option<&image::RgbImage>) -> ImageType {
    let md = server.metadata();
    if !md.rgb {
        return ImageType::Fluorescence;
    }
    let Some(t) = thumb else { return ImageType::BrightfieldHE };
    // Brightfield has a bright background; fluorescence a dark one.
    let mut lum: Vec<u32> = t.pixels().map(|p| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3).collect();
    lum.sort_unstable();
    let median = lum.get(lum.len() / 2).copied().unwrap_or(255);
    if median < 90 {
        return ImageType::Fluorescence;
    }
    // Whichever stain pair explains the stained pixels with the smaller residual wins.
    let he = Stains::default_for(ImageType::BrightfieldHE);
    let hdab = Stains::default_for(ImageType::BrightfieldHDab);
    let (Some(he), Some(hdab)) = (he, hdab) else { return ImageType::BrightfieldHE };
    let (mut r_he, mut r_dab) = (0.0, 0.0);
    for p in t.pixels().filter(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) < 660) {
        let rgb = [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])];
        r_he += he.deconvolve(rgb)[2].abs();
        r_dab += hdab.deconvolve(rgb)[2].abs();
    }
    if r_dab < r_he { ImageType::BrightfieldHDab } else { ImageType::BrightfieldHE }
}

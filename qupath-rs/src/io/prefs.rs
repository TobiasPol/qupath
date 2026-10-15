//! Preferences (QuPath's `PathPrefs`), stored as JSON in the user's config folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Light, dark, or a fastframe palette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ThemeChoice {
    /// Follow the system.
    System,
    /// QuPath's light (Modena-like) look.
    Light,
    /// QuPath's dark look.
    Dark,
    /// A shared fastframe palette, by file name.
    Palette(String),
}

/// User preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Theme.
    pub theme: ThemeChoice,
    /// Draw the interface with the system font instead of Inter.
    pub system_font: bool,
    /// Tile cache size in MB.
    pub tile_cache_mb: usize,
    /// Brush diameter in screen pixels.
    pub brush_diameter: f32,
    /// Brush size scales with magnification.
    pub brush_scale_by_magnification: bool,
    /// Go back to the Move tool after drawing a shape.
    pub return_to_move_tool: bool,
    /// Measurements in µm when the pixel size is known.
    pub use_calibrated_units: bool,
    /// Annotation line thickness in screen pixels.
    pub annotation_stroke: f32,
    /// Detection line thickness in screen pixels.
    pub detection_stroke: f32,
    /// Overlay opacity, 0..1.
    pub overlay_opacity: f32,
    /// Show annotation names in the viewer.
    pub show_names: bool,
    /// Ask for the image type on opening (otherwise estimate it).
    pub prompt_image_type: bool,
    /// Grid spacing in µm (or pixels when uncalibrated).
    pub grid_spacing: f64,
    /// Points tool marker radius in screen pixels.
    pub point_radius: f32,
    /// Recently opened projects.
    pub recent_projects: Vec<PathBuf>,
    /// Folder of the last file dialog.
    pub last_dir: Option<PathBuf>,
    /// Classes shown in the Annotations tab when no project is open.
    pub default_classes: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: ThemeChoice::System,
            system_font: false,
            tile_cache_mb: 768,
            brush_diameter: 50.0,
            brush_scale_by_magnification: true,
            return_to_move_tool: true,
            use_calibrated_units: true,
            annotation_stroke: 2.0,
            detection_stroke: 1.0,
            overlay_opacity: 1.0,
            show_names: true,
            prompt_image_type: false,
            grid_spacing: 250.0,
            point_radius: 5.0,
            recent_projects: Vec::new(),
            last_dir: None,
            default_classes: Vec::new(),
        }
    }
}

/// The app's folders.
pub fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("org", "qupath-rs", "qupath-rs")
}

/// Where preferences live.
pub fn prefs_file() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("prefs.json"))
}

impl Prefs {
    /// Reads preferences, falling back to defaults when missing or damaged.
    pub fn load() -> Prefs {
        prefs_file().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Serialized preferences.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }

    /// Remembers a project at the front of the recent list.
    pub fn add_recent_project(&mut self, p: PathBuf) {
        self.recent_projects.retain(|x| x != &p);
        self.recent_projects.insert(0, p);
        self.recent_projects.truncate(10);
    }
}

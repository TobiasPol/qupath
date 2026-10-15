//! Per-image data: objects (GeoJSON), image type, stains, calibration and the workflow.
//!
//! QuPath keeps this in a Java-serialized `.qpdata`; here it is two readable
//! files in the entry folder: `objects.geojson` (importable into QuPath) and
//! `qupath-rs.json`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::image::stains::{ImageType, Stains};
use crate::objects::Hierarchy;

/// Settings kept with an image.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImageSettings {
    /// QuPath image type.
    #[serde(default)]
    pub image_type: ImageType,
    /// Stain vectors for brightfield images.
    #[serde(default)]
    pub stains: Option<Stains>,
    /// Pixel width override in µm.
    #[serde(default)]
    pub pixel_width_microns: Option<f64>,
    /// Pixel height override in µm.
    #[serde(default)]
    pub pixel_height_microns: Option<f64>,
    /// Commands run on this image, as QuPath script lines.
    #[serde(default)]
    pub workflow: Vec<WorkflowStep>,
}

/// One recorded command.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorkflowStep {
    /// Command name.
    pub name: String,
    /// Equivalent QuPath script line.
    pub script: String,
}

/// File names inside an entry folder.
pub const OBJECTS_FILE: &str = "objects.geojson";
/// Settings file name.
pub const SETTINGS_FILE: &str = "qupath-rs.json";

/// The files to write for an image's data: (path, bytes).
pub fn serialize(dir: &Path, h: &Hierarchy, settings: &ImageSettings) -> Vec<(PathBuf, Vec<u8>)> {
    let top: Vec<_> = h.children(h.root()).to_vec();
    let geo = super::geojson::export(h, &top, true);
    vec![
        (dir.join(OBJECTS_FILE), serde_json::to_vec(&geo).unwrap_or_default()),
        (dir.join(SETTINGS_FILE), serde_json::to_vec_pretty(settings).unwrap_or_default()),
    ]
}

/// Reads an image's data, if any was saved.
pub fn load(dir: &Path) -> Result<(Hierarchy, ImageSettings)> {
    let mut h = Hierarchy::new();
    let objects = dir.join(OBJECTS_FILE);
    if objects.exists() {
        let text = std::fs::read_to_string(&objects).with_context(|| format!("cannot read {}", objects.display()))?;
        let v: serde_json::Value = serde_json::from_str(&text).context("objects file is not valid GeoJSON")?;
        import_nested(&mut h, &v)?;
    }
    let settings_file = dir.join(SETTINGS_FILE);
    let mut settings: ImageSettings = if settings_file.exists() {
        serde_json::from_str(&std::fs::read_to_string(&settings_file)?).context("settings file is damaged")?
    } else {
        ImageSettings::default()
    };
    if let Some(s) = &mut settings.stains {
        s.update_inverse();
    }
    h.clear_selection();
    Ok((h, settings))
}

/// Saved files keep the hierarchy exactly: top-level features go under the root.
fn import_nested(h: &mut Hierarchy, v: &serde_json::Value) -> Result<()> {
    for f in super::geojson::features_of(v) {
        let (o, children) = super::geojson::feature_to_object(&f)?;
        let k = h.add_child(h.root(), o);
        add_children(h, k, &children)?;
    }
    Ok(())
}

fn add_children(h: &mut Hierarchy, parent: crate::objects::ObjKey, children: &[serde_json::Value]) -> Result<()> {
    for c in children {
        let (o, grandchildren) = super::geojson::feature_to_object(c)?;
        let k = h.add_child(parent, o);
        add_children(h, k, &grandchildren)?;
    }
    Ok(())
}

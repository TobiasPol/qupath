//! What an agent has open: an image with its objects and settings, and a project.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};

use crate::image::ImageServer;
use crate::image::stains::{ImageType, Stains};
use crate::io::image_data::{self, ImageSettings, WorkflowStep};
use crate::io::project::Project;
use crate::objects::{Calibration, Hierarchy, ObjKey, PathClass};

/// An image with its objects, as `app::data::ImageData` without the display state.
pub struct OpenImage {
    /// Pixels.
    pub server: Arc<dyn ImageServer>,
    /// Objects.
    pub hierarchy: Hierarchy,
    /// Type, stains, calibration overrides, workflow.
    pub settings: ImageSettings,
    /// Project entry, when opened from a project.
    pub entry: Option<u64>,
}

impl OpenImage {
    /// Opens an image and the data saved for it in `dir`, guessing the type when unset.
    pub fn open(path: &Path, dir: Option<&Path>, entry: Option<u64>) -> Result<Self> {
        let server = crate::image::open(path)?;
        let (hierarchy, mut settings) = match dir.filter(|d| d.exists()) {
            Some(d) => image_data::load(d)?,
            None => Default::default(),
        };
        if settings.image_type == ImageType::Unset {
            let thumb = crate::image::thumbnail(server.as_ref(), 512).ok();
            settings.image_type = crate::app::data::estimate_image_type(server.as_ref(), thumb.as_ref());
            settings.stains = Stains::default_for(settings.image_type);
        }
        Ok(OpenImage { server, hierarchy, settings, entry })
    }

    /// Pixel size: the override, else the file's.
    pub fn calibration(&self) -> Calibration {
        let md = self.server.metadata();
        Calibration::from_microns(
            self.settings.pixel_width_microns.or(md.pixel_width_um),
            self.settings.pixel_height_microns.or(md.pixel_height_um),
        )
    }

    /// Adds a workflow step (with the equivalent QuPath script line).
    pub fn record(&mut self, name: &str, script: impl Into<String>) {
        self.settings.workflow.push(WorkflowStep { name: name.into(), script: script.into() });
    }

    /// Changes the image type, resetting stains to that type's defaults.
    pub fn set_image_type(&mut self, t: ImageType) {
        self.settings.image_type = t;
        self.settings.stains = Stains::default_for(t);
        self.record("Set image type", format!("{};", crate::io::script::call("setImageType", t.script_name())));
        if let Some(s) = &self.settings.stains {
            let script = s.script();
            self.record("Set color deconvolution stains", script);
        }
    }

    /// The object with this UUID.
    pub fn key(&self, id: &str) -> Result<ObjKey> {
        let id = id.trim();
        self.hierarchy
            .iter()
            .find(|(_, o)| o.id.to_string() == id)
            .map(|(k, _)| k)
            .ok_or_else(|| anyhow!("no object with id {id}; list_objects shows the ids"))
    }

    /// Objects for several UUIDs.
    pub fn keys(&self, ids: &[String]) -> Result<Vec<ObjKey>> {
        let index: std::collections::HashMap<String, ObjKey> = self.hierarchy.iter().map(|(k, o)| (o.id.to_string(), k)).collect();
        ids.iter()
            .map(|id| index.get(id.trim()).copied().ok_or_else(|| anyhow!("no object with id {id}; list_objects shows the ids")))
            .collect()
    }

    /// Area annotations to analyze: the given ids, else every area annotation.
    pub fn targets(&self, ids: Option<&[String]>) -> Result<Vec<ObjKey>> {
        let keys = match ids {
            Some(ids) if !ids.is_empty() => self.keys(ids)?,
            _ => self.hierarchy.annotations().map(|(k, _)| k).collect(),
        };
        let area: Vec<ObjKey> =
            keys.into_iter().filter(|k| self.hierarchy.get(*k).is_some_and(|o| o.is_annotation() && o.roi.is_area())).collect();
        if area.is_empty() {
            bail!("no area annotation to work in: add one with add_annotation (shape full_image covers the whole image)");
        }
        Ok(area)
    }
}

/// Everything an agent works with.
#[derive(Default)]
pub struct Session {
    /// The current image.
    pub image: Option<OpenImage>,
    /// The current project.
    pub project: Option<Project>,
}

impl Session {
    /// The current image, or an error telling the agent what to do.
    pub fn image(&self) -> Result<&OpenImage> {
        self.image.as_ref().context("no image is open: call open_image (or open_project_image) first")
    }

    /// The current image, for changing it.
    pub fn image_mut(&mut self) -> Result<&mut OpenImage> {
        self.image.as_mut().context("no image is open: call open_image (or open_project_image) first")
    }

    /// The current project.
    pub fn project(&self) -> Result<&Project> {
        self.project.as_ref().context("no project is open: call open_project or create_project first")
    }

    /// Opens an image file; when it belongs to the open project, that entry's data comes with it.
    pub fn open_image(&mut self, path: &Path) -> Result<&OpenImage> {
        let path = path.canonicalize().with_context(|| format!("cannot find {}", path.display()))?;
        let entry = self.project.as_ref().and_then(|p| p.entries.iter().find(|e| e.path == path).map(|e| e.id));
        match entry {
            Some(id) => self.open_entry(id),
            None => {
                self.image = Some(OpenImage::open(&path, None, None)?);
                self.image()
            }
        }
    }

    /// Opens a project entry with its saved objects.
    pub fn open_entry(&mut self, id: u64) -> Result<&OpenImage> {
        let p = self.project()?;
        let e = p.entry(id).with_context(|| format!("the project has no image with id {id}"))?;
        let dir = p.entry_dir(id);
        self.image = Some(OpenImage::open(&e.path, Some(&dir), Some(id))?);
        self.image()
    }

    /// Writes the current entry's data and the project; returns the data folder, or
    /// `None` for an image outside a project (export_geojson keeps its objects).
    pub fn save(&self) -> Result<Option<PathBuf>> {
        let (Some(img), Some(p)) = (&self.image, &self.project) else { return Ok(None) };
        let Some(id) = img.entry else { return Ok(None) };
        let dir = p.entry_dir(id);
        std::fs::create_dir_all(&dir)?;
        for (file, bytes) in image_data::serialize(&dir, &img.hierarchy, &img.settings) {
            fastframe_file::write(&file, bytes)?;
        }
        self.save_project()?;
        Ok(Some(dir))
    }

    /// Writes `project.qpproj` and its classes, adding classes the objects use.
    pub fn save_project(&self) -> Result<()> {
        let p = self.project()?;
        let mut classes: Vec<PathClass> =
            p.read_classes().unwrap_or_else(|| crate::objects::classes::default_classes().into_iter().flatten().collect());
        if let Some(img) = &self.image {
            for c in img.hierarchy.iter().filter_map(|(_, o)| o.class.clone()) {
                if !classes.contains(&c) {
                    classes.push(c);
                }
            }
        }
        std::fs::create_dir_all(p.base())?;
        fastframe_file::write(&p.file, p.to_json())?;
        if let Some(dir) = p.classes_file().parent() {
            std::fs::create_dir_all(dir)?;
        }
        fastframe_file::write(&p.classes_file(), Project::classes_json(&classes))?;
        Ok(())
    }
}

//! Opening and saving images, projects and object files.

use std::path::{Path, PathBuf};

use super::QuPathApp;
use super::data::{ImageData, estimate_image_type};
use super::tasks::TaskResult;
use crate::image::stains::{ImageType, Stains};
use crate::io::{geojson, image_data, project::Project};
use crate::ui::dialogs::{Dialog, PendingOpen};

/// File dialogs remember the last folder.
pub fn dialog(s: &super::state::AppState) -> rfd::FileDialog {
    let d = rfd::FileDialog::new();
    match &s.prefs.last_dir {
        Some(dir) => d.set_directory(dir),
        None => d,
    }
}

impl QuPathApp {
    /// Opens whatever a path is: a project, objects for the current image, or an image.
    pub fn open_path(&mut self, ctx: &egui::Context, p: &Path) {
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext == "qpproj" || p.join("project.qpproj").exists() {
            self.open_project(ctx, p);
        } else if matches!(ext.as_str(), "geojson" | "json") {
            self.import_objects_from(ctx, p);
        } else {
            self.request_open(ctx, PendingOpen::Image(p.to_path_buf()));
        }
    }

    /// Opens something else, saving or asking about unsaved changes first.
    pub fn request_open(&mut self, ctx: &egui::Context, then: PendingOpen) {
        let changed = self.s.image.as_ref().is_some_and(ImageData::is_changed);
        let in_project = self.s.image.as_ref().is_some_and(|d| d.entry.is_some());
        if changed && in_project {
            self.save_data(ctx);
        } else if changed {
            self.s.dialog = Some(Dialog::ConfirmDiscard { then });
            return;
        }
        self.proceed(ctx, then);
    }

    /// Carries out a pending open after the question was answered.
    pub fn proceed(&mut self, ctx: &egui::Context, then: PendingOpen) {
        match then {
            PendingOpen::Image(p) => self.spawn_open(p, None),
            PendingOpen::Entry(id) => {
                if let Some(e) = self.s.project.as_ref().and_then(|p| p.entry(id)) {
                    let path = e.path.clone();
                    self.spawn_open(path, Some(id));
                }
            }
            PendingOpen::Close => self.s.image = None,
            PendingOpen::Quit => {
                if let Some(d) = &mut self.s.image {
                    d.mark_saved();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn data_dir(&self, path: &Path, entry: Option<u64>) -> Option<PathBuf> {
        match (entry, &self.s.project) {
            (Some(id), Some(p)) => Some(p.entry_dir(id)),
            _ => super::state::AppState::sidecar_dir(path),
        }
    }

    fn spawn_open(&mut self, path: PathBuf, entry: Option<u64>) {
        let id = self.s.new_image_id();
        let dir = self.data_dir(&path, entry);
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.s.tasks.spawn(&format!("Opening {name}"), move |progress| match open_image(id, &path, dir.as_deref(), entry, progress) {
            Ok(d) => TaskResult::ImageOpened(Box::new(d)),
            Err(e) => TaskResult::Error(format!("{e:#}")),
        });
    }

    /// Shows a freshly opened image.
    pub fn install_image(&mut self, ctx: &egui::Context, data: ImageData) {
        let md = data.server.metadata();
        self.s.notify(
            ctx,
            format!("Opened {} ({}×{}, {}, {})", md.name, md.width(), md.height(), md.server_type, data.settings.image_type.label()),
        );
        if let Some(dir) = md.path.parent() {
            self.s.prefs.last_dir = Some(dir.to_path_buf());
        }
        self.ui.reset_for_image();
        self.s.image = Some(data);
        self.viewer.zoom_to_fit();
    }

    /// Saves the current image's objects and settings.
    pub fn save_data(&mut self, ctx: &egui::Context) {
        let Some(data) = &self.s.image else { return };
        let path = data.server.metadata().path.clone();
        let Some(dir) = self.data_dir(&path, data.entry) else {
            self.s.error(ctx, "No folder to save image data in");
            return;
        };
        for (file, bytes) in image_data::serialize(&dir, &data.hierarchy, &data.settings) {
            self.s.writer.write(&file, bytes, fastframe_file::Access::Default);
        }
        if let Some(d) = &mut self.s.image {
            d.mark_saved();
        }
        self.save_project();
        self.s.notify(ctx, format!("Saved data to {}", dir.display()));
    }

    /// Writes `project.qpproj` and the classes.
    pub fn save_project(&self) {
        if let Some(p) = &self.s.project {
            self.s.writer.write(&p.file, p.to_json(), fastframe_file::Access::Default);
            self.s.writer.write(&p.classes_file(), Project::classes_json(&self.s.classes), fastframe_file::Access::Default);
        }
    }

    /// Saves before quitting when the image belongs to a project.
    pub fn save_on_exit(&mut self) {
        let in_project = self.s.image.as_ref().is_some_and(|d| d.entry.is_some() && d.is_changed());
        if in_project {
            let ctx = egui::Context::default();
            self.save_data(&ctx);
        }
        self.s.save_prefs();
    }

    /// File → Project → Open project (or a path).
    pub fn open_project(&mut self, ctx: &egui::Context, p: &Path) {
        match Project::open(p) {
            Ok(project) => {
                if let Some(classes) = project.read_classes().filter(|c| !c.is_empty()) {
                    self.s.classes = classes;
                }
                self.s.prefs.add_recent_project(project.file.clone());
                self.s.notify(ctx, format!("Opened project {} ({} images)", project.name(), project.entries.len()));
                self.ui.project.clear();
                self.s.project = Some(project);
                self.s.image = None;
                self.s.save_prefs();
                self.ui.pane = crate::ui::panes::Pane::Project;
            }
            Err(e) => self.s.error(ctx, format!("{e:#}")),
        }
    }

    /// File → Project → Create project.
    pub fn create_project(&mut self, ctx: &egui::Context) {
        let Some(dir) = dialog(&self.s).set_title("Choose an empty folder for the project").pick_folder() else { return };
        match Project::create(&dir) {
            Ok(p) => {
                self.s.project = Some(p);
                self.save_project();
                self.s.notify(ctx, format!("Created project in {}", dir.display()));
                self.ui.pane = crate::ui::panes::Pane::Project;
            }
            Err(e) => self.s.error(ctx, format!("{e:#}")),
        }
    }

    /// File → Project → Add images.
    pub fn add_images(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let Some(project) = &mut self.s.project else {
            self.s.error(ctx, "Create or open a project first");
            return;
        };
        let mut added = Vec::new();
        for f in files {
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let openslide = matches!(ext.as_str(), "ndpi" | "mrxs" | "scn" | "vms" | "vmu" | "svslide" | "bif");
            let name = f.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let id = project.add_image(&f, &name, openslide);
            added.push((id, f, project.entry_dir(id)));
        }
        let n = added.len();
        self.save_project();
        self.s.notify(ctx, format!("Added {n} image(s) to the project"));
        self.s.tasks.spawn("Creating thumbnails", move |progress| {
            let mut done = Vec::new();
            for (i, (id, path, dir)) in added.iter().enumerate() {
                progress.set(i as f32 / n.max(1) as f32);
                match write_thumbnail(path, dir) {
                    Ok(t) => done.push((*id, t)),
                    Err(e) => log::warn!("No thumbnail for {}: {e:#}", path.display()),
                }
            }
            TaskResult::ProjectThumbnails(done)
        });
    }

    /// Objects from a GeoJSON file into the current image.
    pub fn import_objects_from(&mut self, ctx: &egui::Context, p: &Path) {
        let Some(data) = &mut self.s.image else {
            self.s.error(ctx, "Open an image before importing objects");
            return;
        };
        let result = std::fs::read_to_string(p)
            .map_err(anyhow::Error::from)
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(anyhow::Error::from))
            .and_then(|v| {
                data.undo.checkpoint(&data.hierarchy);
                geojson::import(&mut data.hierarchy, &v)
            });
        match result {
            Ok(n) => {
                data.record("Import objects", format!("importObjectsFromFile('{}')", p.display()));
                self.s.notify(ctx, format!("Imported {n} objects"));
            }
            Err(e) => self.s.error(ctx, format!("Cannot import {}: {e:#}", p.display())),
        }
    }
}

fn open_image(
    id: u64,
    path: &Path,
    dir: Option<&Path>,
    entry: Option<u64>,
    progress: &super::tasks::Progress,
) -> anyhow::Result<ImageData> {
    let server = crate::image::open(path)?;
    progress.set(0.3);
    let thumb = crate::image::thumbnail(server.as_ref(), 512).ok();
    progress.set(0.6);
    let (hierarchy, mut settings) = match dir.filter(|d| d.exists()) {
        Some(d) => image_data::load(d)?,
        None => Default::default(),
    };
    if settings.image_type == ImageType::Unset {
        settings.image_type = estimate_image_type(server.as_ref(), thumb.as_ref());
        settings.stains = Stains::default_for(settings.image_type);
    }
    let mut data = ImageData::new(id, server.clone(), hierarchy, settings, entry);
    let md = server.metadata();
    let ds = (f64::from(md.width().max(md.height())) / 1024.0).max(1.0);
    let req = crate::image::RegionRequest { downsample: ds, x: 0.0, y: 0.0, w: f64::from(md.width()), h: f64::from(md.height()) };
    data.preview = crate::image::read_region(server.as_ref(), &req).ok();
    data.overview = thumb;
    progress.set(1.0);
    Ok(data)
}

/// Writes `thumbnail.jpg` into an entry folder (as QuPath does).
fn write_thumbnail(path: &Path, dir: &Path) -> anyhow::Result<PathBuf> {
    let server = crate::image::open(path)?;
    let thumb = crate::image::thumbnail(server.as_ref(), 256)?;
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85).encode_image(&thumb)?;
    let file = dir.join("thumbnail.jpg");
    fastframe_file::write(&file, bytes)?;
    Ok(file)
}

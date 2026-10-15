//! Running commands from menus, shortcuts, the toolbar and the command list.

use super::QuPathApp;
use super::commands::Command as C;
use crate::ui::dialogs::{Dialog, ExportFormat, PendingOpen};
use crate::viewer::overlay::CellDisplay;
use crate::viewer::tools::Tool;

impl QuPathApp {
    /// Whether a command can run now (menus grey out the others).
    pub fn enabled(&self, cmd: C) -> bool {
        let image = self.s.image.is_some();
        let project = self.s.project.is_some();
        let selected = self.s.image.as_ref().is_some_and(|d| d.hierarchy.primary_selected().is_some());
        match cmd {
            C::ProjectClose | C::ProjectAddImages => project,
            C::Undo => self.s.image.as_ref().is_some_and(|d| d.undo.can_undo()),
            C::Redo => self.s.image.as_ref().is_some_and(|d| d.undo.can_redo()),
            C::DeleteSelected | C::LockSelected | C::UnlockSelected | C::ToggleLock | C::CopySelected | C::InsertIntoHierarchy => selected,
            C::DuplicateAnnotation | C::ExpandAnnotations | C::SplitAnnotations | C::MergeSelected | C::SubtractSelected => selected,
            C::MakeInverse | C::SimplifyShape => selected,
            C::OpenImage | C::ProjectNew | C::ProjectOpen | C::Quit | C::Preferences | C::ResetPreferences | C::ShowAnalysisPane => true,
            C::CommandList | C::ShowLog | C::About | C::Documentation => true,
            c if is_toggle(c) || is_tool(c) => true,
            _ => image,
        }
    }

    /// Runs a command.
    pub fn run(&mut self, ctx: &egui::Context, cmd: C) {
        if !self.enabled(cmd) {
            return;
        }
        if self.run_file(ctx, cmd) || self.run_view(ctx, cmd) || self.run_objects(ctx, cmd) {
            return;
        }
        self.run_analysis(ctx, cmd);
    }

    fn run_file(&mut self, ctx: &egui::Context, cmd: C) -> bool {
        match cmd {
            C::OpenImage => {
                let exts: Vec<&str> = crate::image::EXTENSIONS.to_vec();
                if let Some(p) = super::files::dialog(&self.s).add_filter("Images", &exts).add_filter("All files", &["*"]).pick_file() {
                    self.open_path(ctx, &p);
                }
            }
            C::ProjectNew => self.create_project(ctx),
            C::ProjectOpen => {
                if let Some(p) = super::files::dialog(&self.s).add_filter("QuPath project", &["qpproj"]).pick_file() {
                    self.open_project(ctx, &p);
                }
            }
            C::ProjectClose => {
                self.request_open(ctx, PendingOpen::Close);
                self.s.project = None;
            }
            C::ProjectAddImages => {
                let exts: Vec<&str> = crate::image::EXTENSIONS.to_vec();
                if let Some(files) = super::files::dialog(&self.s).add_filter("Images", &exts).pick_files() {
                    self.add_images(ctx, files);
                }
            }
            C::SaveData => self.save_data(ctx),
            C::ReloadData => {
                if let Some(d) = &self.s.image {
                    let then = d.entry.map_or_else(|| PendingOpen::Image(d.server.metadata().path.clone()), PendingOpen::Entry);
                    self.proceed(ctx, then);
                }
            }
            C::ExportOriginal | C::ExportRendered => {
                let rendered = cmd == C::ExportRendered;
                self.s.dialog = Some(Dialog::Export { downsample: 4.0, format: ExportFormat::OmeTiff, rendered, selection_only: false });
            }
            C::ExportSnapshot => self.request_snapshot(),
            C::ImportObjects => {
                if let Some(p) = super::files::dialog(&self.s).add_filter("GeoJSON", &["geojson", "json"]).pick_file() {
                    self.import_objects_from(ctx, &p);
                }
            }
            C::ExportGeoJson => self.export_geojson(ctx),
            C::Quit => self.request_open(ctx, PendingOpen::Quit),
            _ => return false,
        }
        true
    }

    fn run_view(&mut self, ctx: &egui::Context, cmd: C) -> bool {
        if let Some(tool) = tool_for(cmd) {
            self.s.tool = tool;
            self.viewer.gesture = Default::default();
            return true;
        }
        let o = &mut self.s.overlay;
        let w = &mut self.s.windows;
        match cmd {
            C::SelectionMode => self.s.selection_mode = !self.s.selection_mode,
            C::ShowAnalysisPane => self.s.show_analysis_pane = !self.s.show_analysis_pane,
            C::CommandList => w.command_list = !w.command_list,
            C::BrightnessContrast => w.brightness = !w.brightness,
            C::Preferences => w.prefs = !w.prefs,
            C::ShowLog => w.log = !w.log,
            C::About => w.about = true,
            C::SlideLabel => w.slide_label = !w.slide_label,
            C::AnnotationMeasurements => w.annotation_table = true,
            C::DetectionMeasurements => w.detection_table = true,
            C::Documentation => ctx.open_url(egui::OpenUrl::new_tab("https://qupath.readthedocs.io/")),
            C::ResetPreferences => {
                self.s.prefs = Default::default();
                self.s.save_prefs();
                self.s.palette = crate::ui::style::apply_theme(ctx, &self.s.prefs);
            }
            C::ShowAnnotations => o.show_annotations = !o.show_annotations,
            C::FillAnnotations => o.fill_annotations = !o.fill_annotations,
            C::ShowNames => o.show_names = !o.show_names,
            C::ShowDetections => o.show_detections = !o.show_detections,
            C::FillDetections => o.fill_detections = !o.fill_detections,
            C::CellBoundaries => o.cell_display = CellDisplay::Boundaries,
            C::CellNuclei => o.cell_display = CellDisplay::Nuclei,
            C::CellBoth => o.cell_display = CellDisplay::Both,
            C::CellCentroids => o.cell_display = CellDisplay::Centroids,
            C::ShowOverview => o.show_overview = !o.show_overview,
            C::ShowLocation => o.show_location = !o.show_location,
            C::ShowScalebar => o.show_scalebar = !o.show_scalebar,
            C::ShowGrid => o.show_grid = !o.show_grid,
            C::ZoomToFit => self.viewer.zoom_to_fit(),
            C::ZoomIn | C::ZoomOut | C::Zoom400 | C::Zoom100 | C::Zoom10 | C::Zoom1 => self.zoom(cmd),
            _ => return false,
        }
        true
    }

    fn zoom(&mut self, cmd: C) {
        let Some(d) = &self.s.image else { return };
        let size = (d.server.metadata().width(), d.server.metadata().height());
        let t = &mut self.viewer.t;
        match cmd {
            C::ZoomIn => crate::viewer::input::zoom_center(t, 1.0 / 1.25, size),
            C::ZoomOut => crate::viewer::input::zoom_center(t, 1.25, size),
            C::Zoom400 => t.downsample = 0.25,
            C::Zoom100 => t.downsample = 1.0,
            C::Zoom10 => t.downsample = 10.0,
            _ => t.downsample = 100.0,
        }
    }

    /// Whether a toggle command is on (menus show a check mark).
    pub fn checked(&self, cmd: C) -> Option<bool> {
        let o = &self.s.overlay;
        let w = &self.s.windows;
        Some(match cmd {
            C::SelectionMode => self.s.selection_mode,
            C::ShowAnalysisPane => self.s.show_analysis_pane,
            C::BrightnessContrast => w.brightness,
            C::ShowLog => w.log,
            C::SlideLabel => w.slide_label,
            C::ShowAnnotations => o.show_annotations,
            C::FillAnnotations => o.fill_annotations,
            C::ShowNames => o.show_names,
            C::ShowDetections => o.show_detections,
            C::FillDetections => o.fill_detections,
            C::CellBoundaries => o.cell_display == CellDisplay::Boundaries,
            C::CellNuclei => o.cell_display == CellDisplay::Nuclei,
            C::CellBoth => o.cell_display == CellDisplay::Both,
            C::CellCentroids => o.cell_display == CellDisplay::Centroids,
            C::ShowOverview => o.show_overview,
            C::ShowLocation => o.show_location,
            C::ShowScalebar => o.show_scalebar,
            C::ShowGrid => o.show_grid,
            c => return tool_for(c).map(|t| t == self.s.tool),
        })
    }
}

/// The tool a command selects.
pub fn tool_for(cmd: C) -> Option<Tool> {
    Some(match cmd {
        C::ToolMove => Tool::Move,
        C::ToolRectangle => Tool::Rectangle,
        C::ToolEllipse => Tool::Ellipse,
        C::ToolLine => Tool::Line,
        C::ToolPolygon => Tool::Polygon,
        C::ToolPolyline => Tool::Polyline,
        C::ToolBrush => Tool::Brush,
        C::ToolWand => Tool::Wand,
        C::ToolPoints => Tool::Points,
        _ => return None,
    })
}

fn is_tool(cmd: C) -> bool {
    tool_for(cmd).is_some()
}

fn is_toggle(cmd: C) -> bool {
    matches!(
        cmd,
        C::SelectionMode
            | C::ShowAnnotations
            | C::FillAnnotations
            | C::ShowNames
            | C::ShowDetections
            | C::FillDetections
            | C::CellBoundaries
            | C::CellNuclei
            | C::CellBoth
            | C::CellCentroids
            | C::ShowOverview
            | C::ShowLocation
            | C::ShowScalebar
            | C::ShowGrid
    )
}

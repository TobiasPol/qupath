//! Opening analysis dialogs and carrying out accepted dialogs.

use super::QuPathApp;
use super::commands::Command as C;
use super::tasks::TaskResult;
use crate::analysis::cells::{self, CellParams, DetectionImage, PositiveParams, Source};
use crate::analysis::threshold::{self, ThresholdParams};
use crate::analysis::tiles::{self, TileParams};
use crate::objects::{ObjKey, PathClass, PathObject, Pt, Roi};
use crate::ui::dialogs::{Dialog, DialogEnv, Outcome, PendingOpen, Specify, SpecifyShape};

impl QuPathApp {
    /// Analysis commands open their dialogs.
    pub(super) fn run_analysis(&mut self, ctx: &egui::Context, cmd: C) {
        let Some(d) = &self.s.image else { return };
        let brightfield = d.settings.stains.is_some();
        let stain2 = d.settings.stains.as_ref().map_or("DAB".into(), |s| s.stains[1].name.clone());
        let mut cells = CellParams::default();
        if !brightfield {
            cells.detection_image = DetectionImage::Channel(0);
            cells.threshold = 100.0;
        }
        self.s.dialog = match cmd {
            C::CellDetection => Some(Dialog::Cells(cells)),
            C::PositiveCellDetection => {
                let positive = PositiveParams { compartment: format!("Nucleus: {stain2} OD mean"), ..PositiveParams::default_dab() };
                Some(Dialog::Cells(CellParams { positive: Some(positive), ..cells }))
            }
            C::CreateTiles => Some(Dialog::Tiles(TileParams::default())),
            C::CreateThresholder => {
                let p = if brightfield {
                    ThresholdParams::default()
                } else {
                    ThresholdParams { above: true, threshold: 20.0, ..Default::default() }
                };
                Some(Dialog::Threshold(p))
            }
            C::ExportMeasurements => {
                self.export_measurements(ctx);
                None
            }
            _ => None,
        };
    }

    /// Shows the open dialog and runs it when accepted.
    pub(super) fn show_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.s.dialog.take() else { return };
        let env = DialogEnv {
            calibrated: self.s.image.as_ref().is_some_and(|d| d.calibration(&self.s.prefs).calibrated),
            brightfield: self.s.image.as_ref().is_some_and(|d| d.settings.stains.is_some()),
            channels: self
                .s
                .image
                .as_ref()
                .map(|d| d.server.metadata().channels.iter().map(|c| c.name.clone()).collect())
                .unwrap_or_default(),
            classes: &self.s.classes,
        };
        match crate::ui::dialogs::show(ctx, &mut dialog, &env) {
            Outcome::Open => self.s.dialog = Some(dialog),
            Outcome::Cancel => {}
            Outcome::Accept => self.accept(ctx, dialog, true),
            Outcome::Alternative => self.accept(ctx, dialog, false),
        }
    }

    /// Accepts a dialog as it stands (automation).
    pub(super) fn accept_for_automation(&mut self, ctx: &egui::Context, dialog: Dialog) {
        self.accept(ctx, dialog, true);
    }

    fn accept(&mut self, ctx: &egui::Context, dialog: Dialog, primary: bool) {
        match dialog {
            Dialog::Cells(p) => self.run_cells(ctx, p),
            Dialog::Tiles(p) => self.run_tiles(ctx, &p),
            Dialog::Threshold(p) => self.run_threshold(ctx, p),
            Dialog::Specify(s) => self.add_specified(ctx, &s),
            Dialog::Expand { distance, remove_interior } => self.with_undo(ctx, "Expand annotations", |h, px| {
                crate::objects::ops::expand_selected(h, distance / px, remove_interior);
            }),
            Dialog::Simplify { altitude } => {
                self.with_undo(ctx, "Simplify shape", |h, _| crate::objects::ops::simplify_selected(h, altitude))
            }
            Dialog::Export { downsample, format, rendered, selection_only } => {
                self.export_image(ctx, downsample, format, rendered, selection_only)
            }
            Dialog::EditClass { original, name, color } => self.edit_class(original, &name, color),
            Dialog::Rename { id, name } => {
                if let Some(e) = self.s.project.as_mut().and_then(|p| p.entry_mut(id)) {
                    e.name = name;
                }
                self.save_project();
            }
            Dialog::ConfirmDiscard { then } => {
                if primary {
                    self.save_data(ctx);
                } else if let Some(d) = &mut self.s.image {
                    d.mark_saved();
                }
                self.proceed(ctx, then);
            }
        }
    }

    fn with_undo(&mut self, ctx: &egui::Context, name: &str, f: impl FnOnce(&mut crate::objects::Hierarchy, f64)) {
        let Some(d) = &mut self.s.image else { return };
        let px = d.calibration(&self.s.prefs).average();
        d.undo.checkpoint(&d.hierarchy);
        f(&mut d.hierarchy, px);
        d.record(name, format!("// {name}"));
        self.s.notify(ctx, name);
    }

    /// Selected annotations, or all annotations when none is selected.
    fn targets(&self) -> Vec<(ObjKey, Roi)> {
        let Some(d) = &self.s.image else { return vec![] };
        let h = &d.hierarchy;
        let area = |k: &ObjKey| h.get(*k).filter(|o| o.is_annotation() && o.roi.is_area()).map(|o| (*k, o.roi.clone()));
        let selected: Vec<_> = h.selected().iter().filter_map(area).collect();
        if selected.is_empty() { h.annotations().filter_map(|(k, _)| area(&k)).collect() } else { selected }
    }

    fn run_cells(&mut self, ctx: &egui::Context, p: CellParams) {
        let targets = self.targets();
        let Some(d) = &self.s.image else { return };
        if targets.is_empty() {
            self.s.error(ctx, "Draw or select an annotation first (or Objects › Annotations › Create full image annotation)");
            return;
        }
        let src = Source { server: d.server.clone(), stains: d.settings.stains.clone(), cal: d.calibration(&self.s.prefs) };
        let (image_id, name) = (d.id, if p.positive.is_some() { "Positive cell detection" } else { "Cell detection" });
        let script = format!("selectAnnotations();\n{}", p.script());
        self.s.tasks.spawn(name, move |progress| {
            let n = targets.len() as f32;
            let mut groups = Vec::new();
            for (i, (k, roi)) in targets.iter().enumerate() {
                let report = |f: f32| {
                    progress.set((i as f32 + f) / n);
                    !progress.cancelled()
                };
                match cells::detect(&src, roi, &p, &report) {
                    Ok(c) => groups.push((*k, c)),
                    Err(e) => return TaskResult::Error(format!("{name}: {e:#}")),
                }
            }
            TaskResult::Objects { image_id, groups, replace: true, insert: false, step: (name.into(), script) }
        });
    }

    fn run_tiles(&mut self, ctx: &egui::Context, p: &TileParams) {
        let targets = self.targets();
        let Some(d) = &self.s.image else { return };
        let cal = d.calibration(&self.s.prefs);
        let groups: Vec<(ObjKey, Vec<PathObject>)> =
            targets.iter().map(|(k, roi)| (*k, tiles::create_tiles(roi, p, cal.pixel_width, cal.pixel_height))).collect();
        if groups.is_empty() {
            self.s.error(ctx, "Select an annotation to tile");
            return;
        }
        let id = d.id;
        self.add_objects(ctx, id, groups, false, false, ("Create tiles".into(), p.script()));
    }

    fn run_threshold(&mut self, _ctx: &egui::Context, p: ThresholdParams) {
        let targets = self.targets();
        let Some(d) = &self.s.image else { return };
        let (server, stains, id) = (d.server.clone(), d.settings.stains.clone(), d.id);
        let cal = d.calibration(&self.s.prefs);
        let area_scale = cal.pixel_width * cal.pixel_height;
        let root = d.hierarchy.root();
        let selected_any = d.hierarchy.primary_selected().is_some();
        self.s.tasks.spawn("Thresholding", move |_| {
            let regions: Vec<Option<Roi>> = if selected_any { targets.into_iter().map(|(_, r)| Some(r)).collect() } else { vec![None] };
            let mut objs = Vec::new();
            for r in regions {
                match threshold::run(server.as_ref(), stains.as_ref(), r.as_ref(), &p, area_scale) {
                    Ok(o) => objs.extend(o),
                    Err(e) => return TaskResult::Error(format!("Thresholder: {e:#}")),
                }
            }
            let step = ("Create objects from threshold".to_owned(), format!("// threshold {} at downsample {}", p.threshold, p.downsample));
            TaskResult::Objects { image_id: id, groups: vec![(root, objs)], replace: false, insert: true, step }
        });
    }

    fn add_specified(&mut self, ctx: &egui::Context, s: &Specify) {
        let center = self.viewer.t.center;
        let Some(d) = &mut self.s.image else { return };
        let cal = d.calibration(&self.s.prefs);
        let (sx, sy) = if s.microns && cal.calibrated { (cal.pixel_width, cal.pixel_height) } else { (1.0, 1.0) };
        let (w, h) = (s.w / sx, s.h / sy);
        let (x, y) = if s.centered { (center.x - w / 2.0, center.y - h / 2.0) } else { (s.x / sx, s.y / sy) };
        let (a, b) = (Pt { x, y }, Pt { x: x + w, y: y + h });
        let roi = if s.shape == SpecifyShape::Ellipse { Roi::ellipse_from_corners(a, b) } else { Roi::rect_from_corners(a, b) };
        let mut o = PathObject::annotation(roi, s.class.clone());
        o.name = (!s.name.trim().is_empty()).then(|| s.name.trim().to_owned());
        d.undo.checkpoint(&d.hierarchy);
        let k = d.hierarchy.insert(o);
        d.hierarchy.select(Some(k));
        d.record("Specify annotation", format!("// createAnnotation({x:.1}, {y:.1}, {w:.1}, {h:.1})"));
        self.s.notify(ctx, "Added annotation");
    }

    fn edit_class(&mut self, original: Option<PathClass>, name: &str, color: crate::objects::Rgb) {
        let new = PathClass::with_color(name.trim(), color);
        match original.and_then(|o| self.s.classes.iter().position(|c| *c == o).map(|i| (i, o))) {
            Some((i, old)) => {
                self.s.classes[i] = new.clone();
                if let Some(d) = &mut self.s.image {
                    let keys: Vec<ObjKey> = d.hierarchy.iter().filter(|(_, o)| o.class.as_ref() == Some(&old)).map(|(k, _)| k).collect();
                    for k in keys {
                        if let Some(o) = d.hierarchy.get_mut(k) {
                            o.class = Some(new.clone());
                        }
                    }
                }
            }
            None if !self.s.classes.contains(&new) => self.s.classes.push(new),
            None => {}
        }
        self.save_project();
    }

    /// Asks before discarding unsaved changes, then quits.
    pub fn request_quit(&mut self, ctx: &egui::Context) {
        self.request_open(ctx, PendingOpen::Quit);
    }
}

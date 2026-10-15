//! Object commands (Objects menu, Edit → Copy/Paste, Undo) and analysis results.

use super::QuPathApp;
use super::commands::Command as C;
use crate::io::geojson;
use crate::objects::{ObjKey, PathObject, Roi, ops};
use crate::ui::dialogs::{Dialog, Specify, SpecifyShape};

impl QuPathApp {
    /// Object commands; false if `cmd` is not one.
    pub(super) fn run_objects(&mut self, ctx: &egui::Context, cmd: C) -> bool {
        let calibrated = self.s.image.as_ref().is_some_and(|d| d.calibration(&self.s.prefs).calibrated);
        match cmd {
            C::SpecifyAnnotation => {
                let class = self.s.auto_class.clone();
                let s = Specify {
                    shape: SpecifyShape::Rectangle,
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                    microns: calibrated,
                    centered: true,
                    class,
                    name: String::new(),
                };
                self.s.dialog = Some(Dialog::Specify(s));
            }
            C::ExpandAnnotations => self.s.dialog = Some(Dialog::Expand { distance: 10.0, remove_interior: false }),
            C::SimplifyShape => self.s.dialog = Some(Dialog::Simplify { altitude: 1.0 }),
            C::Paste => self.paste(ctx),
            C::CopySelected | C::CopyAnnotations => self.copy(ctx, cmd == C::CopyAnnotations),
            _ => return self.edit_hierarchy(ctx, cmd),
        }
        true
    }

    /// Commands that change the hierarchy directly (with an undo step).
    fn edit_hierarchy(&mut self, ctx: &egui::Context, cmd: C) -> bool {
        let auto_class = self.s.auto_class.clone();
        let Some(d) = &mut self.s.image else { return false };
        let hi = &mut d.hierarchy;
        match cmd {
            C::Undo => return d.undo.undo(hi),
            C::Redo => return d.undo.redo(hi),
            C::SelectAnnotations => return select(hi, true),
            C::SelectDetections => return select(hi, false),
            C::ResetSelection => {
                return {
                    hi.clear_selection();
                    true
                };
            }
            C::LockSelected | C::UnlockSelected | C::ToggleLock => return lock(hi, cmd),
            c if !MODIFYING.contains(&c) => return false,
            _ => {}
        }
        let size = (f64::from(d.server.metadata().width()), f64::from(d.server.metadata().height()));
        d.undo.checkpoint(&d.hierarchy);
        let Some((name, script)) = modify(&mut d.hierarchy, cmd, size, auto_class) else { return false };
        d.record(name, script);
        self.s.notify(ctx, name);
        true
    }

    fn copy(&mut self, ctx: &egui::Context, annotations: bool) {
        let Some(d) = &self.s.image else { return };
        let h = &d.hierarchy;
        let keys: Vec<ObjKey> = if annotations { h.annotations().map(|(k, _)| k).collect() } else { h.selected() };
        let text = serde_json::to_string_pretty(&geojson::export(h, &keys, false)).unwrap_or_default();
        ctx.copy_text(text);
        self.s.notify(ctx, format!("Copied {} objects as GeoJSON", keys.len()));
    }

    fn paste(&mut self, ctx: &egui::Context) {
        let text = arboard_text();
        let Some(d) = &mut self.s.image else { return };
        match text.and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
            Some(v) => {
                d.undo.checkpoint(&d.hierarchy);
                match geojson::import(&mut d.hierarchy, &v) {
                    Ok(n) => self.s.notify(ctx, format!("Pasted {n} objects")),
                    Err(e) => self.s.error(ctx, format!("Clipboard is not QuPath GeoJSON: {e:#}")),
                }
            }
            None => self.s.error(ctx, "The clipboard holds no GeoJSON"),
        }
    }

    /// Objects from a background task: detections replace those under each parent.
    pub fn add_objects(
        &mut self,
        ctx: &egui::Context,
        image_id: u64,
        groups: Vec<(ObjKey, Vec<PathObject>)>,
        replace: bool,
        insert: bool,
        step: (String, String),
    ) {
        let Some(d) = self.s.image.as_mut().filter(|d| d.id == image_id) else { return };
        d.undo.checkpoint(&d.hierarchy);
        let mut n = 0;
        for (parent, objs) in groups {
            if replace {
                let old: Vec<ObjKey> = d
                    .hierarchy
                    .children(parent)
                    .iter()
                    .copied()
                    .filter(|k| d.hierarchy.get(*k).is_some_and(PathObject::is_detection))
                    .collect();
                d.hierarchy.remove_many(&old, false);
            }
            n += objs.len();
            if insert {
                for o in objs {
                    d.hierarchy.insert(o);
                }
            } else {
                d.hierarchy.add_children(parent, objs);
            }
        }
        d.record(&step.0, step.1.clone());
        self.s.notify(ctx, format!("{}: {n} objects", step.0));
    }

    pub(super) fn export_geojson(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.s.image else { return };
        let default = format!("{}.geojson", d.name());
        let Some(path) = super::files::dialog(&self.s).set_file_name(default).add_filter("GeoJSON", &["geojson"]).save_file() else {
            return;
        };
        let h = &d.hierarchy;
        let keys: Vec<ObjKey> = h.iter().filter(|(_, o)| o.is_annotation() || o.is_detection()).map(|(k, _)| k).collect();
        let v = geojson::export(h, &keys, false);
        self.s.writer.write(&path, serde_json::to_vec_pretty(&v).unwrap_or_default(), fastframe_file::Access::Default);
        self.s.notify(ctx, format!("Exported {} objects to {}", keys.len(), path.display()));
    }
}

fn arboard_text() -> Option<String> {
    // egui only pastes into text fields; read the system clipboard through a shell tool.
    let out = if cfg!(target_os = "macos") {
        std::process::Command::new("pbpaste").output().ok()?
    } else {
        std::process::Command::new("xclip").args(["-selection", "clipboard", "-o"]).output().ok()?
    };
    String::from_utf8(out.stdout).ok()
}

fn select(h: &mut crate::objects::Hierarchy, annotations: bool) -> bool {
    let keys: Vec<ObjKey> = if annotations { h.annotations().map(|(k, _)| k).collect() } else { h.detections().map(|(k, _)| k).collect() };
    h.clear_selection();
    h.add_to_selection(keys);
    true
}

fn lock(h: &mut crate::objects::Hierarchy, cmd: C) -> bool {
    for k in h.selected() {
        if let Some(o) = h.get_mut(k) {
            o.locked = match cmd {
                C::LockSelected => true,
                C::UnlockSelected => false,
                _ => !o.locked,
            };
        }
    }
    true
}

fn reinsert(h: &mut crate::objects::Hierarchy) {
    let sel = h.selected();
    let objs: Vec<PathObject> = sel.iter().filter_map(|k| h.get(*k).cloned()).collect();
    h.remove_many(&sel, true);
    let keys: Vec<ObjKey> = objs.into_iter().map(|o| h.insert(o)).collect();
    h.add_to_selection(keys);
}

fn duplicate(h: &mut crate::objects::Hierarchy) {
    let objs: Vec<PathObject> = h.selected().iter().filter_map(|k| h.get(*k)).filter(|o| o.is_annotation()).cloned().collect();
    h.clear_selection();
    for mut o in objs {
        o.id = uuid::Uuid::new_v4();
        let k = h.insert(o);
        h.add_to_selection([k]);
    }
}

/// Commands handled by [`modify`].
const MODIFYING: &[C] = &[
    C::DeleteSelected,
    C::DeleteAnnotations,
    C::DeleteDetections,
    C::DeleteAll,
    C::FullImageAnnotation,
    C::InsertIntoHierarchy,
    C::ResolveHierarchy,
    C::DuplicateAnnotation,
    C::SplitAnnotations,
    C::MergeSelected,
    C::SubtractSelected,
    C::MakeInverse,
    C::ResetDetectionClassifications,
];

/// Applies a modifying command; returns its workflow name and script line.
fn modify(
    hi: &mut crate::objects::Hierarchy,
    cmd: C,
    size: (f64, f64),
    auto_class: Option<crate::objects::PathClass>,
) -> Option<(&'static str, &'static str)> {
    delete(hi, cmd).or_else(|| restructure(hi, cmd)).or_else(|| reshape(hi, cmd, size, auto_class))
}

fn delete(hi: &mut crate::objects::Hierarchy, cmd: C) -> Option<(&'static str, &'static str)> {
    Some(match cmd {
        C::DeleteSelected => {
            let sel = hi.selected();
            hi.remove_many(&sel, true);
            ("Delete selected objects", "removeSelectedObjects()")
        }
        C::DeleteAnnotations => {
            hi.remove_where(PathObject::is_annotation);
            ("Delete all annotations", "clearAnnotations()")
        }
        C::DeleteDetections => {
            hi.remove_where(PathObject::is_detection);
            ("Delete all detections", "clearDetections()")
        }
        C::DeleteAll => {
            hi.remove_where(|_| true);
            ("Delete all objects", "clearAllObjects()")
        }
        C::ResetDetectionClassifications => {
            let keys: Vec<ObjKey> = hi.detections().map(|(k, _)| k).collect();
            for k in keys {
                if let Some(o) = hi.get_mut(k) {
                    o.class = None;
                }
            }
            ("Reset detection classifications", "resetDetectionClassifications()")
        }
        _ => return None,
    })
}

fn restructure(hi: &mut crate::objects::Hierarchy, cmd: C) -> Option<(&'static str, &'static str)> {
    Some(match cmd {
        C::InsertIntoHierarchy => {
            reinsert(hi);
            ("Insert into hierarchy", "insertObjects(getSelectedObjects())")
        }
        C::ResolveHierarchy => {
            hi.resolve();
            ("Resolve hierarchy", "resolveHierarchy()")
        }
        C::DuplicateAnnotation => {
            duplicate(hi);
            ("Duplicate annotations", "duplicateSelectedAnnotations()")
        }
        C::SplitAnnotations => {
            ops::split_selected(hi);
            ("Split annotations", "splitSelectedAnnotations()")
        }
        _ => return None,
    })
}

fn reshape(
    hi: &mut crate::objects::Hierarchy,
    cmd: C,
    (w, h): (f64, f64),
    auto_class: Option<crate::objects::PathClass>,
) -> Option<(&'static str, &'static str)> {
    Some(match cmd {
        C::FullImageAnnotation => {
            let k = hi.insert(PathObject::annotation(Roi::Rectangle { x: 0.0, y: 0.0, w, h }, auto_class));
            hi.select(Some(k));
            ("Create full image annotation", "createFullImageAnnotation(true)")
        }
        C::MergeSelected => {
            ops::merge_selected(hi);
            ("Merge selected annotations", "mergeSelectedAnnotations()")
        }
        C::SubtractSelected => {
            ops::subtract_from_primary(hi);
            ("Subtract annotations", "// subtract from main selection")
        }
        C::MakeInverse => {
            ops::make_inverse(hi, w, h);
            ("Make inverse annotation", "makeInverseAnnotation()")
        }
        _ => return None,
    })
}

//! Projects in QuPath's format, and the hierarchy's containment rules.

use qupath::io::project::{Project, path_to_uri, uri_to_path};
use qupath::objects::{Hierarchy, PathClass, PathObject, Rgb, Roi, ops};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Roi {
    Roi::Rectangle { x, y, w, h }
}

#[test]
fn project_round_trips_and_escapes_uris_like_java() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().join("my project");
    let mut p = Project::create(&base).expect("create");
    let img = std::path::PathBuf::from("/data/slides/case 1 (HE).svs");
    let id = p.add_image(&img, "case 1", false);
    p.entry_mut(id).expect("entry").tags.push("train".into());
    std::fs::write(&p.file, p.to_json()).expect("write");
    let q = Project::open(&base).expect("open");
    assert_eq!(q.entries.len(), 1);
    assert_eq!(q.entries[0].path, img);
    assert_eq!(q.entries[0].tags, vec!["train".to_string()]);
    assert_eq!(path_to_uri(&img), "file:/data/slides/case%201%20(HE).svs");
    assert_eq!(uri_to_path("file:///data/x%20y.tif"), Some("/data/x y.tif".into()));
    assert!(Project::create(&base).is_err(), "a non-empty folder is refused");
}

#[test]
fn classes_json_uses_java_packed_colors() {
    let c = PathClass::with_color("Lymphocyte", Rgb(10, 20, 30));
    let text = String::from_utf8(Project::classes_json(&[c])).expect("utf8");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["pathClasses"][0]["color"].as_i64(), Some(i64::from(Rgb(10, 20, 30).to_packed())));
}

#[test]
fn objects_go_under_the_smallest_containing_annotation() {
    let mut h = Hierarchy::new();
    let outer = h.insert(PathObject::annotation(rect(0.0, 0.0, 1000.0, 1000.0), None));
    let inner = h.insert(PathObject::annotation(rect(100.0, 100.0, 200.0, 200.0), None));
    let det = h.insert(PathObject::detection(rect(150.0, 150.0, 10.0, 10.0), None));
    let outside = h.insert(PathObject::detection(rect(2000.0, 2000.0, 10.0, 10.0), None));
    assert_eq!(h.get(inner).and_then(|o| o.parent()), Some(outer));
    assert_eq!(h.get(det).and_then(|o| o.parent()), Some(inner));
    assert_eq!(h.get(outside).and_then(|o| o.parent()), Some(h.root()));
    // Drawing a big annotation afterwards adopts what it contains.
    let big = h.insert(PathObject::annotation(rect(-10.0, -10.0, 1100.0, 1100.0), None));
    assert_eq!(h.get(outer).and_then(|o| o.parent()), Some(big));
    // Removing an annotation keeps its children, moved to its parent.
    h.remove(inner, true);
    assert_eq!(h.get(det).and_then(|o| o.parent()), Some(outer));
}

#[test]
fn resolve_rebuilds_parents() {
    let mut h = Hierarchy::new();
    let a = h.add_child(h.root(), PathObject::annotation(rect(0.0, 0.0, 100.0, 100.0), None));
    let d = h.add_child(h.root(), PathObject::detection(rect(10.0, 10.0, 5.0, 5.0), None));
    assert_eq!(h.get(d).and_then(|o| o.parent()), Some(h.root()));
    h.resolve();
    let (ak, _) = h.annotations().next().expect("annotation");
    let (dk, _) = h.detections().next().expect("detection");
    assert_eq!(h.get(dk).and_then(|o| o.parent()), Some(ak));
    let _ = (a, d);
}

#[test]
fn merge_and_subtract_annotations() {
    let mut h = Hierarchy::new();
    let a = h.insert(PathObject::annotation(rect(0.0, 0.0, 10.0, 10.0), None));
    let b = h.insert(PathObject::annotation(rect(5.0, 0.0, 10.0, 10.0), None));
    h.add_to_selection([a, b]);
    let m = ops::merge_selected(&mut h).expect("merged");
    assert!((h.get(m).expect("merged").roi.area(1.0, 1.0) - 150.0).abs() < 1e-6);
    let hole = h.insert(PathObject::annotation(rect(2.0, 2.0, 2.0, 2.0), None));
    h.clear_selection();
    h.add_to_selection([hole, m]);
    let s = ops::subtract_from_primary(&mut h).expect("subtracted");
    assert!((h.get(s).expect("result").roi.area(1.0, 1.0) - 146.0).abs() < 1e-6);
}

#[test]
fn undo_restores_previous_hierarchy() {
    let mut h = Hierarchy::new();
    let mut undo = qupath::app::data::UndoStack::default();
    undo.checkpoint(&h);
    h.insert(PathObject::annotation(rect(0.0, 0.0, 10.0, 10.0), None));
    assert_eq!(h.len(), 1);
    assert!(undo.undo(&mut h));
    assert_eq!(h.len(), 0);
    assert!(undo.redo(&mut h));
    assert_eq!(h.len(), 1);
}

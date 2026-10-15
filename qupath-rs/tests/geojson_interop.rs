//! GeoJSON written by QuPath 0.7.0 reads correctly, and what we write reads back the same.

use qupath::io::geojson;
use qupath::objects::{Hierarchy, ObjKey, ObjectKind, PathClass, PathObject, Pt, Roi};

const QUPATH_EXPORT: &str = include_str!("fixtures/qupath-0.7.0-export.geojson");

fn import(text: &str) -> Hierarchy {
    let v: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
    let mut h = Hierarchy::new();
    geojson::import(&mut h, &v).expect("import");
    h
}

#[test]
fn reads_qupath_cells_with_nuclei_classes_and_measurements() {
    let h = import(QUPATH_EXPORT);
    let anns: Vec<_> = h.annotations().collect();
    let cells: Vec<_> = h.iter().filter(|(_, o)| o.kind == ObjectKind::Cell).collect();
    assert_eq!(anns.len(), 1);
    assert_eq!(cells.len(), 4);
    let (ak, a) = anns[0];
    assert_eq!(a.name.as_deref(), Some("Region A"));
    assert_eq!(a.class, Some(PathClass::new("Tumor")));
    // Cells are inserted under the annotation that contains them.
    assert_eq!(h.children(ak).len(), 4);
    for (_, c) in cells {
        assert!(c.nucleus.is_some(), "cell without nucleus");
        assert!(c.measurements.contains_key("Nucleus: DAB OD mean"));
        assert!(c.class.as_ref().is_some_and(|k| k.is_positive() || k.is_negative()));
    }
}

#[test]
fn export_then_import_keeps_everything() {
    let h = import(QUPATH_EXPORT);
    let keys: Vec<ObjKey> = h.iter().map(|(k, _)| k).collect();
    let text = serde_json::to_string(&geojson::export(&h, &keys, false)).expect("serialize");
    let h2 = import(&text);
    assert_eq!(h2.len(), h.len());
    for (_, o) in h.iter() {
        let (_, o2) = h2.iter().find(|(_, x)| x.id == o.id).expect("same ids");
        assert_eq!(o.kind, o2.kind);
        assert_eq!(o.class, o2.class);
        assert_eq!(o.measurements, o2.measurements);
        assert_eq!(o.roi.outline().len(), o2.roi.outline().len());
    }
}

#[test]
fn every_roi_type_survives_geojson() {
    let pts = vec![Pt { x: 1.0, y: 1.0 }, Pt { x: 9.0, y: 2.0 }, Pt { x: 5.0, y: 8.0 }];
    let rois = [
        Roi::Line { x1: 0.0, y1: 0.0, x2: 10.0, y2: 5.0 },
        Roi::Polyline(pts.clone()),
        Roi::Polygon(pts.clone()),
        Roi::Points(pts.clone()),
        Roi::Points(vec![pts[0]]),
    ];
    for roi in rois {
        let g = geojson::roi_to_geometry(&roi);
        let back = geojson::geometry_to_roi(&g).expect("parse");
        assert_eq!(back.type_name(), roi.type_name(), "{g}");
        assert_eq!(back.outline(), roi.outline());
    }
    // Rectangles become polygons in GeoJSON (as in QuPath), keeping their area.
    let rect = Roi::Rectangle { x: 2.0, y: 3.0, w: 10.0, h: 4.0 };
    let back = geojson::geometry_to_roi(&geojson::roi_to_geometry(&rect)).expect("parse");
    assert!((back.area(1.0, 1.0) - 40.0).abs() < 1e-9);
}

#[test]
fn nested_children_round_trip_through_child_objects() {
    let mut h = Hierarchy::new();
    let a = h.insert(PathObject::annotation(Roi::Rectangle { x: 0.0, y: 0.0, w: 100.0, h: 100.0 }, None));
    h.add_children(a, vec![PathObject::detection(Roi::Rectangle { x: 10.0, y: 10.0, w: 5.0, h: 5.0 }, None)]);
    let v = geojson::export(&h, &[a], true);
    let h2 = import(&serde_json::to_string(&v).expect("serialize"));
    let (a2, _) = h2.annotations().next().expect("annotation");
    assert_eq!(h2.children(a2).len(), 1);
}

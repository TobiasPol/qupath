//! Cell detection on the synthetic slide, whose truth is known.

use std::sync::Arc;

use qupath::analysis::cells::{self, CellParams, PositiveParams, Source};
use qupath::image::plain::PlainServer;
use qupath::image::stains::{ImageType, Stains};
use qupath::image::tiff::writer::PixelSource;
use qupath::objects::{Calibration, Roi};
use qupath::synthetic::SyntheticSlide;

fn slide_server() -> (Arc<PlainServer>, Roi) {
    // A tissue-rich patch of the synthetic slide, held in memory.
    let slide = SyntheticSlide::new(24_000, 18_000);
    let (x, y, w, h) = (11_000, 8_000, 1_200, 1_200);
    let tile = slide.read(1.0, x, y, w, h).expect("render");
    let server = PlainServer::from_tile(tile, std::path::Path::new("synthetic.png")).expect("server");
    (Arc::new(server), Roi::Rectangle { x: 0.0, y: 0.0, w: f64::from(w), h: f64::from(h) })
}

#[test]
fn positive_cell_detection_matches_truth() {
    let (server, roi) = slide_server();
    let src =
        Source { server, stains: Stains::default_for(ImageType::BrightfieldHDab), cal: Calibration::from_microns(Some(0.25), Some(0.25)) };
    let p = CellParams { positive: Some(PositiveParams::default_dab()), ..CellParams::default() };
    let found = cells::detect(&src, &roi, &p, &|_| true).expect("detect");
    // Nuclei sit on a 44 px grid with 88% occupancy: about 650 in 1200² px of tissue.
    assert!((350..900).contains(&found.len()), "{} cells", found.len());
    let positive = found.iter().filter(|c| c.class.as_ref().is_some_and(|k| k.is_positive())).count();
    let fraction = positive as f64 / found.len() as f64;
    assert!((0.22..0.40).contains(&fraction), "positive fraction {fraction}");
    let c = &found[0];
    assert!(c.nucleus.is_some());
    for m in ["Nucleus: Area µm^2", "Nucleus: Hematoxylin OD mean", "Cell: DAB OD mean", "Nucleus/Cell area ratio"] {
        assert!(c.measurements.contains_key(m), "missing {m}");
    }
}

#[test]
fn script_line_matches_qupath_plugin_names() {
    let s = CellParams::default().script();
    assert!(s.starts_with("runPlugin('qupath.imagej.detect.cells.WatershedCellDetection'"));
    let json = s.split("', '").nth(1).expect("json").trim_end_matches("')");
    let v: serde_json::Value = serde_json::from_str(json).expect("valid JSON parameters");
    assert_eq!(v["requestedPixelSizeMicrons"], 0.5);
}

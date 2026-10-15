//! Writes a project and GeoJSON with qupath-rs, for QuPath to read: `cargo run --example interop -- IMAGE OUT_DIR`
use qupath::analysis::cells::{self, CellParams, PositiveParams, Source};
use qupath::image::stains::{ImageType, Stains};
use qupath::io::{geojson, image_data, project::Project};
use qupath::objects::{Calibration, Hierarchy, ObjKey, PathClass, PathObject, Roi};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (image, out) = (std::path::PathBuf::from(&a[0]), std::path::PathBuf::from(&a[1]));
    let _ = std::fs::remove_dir_all(&out);
    let mut project = Project::create(&out.join("project"))?;
    let id = project.add_image(&image, "Synthetic H-DAB", false);
    let server = qupath::image::open(&image)?;
    let md = server.metadata();
    let cal = Calibration::from_microns(md.pixel_width_um, md.pixel_height_um);
    let mut h = Hierarchy::new();
    let roi = Roi::Rectangle { x: 11000.0, y: 8000.0, w: 1500.0, h: 1500.0 };
    let mut ann = PathObject::annotation(roi.clone(), Some(PathClass::new("Tumor")));
    ann.name = Some("Region A".into());
    let parent = h.insert(ann);
    let src = Source { server: server.clone(), stains: Stains::default_for(ImageType::BrightfieldHDab), cal };
    let params = CellParams { positive: Some(PositiveParams::default_dab()), ..CellParams::default() };
    let cells = cells::detect(&src, &roi, &params, &|_| true)?;
    println!("detected {} cells", cells.len());
    h.add_children(parent, cells);
    let settings = image_data::ImageSettings { image_type: ImageType::BrightfieldHDab, stains: src.stains.clone(), ..Default::default() };
    for (p, bytes) in image_data::serialize(&project.entry_dir(id), &h, &settings) {
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, bytes)?;
    }
    std::fs::write(&project.file, project.to_json())?;
    std::fs::create_dir_all(project.classes_file().parent().unwrap())?;
    let classes: Vec<PathClass> = qupath::objects::classes::default_classes().into_iter().flatten().collect();
    std::fs::write(project.classes_file(), Project::classes_json(&classes))?;
    let all: Vec<ObjKey> = h.iter().map(|(k, _)| k).collect();
    std::fs::write(out.join("objects.geojson"), serde_json::to_vec_pretty(&geojson::export(&h, &all, false))?)?;
    println!("wrote {}", out.display());
    Ok(())
}

//! The same benchmark as the QuPath Groovy script: `cargo run --release --example bench -- IMAGE OUT.geojson`
use std::time::Instant;

use qupath::analysis::cells::{self, CellParams, PositiveParams, Source};
use qupath::image::stains::{ImageType, Stains};
use qupath::io::geojson;
use qupath::objects::{Calibration, Hierarchy, ObjKey, PathObject, Roi};

fn ms(t: Instant) -> String {
    format!("{:.1}", t.elapsed().as_secs_f64() * 1000.0)
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let path = std::path::PathBuf::from(&a[0]);
    let t = Instant::now();
    let server = qupath::image::open(&path)?;
    println!("BENCH open_ms={}", ms(t));
    let t = Instant::now();
    for ty in 0..10 {
        for tx in 0..20 {
            server.read_level_region(0, 1024 + tx * 512, 4096 + ty * 512, 512, 512)?;
        }
    }
    println!("BENCH tiles200_ms={}", ms(t));
    let md = server.metadata();
    let src = Source {
        server: server.clone(),
        stains: Stains::default_for(ImageType::BrightfieldHDab),
        cal: Calibration::from_microns(md.pixel_width_um, md.pixel_height_um),
    };
    let params = CellParams { positive: Some(PositiveParams::default_dab()), ..CellParams::default() };
    let mut last = Vec::new();
    for (size, label) in [(2000.0, "small"), (6000.0, "large")] {
        for run in 1..=2 {
            let roi = Roi::Rectangle { x: 12000.0 - size / 2.0, y: 9000.0 - size / 2.0, w: size, h: size };
            let t = Instant::now();
            let found = cells::detect(&src, &roi, &params, &|_| true)?;
            let pos = found.iter().filter(|c| c.class.as_ref().is_some_and(|k| k.is_positive())).count();
            println!("BENCH cells_{label}_run{run}_ms={} n={} positive={pos} meas={}", ms(t), found.len(), found[0].measurements.len());
            last = found;
        }
    }
    let mut h = Hierarchy::new();
    let parent = h.insert(PathObject::annotation(Roi::Rectangle { x: 9000.0, y: 6000.0, w: 6000.0, h: 6000.0 }, None));
    h.add_children(parent, last);
    let keys: Vec<ObjKey> = h.iter().map(|(k, _)| k).collect();
    let t = Instant::now();
    let bytes = serde_json::to_vec(&geojson::export(&h, &keys, false))?;
    std::fs::write(&a[1], &bytes)?;
    println!("BENCH geojson_export_ms={} bytes={}", ms(t), bytes.len());
    let t = Instant::now();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&a[1])?)?;
    let mut h2 = Hierarchy::new();
    let n = geojson::import(&mut h2, &v)?;
    println!("BENCH geojson_import_ms={} n={n}", ms(t));
    Ok(())
}

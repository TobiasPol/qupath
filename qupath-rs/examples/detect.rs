//! Headless positive cell detection: `cargo run --release --example detect -- IMAGE [X Y W H]`
use qupath::analysis::cells::{self, CellParams, PositiveParams, Source};
use qupath::image::stains::{ImageType, Stains};
use qupath::objects::{Calibration, Roi};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let server = qupath::image::open(std::path::Path::new(&a[0]))?;
    let n = |i: usize, d: f64| a.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let roi = Roi::Rectangle { x: n(1, 11000.0), y: n(2, 8000.0), w: n(3, 2000.0), h: n(4, 2000.0) };
    let md = server.metadata();
    let src = Source {
        server: server.clone(),
        stains: Stains::default_for(ImageType::BrightfieldHDab),
        cal: Calibration::from_microns(md.pixel_width_um, md.pixel_height_um),
    };
    let params = CellParams { positive: Some(PositiveParams::default_dab()), ..CellParams::default() };
    let t = std::time::Instant::now();
    let cells = cells::detect(&src, &roi, &params, &|_| true)?;
    let pos = cells.iter().filter(|c| c.class.as_ref().is_some_and(|k| k.is_positive())).count();
    println!("{} cells in {:?}; positive {} ({:.1}%)", cells.len(), t.elapsed(), pos, 100.0 * pos as f64 / cells.len().max(1) as f64);
    if let Some(c) = cells.first() {
        for (k, v) in c.measurements.iter().take(40) {
            println!("  {k} = {v:.4}");
        }
    }
    let areas: Vec<f64> = cells.iter().filter_map(|c| c.measurements.get("Nucleus: Area µm^2").copied()).collect();
    let mean = areas.iter().sum::<f64>() / areas.len().max(1) as f64;
    println!("mean nucleus area {mean:.1} µm² (expected ≈ π·(13·0.25)²/(1.1·0.92) ≈ 32.7)");
    println!("{}", params.script());
    Ok(())
}

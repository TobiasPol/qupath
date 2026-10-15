//! Prints pixel values at fixed positions: `cargo run --release --example pixels -- IMAGE`
fn main() -> anyhow::Result<()> {
    let s = qupath::image::open(std::path::Path::new(&std::env::args().nth(1).expect("image")))?;
    let t = s.read_level_region(0, 12000, 9000, 64, 64)?;
    let v: Vec<String> = [(0, 0), (10, 10), (20, 30), (40, 40), (63, 63)]
        .iter()
        .map(|(x, y)| format!("({x},{y})={},{},{}", t.get(*x, *y, 0), t.get(*x, *y, 1), t.get(*x, *y, 2)))
        .collect();
    println!("RS-PIXELS {}", v.join(" "));
    let src = qupath::synthetic::SyntheticSlide::new(24000, 18000);
    let truth = qupath::image::tiff::writer::PixelSource::read(&src, 1.0, 12000, 9000, 64, 64)?;
    let v: Vec<String> = [(0, 0), (10, 10), (20, 30), (40, 40), (63, 63)]
        .iter()
        .map(|(x, y)| format!("({x},{y})={},{},{}", truth.get(*x, *y, 0), truth.get(*x, *y, 1), truth.get(*x, *y, 2)))
        .collect();
    println!("TRUTH     {}", v.join(" "));
    Ok(())
}

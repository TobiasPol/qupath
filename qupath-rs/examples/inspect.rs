//! Prints metadata of an image and writes a thumbnail: `cargo run --example inspect -- IMAGE OUT.png`
fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let path = std::path::PathBuf::from(a.next().expect("image path"));
    let out = a.next().unwrap_or_else(|| "thumbnail.png".into());
    let t = std::time::Instant::now();
    let server = qupath::image::open(&path)?;
    let md = server.metadata();
    println!("{} via {} in {:?}", md.name, md.server_type, t.elapsed());
    println!(
        "size {}x{}, rgb {}, {:?}, tile {}x{}, px {:?} µm, mag {:?}",
        md.width(),
        md.height(),
        md.rgb,
        md.pixel_type,
        md.tile_width,
        md.tile_height,
        md.pixel_width_um,
        md.magnification
    );
    for l in &md.levels {
        println!("  level {}x{} ds {:.3}", l.width, l.height, l.downsample);
    }
    println!("associated {:?}", md.associated);
    let t = std::time::Instant::now();
    qupath::image::thumbnail(server.as_ref(), 1024)?.save(&out)?;
    println!("thumbnail in {:?}", t.elapsed());
    let tile = server.read_level_region(0, 12000, 9000, 512, 512)?;
    let rgb = tile.to_rgb8();
    image::RgbImage::from_raw(512, 512, rgb).unwrap().save(out.replace(".png", "-full.png"))?;
    Ok(())
}

//! File → Export images, Export snapshot, and Measure → Export measurements.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use super::QuPathApp;
use super::tasks::TaskResult;
use crate::image::display::ImageDisplay;
use crate::image::region::resample;
use crate::image::tiff::writer::{Compression, PixelSource, WriteOptions, write_ome_tiff};
use crate::image::{ImageServer, RegionRequest, Tile, TileData, read_region};
use crate::objects::{Bounds, ObjKey, measurements};
use crate::ui::dialogs::ExportFormat;

/// A region of an image at an export resolution, optionally rendered.
struct ServerSource {
    server: Arc<dyn ImageServer>,
    region: Bounds,
    downsample: f64,
    display: Option<ImageDisplay>,
}

impl PixelSource for ServerSource {
    fn width(&self) -> u32 {
        (self.region.width() / self.downsample).ceil() as u32
    }
    fn height(&self) -> u32 {
        (self.region.height() / self.downsample).ceil() as u32
    }
    fn read(&self, ds: f64, x: u32, y: u32, w: u32, h: u32) -> Result<Tile> {
        let d = self.downsample * ds;
        let req = RegionRequest {
            downsample: d,
            x: self.region.x0 + f64::from(x) * self.downsample,
            y: self.region.y0 + f64::from(y) * self.downsample,
            w: f64::from(w) * d,
            h: f64::from(h) * d,
        };
        let mut t = read_region(self.server.as_ref(), &req)?;
        if t.width != w || t.height != h {
            t = resample(&t, w, h);
        }
        Ok(match &self.display {
            Some(disp) => {
                let rgb = disp.to_rgba(&t).as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
                Tile { width: w, height: h, channels: 3, data: TileData::U8(rgb) }
            }
            None => t,
        })
    }
}

fn write(src: &ServerSource, path: &PathBuf, format: ExportFormat, name: &str, px_um: Option<f64>) -> Result<()> {
    match format {
        ExportFormat::OmeTiff => {
            let rgb8 = src.read(1.0, 0, 0, 1, 1).map(|t| t.channels == 3 && matches!(t.data, TileData::U8(_)))?;
            let levels = (1..).take_while(|l| (src.width().max(src.height()) >> l) >= 256).count() + 1;
            let opts = WriteOptions {
                tile_size: 512,
                compression: if rgb8 { Compression::Jpeg(90) } else { Compression::Deflate },
                pixel_size_um: px_um.map(|p| p * src.downsample),
                name: name.into(),
                levels,
            };
            write_ome_tiff(path, src, &opts)
        }
        ExportFormat::Png | ExportFormat::Jpeg => {
            let (w, h) = (src.width(), src.height());
            anyhow::ensure!(
                u64::from(w) * u64::from(h) <= 400_000_000,
                "image too large for {format:?} ({w}×{h}); increase the downsample or use OME-TIFF"
            );
            let t = src.read(1.0, 0, 0, w, h)?;
            let img = image::RgbImage::from_raw(w, h, t.to_rgb8()).context("size mismatch")?;
            img.save(path).with_context(|| format!("cannot write {}", path.display()))
        }
    }
}

impl QuPathApp {
    /// Writes the image (or the selected object's bounds) in the background.
    pub(super) fn export_image(
        &mut self,
        _ctx: &egui::Context,
        downsample: f64,
        format: ExportFormat,
        rendered: bool,
        selection_only: bool,
    ) {
        let Some(d) = &self.s.image else { return };
        let md = d.server.metadata();
        let full = Bounds { x0: 0.0, y0: 0.0, x1: f64::from(md.width()), y1: f64::from(md.height()) };
        let region = if selection_only { d.hierarchy.primary_bounds().unwrap_or(full) } else { full };
        let ext = match format {
            ExportFormat::OmeTiff => "ome.tif",
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
        };
        let stem = md.name.split('.').next().unwrap_or("image").to_owned();
        let Some(path) = super::files::dialog(&self.s).set_file_name(format!("{stem}.{ext}")).save_file() else { return };
        let src = ServerSource { server: d.server.clone(), region, downsample, display: rendered.then(|| d.display.clone()) };
        let px = md.pixel_width_um;
        self.s.tasks.spawn("Exporting image", move |_| match write(&src, &path, format, &stem, px) {
            Ok(()) => TaskResult::Message(format!("Exported {}", path.display())),
            Err(e) => TaskResult::Error(format!("Export failed: {e:#}")),
        });
    }

    /// Measure → Export measurements: annotations and detections of the current image.
    pub(super) fn export_measurements(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.s.image else { return };
        let Some(path) = super::files::dialog(&self.s)
            .set_file_name(format!("{}.tsv", d.name()))
            .add_filter("Tab-separated", &["tsv", "txt"])
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };
        let sep = if path.extension().is_some_and(|e| e == "csv") { ',' } else { '\t' };
        let cal = d.calibration(&self.s.prefs);
        let keys: Vec<ObjKey> = d.hierarchy.iter().map(|(k, _)| k).collect();
        let (cols, rows) = measurements::table(&d.hierarchy, &keys, &cal);
        let mut cols = cols;
        cols.insert(0, "Image".into());
        let rows: Vec<_> = rows
            .into_iter()
            .map(|mut r| {
                r.shift_insert(0, "Image".into(), measurements::Value::Text(d.name().into()));
                r
            })
            .collect();
        self.s.writer.write(&path, measurements::to_tsv(&cols, &rows, sep), fastframe_file::Access::Default);
        self.s.notify(ctx, format!("Exported {} rows to {}", rows.len(), path.display()));
    }

    /// File → Export snapshot → Current viewer content.
    pub(super) fn request_snapshot(&mut self) {
        let Some(path) = super::files::dialog(&self.s).set_file_name("snapshot.png").add_filter("PNG", &["png"]).save_file() else {
            return;
        };
        self.s.snapshot_requested = Some(path);
    }

    /// Takes the screenshot once requested, and saves the viewer part of it.
    pub(super) fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.s.snapshot_requested.clone() else { return };
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(img) = shot else {
            if !self.ui.screenshot_pending {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.ui.screenshot_pending = true;
            }
            return;
        };
        self.ui.screenshot_pending = false;
        self.s.snapshot_requested = None;
        let region = img.region(&self.viewer.t.rect, Some(ctx.pixels_per_point()));
        let rgba: Vec<u8> = region.pixels.iter().flat_map(|c| c.to_array()).collect();
        let result = image::RgbaImage::from_raw(region.size[0] as u32, region.size[1] as u32, rgba).map(|i| i.save(&path));
        match result {
            Some(Ok(())) => self.s.notify(ctx, format!("Saved snapshot {}", path.display())),
            _ => self.s.error(ctx, "Could not save the snapshot"),
        }
    }
}

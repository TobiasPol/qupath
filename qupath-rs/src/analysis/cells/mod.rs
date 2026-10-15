//! Cell detection, following QuPath's `WatershedCellDetection` and `PositiveCellDetection`.

mod chunk;
mod measure;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Result, bail};
use rayon::prelude::*;

use crate::image::ImageServer;
use crate::image::stains::Stains;
use crate::objects::{Calibration, PathClass, PathObject, Roi};

/// Which image nuclei are found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectionImage {
    /// The first deconvolved stain (brightfield).
    Hematoxylin,
    /// Sum of optical densities (brightfield).
    OpticalDensitySum,
    /// A raw channel (fluorescence).
    Channel(usize),
}

/// Parameters with QuPath's names and defaults (µm when calibrated, else pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct CellParams {
    /// Image to detect in.
    pub detection_image: DetectionImage,
    /// Resolution to work at.
    pub requested_pixel_size: f64,
    /// Radius of the background estimate (0 to skip).
    pub background_radius: f64,
    /// Gaussian smoothing.
    pub sigma: f64,
    /// Smallest nucleus area.
    pub min_area: f64,
    /// Largest nucleus area.
    pub max_area: f64,
    /// Intensity a nucleus must exceed.
    pub threshold: f64,
    /// Background above this is not searched.
    pub max_background: f64,
    /// Split touching nuclei.
    pub watershed: bool,
    /// How far cells extend beyond nuclei.
    pub cell_expansion: f64,
    /// Smooth boundaries.
    pub smooth_boundaries: bool,
    /// Compute intensity and shape measurements.
    pub make_measurements: bool,
    /// Classify cells as positive or negative.
    pub positive: Option<PositiveParams>,
}

/// Positive cell detection: one threshold, or three for 1+/2+/3+ scoring.
#[derive(Clone, Debug, PartialEq)]
pub struct PositiveParams {
    /// Measurement compared with the thresholds.
    pub compartment: String,
    /// Thresholds for 1+, 2+, 3+.
    pub thresholds: [f64; 3],
    /// Use only the first threshold (Positive/Negative).
    pub single_threshold: bool,
}

impl Default for CellParams {
    fn default() -> Self {
        CellParams {
            detection_image: DetectionImage::Hematoxylin,
            requested_pixel_size: 0.5,
            background_radius: 8.0,
            sigma: 1.5,
            min_area: 10.0,
            max_area: 400.0,
            threshold: 0.1,
            max_background: 2.0,
            watershed: true,
            cell_expansion: 5.0,
            smooth_boundaries: true,
            make_measurements: true,
            positive: None,
        }
    }
}

impl PositiveParams {
    /// QuPath's defaults for DAB.
    pub fn default_dab() -> Self {
        PositiveParams { compartment: "Nucleus: DAB OD mean".into(), thresholds: [0.2, 0.4, 0.6], single_threshold: true }
    }
}

impl CellParams {
    /// The QuPath script line that runs the same detection.
    pub fn script(&self) -> String {
        let image = match self.detection_image {
            DetectionImage::Hematoxylin => "Hematoxylin OD",
            DetectionImage::OpticalDensitySum => "Optical density sum",
            DetectionImage::Channel(_) => "Channel 1",
        };
        let mut v = serde_json::json!({
            "detectionImageBrightfield": image,
            "requestedPixelSizeMicrons": self.requested_pixel_size,
            "backgroundRadiusMicrons": self.background_radius,
            "medianRadiusMicrons": 0.0,
            "sigmaMicrons": self.sigma,
            "minAreaMicrons": self.min_area,
            "maxAreaMicrons": self.max_area,
            "threshold": self.threshold,
            "maxBackground": self.max_background,
            "watershedPostProcess": self.watershed,
            "cellExpansionMicrons": self.cell_expansion,
            "includeNuclei": true,
            "smoothBoundaries": self.smooth_boundaries,
            "makeMeasurements": self.make_measurements,
        });
        let plugin = match (&self.positive, v.as_object_mut()) {
            (Some(p), Some(m)) => {
                m.insert("thresholdCompartment".into(), p.compartment.clone().into());
                m.insert("thresholdPositive1".into(), p.thresholds[0].into());
                m.insert("thresholdPositive2".into(), p.thresholds[1].into());
                m.insert("thresholdPositive3".into(), p.thresholds[2].into());
                m.insert("singleThreshold".into(), p.single_threshold.into());
                "qupath.imagej.detect.cells.PositiveCellDetection"
            }
            _ => "qupath.imagej.detect.cells.WatershedCellDetection",
        };
        crate::io::script::run_plugin(plugin, &v)
    }
}

/// What detection reads from.
pub struct Source {
    /// Pixels.
    pub server: Arc<dyn ImageServer>,
    /// Stains (brightfield).
    pub stains: Option<Stains>,
    /// Pixel size.
    pub cal: Calibration,
}

/// Output pixels per side of a processing chunk.
const CHUNK: f64 = 1536.0;

/// Detects cells inside a ROI; `progress` gets 0..1 and returns false to cancel.
pub fn detect(src: &Source, roi: &Roi, p: &CellParams, progress: &(dyn Fn(f32) -> bool + Sync)) -> Result<Vec<PathObject>> {
    let md = src.server.metadata();
    if !roi.is_area() {
        bail!("cell detection needs an area annotation");
    }
    if md.rgb && src.stains.is_none() && p.detection_image != DetectionImage::OpticalDensitySum {
        bail!("set a brightfield image type (Image tab) so hematoxylin can be separated");
    }
    let px = src.cal.average();
    let ds = if src.cal.calibrated { (p.requested_pixel_size / px).max(1.0) } else { 1.0 };
    let out_px = px * ds;
    let b = roi.bounds();
    let (x0, y0) = (b.x0.max(0.0), b.y0.max(0.0));
    let (x1, y1) = (b.x1.min(f64::from(md.width())), b.y1.min(f64::from(md.height())));
    let step = CHUNK * ds;
    let pad = ((p.cell_expansion + (p.max_area / std::f64::consts::PI).sqrt() + p.background_radius) / out_px + 4.0) * ds;
    let mut cores = Vec::new();
    let mut y = y0;
    while y < y1 {
        let mut x = x0;
        while x < x1 {
            cores.push([x, y, (x + step).min(x1), (y + step).min(y1)]);
            x += step;
        }
        y += step;
    }
    let done = AtomicUsize::new(0);
    let n = cores.len().max(1);
    let parts: Vec<Result<Vec<PathObject>>> = cores
        .par_iter()
        .map(|core| {
            if !progress(done.load(Ordering::Relaxed) as f32 / n as f32) {
                bail!("cancelled");
            }
            let r = chunk::process(src, roi, p, *core, pad, ds);
            progress((done.fetch_add(1, Ordering::Relaxed) + 1) as f32 / n as f32);
            r
        })
        .collect();
    let mut cells = Vec::new();
    for part in parts {
        cells.extend(part?);
    }
    if let Some(pos) = &p.positive {
        classify_positive(&mut cells, pos);
    }
    Ok(cells)
}

/// Sets Positive/Negative (or 1+/2+/3+) from a measurement.
pub fn classify_positive(cells: &mut [PathObject], p: &PositiveParams) {
    for c in cells {
        let v = c.measurements.get(&p.compartment).copied().unwrap_or(f64::NAN);
        let name = if p.single_threshold {
            if v >= p.thresholds[0] { "Positive" } else { "Negative" }
        } else if v >= p.thresholds[2] {
            "3+"
        } else if v >= p.thresholds[1] {
            "2+"
        } else if v >= p.thresholds[0] {
            "1+"
        } else {
            "Negative"
        };
        c.class = Some(PathClass::new(name));
    }
}

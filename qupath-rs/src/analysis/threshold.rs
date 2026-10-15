//! Classify → Pixel classification → Create thresholder (simplified): threshold
//! one channel at a low resolution and turn the result into objects.

use anyhow::{Result, bail};
use geo::{Area, MultiPolygon, Polygon};
use image::GrayImage;

use crate::analysis::contours::mask_to_polygons;
use crate::analysis::morphology::{FImage, gaussian};
use crate::image::stains::Stains;
use crate::image::{ImageServer, RegionRequest, read_region};
use crate::objects::{Bounds, PathClass, PathObject, Roi};

/// What is thresholded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThresholdChannel {
    /// Mean of RGB.
    Average,
    /// Optical density sum (brightfield).
    OdSum,
    /// A deconvolved stain (brightfield).
    Stain(usize),
    /// One image channel.
    Channel(usize),
}

/// Thresholder parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ThresholdParams {
    /// Downsample of the full-resolution image to work at.
    pub downsample: f64,
    /// Channel.
    pub channel: ThresholdChannel,
    /// Gaussian smoothing, in pixels at that resolution.
    pub sigma: f64,
    /// Threshold.
    pub threshold: f64,
    /// Keep pixels above (else below) the threshold.
    pub above: bool,
    /// Smallest object kept, in calibrated units².
    pub min_object_area: f64,
    /// Smallest hole kept, in calibrated units².
    pub min_hole_area: f64,
    /// One object per connected region.
    pub split: bool,
    /// Make detections rather than annotations.
    pub detections: bool,
    /// Class of the result.
    pub class: Option<PathClass>,
}

impl Default for ThresholdParams {
    fn default() -> Self {
        ThresholdParams {
            downsample: 16.0,
            channel: ThresholdChannel::Average,
            sigma: 2.0,
            threshold: 220.0,
            above: false,
            min_object_area: 10_000.0,
            min_hole_area: 5_000.0,
            split: false,
            detections: false,
            class: Some(PathClass::new("Region*")),
        }
    }
}

const MAX_PIXELS: f64 = 80_000_000.0;

/// Thresholds a region (the whole image when `region` is `None`); `area_scale` converts px² to calibrated units².
pub fn run(
    server: &dyn ImageServer,
    stains: Option<&Stains>,
    region: Option<&Roi>,
    p: &ThresholdParams,
    area_scale: f64,
) -> Result<Vec<PathObject>> {
    let md = server.metadata();
    let full = Bounds { x0: 0.0, y0: 0.0, x1: f64::from(md.width()), y1: f64::from(md.height()) };
    let b = region.map_or(full, Roi::bounds);
    if b.width() * b.height() / (p.downsample * p.downsample) > MAX_PIXELS {
        bail!("the region is too large at downsample {}; increase the downsample", p.downsample);
    }
    let t = read_region(server, &RegionRequest { downsample: p.downsample, x: b.x0, y: b.y0, w: b.width(), h: b.height() })?;
    let (w, h) = (t.width as usize, t.height as usize);
    let scale = b.width().min(full.x1 - b.x0.max(0.0)) / w as f64;
    let mut img = FImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let rgb = || {
                [
                    t.get(x as u32, y as u32, 0),
                    t.get(x as u32, y as u32, 1.min(t.channels - 1)),
                    t.get(x as u32, y as u32, 2.min(t.channels - 1)),
                ]
            };
            img.data[y * w + x] = match p.channel {
                ThresholdChannel::Average => rgb().iter().sum::<f32>() / 3.0,
                ThresholdChannel::OdSum => stains.map_or(0.0, |s| s.od_sum(rgb()) as f32),
                ThresholdChannel::Stain(i) => stains.map_or(0.0, |s| s.deconvolve(rgb())[i.min(2)] as f32),
                ThresholdChannel::Channel(c) => t.get(x as u32, y as u32, c.min(t.channels - 1)),
            };
        }
    }
    let smooth = gaussian(&img, p.sigma as f32);
    let mut mask = GrayImage::new(w as u32, h as u32);
    for (i, v) in smooth.data.iter().enumerate() {
        if (f64::from(*v) > p.threshold) == p.above {
            mask.put_pixel((i % w) as u32, (i / w) as u32, image::Luma([255]));
        }
    }
    let mut mp = mask_to_polygons(&mask, (b.x0.max(0.0), b.y0.max(0.0)), scale, scale * 0.5);
    if let Some(geom) = region.and_then(Roi::to_multipolygon) {
        mp = geo::BooleanOps::intersection(&mp, &geom);
    }
    Ok(to_objects(filter_sizes(mp, p, area_scale), p))
}

/// Removes small objects and small holes.
fn filter_sizes(mp: MultiPolygon<f64>, p: &ThresholdParams, area_scale: f64) -> MultiPolygon<f64> {
    let polys =
        mp.0.into_iter()
            .filter(|poly| poly.unsigned_area() * area_scale >= p.min_object_area)
            .map(|poly| {
                let (ext, holes) = poly.into_inner();
                let keep =
                    holes.into_iter().filter(|r| Polygon::new(r.clone(), vec![]).unsigned_area() * area_scale >= p.min_hole_area).collect();
                Polygon::new(ext, keep)
            })
            .collect();
    MultiPolygon::new(polys)
}

fn to_objects(mp: MultiPolygon<f64>, p: &ThresholdParams) -> Vec<PathObject> {
    let make = |roi: Roi| {
        if p.detections { PathObject::detection(roi, p.class.clone()) } else { PathObject::annotation(roi, p.class.clone()) }
    };
    if p.split {
        mp.0.into_iter().filter_map(|poly| Roi::from_multipolygon(MultiPolygon::new(vec![poly]))).map(make).collect()
    } else {
        Roi::from_multipolygon(mp).map(make).into_iter().collect()
    }
}

//! Analysis: cell detection, tiles and thresholding inside annotations.

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::{Effect, Output, Tool, ids_property, schema};
use crate::agent::args::{self, round};
use crate::agent::session::{OpenImage, Session};
use crate::analysis::cells::{self, CellParams, DetectionImage, PositiveParams, Source};
use crate::analysis::threshold::{self, ThresholdChannel, ThresholdParams};
use crate::analysis::tiles::{self, TileParams};
use crate::objects::{ObjKey, PathClass, PathObject};

pub(super) fn tools() -> Vec<Tool> {
    vec![DETECT_CELLS, CREATE_TILES, THRESHOLD_REGIONS]
}

const DETECT_CELLS: Tool = Tool {
    name: "detect_cells",
    title: "Cell detection",
    description: "QuPath's (positive) cell detection — watershed nuclei from colour deconvolution, expanded to cells, with QuPath's \
                  measurements — inside annotations (given ids, else all area annotations). Existing detections in those \
                  annotations are replaced. Brightfield needs image type H&E or H-DAB. With positive=true (IHC), cells are \
                  classified Positive/Negative (or 1+/2+/3+) by 'Nucleus: DAB OD mean'. Distances in µm when calibrated.",
    schema: cells_schema,
    effect: Effect::Edit,
    run: detect,
};

const CREATE_TILES: Tool = Tool {
    name: "create_tiles",
    title: "Create tiles",
    description: "Cover annotations (ids, else all) with square tiles (µm when calibrated), as tile objects or annotations.",
    schema: || {
        let n = |d: &str| json!({ "type": "number", "description": d });
        let props = json!({
            "parent_ids": ids_property("Annotations to tile (default all)"), "size": n("Tile width (default 100)"),
            "height": n("Tile height (default = size)"), "trim_to_roi": { "type": "boolean" }, "make_annotations": { "type": "boolean" },
        });
        schema(props, &[])
    },
    effect: Effect::Edit,
    run: create_tiles,
};

const THRESHOLD_REGIONS: Tool = Tool {
    name: "threshold_regions",
    title: "Create objects from threshold",
    description: "Threshold a smoothed, downsampled channel to make objects — e.g. tissue detection on brightfield: channel \
                  'average', threshold ~220, above=false (tissue is darker than glass). Use region_stats to choose the threshold. \
                  Runs on the whole image, or inside parent_ids. Areas are in calibrated units² (µm²).",
    schema: threshold_schema,
    effect: Effect::Edit,
    run: run_threshold,
};

fn cells_schema() -> Value {
    let n = |d: &str| json!({ "type": "number", "description": d });
    schema(
        json!({
            "parent_ids": ids_property("Annotations to detect in (default all area annotations)"),
            "positive": { "type": "boolean", "description": "Positive cell detection (classify by DAB). Default false" },
            "detection_image": { "type": "string", "description": "hematoxylin (default), od_sum, or channel:N (fluorescence, 0-based)" },
            "requested_pixel_size": n("µm; default 0.5"),
            "background_radius": n("µm; default 8 (0 = none)"),
            "sigma": n("µm; default 1.5"),
            "min_area": n("µm²; default 10"),
            "max_area": n("µm²; default 400"),
            "threshold": n("Nucleus intensity threshold; default 0.1 (OD)"),
            "max_background": n("default 2"),
            "watershed": { "type": "boolean", "description": "Split touching nuclei; default true" },
            "cell_expansion": n("µm; default 5"),
            "smooth_boundaries": { "type": "boolean" },
            "positive_measurement": { "type": "string", "description": "default 'Nucleus: DAB OD mean'" },
            "positive_thresholds": { "type": "array", "items": { "type": "number" }, "description": "1 value → Positive/Negative; 3 values → 1+/2+/3+. Default [0.2]" },
        }),
        &[],
    )
}

fn cell_params(a: &Value) -> Result<CellParams> {
    let d = CellParams::default();
    let detection_image = match args::opt_str(a, "detection_image").unwrap_or("hematoxylin") {
        "hematoxylin" => DetectionImage::Hematoxylin,
        "od_sum" => DetectionImage::OpticalDensitySum,
        s => match s.strip_prefix("channel:").and_then(|n| n.parse().ok()) {
            Some(c) => DetectionImage::Channel(c),
            None => bail!("detection_image is hematoxylin, od_sum or channel:N"),
        },
    };
    let positive = args::bool_or(a, "positive", false).then(|| {
        let mut p = PositiveParams::default_dab();
        if let Some(m) = args::opt_str(a, "positive_measurement") {
            p.compartment = m.to_owned();
        }
        let t: Vec<f64> = a
            .get("positive_thresholds")
            .and_then(Value::as_array)
            .map(|v| v.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        match t.as_slice() {
            [one] => p.thresholds[0] = *one,
            [a, b, c, ..] => (p.thresholds, p.single_threshold) = ([*a, *b, *c], false),
            _ => {}
        }
        p
    });
    let f = |k: &str, v: f64| args::f64_or(a, k, v);
    Ok(CellParams {
        detection_image,
        requested_pixel_size: f("requested_pixel_size", d.requested_pixel_size),
        background_radius: f("background_radius", d.background_radius),
        sigma: f("sigma", d.sigma),
        min_area: f("min_area", d.min_area),
        max_area: f("max_area", d.max_area),
        threshold: f("threshold", d.threshold),
        max_background: f("max_background", d.max_background),
        watershed: args::bool_or(a, "watershed", d.watershed),
        cell_expansion: f("cell_expansion", d.cell_expansion),
        smooth_boundaries: args::bool_or(a, "smooth_boundaries", d.smooth_boundaries),
        make_measurements: true,
        positive,
    })
}

/// Replaces the detections under each parent with new objects.
fn replace_children(img: &mut OpenImage, groups: Vec<(ObjKey, Vec<PathObject>)>) {
    for (parent, objs) in groups {
        let old: Vec<ObjKey> = img
            .hierarchy
            .children(parent)
            .iter()
            .copied()
            .filter(|k| img.hierarchy.get(*k).is_some_and(PathObject::is_detection))
            .collect();
        let ids: std::collections::HashSet<uuid::Uuid> = old.iter().filter_map(|k| img.hierarchy.get(*k)).map(|o| o.id).collect();
        img.hierarchy.remove_where(|o| ids.contains(&o.id));
        img.hierarchy.add_children(parent, objs);
    }
}

fn detect(s: &mut Session, a: &Value) -> Result<Output> {
    let p = cell_params(a)?;
    let img = s.image_mut()?;
    let targets = img.targets(args::opt_strings(a, "parent_ids").as_deref())?;
    let src = Source { server: img.server.clone(), stains: img.settings.stains.clone(), cal: img.calibration() };
    let t = std::time::Instant::now();
    let mut groups = Vec::new();
    let mut per_parent = Vec::new();
    for k in targets {
        let Some(o) = img.hierarchy.get(k) else { continue };
        let found = cells::detect(&src, &o.roi, &p, &|_| true)?;
        let pos = found.iter().filter(|c| c.class.as_ref().is_some_and(PathClass::is_positive)).count();
        let mut row = json!({ "parent_id": o.id.to_string(), "parent_name": o.name, "cells": found.len() });
        if p.positive.is_some() {
            row["positive"] = json!(pos);
            row["positive_percent"] = json!(round(100.0 * pos as f64 / found.len().max(1) as f64, 2));
        }
        per_parent.push(row);
        groups.push((k, found));
    }
    let total: usize = groups.iter().map(|(_, c)| c.len()).sum();
    replace_children(img, groups);
    let name = if p.positive.is_some() { "Positive cell detection" } else { "Cell detection" };
    img.record(name, format!("selectAnnotations();\n{}", p.script()));
    Ok(json!({ "cells": total, "seconds": round(t.elapsed().as_secs_f64(), 2), "per_annotation": per_parent }).into())
}

fn create_tiles(s: &mut Session, a: &Value) -> Result<Output> {
    let size = args::f64_or(a, "size", 100.0);
    let p = TileParams {
        width: size,
        height: args::f64_or(a, "height", size),
        trim_to_roi: args::bool_or(a, "trim_to_roi", true),
        make_annotations: args::bool_or(a, "make_annotations", false),
    };
    let img = s.image_mut()?;
    let targets = img.targets(args::opt_strings(a, "parent_ids").as_deref())?;
    let cal = img.calibration();
    let mut n = 0;
    for k in targets {
        let Some(roi) = img.hierarchy.get(k).map(|o| o.roi.clone()) else { continue };
        let tiles = tiles::create_tiles(&roi, &p, cal.pixel_width, cal.pixel_height);
        n += tiles.len();
        img.hierarchy.add_children(k, tiles);
    }
    img.record("Create tiles", p.script());
    Ok(json!({ "tiles": n }).into())
}

fn threshold_schema() -> Value {
    let n = |d: &str| json!({ "type": "number", "description": d });
    schema(
        json!({
            "channel": { "type": "string", "description": "average (default), od_sum, stain:N (0=hematoxylin, 1=eosin/DAB), channel:N" },
            "threshold": n("Default 220 (for average)"),
            "above": { "type": "boolean", "description": "Keep pixels above the threshold (default false: below)" },
            "downsample": n("Default 16"),
            "sigma": n("Gaussian smoothing in px at that downsample; default 2"),
            "min_object_area": n("Smallest object, µm²; default 10000"),
            "min_hole_area": n("Smallest hole kept, µm²; default 5000"),
            "split": { "type": "boolean", "description": "One object per connected region (default false)" },
            "detections": { "type": "boolean", "description": "Make detections instead of annotations" },
            "class": { "type": "string", "description": "Class of the result (default Region*; empty for none)" },
            "parent_ids": ids_property("Only inside these annotations (default: whole image)"),
        }),
        &[],
    )
}

fn threshold_params(a: &Value) -> Result<ThresholdParams> {
    let d = ThresholdParams::default();
    let index = |s: &str, prefix: &str| s.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok());
    let channel = match args::opt_str(a, "channel").unwrap_or("average") {
        "average" => ThresholdChannel::Average,
        "od_sum" => ThresholdChannel::OdSum,
        s if index(s, "stain:").is_some() => ThresholdChannel::Stain(index(s, "stain:").unwrap_or(0)),
        s if index(s, "channel:").is_some() => ThresholdChannel::Channel(index(s, "channel:").unwrap_or(0)),
        _ => bail!("channel is average, od_sum, stain:N or channel:N"),
    };
    let class = match args::opt_str(a, "class") {
        Some("") => None,
        Some(c) => Some(PathClass::new(c)),
        None => d.class.clone(),
    };
    Ok(ThresholdParams {
        downsample: args::f64_or(a, "downsample", d.downsample).max(1.0),
        channel,
        sigma: args::f64_or(a, "sigma", d.sigma),
        threshold: args::f64_or(a, "threshold", d.threshold),
        above: args::bool_or(a, "above", d.above),
        min_object_area: args::f64_or(a, "min_object_area", d.min_object_area),
        min_hole_area: args::f64_or(a, "min_hole_area", d.min_hole_area),
        split: args::bool_or(a, "split", d.split),
        detections: args::bool_or(a, "detections", d.detections),
        class,
    })
}

fn run_threshold(s: &mut Session, a: &Value) -> Result<Output> {
    let p = threshold_params(a)?;
    let img = s.image_mut()?;
    let cal = img.calibration();
    let regions: Vec<Option<crate::objects::Roi>> = match args::opt_strings(a, "parent_ids") {
        Some(ids) => img.targets(Some(&ids))?.iter().filter_map(|k| img.hierarchy.get(*k)).map(|o| Some(o.roi.clone())).collect(),
        None => vec![None],
    };
    let mut objs = Vec::new();
    for r in &regions {
        objs.extend(threshold::run(img.server.as_ref(), img.settings.stains.as_ref(), r.as_ref(), &p, cal.pixel_width * cal.pixel_height)?);
    }
    let ids: Vec<String> = objs.iter().map(|o| o.id.to_string()).collect();
    let area: f64 = objs.iter().map(|o| o.roi.area(cal.pixel_width, cal.pixel_height)).sum();
    for o in objs {
        img.hierarchy.insert(o);
    }
    img.record("Create objects from threshold", format!("// threshold {} at downsample {}", p.threshold, p.downsample));
    Ok(json!({ "created": ids.len(), "ids": ids, format!("total_area_{}2", cal.unit()): round(area, 1) }).into())
}

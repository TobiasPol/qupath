//! Images: open, describe, set type and pixel size, look at regions, pixel statistics.

use std::path::Path;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::{Effect, Output, Tool, schema};
use crate::agent::args::{self, round};
use crate::agent::render;
use crate::agent::session::Session;
use crate::image::stains::ImageType;

pub(super) fn tools() -> Vec<Tool> {
    vec![OPEN_IMAGE, IMAGE_INFO, SET_IMAGE_TYPE, SET_PIXEL_SIZE, VIEW_IMAGE, REGION_STATS]
}

const OPEN_IMAGE: Tool = Tool {
    name: "open_image",
    title: "Open image",
    description: "Open a whole-slide or plain image file and make it current. If it belongs to the open project, its saved objects load too. \
                  Returns size, pyramid levels, pixel size (µm), channels and the (estimated) image type.",
    schema: || schema(json!({ "path": { "type": "string", "description": "Image file" } }), &["path"]),
    effect: Effect::Files,
    run: open,
};

const IMAGE_INFO: Tool = Tool {
    name: "image_info",
    title: "Image info",
    description: "Describe the current image: size, levels, pixel size, channels, image type, stains, object counts by kind and class.",
    schema: || schema(json!({}), &[]),
    effect: Effect::Read,
    run: info,
};

const SET_IMAGE_TYPE: Tool = Tool {
    name: "set_image_type",
    title: "Set image type",
    description: "Set the image type, which picks default stain vectors for colour deconvolution. Cell detection on brightfield needs \
                  BRIGHTFIELD_H_E or BRIGHTFIELD_H_DAB (IHC); use FLUORESCENCE for fluorescence.",
    schema: || {
        let t = json!({ "type": "string", "enum": ["BRIGHTFIELD_H_E", "BRIGHTFIELD_H_DAB", "BRIGHTFIELD_OTHER", "FLUORESCENCE", "OTHER"] });
        schema(json!({ "type": t }), &["type"])
    },
    effect: Effect::Edit,
    run: set_type,
};

const SET_PIXEL_SIZE: Tool = Tool {
    name: "set_pixel_size",
    title: "Set pixel size",
    description: "Override the pixel size in µm when the file lacks it or is wrong (measurements and µm parameters depend on it).",
    schema: || {
        let w = json!({ "type": "number", "exclusiveMinimum": 0, "description": "Pixel width, µm" });
        let h = json!({ "type": "number", "exclusiveMinimum": 0, "description": "Pixel height, µm (defaults to width)" });
        schema(json!({ "pixel_width_um": w, "pixel_height_um": h }), &["pixel_width_um"])
    },
    effect: Effect::Edit,
    run: set_pixel_size,
};

const VIEW_IMAGE: Tool = Tool {
    name: "view_image",
    title: "View image region",
    description: "Render the whole image or a region (full-resolution pixel coordinates) as a PNG the model can see, optionally with \
                  object outlines in class colours. Use it to find tissue, check annotations and verify detection results. \
                  The output is scaled to fit max_size; the returned downsample maps output pixels back to image pixels.",
    schema: region_schema,
    effect: Effect::Read,
    run: view,
};

const REGION_STATS: Tool = Tool {
    name: "region_stats",
    title: "Region pixel statistics",
    description: "Per-channel mean/std/min/max of a region (whole image by default), plus mean optical densities of the stains on \
                  brightfield images. Use it to choose thresholds for threshold_regions or detect_cells.",
    schema: region_schema,
    effect: Effect::Read,
    run: stats,
};

fn region_schema() -> Value {
    let n = |d: &str| json!({ "type": "number", "description": d });
    schema(
        json!({
            "x": n("Left, full-resolution px (default 0)"),
            "y": n("Top, full-resolution px (default 0)"),
            "width": n("Width, px (default: whole image)"),
            "height": n("Height, px (default: whole image)"),
            "downsample": n("Minimum downsample (default: whatever fits max_size)"),
            "max_size": { "type": "integer", "description": "Longest output side in px (default 1024, at most 2048)" },
            "overlay": { "type": "boolean", "description": "Draw objects (default true; view_image only)" },
        }),
        &[],
    )
}

fn region(a: &Value) -> Option<[f64; 4]> {
    let x = args::f64_or(a, "x", 0.0);
    let y = args::f64_or(a, "y", 0.0);
    match (args::opt_f64(a, "width"), args::opt_f64(a, "height")) {
        (Some(w), Some(h)) => Some([x, y, w, h]),
        (None, None) if x == 0.0 && y == 0.0 => None,
        _ => Some([x, y, f64::MAX / 4.0, f64::MAX / 4.0]),
    }
}

fn open(s: &mut Session, a: &Value) -> Result<Output> {
    s.open_image(Path::new(args::text(a, "path")?))?;
    info(s, a)
}

pub(super) fn info(s: &mut Session, _a: &Value) -> Result<Output> {
    let img = s.image()?;
    let md = img.server.metadata();
    let cal = img.calibration();
    let levels: Vec<Value> = md.levels.iter().map(|l| json!([l.width, l.height, round(l.downsample, 3)])).collect();
    let channels: Vec<&str> = md.channels.iter().map(|c| c.name.as_str()).collect();
    let mut counts: indexmap::IndexMap<String, usize> = indexmap::IndexMap::new();
    for (_, o) in img.hierarchy.iter() {
        let class = crate::objects::classes::display_name(o.class.as_ref());
        *counts.entry(format!("{} ({class})", o.kind.display_name())).or_default() += 1;
    }
    Ok(json!({
        "name": md.name,
        "path": md.path,
        "reader": md.server_type,
        "width": md.width(),
        "height": md.height(),
        "levels [width, height, downsample]": levels,
        "pixel_size_um": cal.calibrated.then_some([cal.pixel_width, cal.pixel_height]),
        "magnification": md.magnification,
        "channels": channels,
        "pixel_type": md.pixel_type.name(),
        "rgb": md.rgb,
        "image_type": img.settings.image_type.script_name(),
        "stains": img.settings.stains.as_ref().map(|st| st.stains.iter().map(|v| v.name.as_str()).collect::<Vec<_>>()),
        "project_entry": img.entry,
        "objects": img.hierarchy.len(),
        "object_counts": counts,
    })
    .into())
}

fn set_type(s: &mut Session, a: &Value) -> Result<Output> {
    let name = args::text(a, "type")?;
    let Some(t) = ImageType::ALL.into_iter().find(|t| t.script_name().eq_ignore_ascii_case(name)) else {
        bail!("unknown image type {name}");
    };
    let img = s.image_mut()?;
    img.set_image_type(t);
    info(s, a)
}

fn set_pixel_size(s: &mut Session, a: &Value) -> Result<Output> {
    let w = args::num(a, "pixel_width_um")?;
    let h = args::f64_or(a, "pixel_height_um", w);
    if w <= 0.0 || h <= 0.0 {
        bail!("pixel sizes must be positive");
    }
    let img = s.image_mut()?;
    img.settings.pixel_width_microns = Some(w);
    img.settings.pixel_height_microns = Some(h);
    img.record("Set pixel size", format!("setPixelSizeMicrons({w}, {h});"));
    Ok(json!({ "pixel_size_um": [w, h] }).into())
}

fn view(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image()?;
    let max = args::usize_or(a, "max_size", 1024).clamp(64, 2048) as u32;
    let req = render::request(img, region(a), args::opt_f64(a, "downsample"), max);
    let v = render::render(img, &req, args::bool_or(a, "overlay", true))?;
    let json = json!({
        "region [x, y, width, height]": [round(req.x, 1), round(req.y, 1), round(req.w, 1), round(req.h, 1)],
        "downsample": round(req.downsample, 4),
        "output_size": [v.width, v.height],
        "objects_drawn": v.drawn,
        "detections_skipped": v.detections_skipped,
        "hint": "image px = region x/y + output px × downsample",
    });
    Ok(Output { json, images: vec![v.png] })
}

fn stats(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image()?;
    let max = args::usize_or(a, "max_size", 1024).clamp(64, 2048) as u32;
    let req = render::request(img, region(a), args::opt_f64(a, "downsample"), max);
    let t = crate::image::read_region(img.server.as_ref(), &req)?;
    let md = img.server.metadata();
    let n = (t.width * t.height) as usize;
    let mut channels = Vec::new();
    for c in 0..t.channels {
        let vals = (0..n).map(|i| t.get(i as u32 % t.width, i as u32 / t.width, c) as f64);
        let name = md.channels.get(c).map_or_else(|| format!("Channel {}", c + 1), |ch| ch.name.clone());
        channels.push(summarize(&name, vals));
    }
    let mut out = json!({ "downsample": round(req.downsample, 4), "pixels": n, "channels": channels });
    if let (Some(st), true) = (&img.settings.stains, md.rgb) {
        let px = |i: usize| [0, 1, 2].map(|c| t.get(i as u32 % t.width, i as u32 / t.width, c));
        let mut stains: Vec<Value> = (0..3).map(|k| summarize(&st.stains[k].name, (0..n).map(|i| st.deconvolve(px(i))[k]))).collect();
        stains.push(summarize("Optical density sum", (0..n).map(|i| st.od_sum(px(i)))));
        out["stain_od"] = Value::Array(stains);
    }
    Ok(out.into())
}

fn summarize(name: &str, vals: impl Iterator<Item = f64>) -> Value {
    let (mut n, mut sum, mut sq, mut lo, mut hi) = (0f64, 0f64, 0f64, f64::INFINITY, f64::NEG_INFINITY);
    for v in vals.filter(|v| v.is_finite()) {
        n += 1.0;
        sum += v;
        sq += v * v;
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let mean = sum / n.max(1.0);
    let std = (sq / n.max(1.0) - mean * mean).max(0.0).sqrt();
    json!({ "name": name, "mean": round(mean, 4), "std": round(std, 4), "min": round(lo, 4), "max": round(hi, 4) })
}

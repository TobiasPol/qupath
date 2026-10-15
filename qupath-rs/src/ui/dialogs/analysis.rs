//! Forms for cell detection, tiles and the thresholder.

use egui::Grid;

use super::{DialogEnv, Outcome, buttons, number};
use crate::analysis::cells::{CellParams, DetectionImage};
use crate::analysis::threshold::{ThresholdChannel, ThresholdParams};
use crate::analysis::tiles::TileParams;
use crate::objects::PathClass;

fn unit(env: &DialogEnv<'_>) -> &'static str {
    if env.calibrated { " µm" } else { " px" }
}

/// Cell detection and positive cell detection.
pub fn cells(ui: &mut egui::Ui, p: &mut CellParams, env: &DialogEnv<'_>) -> Outcome {
    let u = unit(env);
    let u2 = if env.calibrated { " µm²" } else { " px²" };
    ui.label("Runs inside the selected annotations (or all annotations when none is selected).");
    Grid::new("cells").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        detection_image(ui, p, env);
        number(ui, "Requested pixel size", &mut p.requested_pixel_size, 0.01, 0.05..=20.0, u);
        number(ui, "Background radius", &mut p.background_radius, 0.1, 0.0..=100.0, u);
        number(ui, "Sigma", &mut p.sigma, 0.05, 0.0..=20.0, u);
        number(ui, "Minimum area", &mut p.min_area, 0.5, 0.0..=10_000.0, u2);
        number(ui, "Maximum area", &mut p.max_area, 1.0, 0.0..=100_000.0, u2);
        number(ui, "Threshold", &mut p.threshold, 0.005, 0.0..=1000.0, "");
        number(ui, "Max background intensity", &mut p.max_background, 0.05, 0.0..=1000.0, "");
        check(ui, "Split by shape", &mut p.watershed);
        number(ui, "Cell expansion", &mut p.cell_expansion, 0.1, 0.0..=100.0, u);
        check(ui, "Smooth boundaries", &mut p.smooth_boundaries);
        check(ui, "Make measurements", &mut p.make_measurements);
    });
    if let Some(pos) = &mut p.positive {
        intensity(ui, pos);
    }
    buttons(ui, "Run")
}

fn check(ui: &mut egui::Ui, label: &str, v: &mut bool) {
    ui.label(label);
    ui.checkbox(v, "");
    ui.end_row();
}

fn detection_image(ui: &mut egui::Ui, p: &mut CellParams, env: &DialogEnv<'_>) {
    ui.label("Detection image");
    let text = match p.detection_image {
        DetectionImage::Hematoxylin => "Hematoxylin OD".to_owned(),
        DetectionImage::OpticalDensitySum => "Optical density sum".to_owned(),
        DetectionImage::Channel(c) => env.channels.get(c).cloned().unwrap_or_default(),
    };
    egui::ComboBox::from_id_salt("det-image").selected_text(text).show_ui(ui, |ui| {
        if env.brightfield {
            ui.selectable_value(&mut p.detection_image, DetectionImage::Hematoxylin, "Hematoxylin OD");
            ui.selectable_value(&mut p.detection_image, DetectionImage::OpticalDensitySum, "Optical density sum");
        } else {
            for (i, c) in env.channels.iter().enumerate() {
                ui.selectable_value(&mut p.detection_image, DetectionImage::Channel(i), c);
            }
        }
    });
    ui.end_row();
}

fn intensity(ui: &mut egui::Ui, pos: &mut crate::analysis::cells::PositiveParams) {
    ui.separator();
    ui.label(egui::RichText::new("Intensity parameters").strong());
    Grid::new("positive").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        ui.label("Score compartment");
        ui.text_edit_singleline(&mut pos.compartment);
        ui.end_row();
        number(ui, "Threshold 1+", &mut pos.thresholds[0], 0.005, 0.0..=5.0, "");
        number(ui, "Threshold 2+", &mut pos.thresholds[1], 0.005, 0.0..=5.0, "");
        number(ui, "Threshold 3+", &mut pos.thresholds[2], 0.005, 0.0..=5.0, "");
        check(ui, "Single threshold", &mut pos.single_threshold);
    });
}

/// Create tiles.
pub fn tiles(ui: &mut egui::Ui, p: &mut TileParams, env: &DialogEnv<'_>) -> Outcome {
    let u = unit(env);
    Grid::new("tiles").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        number(ui, "Tile width", &mut p.width, 1.0, 1.0..=100_000.0, u);
        number(ui, "Tile height", &mut p.height, 1.0, 1.0..=100_000.0, u);
        ui.label("Trim to ROI");
        ui.checkbox(&mut p.trim_to_roi, "");
        ui.end_row();
        ui.label("Make annotation tiles");
        ui.checkbox(&mut p.make_annotations, "");
        ui.end_row();
    });
    buttons(ui, "Run")
}

fn channel_label(c: ThresholdChannel, env: &DialogEnv<'_>) -> String {
    match c {
        ThresholdChannel::Average => "Average channels".into(),
        ThresholdChannel::OdSum => "Optical density sum".into(),
        ThresholdChannel::Stain(0) => "Hematoxylin".into(),
        ThresholdChannel::Stain(_) => "Stain 2".into(),
        ThresholdChannel::Channel(i) => env.channels.get(i).cloned().unwrap_or_default(),
    }
}

/// Create thresholder.
pub fn threshold(ui: &mut egui::Ui, p: &mut ThresholdParams, env: &DialogEnv<'_>) -> Outcome {
    let u2 = if env.calibrated { " µm²" } else { " px²" };
    ui.label("Thresholds at low resolution and creates objects (e.g. tissue detection).");
    Grid::new("threshold").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        number(ui, "Resolution (downsample)", &mut p.downsample, 0.5, 1.0..=256.0, "×");
        ui.label("Channel");
        egui::ComboBox::from_id_salt("thr-channel").selected_text(channel_label(p.channel, env)).show_ui(ui, |ui| {
            let mut opts = vec![ThresholdChannel::Average];
            if env.brightfield {
                opts.extend([ThresholdChannel::OdSum, ThresholdChannel::Stain(0), ThresholdChannel::Stain(1)]);
            }
            opts.extend((0..env.channels.len()).map(ThresholdChannel::Channel));
            for o in opts {
                ui.selectable_value(&mut p.channel, o, channel_label(o, env));
            }
        });
        ui.end_row();
        number(ui, "Prefilter (Gaussian σ, px)", &mut p.sigma, 0.1, 0.0..=50.0, "");
        number(ui, "Threshold", &mut p.threshold, 0.5, -1e6..=1e6, "");
        ui.label("Keep");
        ui.horizontal(|ui| {
            ui.radio_value(&mut p.above, true, "Above");
            ui.radio_value(&mut p.above, false, "Below");
        });
        ui.end_row();
        number(ui, "Minimum object size", &mut p.min_object_area, 100.0, 0.0..=1e10, u2);
        number(ui, "Minimum hole size", &mut p.min_hole_area, 100.0, 0.0..=1e10, u2);
        ui.label("Split objects");
        ui.checkbox(&mut p.split, "");
        ui.end_row();
        ui.label("Create detections");
        ui.checkbox(&mut p.detections, "");
        ui.end_row();
        ui.label("Classification");
        class_combo(ui, "thr-class", &mut p.class, env.classes);
        ui.end_row();
    });
    buttons(ui, "Create objects")
}

/// A class picker including Unclassified.
pub fn class_combo(ui: &mut egui::Ui, id: &str, class: &mut Option<PathClass>, classes: &[PathClass]) {
    let text = crate::objects::classes::display_name(class.as_ref()).to_owned();
    egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        ui.selectable_value(class, None, "Unclassified");
        for c in classes {
            ui.selectable_value(class, Some(c.clone()), c.name());
        }
    });
}

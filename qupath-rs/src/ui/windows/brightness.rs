//! View → Brightness/Contrast: channels, display range, gamma and histogram.

use egui::{Color32, Sense, Stroke, pos2, vec2};

use crate::app::QuPathApp;
use crate::image::display::Selection;

/// Draws the Brightness/Contrast window contents.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp) {
    let Some(d) = &mut app.s.image else {
        ui.label("No image open.");
        return;
    };
    let rgb = d.server.metadata().rgb;
    let n = d.display.channels.len();
    app.ui.bc_channel = app.ui.bc_channel.min(n.saturating_sub(1));
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        if rgb && ui.selectable_label(d.display.selection == Selection::Original, "Original (RGB)").clicked() {
            d.display.selection = Selection::Original;
            d.display.changed();
        }
        for i in 0..n {
            channel_row(ui, d, i, &mut app.ui.bc_channel, rgb);
        }
    });
    ui.separator();
    let c = app.ui.bc_channel;
    let key = (d.id, c, d.display.stains.as_ref().map(|s| s.stains[0].od));
    if app.ui.bc_hist.as_ref().is_none_or(|(k, _)| *k != key)
        && let Some(p) = &d.preview
    {
        app.ui.bc_hist = Some((key, d.display.histogram(p, c, 128)));
    }
    if let Some((_, (hist, lo, hi))) = &app.ui.bc_hist {
        histogram(ui, hist, *lo, *hi, &d.display.channels[c]);
        range_controls(ui, d, c, *lo, *hi);
    }
}

fn channel_row(ui: &mut egui::Ui, d: &mut crate::app::data::ImageData, i: usize, current: &mut usize, rgb: bool) {
    let disp = &mut d.display;
    ui.horizontal(|ui| {
        let mut vis = disp.channels[i].visible;
        let composite_member = !rgb || i < 3;
        if composite_member && ui.checkbox(&mut vis, "").on_hover_text("Show in the composite").changed() {
            disp.channels[i].visible = vis;
            disp.changed();
        }
        super::super::panes::swatch(ui, disp.channels[i].color.to_color32());
        let showing = disp.selection == Selection::Single(i);
        let r = ui.selectable_label(*current == i || showing, &disp.channels[i].name);
        if r.clicked() {
            *current = i;
        }
        if r.double_clicked() || (r.clicked() && !composite_member) {
            disp.selection = Selection::Single(i);
            disp.changed();
        }
    });
}

fn histogram(ui: &mut egui::Ui, hist: &[u32], lo: f32, hi: f32, ch: &crate::image::display::DisplayChannel) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 110.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    // Log scale, as QuPath draws it.
    let max = hist.iter().map(|v| f32::ln_1p(*v as f32)).fold(1.0, f32::max);
    let bw = rect.width() / hist.len() as f32;
    let col = ch.color.to_color32().gamma_multiply(0.8);
    for (i, v) in hist.iter().enumerate() {
        let h = f32::ln_1p(*v as f32) / max * rect.height();
        let x = rect.left() + i as f32 * bw;
        p.rect_filled(egui::Rect::from_min_max(pos2(x, rect.bottom() - h), pos2(x + bw, rect.bottom())), 0.0, col);
    }
    let to_x = |v: f32| rect.left() + (v - lo) / (hi - lo).max(1e-6) * rect.width();
    for v in [ch.min, ch.max] {
        let x = to_x(v).clamp(rect.left(), rect.right());
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(255, 80, 80)));
    }
}

fn range_controls(ui: &mut egui::Ui, d: &mut crate::app::data::ImageData, c: usize, lo: f32, hi: f32) {
    let disp = &mut d.display;
    let step = ((hi - lo) / 500.0).max(1e-4);
    let mut changed = false;
    egui::Grid::new("bc-range").num_columns(2).show(ui, |ui| {
        let ch = &mut disp.channels[c];
        ui.label("Min display");
        changed |= ui.add(egui::Slider::new(&mut ch.min, lo..=hi).step_by(f64::from(step))).changed();
        ui.end_row();
        ui.label("Max display");
        changed |= ui.add(egui::Slider::new(&mut ch.max, lo..=hi).step_by(f64::from(step))).changed();
        ui.end_row();
        ui.label("Gamma");
        changed |= ui.add(egui::Slider::new(&mut disp.gamma, 0.1..=4.0).logarithmic(true)).changed();
        ui.end_row();
    });
    ui.horizontal(|ui| {
        if ui.button("Auto").on_hover_text("Saturate 0.1% at each end").clicked()
            && let Some(p) = &d.preview
        {
            disp.auto_range(p, c, 0.1);
        }
        if ui.button("Reset").clicked() {
            let fresh = crate::image::display::ImageDisplay::new(d.server.metadata(), d.settings.image_type, d.settings.stains.clone());
            disp.channels[c].min = fresh.channels[c].min;
            disp.channels[c].max = fresh.channels[c].max;
            disp.gamma = 1.0;
            changed = true;
        }
        changed |= ui.checkbox(&mut disp.grayscale, "Show grayscale").changed();
    });
    if changed {
        disp.changed();
    }
}

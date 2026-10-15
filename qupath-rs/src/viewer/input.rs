//! Zooming and panning with wheel, trackpad, pinch and keys; pointer state for tools.

use egui::{Event, Key, MouseWheelUnit, Response};

use super::tools::Pointer;
use super::transform::Transform;

/// Most zoomed in: 1 image pixel = 40 screen points.
pub const MIN_DOWNSAMPLE: f64 = 1.0 / 40.0;

/// Applies wheel, pinch and arrow-key navigation. Returns whether the view changed.
pub fn navigate(ui: &egui::Ui, response: &Response, t: &mut Transform, image_size: (u32, u32)) -> bool {
    let max_ds = (f64::from(image_size.0.max(image_size.1)) / 64.0).max(4.0);
    let before = *t;
    if response.hovered() {
        wheel_and_pinch(ui, response, t, max_ds);
    }
    if response.has_focus() || response.hovered() {
        arrow_keys(ui, t);
    }
    *t != before
}

fn wheel_and_pinch(ui: &egui::Ui, response: &Response, t: &mut Transform, max_ds: f64) {
    let anchor = response.hover_pos().unwrap_or(t.rect.center());
    let (events, zoom) = ui.input(|i| (i.events.clone(), i.zoom_delta()));
    for e in events {
        let Event::MouseWheel { unit, delta, modifiers, .. } = e else { continue };
        if modifiers.command {
            continue; // handled as zoom_delta
        }
        match unit {
            // Trackpads scroll in points: pan, as QuPath does with scroll gestures.
            MouseWheelUnit::Point => t.pan(delta),
            // Mouse wheels zoom about the cursor.
            _ => t.zoom_about(anchor, 1.15f64.powf(-f64::from(delta.y)), MIN_DOWNSAMPLE, max_ds),
        }
    }
    if (zoom - 1.0).abs() > f32::EPSILON {
        t.zoom_about(anchor, 1.0 / f64::from(zoom), MIN_DOWNSAMPLE, max_ds);
    }
}

fn arrow_keys(ui: &egui::Ui, t: &mut Transform) {
    let step = 40.0;
    let down = |k: Key| ui.input(|i| i.key_down(k) && !i.modifiers.any());
    let axis = |neg: Key, pos: Key| {
        if down(neg) {
            step
        } else if down(pos) {
            -step
        } else {
            0.0
        }
    };
    let d = egui::vec2(axis(Key::ArrowLeft, Key::ArrowRight), axis(Key::ArrowUp, Key::ArrowDown));
    if d != egui::Vec2::ZERO {
        t.pan(d * ui.input(|i| i.stable_dt).min(0.1) * 12.0);
        ui.ctx().request_repaint();
    }
}

/// Zooms by a factor around the viewer center (toolbar and menu commands).
pub fn zoom_center(t: &mut Transform, factor: f64, image_size: (u32, u32)) {
    let max_ds = (f64::from(image_size.0.max(image_size.1)) / 64.0).max(4.0);
    let c = t.rect.center();
    t.zoom_about(c, factor, MIN_DOWNSAMPLE, max_ds);
}

/// Pointer state for tools, from the viewer's response.
pub fn pointer(ui: &egui::Ui, response: &Response, t: &Transform) -> Option<Pointer> {
    let pos = response.interact_pointer_pos().or_else(|| response.hover_pos())?;
    let primary = ui.input(|i| i.pointer.primary_down() || i.pointer.primary_released());
    let m = ui.input(|i| i.modifiers);
    Some(Pointer {
        pos: t.to_image(pos),
        drag_started: response.drag_started_by(egui::PointerButton::Primary),
        dragging: response.dragged_by(egui::PointerButton::Primary) && primary,
        drag_stopped: response.drag_stopped_by(egui::PointerButton::Primary),
        clicked: response.clicked_by(egui::PointerButton::Primary),
        double_clicked: response.double_clicked(),
        shift: m.shift,
        alt: m.alt,
        command: m.command,
        delta: response.drag_delta(),
    })
}

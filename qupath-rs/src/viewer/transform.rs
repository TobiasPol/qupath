//! Mapping between image pixels and screen points.

use egui::{Pos2, Rect, pos2, vec2};

use crate::objects::{Bounds, Pt};

/// Where the viewer looks and how far it is zoomed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Image position at the center of the viewer.
    pub center: Pt,
    /// Image pixels per screen point (QuPath's "downsample").
    pub downsample: f64,
    /// The viewer's screen rectangle.
    pub rect: Rect,
}

impl Transform {
    /// Image position to screen point.
    #[inline]
    pub fn to_screen(&self, p: Pt) -> Pos2 {
        let c = self.rect.center();
        pos2(c.x + ((p.x - self.center.x) / self.downsample) as f32, c.y + ((p.y - self.center.y) / self.downsample) as f32)
    }

    /// Screen point to image position.
    #[inline]
    pub fn to_image(&self, s: Pos2) -> Pt {
        let c = self.rect.center();
        Pt { x: self.center.x + f64::from(s.x - c.x) * self.downsample, y: self.center.y + f64::from(s.y - c.y) * self.downsample }
    }

    /// Screen rectangle of image bounds.
    pub fn rect_to_screen(&self, b: &Bounds) -> Rect {
        Rect::from_two_pos(self.to_screen(Pt { x: b.x0, y: b.y0 }), self.to_screen(Pt { x: b.x1, y: b.y1 }))
    }

    /// The image region visible in the viewer.
    pub fn visible_bounds(&self) -> Bounds {
        let a = self.to_image(self.rect.min);
        let b = self.to_image(self.rect.max);
        Bounds { x0: a.x, y0: a.y, x1: b.x, y1: b.y }
    }

    /// Zooms by a factor (> 1 zooms out) keeping the image point under `anchor` fixed.
    pub fn zoom_about(&mut self, anchor: Pos2, factor: f64, min_ds: f64, max_ds: f64) {
        let before = self.to_image(anchor);
        self.downsample = (self.downsample * factor).clamp(min_ds, max_ds);
        let after = self.to_image(anchor);
        self.center.x += before.x - after.x;
        self.center.y += before.y - after.y;
    }

    /// Fits bounds in the viewer with a margin.
    pub fn fit(&mut self, b: &Bounds, margin: f32) {
        let avail = self.rect.size() - vec2(margin * 2.0, margin * 2.0);
        if avail.x <= 1.0 || avail.y <= 1.0 {
            return;
        }
        self.downsample = (b.width() / f64::from(avail.x)).max(b.height() / f64::from(avail.y)).max(1e-3);
        self.center = Pt { x: (b.x0 + b.x1) / 2.0, y: (b.y0 + b.y1) / 2.0 };
    }

    /// Pans by a screen delta.
    pub fn pan(&mut self, delta: egui::Vec2) {
        self.center.x -= f64::from(delta.x) * self.downsample;
        self.center.y -= f64::from(delta.y) * self.downsample;
    }
}

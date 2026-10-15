//! Fonts, text rendering and colors: fastframe for the platform, QuPath for the look.

use std::collections::BTreeSet;

use egui::{Color32, Stroke, Visuals};
use fastframe_theme::Base;

use crate::io::prefs::{Prefs, ThemeChoice};

/// The sixteen fastframe base colors, which every palette file can override.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// Light or dark base.
    pub dark: bool,
    /// Window background.
    pub window: Color32,
    /// Panels.
    pub panel: Color32,
    /// Controls.
    pub surface: Color32,
    /// Hovered controls.
    pub surface_hover: Color32,
    /// Pressed controls.
    pub surface_active: Color32,
    /// Borders.
    pub outline: Color32,
    /// Text.
    pub text: Color32,
    /// Secondary text.
    pub secondary: Color32,
    /// Disabled text.
    pub dim: Color32,
    /// Selection and focus.
    pub accent: Color32,
    /// Hovered accent.
    pub accent_hover: Color32,
    /// Text on accent.
    pub on_accent: Color32,
    /// Errors.
    pub danger: Color32,
    /// Warnings.
    pub warning: Color32,
    /// Scrims.
    pub overlay: Color32,
    /// Shadows.
    pub shadow: Color32,
}

impl Palette {
    /// QuPath's default (JavaFX Modena) look.
    pub fn light() -> Self {
        let c = Color32::from_rgb;
        Palette {
            dark: false,
            window: c(0xf4, 0xf4, 0xf4),
            panel: c(0xec, 0xec, 0xec),
            surface: c(0xfb, 0xfb, 0xfb),
            surface_hover: c(0xe4, 0xe9, 0xf0),
            surface_active: c(0xd2, 0xdc, 0xe8),
            outline: c(0xb5, 0xb5, 0xb5),
            text: c(0x1f, 0x1f, 0x1f),
            secondary: c(0x55, 0x55, 0x55),
            dim: c(0x99, 0x99, 0x99),
            accent: c(0x00, 0x96, 0xc9),
            accent_hover: c(0x0a, 0xa6, 0xdb),
            on_accent: Color32::WHITE,
            danger: c(0xc6, 0x28, 0x28),
            warning: c(0xe0, 0x9a, 0x00),
            overlay: Color32::from_black_alpha(90),
            shadow: Color32::from_black_alpha(40),
        }
    }

    /// QuPath's dark theme.
    pub fn dark() -> Self {
        let c = Color32::from_rgb;
        Palette {
            dark: true,
            window: c(0x28, 0x2a, 0x2e),
            panel: c(0x2f, 0x31, 0x36),
            surface: c(0x3a, 0x3d, 0x43),
            surface_hover: c(0x45, 0x49, 0x50),
            surface_active: c(0x52, 0x57, 0x60),
            outline: c(0x55, 0x59, 0x60),
            text: c(0xe6, 0xe6, 0xe6),
            secondary: c(0xb0, 0xb3, 0xb8),
            dim: c(0x7c, 0x80, 0x87),
            accent: c(0x2b, 0xa5, 0xd8),
            accent_hover: c(0x4c, 0xb5, 0xe2),
            on_accent: Color32::WHITE,
            danger: c(0xe5, 0x53, 0x53),
            warning: c(0xf0, 0xb4, 0x29),
            overlay: Color32::from_black_alpha(140),
            shadow: Color32::from_black_alpha(90),
        }
    }
}

impl fastframe_theme::Palette for Palette {
    fn base(base: Base) -> Self {
        match base {
            Base::Dark => Palette::dark(),
            Base::Light => Palette::light(),
        }
    }

    fn set(&mut self, name: &str, color: Color32) -> bool {
        let slot = match name {
            "window" => &mut self.window,
            "panel" => &mut self.panel,
            "surface" => &mut self.surface,
            "surface_hover" => &mut self.surface_hover,
            "surface_active" => &mut self.surface_active,
            "outline" => &mut self.outline,
            "text" => &mut self.text,
            "secondary" => &mut self.secondary,
            "dim" => &mut self.dim,
            "accent" => &mut self.accent,
            "accent_hover" => &mut self.accent_hover,
            "on_accent" => &mut self.on_accent,
            "danger" => &mut self.danger,
            "warning" => &mut self.warning,
            "overlay" => &mut self.overlay,
            "shadow" => &mut self.shadow,
            _ => return false,
        };
        *slot = color;
        true
    }

    fn derive(&mut self, _given: &BTreeSet<&str>) {}
}

/// The shared fastframe palettes, by file name.
pub fn shared_palettes() -> Vec<(String, Palette)> {
    fastframe_theme::presets::themes::<Palette>().map(|t| (t.filename, t.palette)).collect()
}

/// The palette for the user's choice.
pub fn palette_for(choice: &ThemeChoice, system_dark: bool) -> Palette {
    match choice {
        ThemeChoice::Light => Palette::light(),
        ThemeChoice::Dark => Palette::dark(),
        ThemeChoice::System => {
            if system_dark {
                Palette::dark()
            } else {
                Palette::light()
            }
        }
        ThemeChoice::Palette(name) => shared_palettes()
            .into_iter()
            .find(|(f, _)| f == name)
            .map_or_else(|| if system_dark { Palette::dark() } else { Palette::light() }, |(_, p)| p),
    }
}

/// egui visuals from a palette.
pub fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.window_fill = p.panel;
    v.panel_fill = p.window;
    v.faint_bg_color = p.panel;
    v.extreme_bg_color = p.surface;
    v.code_bg_color = p.surface;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.danger;
    v.warn_fg_color = p.warning;
    v.window_stroke = Stroke::new(1.0, p.outline);
    v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.55 } else { 0.35 });
    v.selection.stroke = Stroke::new(1.0, p.accent);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.window;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.outline);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.secondary);
    w.inactive.bg_fill = p.surface;
    w.inactive.weak_bg_fill = p.surface;
    w.inactive.bg_stroke = Stroke::new(1.0, p.outline.gamma_multiply(0.6));
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.bg_fill = p.surface_hover;
    w.hovered.weak_bg_fill = p.surface_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.bg_fill = p.surface_active;
    w.active.weak_bg_fill = p.surface_active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.weak_bg_fill = p.surface_active;
    v
}

/// Fonts (Inter or the system face, with fallbacks for every script) and the desktop's hinting.
pub fn install_fonts(ctx: &egui::Context, prefs: &Prefs) {
    let primary = if prefs.system_font { fastframe_fonts::Primary::System } else { fastframe_fonts::Primary::Inter };
    let mut fonts = fastframe_fonts::FontSetup::default().primary(primary).definitions();
    let rendering = fastframe_text::detect();
    rendering.apply_to(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Applies the theme (after fonts, so fastframe-text can adjust the visuals).
pub fn apply_theme(ctx: &egui::Context, prefs: &Prefs) -> Palette {
    let system_dark = ctx.system_theme().is_none_or(|t| t == egui::Theme::Dark);
    let p = palette_for(&prefs.theme, system_dark);
    let vis = visuals(&p);
    let rendering = fastframe_text::detect();
    ctx.all_styles_mut(|s| {
        s.visuals = vis.clone();
        rendering.apply_to_visuals(&mut s.visuals);
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(6.0, 3.0);
        s.interaction.tooltip_delay = 0.4;
    });
    p
}

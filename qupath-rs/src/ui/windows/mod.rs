//! Tool windows: Brightness/Contrast, measurement tables, preferences, log, command list, about, slide label.

pub mod brightness;
pub mod log;
pub mod measurements;
pub mod prefs;

use egui::Window;

use crate::app::QuPathApp;
use crate::app::commands::Command;
use crate::app::menu_table::menu_path;

/// Draws every open tool window.
pub fn show(ctx: &egui::Context, app: &mut QuPathApp) {
    let screen = ctx.content_rect();
    let mut open = app.s.windows.brightness;
    Window::new("Brightness & contrast")
        .open(&mut open)
        .default_width(340.0)
        .default_pos([screen.right() - 370.0, 90.0])
        .show(ctx, |ui| brightness::show(ui, app));
    app.s.windows.brightness = open;
    table_window(ctx, app, false);
    table_window(ctx, app, true);
    let mut open = app.s.windows.prefs;
    Window::new("Preferences")
        .open(&mut open)
        .default_width(420.0)
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos([screen.center().x, 90.0])
        .show(ctx, |ui| prefs::show(ui, app));
    app.s.windows.prefs = open;
    let mut open = app.s.windows.log;
    Window::new("Log")
        .open(&mut open)
        .default_size([640.0, 360.0])
        .pivot(egui::Align2::CENTER_BOTTOM)
        .default_pos([screen.center().x, screen.bottom() - 40.0])
        .show(ctx, |ui| log::show(ui, app));
    app.s.windows.log = open;
    command_list(ctx, app);
    about(ctx, app);
    slide_label(ctx, app);
}

fn table_window(ctx: &egui::Context, app: &mut QuPathApp, detections: bool) {
    let mut open = if detections { app.s.windows.detection_table } else { app.s.windows.annotation_table };
    let title = if detections { "Detection measurements" } else { "Annotation measurements" };
    let mut clicked = None;
    let screen = ctx.content_rect();
    let y = screen.bottom() - if detections { 420.0 } else { 460.0 };
    Window::new(title).open(&mut open).default_size([720.0, 360.0]).default_pos([380.0 + if detections { 30.0 } else { 0.0 }, y]).show(
        ctx,
        |ui| {
            clicked = measurements::show(ui, app, detections);
        },
    );
    if let Some(k) = clicked {
        crate::ui::panes::select_and_show(app, k, false);
    }
    if detections { app.s.windows.detection_table = open } else { app.s.windows.annotation_table = open }
}

fn command_list(ctx: &egui::Context, app: &mut QuPathApp) {
    let mut open = app.s.windows.command_list;
    let mut run = None;
    Window::new("Command list")
        .open(&mut open)
        .default_size([460.0, 420.0])
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos([ctx.content_rect().center().x, 80.0])
        .show(ctx, |ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut app.ui.command_filter).hint_text("Type to search commands").desired_width(f32::INFINITY),
            );
            r.request_focus();
            let f = app.ui.command_filter.to_lowercase();
            let matches: Vec<Command> = Command::ALL
                .iter()
                .copied()
                .filter(|c| f.is_empty() || c.label().to_lowercase().contains(&f) || menu_path(*c).to_lowercase().contains(&f))
                .collect();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                run = matches.first().copied();
            }
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                egui::Grid::new("cmds").num_columns(3).striped(true).show(ui, |ui| {
                    for c in matches {
                        let enabled = app.enabled(c);
                        if ui.add_enabled(enabled, egui::Button::new(c.label()).frame(false)).clicked() {
                            run = Some(c);
                        }
                        ui.weak(menu_path(c));
                        ui.weak(c.shortcut().map(|s| ui.ctx().format_shortcut(&s)).unwrap_or_default());
                        ui.end_row();
                    }
                });
            });
        });
    if let Some(c) = run {
        app.s.queued.push(c);
        open = false;
    }
    app.s.windows.command_list = open;
}

fn about(ctx: &egui::Context, app: &mut QuPathApp) {
    let mut open = app.s.windows.about;
    Window::new("About qupath-rs").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.heading(format!("qupath-rs {}", env!("CARGO_PKG_VERSION")));
        ui.label("The core of QuPath — the open source software for bioimage analysis — rebuilt in Rust.");
        ui.add_space(6.0);
        ui.label("Interface: egui, with fastframe for fonts, text rendering, icons, palettes, logging and safe saves.");
        ui.label("Compatible with QuPath projects (.qpproj), classes and GeoJSON.");
        ui.add_space(6.0);
        ui.hyperlink_to("QuPath", "https://qupath.github.io");
        ui.hyperlink_to("fastframe", "https://fastframe.dev");
        ui.hyperlink_to("egui", "https://www.egui.rs");
        ui.add_space(6.0);
        ui.weak("QuPath is GPL-3.0; please cite Bankhead et al., Scientific Reports (2017).");
    });
    app.s.windows.about = open;
}

fn slide_label(ctx: &egui::Context, app: &mut QuPathApp) {
    if !app.s.windows.slide_label {
        return;
    }
    let Some(d) = &app.s.image else { return };
    if app.ui.label.as_ref().is_none_or(|(id, _)| *id != d.id) {
        let md = d.server.metadata();
        let name = md.associated.iter().find(|n| n.contains("label")).or_else(|| md.associated.first()).cloned();
        if let Some(img) = name.and_then(|n| d.server.read_associated(&n).ok()) {
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            app.ui.label = Some((d.id, ctx.load_texture("slide-label", ci, egui::TextureOptions::LINEAR)));
        }
    }
    let mut open = true;
    Window::new("Slide label").open(&mut open).show(ctx, |ui| match &app.ui.label {
        Some((_, tex)) => {
            ui.add(egui::Image::new(tex).max_size(egui::vec2(400.0, 400.0)));
        }
        None => {
            ui.label("This image has no label or macro image.");
        }
    });
    app.s.windows.slide_label = open;
}

//! qupath-rs: start logging, read preferences, and open the window.

use std::path::PathBuf;

use qupath::app::QuPathApp;
use qupath::io::prefs::{Prefs, dirs};
use qupath::ui::{icons::Icon, style};

fn start_logging() -> Option<PathBuf> {
    let logs = dirs().map(|d| d.data_local_dir().join("logs"));
    let mut logging = fastframe_log::Logging::new("qupath-rs", env!("CARGO_PKG_VERSION")).filter("warn,qupath=info,qupath_rs=info");
    let file = logs.as_ref().map(|l| l.join("qupath-rs.log"));
    if let Some(f) = &file {
        logging = logging.file(f.clone());
    }
    if let Some(l) = &logs {
        logging = logging.panic_log(l.join("panic.log"));
    }
    if let Err(e) = logging.init() {
        eprintln!("logging unavailable: {e}");
    }
    file
}

fn main() -> eframe::Result {
    let log_file = start_logging();
    qupath::image::openslide::preload();
    let prefs = Prefs::load();
    let open = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("QuPath (qupath-rs)")
            .with_app_id("qupath-rs")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([820.0, 520.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "qupath-rs",
        options,
        Box::new(move |cc| {
            fastframe_file::mark_ui_thread();
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fastframe_icons::install::<Icon>(&cc.egui_ctx);
            style::install_fonts(&cc.egui_ctx, &prefs);
            let palette = style::apply_theme(&cc.egui_ctx, &prefs);
            Ok(Box::new(QuPathApp::new(&cc.egui_ctx, prefs, palette, log_file, open)))
        }),
    )
}

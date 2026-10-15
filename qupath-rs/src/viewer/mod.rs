//! The image viewer (QuPath's `QuPathViewer`).

pub mod decorations;
pub mod input;
pub mod overlay;
pub mod paths;
pub mod render;
pub mod tiles;
pub mod tools;
pub mod transform;

use egui::{Color32, Rect, Response, Sense, Stroke, TextureHandle, TextureOptions};

use crate::app::data::ImageData;
use crate::io::prefs::Prefs;
use crate::objects::spatial::SpatialIndex;
use crate::objects::{Bounds, PathClass, Pt};
use overlay::{FillCache, OverlayOptions, Strokes};
use tiles::TileManager;
use tools::{Gesture, Tool, ToolCtx};
use transform::Transform;

/// What the viewer needs from the app each frame.
pub struct ViewerArgs<'a> {
    /// The open image.
    pub data: &'a mut ImageData,
    /// What to draw.
    pub overlay: &'a OverlayOptions,
    /// The active tool.
    pub tool: Tool,
    /// Preferences.
    pub prefs: &'a Prefs,
    /// Class for new annotations.
    pub auto_class: Option<PathClass>,
    /// Shapes select instead of creating annotations.
    pub selection_mode: bool,
}

/// The viewer's state.
pub struct Viewer {
    /// Position and zoom.
    pub t: Transform,
    /// Tile cache.
    pub tiles: TileManager,
    /// Interaction in progress.
    pub gesture: Gesture,
    /// Spatial index of the hierarchy.
    pub index: SpatialIndex,
    /// Image point under the cursor.
    pub cursor: Option<Pt>,
    /// Detections inside the view last frame.
    pub detections_in_view: usize,
    fills: FillCache,
    image_id: Option<u64>,
    overview: Option<TextureHandle>,
    needs_fit: bool,
}

impl Viewer {
    /// A viewer with its tile readers.
    pub fn new(ctx: &egui::Context, cache_mb: usize) -> Self {
        Viewer {
            t: Transform {
                center: Pt { x: 0.0, y: 0.0 },
                downsample: 1.0,
                rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            },
            tiles: TileManager::new(ctx, cache_mb),
            gesture: Gesture::None,
            index: SpatialIndex::default(),
            cursor: None,
            detections_in_view: 0,
            fills: FillCache::default(),
            image_id: None,
            overview: None,
            needs_fit: true,
        }
    }

    /// Fits the whole image on the next frame.
    pub fn zoom_to_fit(&mut self) {
        self.needs_fit = true;
    }

    /// Centers on bounds, zooming out if needed to show them.
    pub fn center_on(&mut self, b: &Bounds) {
        let mut t = self.t;
        t.fit(b, 60.0);
        self.t.center = t.center;
        self.t.downsample = self.t.downsample.max(t.downsample);
    }

    fn switch_image(&mut self, ctx: &egui::Context, data: &ImageData, prefs: &Prefs) {
        self.image_id = Some(data.id);
        self.tiles.clear(prefs.tile_cache_mb);
        self.gesture = Gesture::None;
        self.needs_fit = true;
        self.overview = data.overview.as_ref().map(|img| {
            let ci = egui::ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw());
            ctx.load_texture("overview", ci, TextureOptions::LINEAR)
        });
    }

    /// Shows the viewer; returns its response (for context menus).
    pub fn show(&mut self, ui: &mut egui::Ui, mut a: ViewerArgs<'_>) -> (Response, bool) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(24));
        if self.image_id != Some(a.data.id) {
            self.switch_image(ui.ctx(), a.data, a.prefs);
        }
        let md = a.data.server.metadata().clone();
        self.t.rect = rect;
        if self.needs_fit && rect.width() > 10.0 {
            self.t.fit(&Bounds { x0: 0.0, y0: 0.0, x1: f64::from(md.width()), y1: f64::from(md.height()) }, 10.0);
            self.needs_fit = false;
        }
        let overview_rect =
            (a.overlay.show_overview && self.overview.is_some()).then(|| decorations::overview_rect(rect, md.width(), md.height()));
        let on_overview = response.hover_pos().zip(overview_rect).is_some_and(|(p, r)| r.contains(p));
        input::navigate(ui, &response, &mut self.t, (md.width(), md.height()));
        self.index.update(&a.data.hierarchy);
        let finished =
            if on_overview { self.overview_click(ui, &response, overview_rect, &md) } else { self.run_tool(ui, &response, &mut a) };
        self.cursor = response.hover_pos().map(|p| self.t.to_image(p));
        self.draw(ui, &painter, &a, &md);
        if self.tiles.busy() || !matches!(self.gesture, Gesture::None) {
            ui.ctx().request_repaint();
        }
        (response, finished)
    }

    fn overview_click(&mut self, ui: &egui::Ui, response: &Response, r: Option<Rect>, md: &crate::image::ImageMetadata) -> bool {
        let down = ui.input(|i| i.pointer.primary_down());
        if let (Some(r), Some(p), true) = (r, response.interact_pointer_pos(), down) {
            self.t.center = decorations::overview_to_image(r, p, md.width(), md.height());
        }
        false
    }

    fn run_tool(&mut self, ui: &egui::Ui, response: &Response, a: &mut ViewerArgs<'_>) -> bool {
        let Some(mut p) = input::pointer(ui, response, &self.t) else { return false };
        let space = ui.input(|i| i.key_down(egui::Key::Space));
        // Holding space pans with any tool, as in QuPath.
        let tool = if space && matches!(self.gesture, Gesture::None | Gesture::Pan) { Tool::Move } else { a.tool };
        if space && tool == Tool::Move {
            p.clicked = false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.gesture = Gesture::None;
        }
        let (auto_class, selection_mode, detections) = (a.auto_class.clone(), a.selection_mode, a.overlay.show_detections);
        let brush = a.prefs.brush_diameter;
        let data = &mut *a.data;
        let mut ctx = ToolCtx::new(
            &mut data.hierarchy,
            &mut data.undo,
            &self.index,
            &mut self.t,
            &data.server,
            auto_class,
            selection_mode,
            detections,
            brush,
        );
        tools::handle(tool, &mut self.gesture, &mut ctx, &p);
        ctx.finished
    }

    fn draw(&mut self, ui: &egui::Ui, painter: &egui::Painter, a: &ViewerArgs<'_>, md: &crate::image::ImageMetadata) {
        self.tiles.begin_frame();
        render::draw_image(ui.ctx(), painter, &self.t, &a.data.server, a.data.id, &mut self.tiles, &a.data.display);
        self.tiles.end_frame();
        let strokes =
            Strokes { annotation: a.prefs.annotation_stroke, detection: a.prefs.detection_stroke, point_radius: a.prefs.point_radius };
        self.detections_in_view =
            overlay::draw_objects(painter, &self.t, &a.data.hierarchy, &self.index, a.overlay, strokes, &mut self.fills);
        if let Some(roi) = tools::preview(a.tool, &self.gesture, self.cursor) {
            overlay::draw_roi(painter, &self.t, &roi, Stroke::new(a.prefs.annotation_stroke, Color32::YELLOW), None, a.prefs.point_radius);
        }
        if a.tool == Tool::Move
            && let Some(o) = a.data.hierarchy.primary_selected().and_then(|k| a.data.hierarchy.get(k))
            && o.is_annotation()
            && !o.locked
        {
            tools::draw_handles(painter, &self.t, &o.roi);
        }
        if matches!(a.tool, Tool::Brush | Tool::Wand)
            && let Some(c) = self.cursor
        {
            let r = a.prefs.brush_diameter / 2.0;
            painter.circle_stroke(self.t.to_screen(c), r, Stroke::new(1.0, Color32::from_white_alpha(180)));
        }
        self.draw_decorations(painter, a, md);
    }

    fn draw_decorations(&mut self, painter: &egui::Painter, a: &ViewerArgs<'_>, md: &crate::image::ImageMetadata) {
        let cal = a.data.calibration(a.prefs);
        if a.overlay.show_grid {
            decorations::draw_grid(painter, &self.t, a.prefs.grid_spacing, &cal, md.width(), md.height());
        }
        if a.overlay.show_overview
            && let Some(tex) = &self.overview
        {
            decorations::draw_overview(painter, &self.t, tex.id(), md.width(), md.height());
        }
        if a.overlay.show_scalebar {
            decorations::draw_scalebar(painter, &self.t, &cal, Color32::WHITE);
        }
        if a.overlay.show_location
            && let Some(c) = self.cursor.filter(|c| c.x >= 0.0 && c.y >= 0.0 && c.x < f64::from(md.width()) && c.y < f64::from(md.height()))
        {
            let values = self.pixel_values(md, c);
            decorations::draw_location(painter, &self.t, c, &cal, values);
        }
    }

    /// Pixel values under the cursor, from cached tiles.
    fn pixel_values(&mut self, md: &crate::image::ImageMetadata, p: Pt) -> Option<String> {
        let level = md.preferred_level(self.t.downsample);
        let l = md.levels[level];
        let (lx, ly) = ((p.x / l.downsample) as u32, (p.y / l.downsample) as u32);
        let key = tiles::TileKey { image: self.image_id?, level, tx: lx / md.tile_width, ty: ly / md.tile_height };
        let tile = self.tiles.raw(&key)?;
        let (x, y) = (lx % md.tile_width, ly % md.tile_height);
        if x >= tile.width || y >= tile.height {
            return None;
        }
        let vals: Vec<String> = md
            .channels
            .iter()
            .enumerate()
            .map(|(c, ch)| format!("{}: {}", if md.rgb { &ch.name[..1] } else { &ch.name }, tile.get(x, y, c.min(tile.channels - 1))))
            .collect();
        Some(vals.join(", "))
    }
}

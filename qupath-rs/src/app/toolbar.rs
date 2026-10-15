//! The toolbar: tools, display toggles, opacity and tool windows, as in QuPath.

use egui::{Button, Color32, Vec2};

use super::QuPathApp;
use super::commands::Command as C;
use crate::ui::icons::Icon;
use crate::viewer::tools::Tool;

const ICON: f32 = 18.0;

impl QuPathApp {
    fn icon_button(&mut self, ui: &mut egui::Ui, icon: Icon, cmd: C, tip: &str) {
        let selected = self.checked(cmd).unwrap_or(false);
        let tint = if selected { self.s.palette.accent } else { ui.visuals().text_color() };
        let mut b = Button::image(icon.image(tint, ICON)).selected(selected).min_size(Vec2::splat(28.0));
        if !selected {
            b = b.frame_when_inactive(false);
        }
        let sc = cmd.shortcut().map(|s| format!(" ({})", ui.ctx().format_shortcut(&s))).unwrap_or_default();
        if ui.add_enabled(self.enabled(cmd), b).on_hover_text(format!("{tip}{sc}")).clicked() {
            self.s.queued.push(cmd);
        }
    }

    /// Draws the toolbar.
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            self.icon_button(ui, Icon::AnalysisPane, C::ShowAnalysisPane, "Show analysis pane");
            ui.separator();
            for (tool, cmd) in Tool::ALL.iter().zip([
                C::ToolMove,
                C::ToolRectangle,
                C::ToolEllipse,
                C::ToolLine,
                C::ToolPolygon,
                C::ToolPolyline,
                C::ToolBrush,
                C::ToolWand,
                C::ToolPoints,
            ]) {
                self.icon_button(ui, tool.icon(), cmd, tool.label());
            }
            ui.separator();
            self.icon_button(ui, Icon::Selection, C::SelectionMode, "Selection mode: shapes select objects");
            ui.separator();
            self.icon_button(ui, Icon::Contrast, C::BrightnessContrast, "Brightness/Contrast");
            self.icon_button(ui, Icon::ZoomFit, C::ZoomToFit, "Zoom to fit");
            ui.separator();
            self.icon_button(ui, Icon::Overview, C::ShowOverview, "Show slide overview");
            self.icon_button(ui, Icon::Location, C::ShowLocation, "Show cursor location");
            self.icon_button(ui, Icon::Scalebar, C::ShowScalebar, "Show scalebar");
            self.icon_button(ui, Icon::Grid, C::ShowGrid, "Show counting grid");
            ui.separator();
            self.icon_button(ui, Icon::Annotations, C::ShowAnnotations, "Show annotations");
            self.icon_button(ui, Icon::FillAnnotations, C::FillAnnotations, "Fill annotations");
            self.icon_button(ui, Icon::Names, C::ShowNames, "Show names");
            self.icon_button(ui, Icon::Detections, C::ShowDetections, "Show detections");
            self.icon_button(ui, Icon::FillDetections, C::FillDetections, "Fill detections");
            ui.add_space(6.0);
            ui.label("Opacity").on_hover_text("Overlay opacity");
            ui.add(egui::Slider::new(&mut self.s.overlay.opacity, 0.0..=1.0).show_value(false));
            ui.separator();
            self.icon_button(ui, Icon::Table, C::AnnotationMeasurements, "Show annotation measurements");
            self.icon_button(ui, Icon::Cells, C::DetectionMeasurements, "Show detection measurements");
            ui.separator();
            self.icon_button(ui, Icon::Command, C::CommandList, "Command list");
            self.icon_button(ui, Icon::Log, C::ShowLog, "Show log");
            self.icon_button(ui, Icon::Settings, C::Preferences, "Preferences");
            self.zoom_label(ui);
        });
    }

    fn zoom_label(&self, ui: &mut egui::Ui) {
        if self.s.image.is_none() {
            return;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mag = self.s.image.as_ref().and_then(|d| d.server.metadata().magnification);
            let ds = self.viewer.t.downsample;
            let text = match mag {
                Some(m) => format!("{:.1}×  ({:.0}%)", m / ds, 100.0 / ds),
                None => format!("{:.0}%", 100.0 / ds),
            };
            ui.label(egui::RichText::new(text).color(Color32::GRAY)).on_hover_text("Magnification (zoom)");
        });
    }
}

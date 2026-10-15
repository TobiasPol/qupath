//! Measure → Show annotation/detection measurements: a sortable, virtual table.

use egui_extras::{Column, TableBuilder};
use indexmap::IndexMap;

use crate::app::QuPathApp;
use crate::objects::ObjKey;
use crate::objects::measurements::{self, Value};

/// A measurement table and its cache.
#[derive(Default)]
pub struct Table {
    version: Option<u64>,
    cols: Vec<String>,
    rows: Vec<IndexMap<String, Value>>,
    keys: Vec<ObjKey>,
    order: Vec<usize>,
    sort: Option<(usize, bool)>,
}

impl Table {
    fn refresh(&mut self, app: &QuPathApp, detections: bool) {
        let Some(d) = &app.s.image else { return };
        let h = &d.hierarchy;
        if self.version == Some(h.version()) {
            return;
        }
        let keys: Vec<ObjKey> =
            if detections { h.detections().map(|(k, _)| k).collect() } else { h.annotations().map(|(k, _)| k).collect() };
        let cal = d.calibration(&app.s.prefs);
        let (cols, rows) = measurements::table(h, &keys, &cal);
        *self = Table { version: Some(h.version()), cols, rows, order: (0..keys.len()).collect(), keys, sort: self.sort };
        self.apply_sort();
    }

    fn apply_sort(&mut self) {
        let Some((c, asc)) = self.sort else { return };
        let Some(col) = self.cols.get(c) else { return };
        let rows = &self.rows;
        self.order.sort_by(|a, b| {
            let (va, vb) = (rows[*a].get(col), rows[*b].get(col));
            let ord = match (va.and_then(Value::as_f64), vb.and_then(Value::as_f64)) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                _ => va.map(|v| v.format(4)).cmp(&vb.map(|v| v.format(4))),
            };
            if asc { ord } else { ord.reverse() }
        });
    }
}

/// Draws a measurement table; returns an object to select.
pub fn show(ui: &mut egui::Ui, app: &mut QuPathApp, detections: bool) -> Option<ObjKey> {
    let mut table = std::mem::take(if detections { &mut app.ui.detection_table } else { &mut app.ui.annotation_table });
    table.refresh(app, detections);
    let clicked = draw(ui, &mut table);
    *(if detections { &mut app.ui.detection_table } else { &mut app.ui.annotation_table }) = table;
    clicked
}

fn draw(ui: &mut egui::Ui, t: &mut Table) -> Option<ObjKey> {
    ui.horizontal(|ui| {
        ui.weak(format!("{} rows", t.rows.len()));
        if ui.button("Copy").on_hover_text("Copy as tab-separated text").clicked() {
            let rows: Vec<_> = t.order.iter().map(|i| t.rows[*i].clone()).collect();
            ui.ctx().copy_text(measurements::to_tsv(&t.cols, &rows, '\t'));
        }
    });
    let mut clicked = None;
    let mut resort = None;
    let row_h = 20.0;
    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .columns(Column::auto().at_least(60.0).clip(true), t.cols.len())
        .header(row_h, |mut header| {
            for (i, c) in t.cols.iter().enumerate() {
                header.col(|ui| {
                    let arrow = match t.sort {
                        Some((s, true)) if s == i => " ⏶",
                        Some((s, false)) if s == i => " ⏷",
                        _ => "",
                    };
                    if ui.add(egui::Label::new(egui::RichText::new(format!("{c}{arrow}")).strong()).sense(egui::Sense::click())).clicked() {
                        resort = Some(i);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(row_h, t.order.len(), |mut row| {
                let r = t.order[row.index()];
                for c in &t.cols {
                    row.col(|ui| {
                        ui.label(t.rows[r].get(c).map_or_else(String::new, |v| v.format(3)));
                    });
                }
                if row.response().clicked() {
                    clicked = Some(t.keys[r]);
                }
            });
        });
    if let Some(i) = resort {
        t.sort = Some((i, t.sort.is_none_or(|(s, asc)| s != i || !asc)));
        t.apply_sort();
    }
    clicked
}

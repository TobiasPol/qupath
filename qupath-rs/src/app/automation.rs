//! Scripted runs for screenshots and UI regression checks.
//!
//! `QUPATH_RS_AUTOMATION="zoom:2; rect:11000,8000,2000,2000; cmd:PositiveCellDetection; accept; shot:/tmp/a.png; quit"`
//! Each step waits until no task runs and no tile is loading.

use std::path::PathBuf;

use super::QuPathApp;
use super::commands::Command;
use crate::objects::{PathObject, Pt, Roi};
use crate::ui::panes::Pane;

/// Steps still to run.
#[derive(Default)]
pub struct Automation {
    steps: Vec<String>,
    settle: u32,
    shot: Option<PathBuf>,
}

impl Automation {
    /// Reads `QUPATH_RS_AUTOMATION`.
    pub fn from_env() -> Self {
        let steps = std::env::var("QUPATH_RS_AUTOMATION")
            .map(|s| s.split(';').map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect())
            .unwrap_or_default();
        Automation { steps, settle: 0, shot: None }
    }
}

impl QuPathApp {
    /// Runs the next step once the app is idle.
    pub(super) fn automate(&mut self, ctx: &egui::Context) {
        if self.auto.steps.is_empty() && self.auto.shot.is_none() {
            return;
        }
        ctx.request_repaint();
        if self.save_automation_shot(ctx) {
            return;
        }
        let busy = self.s.tasks.busy() || self.viewer.tiles.busy() || (self.s.image.is_none() && self.s.tasks.busy());
        self.auto.settle = if busy { 0 } else { self.auto.settle + 1 };
        if self.auto.settle < 8 {
            return;
        }
        self.auto.settle = 0;
        let step = self.auto.steps.remove(0);
        log::info!("automation: {step}");
        if let Err(e) = self.automation_step(ctx, &step) {
            log::error!("automation step '{step}' failed: {e}");
        }
    }

    fn automation_step(&mut self, ctx: &egui::Context, step: &str) -> Result<(), String> {
        let (op, arg) = step.split_once(':').unwrap_or((step, ""));
        let nums = || -> Vec<f64> { arg.split(',').filter_map(|v| v.trim().parse().ok()).collect() };
        match op {
            "cmd" => {
                let cmd = Command::ALL.iter().copied().find(|c| format!("{c:?}") == arg).ok_or("unknown command")?;
                self.run(ctx, cmd);
            }
            "zoom" => self.viewer.t.downsample = nums().first().copied().ok_or("zoom needs a downsample")?,
            "center" => {
                let n = nums();
                self.viewer.t.center = Pt { x: n[0], y: n[1] };
            }
            "rect" => {
                let n = nums();
                let d = self.s.image.as_mut().ok_or("no image")?;
                let k = d
                    .hierarchy
                    .insert(PathObject::annotation(Roi::Rectangle { x: n[0], y: n[1], w: n[2], h: n[3] }, self.s.auto_class.clone()));
                d.hierarchy.select(Some(k));
            }
            "select-first-detection" => {
                let d = self.s.image.as_mut().ok_or("no image")?;
                let k = d.hierarchy.detections().map(|(k, _)| k).next();
                d.hierarchy.select(k);
            }
            "accept" => {
                if let Some(dialog) = self.s.dialog.take() {
                    self.accept_for_automation(ctx, dialog);
                }
            }
            "pane" => self.ui.pane = pane(arg)?,
            "entry" => {
                let id = arg.parse().map_err(|_| "entry needs an id")?;
                self.request_open(ctx, crate::ui::dialogs::PendingOpen::Entry(id));
            }
            "shot" => self.request_automation_shot(ctx, PathBuf::from(arg)),
            "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            "wait" => {}
            _ => return Err(format!("unknown step {op}")),
        }
        Ok(())
    }

    fn request_automation_shot(&mut self, ctx: &egui::Context, p: PathBuf) {
        self.auto.shot = Some(p);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
    }

    /// Saves the whole window when the screenshot arrives; true while waiting for it.
    fn save_automation_shot(&mut self, ctx: &egui::Context) -> bool {
        let Some(path) = self.auto.shot.clone() else { return false };
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(img) = img else { return true };
        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        if let Some(i) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)
            && let Err(e) = i.save(&path)
        {
            log::error!("cannot save {}: {e}", path.display());
        }
        log::info!("automation: saved {}", path.display());
        self.auto.shot = None;
        false
    }
}

fn pane(name: &str) -> Result<Pane, String> {
    Ok(match name {
        "project" => Pane::Project,
        "image" => Pane::Image,
        "annotations" => Pane::Annotations,
        "hierarchy" => Pane::Hierarchy,
        "workflow" => Pane::Workflow,
        _ => return Err("unknown pane".into()),
    })
}

//! Background work with progress, so the window never waits (QuPath's task runner).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crossbeam_channel::{Receiver, Sender, unbounded};

use super::data::ImageData;
use crate::objects::{ObjKey, PathObject};

/// What finished work hands back to the interface.
pub enum TaskResult {
    /// An image is ready to show.
    ImageOpened(Box<ImageData>),
    /// Objects to add under parents (detections, tiles, thresholded regions).
    Objects {
        /// The image they belong to.
        image_id: u64,
        /// Parent and its new children; existing detections under each parent are replaced when `replace`.
        groups: Vec<(ObjKey, Vec<PathObject>)>,
        /// Replace detections under each parent.
        replace: bool,
        /// Insert by containment instead of directly under the parent.
        insert: bool,
        /// Workflow entry.
        step: (String, String),
    },
    /// Thumbnails written for project entries.
    ProjectThumbnails(Vec<(u64, PathBuf)>),
    /// Something to tell the user.
    Message(String),
    /// Something went wrong.
    Error(String),
}

/// Lets work report progress and notice cancellation.
#[derive(Clone)]
pub struct Progress {
    value: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Progress {
    /// Sets progress in 0..=1.
    pub fn set(&self, f: f32) {
        self.value.store((f.clamp(0.0, 1.0) * 1000.0) as u32, Ordering::Relaxed);
        self.ctx.request_repaint();
    }
    /// Whether the user cancelled.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// A task shown in the status bar.
pub struct Running {
    /// Name.
    pub name: String,
    progress: Progress,
    done: Arc<AtomicBool>,
}

impl Running {
    /// Progress in 0..=1.
    pub fn fraction(&self) -> f32 {
        self.progress.value.load(Ordering::Relaxed) as f32 / 1000.0
    }
    /// Asks the task to stop.
    pub fn cancel(&self) {
        self.progress.cancel.store(true, Ordering::Relaxed);
    }
}

/// Runs and tracks background tasks.
pub struct Tasks {
    tx: Sender<TaskResult>,
    rx: Receiver<TaskResult>,
    /// Tasks still running.
    pub running: Vec<Running>,
    ctx: egui::Context,
}

impl Tasks {
    /// Ready to run tasks.
    pub fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = unbounded();
        Tasks { tx, rx, running: Vec::new(), ctx: ctx.clone() }
    }

    /// Runs `work` on a thread of its own.
    pub fn spawn(&mut self, name: &str, work: impl FnOnce(&Progress) -> TaskResult + Send + 'static) {
        let progress = Progress { value: Arc::default(), cancel: Arc::default(), ctx: self.ctx.clone() };
        let done = Arc::new(AtomicBool::new(false));
        let (tx, p, d, ctx) = (self.tx.clone(), progress.clone(), done.clone(), self.ctx.clone());
        let label = name.to_owned();
        let spawned = std::thread::Builder::new().name(format!("task: {name}")).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&p)))
                .unwrap_or_else(|_| TaskResult::Error(format!("{label} failed unexpectedly (see the log)")));
            let _ = tx.send(result);
            d.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.running.push(Running { name: name.into(), progress, done }),
            Err(e) => {
                let _ = self.tx.send(TaskResult::Error(format!("cannot start {name}: {e}")));
            }
        }
    }

    /// Finished results; forgets finished tasks.
    pub fn poll(&mut self) -> Vec<TaskResult> {
        self.running.retain(|r| !r.done.load(Ordering::Relaxed));
        self.rx.try_iter().collect()
    }

    /// Whether anything is running.
    pub fn busy(&self) -> bool {
        !self.running.is_empty()
    }
}

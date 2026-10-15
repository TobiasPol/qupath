//! The Points tool: click to add points to the selected points annotation
//! (or start a new one); Alt+click removes; drag a point to move it.

use super::handles::Handle;
use super::{Gesture, Pointer, ToolCtx};
use crate::objects::{ObjKey, Pt, Roi};

/// One frame of Points tool input.
pub fn handle(g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    let target = selected_points(ctx);
    if p.clicked {
        click(ctx, target, p);
        return;
    }
    if p.drag_started {
        *g = match target.and_then(|k| nearest(ctx, k, p.pos).map(|i| (k, i))) {
            Some((key, i)) => Gesture::Handle { key, handle: Handle::Vertex(i) },
            None => Gesture::Pan,
        };
    }
    if p.dragging {
        match g {
            Gesture::Handle { key, handle } => {
                ctx.checkpoint();
                let (key, handle) = (*key, *handle);
                if let Some(o) = ctx.h.get_mut(key) {
                    o.roi = super::handles::apply(&o.roi, handle, p.pos);
                }
            }
            Gesture::Pan => ctx.t.pan(p.delta),
            _ => {}
        }
    }
    if p.drag_stopped {
        *g = Gesture::None;
    }
}

fn click(ctx: &mut ToolCtx<'_>, target: Option<ObjKey>, p: &Pointer) {
    match (target, p.alt) {
        (Some(k), true) => {
            if let Some(i) = nearest(ctx, k, p.pos) {
                ctx.checkpoint();
                remove_point(ctx, k, i);
            }
        }
        (Some(k), false) => {
            ctx.checkpoint();
            if let Some(o) = ctx.h.get_mut(k)
                && let Roi::Points(pts) = &mut o.roi
            {
                pts.push(p.pos);
            }
        }
        (None, false) => ctx.create(Roi::Points(vec![p.pos])),
        (None, true) => {}
    }
}

fn remove_point(ctx: &mut ToolCtx<'_>, k: ObjKey, i: usize) {
    let empty = match ctx.h.get_mut(k).map(|o| &mut o.roi) {
        Some(Roi::Points(pts)) => {
            pts.remove(i);
            pts.is_empty()
        }
        _ => false,
    };
    if empty {
        ctx.h.remove(k, true);
    }
}

/// The selected, unlocked points annotation.
fn selected_points(ctx: &ToolCtx<'_>) -> Option<ObjKey> {
    ctx.h.primary_selected().filter(|k| ctx.h.get(*k).is_some_and(|o| o.is_annotation() && !o.locked && o.roi.is_points()))
}

/// Index of the point within 7 screen points of `p`.
fn nearest(ctx: &ToolCtx<'_>, k: ObjKey, p: Pt) -> Option<usize> {
    let Roi::Points(pts) = &ctx.h.get(k)?.roi else { return None };
    let s = ctx.t.to_screen(p);
    pts.iter()
        .enumerate()
        .map(|(i, q)| (i, ctx.t.to_screen(*q).distance(s)))
        .filter(|(_, d)| *d <= 7.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

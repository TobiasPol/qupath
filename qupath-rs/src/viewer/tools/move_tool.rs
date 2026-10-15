//! The Move tool: pan, select, move selected annotations, drag handles.

use super::handles::{self, Handle};
use super::{Gesture, Pointer, ToolCtx};
use crate::objects::{ObjKey, ops};

/// One frame of Move tool input.
pub fn handle(g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    if p.clicked {
        ctx.click_select(p.pos, p.command);
        return;
    }
    if p.drag_started {
        *g = start(ctx, p);
    }
    if p.dragging {
        drag(g, ctx, p);
    }
    if p.drag_stopped {
        finish(g, ctx);
        *g = Gesture::None;
    }
}

/// Chooses what a drag does: a handle, moving the selection, or panning.
fn start(ctx: &mut ToolCtx<'_>, p: &Pointer) -> Gesture {
    let screen = ctx.t.to_screen(p.pos);
    if let Some(k) = ctx.h.primary_selected()
        && let Some(o) = ctx.h.get(k).filter(|o| o.is_annotation() && !o.locked)
    {
        if let Some(handle) = handles::hit_handle(&o.roi, ctx.t, screen) {
            return Gesture::Handle { key: k, handle };
        }
        let tol = 4.0 * ctx.t.downsample;
        let on_selected = ctx.index.hit(ctx.h, p.pos, tol, false) == Some(k) || o.roi.contains(p.pos);
        if on_selected {
            let keys: Vec<ObjKey> =
                ctx.h.selected().into_iter().filter(|s| ctx.h.get(*s).is_some_and(|o| o.is_annotation() && !o.locked)).collect();
            return Gesture::MoveObjects { keys, last: p.pos };
        }
    }
    Gesture::Pan
}

fn drag(g: &mut Gesture, ctx: &mut ToolCtx<'_>, p: &Pointer) {
    match g {
        Gesture::Pan => ctx.t.pan(p.delta),
        Gesture::MoveObjects { keys, last } => {
            let (dx, dy) = (p.pos.x - last.x, p.pos.y - last.y);
            if dx == 0.0 && dy == 0.0 {
                return;
            }
            ctx.checkpoint();
            for k in keys.iter() {
                if let Some(o) = ctx.h.get_mut(*k) {
                    o.roi = o.roi.translate(dx, dy);
                }
            }
            *last = p.pos;
        }
        Gesture::Handle { key, handle } => {
            ctx.checkpoint();
            let (key, handle) = (*key, *handle);
            if let Some(o) = ctx.h.get_mut(key) {
                o.roi = handles::apply(&o.roi, handle, p.pos);
            }
        }
        _ => {}
    }
}

/// After moving or reshaping, re-insert so parents reflect the new position.
fn finish(g: &Gesture, ctx: &mut ToolCtx<'_>) {
    let keys: Vec<ObjKey> = match g {
        Gesture::MoveObjects { keys, .. } => keys.clone(),
        Gesture::Handle { key, handle } => {
            normalize_box(ctx, *key, *handle);
            vec![*key]
        }
        _ => return,
    };
    let mut new_keys = Vec::new();
    for k in keys {
        if let Some(roi) = ctx.h.get(k).map(|o| o.roi.clone())
            && let Some(nk) = ops::replace_roi(ctx.h, k, roi)
        {
            new_keys.push(nk);
        }
    }
    ctx.h.clear_selection();
    ctx.h.add_to_selection(new_keys);
}

/// Drop zero-size boxes after a handle drag.
fn normalize_box(ctx: &mut ToolCtx<'_>, key: ObjKey, handle: Handle) {
    if let Handle::Box(_) = handle
        && let Some(o) = ctx.h.get(key)
        && o.roi.bounds().width() * o.roi.bounds().height() == 0.0
    {
        ctx.h.remove(key, true);
    }
}

//! Annotation geometry operations from QuPath's Objects › Annotations menu.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{Effect, Output, Tool, ids_property, schema};
use crate::agent::args;
use crate::agent::session::Session;
use crate::objects::{ObjKey, ops};

pub(super) fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "annotation_operation",
        title: "Annotation operation",
        description: "Change annotation shapes: merge (union of ids into one), subtract (the other ids from the first), inverse (image \
                      minus the first id), expand (grow by distance µm, negative shrinks; remove_interior makes a ring), simplify \
                      (altitude px), split (one annotation per separate part).",
        schema: || {
            let op = json!({ "type": "string", "enum": ["merge", "subtract", "inverse", "expand", "simplify", "split"] });
            let props = json!({
                "operation": op, "ids": ids_property("Annotations; for subtract and inverse the first is the main one"),
                "distance": { "type": "number", "description": "expand: µm when calibrated, else px" },
                "remove_interior": { "type": "boolean" }, "altitude": { "type": "number", "description": "simplify: px, default 1" },
            });
            schema(props, &["operation", "ids"])
        },
        effect: Effect::Edit,
        run: operate,
    }]
}

fn operate(s: &mut Session, a: &Value) -> Result<Output> {
    let op = args::text(a, "operation")?.to_owned();
    let img = s.image_mut()?;
    let keys = img.keys(&args::opt_strings(a, "ids").context("missing `ids`")?)?;
    if keys.iter().any(|k| !img.hierarchy.get(*k).is_some_and(|o| o.is_annotation())) {
        bail!("annotation_operation works on annotations only");
    }
    let md = img.server.metadata();
    let (w, h) = (f64::from(md.width()), f64::from(md.height()));
    let cal = img.calibration();
    let hier = &mut img.hierarchy;
    // ops act on the selection; the main selection is the last one selected.
    hier.clear_selection();
    let (first, rest) = keys.split_first().context("give at least one id")?;
    hier.add_to_selection(rest.iter().copied());
    hier.add_to_selection([*first]);
    let made: Vec<ObjKey> = match op.as_str() {
        "merge" => ops::merge_selected(hier).into_iter().collect(),
        "subtract" => ops::subtract_from_primary(hier).into_iter().collect(),
        "inverse" => ops::make_inverse(hier, w, h).into_iter().collect(),
        "expand" => ops::expand_selected(hier, args::num(a, "distance")? / cal.average(), args::bool_or(a, "remove_interior", false)),
        "simplify" => {
            ops::simplify_selected(hier, args::f64_or(a, "altitude", 1.0));
            keys.clone()
        }
        "split" => ops::split_selected(hier),
        other => bail!("unknown operation {other}"),
    };
    hier.clear_selection();
    let ids: Vec<String> = made.iter().filter_map(|k| hier.get(*k)).map(|o| o.id.to_string()).collect();
    img.record(&format!("Annotation {op}"), format!("// {op} annotations"));
    Ok(json!({ "operation": op, "result_ids": ids, "objects": img.hierarchy.len() }).into())
}

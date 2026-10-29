//! Results: measurement tables, their export, and the workflow as a QuPath script.

use anyhow::Result;
use indexmap::IndexMap;
use serde_json::{Value, json};

use super::objects::filtered;
use super::{Effect, Output, Tool, ids_property, schema};
use crate::agent::args;
use crate::agent::session::Session;
use crate::objects::measurements::{self, Value as Cell};

pub(super) fn tools() -> Vec<Tool> {
    vec![MEASUREMENTS, EXPORT_MEASUREMENTS, EXPORT_SCRIPT]
}

const MEASUREMENTS: Tool = Tool {
    name: "measurements",
    title: "Measurements",
    description: "QuPath's measurement table for annotations (area, Num Detections, Num Positive, Positive %, …) or detections/cells \
                  (Nucleus: Area µm^2, DAB OD mean, …). Filter by ids or class, pick columns, page with offset/limit. For large \
                  tables prefer summary=true (column means) or export_measurements.",
    schema: || {
        let kind =
            json!({ "type": "string", "enum": ["annotation", "detection", "cell", "tile", "all"], "description": "Default annotation" });
        let cols =
            json!({ "type": "array", "items": { "type": "string" }, "description": "Only these columns (names as QuPath writes them)" });
        let props = json!({
            "kind": kind, "class": { "type": "string" }, "ids": ids_property("Only these objects"), "columns": cols,
            "offset": { "type": "integer" }, "limit": { "type": "integer", "description": "Default 50, max 500" },
            "summary": { "type": "boolean", "description": "Return count, mean, min and max of each numeric column instead of rows" },
        });
        schema(props, &[])
    },
    effect: Effect::Read,
    run: table,
};

const EXPORT_MEASUREMENTS: Tool = Tool {
    name: "export_measurements",
    title: "Export measurements",
    description: "Write a measurement table (QuPath's column names) to a TSV or CSV file.",
    schema: || {
        let kind = json!({ "type": "string", "enum": ["annotation", "detection", "cell", "tile", "all"] });
        let sep = json!({ "type": "string", "enum": ["tab", "comma"], "description": "Default tab (comma when the path ends in .csv)" });
        schema(json!({ "path": { "type": "string" }, "kind": kind, "class": { "type": "string" }, "separator": sep }), &["path"])
    },
    effect: Effect::Files,
    run: export,
};

const EXPORT_SCRIPT: Tool = Tool {
    name: "export_script",
    title: "Export QuPath script",
    description: "The commands run on this image as a QuPath Groovy script (QuPath's Workflow), so the analysis can be rerun in \
                  QuPath on other images. Returns the script; writes it too when path is given.",
    schema: || schema(json!({ "path": { "type": "string", "description": "Optional .groovy file" } }), &[]),
    effect: Effect::Files,
    run: script,
};

/// Column names and one row per object.
type Table = (Vec<String>, Vec<IndexMap<String, Cell>>);

fn rows(s: &Session, a: &Value) -> Result<Table> {
    let img = s.image()?;
    let keys = match args::opt_strings(a, "ids") {
        Some(ids) => img.keys(&ids)?,
        None => filtered(img, Some(args::opt_str(a, "kind").unwrap_or("annotation")), args::opt_str(a, "class")),
    };
    let (mut cols, mut rows) = measurements::table(&img.hierarchy, &keys, &img.calibration());
    if let Some(only) = args::opt_strings(a, "columns") {
        cols.retain(|c| only.contains(c) || c == "Object ID");
        for r in &mut rows {
            r.retain(|k, _| cols.contains(k));
        }
    }
    Ok((cols, rows))
}

fn cell_json(v: &Cell) -> Value {
    match v {
        Cell::Text(s) => json!(s),
        Cell::Number(n) if n.is_finite() => json!(args::round(*n, 4)),
        Cell::Number(_) => Value::Null,
    }
}

fn table(s: &mut Session, a: &Value) -> Result<Output> {
    let (cols, rows) = rows(s, a)?;
    if args::bool_or(a, "summary", false) {
        return Ok(json!({ "rows": rows.len(), "columns": summarize(&cols, &rows) }).into());
    }
    let (offset, limit) = (args::usize_or(a, "offset", 0), args::usize_or(a, "limit", 50).min(500));
    let page: Vec<Value> =
        rows.iter().skip(offset).take(limit).map(|r| Value::Object(r.iter().map(|(k, v)| (k.clone(), cell_json(v))).collect())).collect();
    Ok(json!({ "total": rows.len(), "offset": offset, "returned": page.len(), "columns": cols, "rows": page }).into())
}

fn summarize(cols: &[String], rows: &[IndexMap<String, Cell>]) -> Value {
    let mut out = serde_json::Map::new();
    for c in cols {
        let vals: Vec<f64> = rows.iter().filter_map(|r| r.get(c).and_then(Cell::as_f64)).filter(|v| v.is_finite()).collect();
        if vals.is_empty() {
            continue;
        }
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        let (lo, hi) = vals.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| (l.min(*v), h.max(*v)));
        let r = |v: f64| args::round(v, 4);
        out.insert(c.clone(), json!({ "n": vals.len(), "mean": r(mean), "min": r(lo), "max": r(hi) }));
    }
    Value::Object(out)
}

fn export(s: &mut Session, a: &Value) -> Result<Output> {
    let path = args::text(a, "path")?;
    let comma = match args::opt_str(a, "separator") {
        Some(sep) => sep == "comma",
        None => path.to_ascii_lowercase().ends_with(".csv"),
    };
    let (cols, rows) = rows(s, a)?;
    let text = measurements::to_tsv(&cols, &rows, if comma { ',' } else { '\t' });
    fastframe_file::write(std::path::Path::new(path), text.into_bytes())?;
    Ok(json!({ "path": path, "rows": rows.len(), "columns": cols.len() }).into())
}

fn script(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image()?;
    let mut text = format!("// QuPath script recorded by qupath-rs for {}\n", img.server.metadata().name);
    for step in &img.settings.workflow {
        text.push_str(&format!("\n// {}\n{}\n", step.name, step.script));
    }
    if let Some(path) = args::opt_str(a, "path") {
        fastframe_file::write(std::path::Path::new(path), text.clone().into_bytes())?;
    }
    Ok(json!({ "steps": img.settings.workflow.len(), "script": text }).into())
}

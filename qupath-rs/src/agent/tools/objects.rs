//! Objects: add annotations, list, delete, classify and rename, GeoJSON import/export.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{Effect, Output, Tool, ids_property, output_path, overwrite_property, path_property, schema};
use crate::agent::args::{self, bounds_json, round};
use crate::agent::session::{OpenImage, Session};
use crate::io::script::groovy_str;
use crate::objects::{ObjKey, PathClass, PathObject, Roi};

pub(super) fn tools() -> Vec<Tool> {
    let mut v = vec![ADD_ANNOTATION, LIST_OBJECTS, DELETE_OBJECTS, SET_OBJECT_PROPERTIES, IMPORT_GEOJSON, EXPORT_GEOJSON];
    v.extend(super::shapes::tools());
    v
}

const ADD_ANNOTATION: Tool = Tool {
    name: "add_annotation",
    title: "Add annotation",
    description: "Add an annotation (a region to analyze or label). Shapes: {type:'rectangle'|'ellipse', x, y, width, height}, \
                  {type:'polygon'|'polyline'|'points', points:[[x,y],…]}, {type:'line', x1, y1, x2, y2}, {type:'full_image'}, or a \
                  GeoJSON geometry. Coordinates are full-resolution pixels unless units='um'. It is placed in the hierarchy by containment.",
    schema: || {
        let shape = json!({ "type": "object", "description": "Shape (see description)" });
        let units = json!({ "type": "string", "enum": ["px", "um"], "description": "Units of the shape (default px)" });
        let class = json!({ "type": "string", "description": "Class, e.g. Tumor, Stroma, Region*" });
        schema(json!({ "shape": shape, "units": units, "class": class, "name": { "type": "string" } }), &["shape"])
    },
    effect: Effect::Edit,
    run: add_annotation,
};

const LIST_OBJECTS: Tool = Tool {
    name: "list_objects",
    title: "List objects",
    description: "List objects with id, kind, class, name, parent id, bounding box [x, y, w, h] in px, area and child count. \
                  Filter by kind and class; page with offset/limit. Use measurements for full per-object measurements.",
    schema: || {
        let kind = json!({ "type": "string", "enum": ["annotation", "detection", "cell", "tile", "all"], "description": "Default all" });
        let parent = json!({ "type": "string", "description": "Only children of this object id" });
        let n = json!({ "type": "integer" });
        schema(json!({ "kind": kind, "class": { "type": "string" }, "parent_id": parent, "offset": n, "limit": n }), &[])
    },
    effect: Effect::Read,
    run: list,
};

const DELETE_OBJECTS: Tool = Tool {
    name: "delete_objects",
    title: "Delete objects",
    description: "Delete objects by id, or every object of a kind and/or class. Children of deleted annotations are kept unless delete_children.",
    schema: || {
        let kind = json!({ "type": "string", "enum": ["annotation", "detection", "cell", "tile", "all"] });
        let props = json!({ "ids": ids_property("Object ids"), "kind": kind, "class": { "type": "string" }, "delete_children": { "type": "boolean" } });
        schema(props, &[])
    },
    effect: Effect::Delete,
    run: delete,
};

const SET_OBJECT_PROPERTIES: Tool = Tool {
    name: "set_object_properties",
    title: "Set object class/name/colour",
    description: "Set the class (empty string to unclassify), name, colour (#RRGGBB) or lock state of objects by id.",
    schema: || {
        let props = json!({
            "ids": ids_property("Object ids"), "class": { "type": "string" }, "name": { "type": "string" },
            "color": { "type": "string", "description": "#RRGGBB" }, "locked": { "type": "boolean" },
        });
        schema(props, &["ids"])
    },
    effect: Effect::Edit,
    run: set_properties,
};

const IMPORT_GEOJSON: Tool = Tool {
    name: "import_geojson",
    title: "Import GeoJSON",
    description: "Import objects from a QuPath-flavoured GeoJSON file (from QuPath, qupath-rs or other tools).",
    schema: || schema(json!({ "path": { "type": "string" } }), &["path"]),
    effect: Effect::Edit,
    run: import,
};

const EXPORT_GEOJSON: Tool = Tool {
    name: "export_geojson",
    title: "Export GeoJSON",
    description: "Write objects as QuPath-flavoured GeoJSON (QuPath's File › Import objects reads it). Default: every object, nested.",
    schema: || {
        let kind = json!({ "type": "string", "enum": ["annotation", "detection", "cell", "tile", "all"] });
        let props = json!({
            "path": path_property(".geojson file"), "kind": kind, "ids": ids_property("Only these objects"),
            "overwrite": overwrite_property(),
        });
        schema(props, &["path"])
    },
    effect: Effect::Export,
    run: export,
};

/// Whether an object matches a kind filter (`annotation`, `detection` = any detection, `cell`, `tile`, `all`).
pub(super) fn kind_matches(o: &PathObject, kind: Option<&str>) -> bool {
    match kind.unwrap_or("all") {
        "annotation" => o.is_annotation(),
        "detection" => o.is_detection(),
        "all" => true,
        k => o.kind.geojson_name() == k,
    }
}

/// Keys of objects matching kind and class filters.
pub(super) fn filtered(img: &OpenImage, kind: Option<&str>, class: Option<&str>) -> Vec<ObjKey> {
    img.hierarchy
        .iter()
        .filter(|(_, o)| kind_matches(o, kind))
        .filter(|(_, o)| class.is_none_or(|c| crate::objects::classes::display_name(o.class.as_ref()) == c))
        .map(|(k, _)| k)
        .collect()
}

fn add_annotation(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image_mut()?;
    let md = img.server.metadata();
    let shape = a.get("shape").context("missing `shape`")?;
    let roi = if args::opt_str(shape, "type") == Some("full_image") {
        Roi::Rectangle { x: 0.0, y: 0.0, w: f64::from(md.width()), h: f64::from(md.height()) }
    } else {
        let cal = img.calibration();
        let um = args::opt_str(a, "units") == Some("um");
        if um && !cal.calibrated {
            bail!("the image has no pixel size, so µm cannot be used: set_pixel_size first or use px");
        }
        let scale = if um { (1.0 / cal.pixel_width, 1.0 / cal.pixel_height) } else { (1.0, 1.0) };
        args::shape(shape, scale)?
    };
    let class = args::opt_str(a, "class").filter(|c| !c.is_empty()).map(PathClass::new);
    let mut o = PathObject::annotation(roi, class);
    o.name = args::opt_str(a, "name").map(str::to_owned);
    let id = o.id.to_string();
    let script = format!("// add {} annotation {}", o.roi.type_name(), groovy_str(o.name.as_deref().unwrap_or("")));
    let k = img.hierarchy.insert(o);
    img.record("Add annotation", script);
    Ok(json!({ "added": describe(img, k) , "id": id }).into())
}

/// A compact description of one object.
pub(super) fn describe(img: &OpenImage, k: ObjKey) -> Value {
    let h = &img.hierarchy;
    let Some(o) = h.get(k) else { return Value::Null };
    let cal = img.calibration();
    let parent = o.parent().filter(|p| *p != h.root()).and_then(|p| h.get(p)).map(|p| p.id.to_string());
    let mut v = json!({
        "id": o.id.to_string(),
        "kind": o.kind.geojson_name(),
        "class": o.class.as_ref().map(PathClass::name),
        "name": o.name,
        "parent_id": parent,
        "roi": o.roi.type_name(),
        "bounds": bounds_json(&o.roi),
        "children": o.children().len(),
    });
    if o.roi.is_area() {
        v[format!("area_{}2", cal.unit())] = json!(round(o.roi.area(cal.pixel_width, cal.pixel_height), 2));
    }
    v
}

fn list(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image()?;
    let mut keys = filtered(img, args::opt_str(a, "kind"), args::opt_str(a, "class"));
    if let Some(p) = args::opt_str(a, "parent_id") {
        let parent = img.key(p)?;
        keys.retain(|k| img.hierarchy.get(*k).and_then(PathObject::parent) == Some(parent));
    }
    let (offset, limit) = (args::usize_or(a, "offset", 0), args::usize_or(a, "limit", 100).min(1000));
    let items: Vec<Value> = keys.iter().skip(offset).take(limit).map(|k| describe(img, *k)).collect();
    Ok(json!({ "total": keys.len(), "offset": offset, "returned": items.len(), "objects": items }).into())
}

fn delete(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image_mut()?;
    let keys = match args::opt_strings(a, "ids") {
        Some(ids) => img.keys(&ids)?,
        None if a.get("kind").is_some() || a.get("class").is_some() => filtered(img, args::opt_str(a, "kind"), args::opt_str(a, "class")),
        None => bail!("give `ids`, or `kind`/`class` (kind 'all' deletes everything)"),
    };
    let before = img.hierarchy.len();
    if args::bool_or(a, "delete_children", false) {
        img.hierarchy.remove_many(&keys, false);
    } else {
        // remove_where takes the fast path when only detections go.
        let ids: std::collections::HashSet<uuid::Uuid> = keys.iter().filter_map(|k| img.hierarchy.get(*k)).map(|o| o.id).collect();
        img.hierarchy.remove_where(|o| ids.contains(&o.id));
    }
    let removed = before - img.hierarchy.len();
    img.record("Delete objects", format!("// deleted {removed} objects"));
    Ok(json!({ "removed": removed, "remaining": img.hierarchy.len() }).into())
}

fn set_properties(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image_mut()?;
    let keys = img.keys(&args::opt_strings(a, "ids").context("missing `ids`")?)?;
    let class = args::opt_str(a, "class").map(|c| (!c.trim().is_empty()).then(|| PathClass::new(c)));
    let color = a.get("color").map(args::color).transpose()?;
    for k in &keys {
        let Some(o) = img.hierarchy.get_mut(*k) else { continue };
        if let Some(c) = &class {
            o.class = c.clone();
        }
        if let Some(n) = args::opt_str(a, "name") {
            o.name = (!n.is_empty()).then(|| n.to_owned());
        }
        if color.is_some() {
            o.color = color;
        }
        if let Some(l) = a.get("locked").and_then(Value::as_bool) {
            o.locked = l;
        }
    }
    if let Some(c) = &class {
        let name = c.as_ref().map_or("null".into(), |c| groovy_str(c.name()));
        img.record("Set classification", format!("getSelectedObjects().each {{ it.setPathClass(getPathClass({name})) }}"));
    }
    Ok(json!({ "updated": keys.len() }).into())
}

fn import(s: &mut Session, a: &Value) -> Result<Output> {
    let path = args::text(a, "path")?;
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
    let v: Value = serde_json::from_str(&text).context("not valid JSON")?;
    let img = s.image_mut()?;
    let n = crate::io::geojson::import(&mut img.hierarchy, &v)?;
    img.record("Import objects", format!("importObjectsFromFile({})", groovy_str(path)));
    Ok(json!({ "imported": n, "total_objects": img.hierarchy.len() }).into())
}

fn export(s: &mut Session, a: &Value) -> Result<Output> {
    let img = s.image()?;
    let h = &img.hierarchy;
    let (keys, nested) = match (args::opt_strings(a, "ids"), args::opt_str(a, "kind")) {
        (Some(ids), _) => (img.keys(&ids)?, false),
        (None, Some(k)) if k != "all" => (filtered(img, Some(k), None), false),
        _ => (h.children(h.root()).to_vec(), true),
    };
    let v = crate::io::geojson::export(h, &keys, nested);
    let path = output_path(a)?;
    fastframe_file::write(&path, serde_json::to_vec(&v)?)?;
    Ok(json!({ "path": path, "features": keys.len(), "nested_children": nested }).into())
}

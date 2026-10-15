//! GeoJSON in QuPath's flavor, so objects move between this app and QuPath.
//!
//! Matches `qupath.lib.io.QuPathTypeAdapters`: each object is a `Feature` with
//! `properties.objectType`, `classification` (`name` or `names`, and `color`),
//! `isLocked`, `measurements`, and optionally `childObjects`. Cells add
//! `nucleusGeometry`.

use anyhow::{Context, Result, bail};
use geo::{Coord, LineString, MultiPolygon, Polygon};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::objects::{Hierarchy, ObjKey, ObjectKind, PathClass, PathObject, Pt, Rgb, Roi};

/// Writes objects (optionally with their descendants nested) as a FeatureCollection.
pub fn export(h: &Hierarchy, keys: &[ObjKey], include_children: bool) -> Value {
    let features: Vec<Value> = keys.iter().filter_map(|k| feature(h, *k, include_children)).collect();
    json!({ "type": "FeatureCollection", "features": features })
}

/// One object as a Feature.
pub fn feature(h: &Hierarchy, k: ObjKey, include_children: bool) -> Option<Value> {
    let o = h.get(k)?;
    let mut props = Map::new();
    props.insert("objectType".into(), json!(o.kind.geojson_name()));
    if let Some(n) = &o.name {
        props.insert("name".into(), json!(n));
    }
    if let Some(c) = o.color {
        props.insert("color".into(), json!([c.0, c.1, c.2]));
    }
    if let Some(c) = &o.class {
        props.insert("classification".into(), class_to_json(c));
    }
    if o.locked {
        props.insert("isLocked".into(), json!(true));
    }
    if !o.measurements.is_empty() {
        let m: Map<String, Value> = o.measurements.iter().map(|(k, v)| (k.clone(), number(*v))).collect();
        props.insert("measurements".into(), Value::Object(m));
    }
    if include_children && !o.children().is_empty() {
        let kids: Vec<Value> = o.children().iter().filter_map(|c| feature(h, *c, true)).collect();
        props.insert("childObjects".into(), Value::Array(kids));
    }
    let mut f = Map::new();
    f.insert("type".into(), json!("Feature"));
    f.insert("id".into(), json!(o.id.to_string()));
    f.insert("geometry".into(), roi_to_geometry(&o.roi));
    if let Some(n) = &o.nucleus {
        f.insert("nucleusGeometry".into(), roi_to_geometry(n));
    }
    f.insert("properties".into(), Value::Object(props));
    Some(Value::Object(f))
}

fn number(v: f64) -> Value {
    serde_json::Number::from_f64(v).map_or(Value::String("NaN".into()), Value::Number)
}

/// `{"name": "Tumor", "color": [200, 0, 0]}` or `{"names": [...], "color": [...]}`.
pub fn class_to_json(c: &PathClass) -> Value {
    let col = c.color();
    let names = c.names();
    if names.len() == 1 {
        json!({ "name": names[0], "color": [col.0, col.1, col.2] })
    } else {
        json!({ "names": names, "color": [col.0, col.1, col.2] })
    }
}

/// Reads a classification object (or a bare string).
pub fn class_from_json(v: &Value) -> Option<PathClass> {
    let class = match v {
        Value::String(s) => PathClass::new(s),
        Value::Object(o) => {
            let c = if let Some(n) = o.get("name").and_then(Value::as_str) {
                PathClass::new(n)
            } else {
                let names: Vec<&str> = o.get("names")?.as_array()?.iter().filter_map(Value::as_str).collect();
                PathClass::from_names(&names)
            };
            if let Some(col) = o.get("color").and_then(color_from_json) {
                c.set_color(col);
            }
            c
        }
        _ => return None,
    };
    Some(class)
}

fn color_from_json(v: &Value) -> Option<Rgb> {
    match v {
        Value::Array(a) if a.len() >= 3 => {
            let c = |i: usize| a[i].as_u64().map(|x| x.min(255) as u8);
            Some(Rgb(c(0)?, c(1)?, c(2)?))
        }
        Value::Number(n) => n.as_i64().map(|x| Rgb::from_packed(x as i32)),
        _ => None,
    }
}

fn coords(pts: &[Pt]) -> Value {
    Value::Array(pts.iter().map(|p| json!([p.x, p.y])).collect())
}

fn ring(pts: &[Pt]) -> Value {
    coords(&crate::objects::roi::close(pts.to_vec()))
}

fn polygon_json(p: &Polygon<f64>) -> Value {
    let mut rings = vec![ring(&p.exterior().0)];
    rings.extend(p.interiors().iter().map(|r| ring(&r.0)));
    Value::Array(rings)
}

/// A ROI as a GeoJSON geometry, as QuPath writes it.
pub fn roi_to_geometry(roi: &Roi) -> Value {
    match roi {
        Roi::Line { .. } | Roi::Polyline(_) => json!({ "type": "LineString", "coordinates": coords(&roi.outline()) }),
        Roi::Points(p) if p.len() == 1 => json!({ "type": "Point", "coordinates": [p[0].x, p[0].y] }),
        Roi::Points(p) => json!({ "type": "MultiPoint", "coordinates": coords(p) }),
        Roi::Area(mp) if mp.0.len() == 1 => json!({ "type": "Polygon", "coordinates": polygon_json(&mp.0[0]) }),
        Roi::Area(mp) => {
            let polys: Vec<Value> = mp.0.iter().map(polygon_json).collect();
            json!({ "type": "MultiPolygon", "coordinates": polys })
        }
        r => json!({ "type": "Polygon", "coordinates": [ring(&r.outline())] }),
    }
}

fn parse_pt(v: &Value) -> Option<Pt> {
    let a = v.as_array()?;
    Some(Coord { x: a.first()?.as_f64()?, y: a.get(1)?.as_f64()? })
}

fn parse_pts(v: &Value) -> Option<Vec<Pt>> {
    v.as_array()?.iter().map(parse_pt).collect()
}

fn parse_polygon(v: &Value) -> Option<Polygon<f64>> {
    let rings: Vec<Vec<Pt>> = v.as_array()?.iter().map(parse_pts).collect::<Option<_>>()?;
    let mut it = rings.into_iter();
    let ext = LineString::new(it.next()?);
    Some(Polygon::new(ext, it.map(LineString::new).collect()))
}

/// A GeoJSON geometry as a ROI.
pub fn geometry_to_roi(g: &Value) -> Result<Roi> {
    let t = g.get("type").and_then(Value::as_str).context("geometry without type")?;
    let c = g.get("coordinates");
    let roi = match t {
        "Point" => Roi::Points(vec![parse_pt(c.context("no coordinates")?).context("bad point")?]),
        "MultiPoint" => Roi::Points(parse_pts(c.context("no coordinates")?).context("bad points")?),
        "LineString" => {
            let pts = parse_pts(c.context("no coordinates")?).context("bad line")?;
            match pts.as_slice() {
                [a, b] => Roi::Line { x1: a.x, y1: a.y, x2: b.x, y2: b.y },
                _ => Roi::Polyline(pts),
            }
        }
        "Polygon" => polys_to_roi(vec![parse_polygon(c.context("no coordinates")?).context("bad polygon")?])?,
        "MultiPolygon" => {
            let arr = c.and_then(Value::as_array).context("bad multipolygon")?;
            polys_to_roi(arr.iter().map(parse_polygon).collect::<Option<_>>().context("bad polygon")?)?
        }
        "GeometryCollection" => collection_to_roi(g)?,
        other => bail!("unsupported geometry type {other}"),
    };
    Ok(roi)
}

fn polys_to_roi(polys: Vec<Polygon<f64>>) -> Result<Roi> {
    Roi::from_multipolygon(MultiPolygon::new(polys)).context("empty polygon")
}

fn collection_to_roi(g: &Value) -> Result<Roi> {
    let parts = g.get("geometries").and_then(Value::as_array).context("no geometries")?;
    let rois: Vec<Roi> = parts.iter().map(geometry_to_roi).collect::<Result<_>>()?;
    let mp = crate::objects::ops::union_all(&rois).context("no area in geometry collection")?;
    polys_to_roi(mp.0)
}

/// Reads one Feature (and nested `childObjects`) into an object tree.
pub fn feature_to_object(f: &Value) -> Result<(PathObject, Vec<Value>)> {
    let geom = f.get("geometry").context("feature without geometry")?;
    let roi = geometry_to_roi(geom)?;
    let props = f.get("properties").cloned().unwrap_or(Value::Null);
    let kind_name = props.get("objectType").and_then(Value::as_str).unwrap_or("annotation");
    let kind = ObjectKind::from_geojson_name(kind_name).unwrap_or(ObjectKind::Annotation);
    let mut o = PathObject::new(kind, roi);
    if let Some(id) = f.get("id").and_then(Value::as_str).and_then(|s| Uuid::parse_str(s).ok()) {
        o.id = id;
    }
    if let Some(n) = f.get("nucleusGeometry") {
        o.nucleus = geometry_to_roi(n).ok();
    }
    o.name = props.get("name").and_then(Value::as_str).map(str::to_owned);
    o.color = props.get("color").and_then(color_from_json);
    o.class = props.get("classification").and_then(class_from_json);
    o.locked = props.get("isLocked").and_then(Value::as_bool).unwrap_or(false);
    read_measurements(props.get("measurements"), &mut o);
    let children = props.get("childObjects").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok((o, children))
}

fn read_measurements(m: Option<&Value>, o: &mut PathObject) {
    match m {
        Some(Value::Object(map)) => {
            for (k, v) in map {
                o.measurements.insert(k.clone(), v.as_f64().unwrap_or(f64::NAN));
            }
        }
        // Older QuPath versions wrote [{"name": .., "value": ..}].
        Some(Value::Array(arr)) => {
            for e in arr {
                if let (Some(n), Some(v)) = (e.get("name").and_then(Value::as_str), e.get("value").and_then(Value::as_f64)) {
                    o.measurements.insert(n.to_owned(), v);
                }
            }
        }
        _ => {}
    }
}

/// Lists the features in a FeatureCollection, a single Feature or an array.
pub fn features_of(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("FeatureCollection") => {
            o.get("features").and_then(Value::as_array).cloned().unwrap_or_default()
        }
        Value::Object(_) => vec![v.clone()],
        _ => vec![],
    }
}

/// Imports GeoJSON into a hierarchy. Nested children keep their parent;
/// top-level objects are inserted where they belong. Returns the number added.
pub fn import(h: &mut Hierarchy, v: &Value) -> Result<usize> {
    let mut n = 0;
    for f in features_of(v) {
        let (o, children) = feature_to_object(&f)?;
        if o.kind == ObjectKind::Root {
            n += import_children(h, h.root(), &children)?;
            continue;
        }
        let k = h.insert(o);
        n += 1 + import_children(h, k, &children)?;
    }
    Ok(n)
}

fn import_children(h: &mut Hierarchy, parent: ObjKey, children: &[Value]) -> Result<usize> {
    let mut n = 0;
    for c in children {
        let (o, grandchildren) = feature_to_object(c)?;
        let k = h.add_child(parent, o);
        n += 1 + import_children(h, k, &grandchildren)?;
    }
    Ok(n)
}

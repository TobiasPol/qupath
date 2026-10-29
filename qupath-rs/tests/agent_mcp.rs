//! The MCP server and agent tools, driven the way an AI coding agent drives them.

use std::path::Path;

use base64::Engine;
use qupath::agent::Session;
use qupath::mcp;
use qupath::synthetic::SyntheticSlide;
use serde_json::{Value, json};

/// A tissue-rich 1200² px patch of the synthetic H-DAB slide, saved as a PNG (no pixel size).
fn write_patch(dir: &Path) -> std::path::PathBuf {
    use qupath::image::tiff::writer::PixelSource;
    let tile = SyntheticSlide::new(24_000, 18_000).read(1.0, 11_000, 8_000, 1_200, 1_200).expect("render");
    let path = dir.join("patch.png");
    image::RgbImage::from_raw(1_200, 1_200, tile.to_rgb8()).expect("size").save(&path).expect("save");
    path
}

fn request(s: &mut Session, method: &str, params: Value) -> Value {
    mcp::handle(s, &json!({ "jsonrpc": "2.0", "id": 7, "method": method, "params": params })).expect("a reply")
}

/// Calls a tool; returns its JSON, the PNGs it returned, and whether it failed.
fn call(s: &mut Session, name: &str, args: Value) -> (Value, Vec<Vec<u8>>, bool) {
    let r = request(s, "tools/call", json!({ "name": name, "arguments": args }))["result"].clone();
    let content = r["content"].as_array().expect("content").clone();
    let text = content[0]["text"].as_str().expect("text").to_owned();
    let pngs = content[1..]
        .iter()
        .map(|c| base64::engine::general_purpose::STANDARD.decode(c["data"].as_str().expect("data")).expect("base64"))
        .collect();
    let error = r["isError"].as_bool().expect("isError");
    (serde_json::from_str(&text).unwrap_or(Value::String(text)), pngs, error)
}

fn ok(s: &mut Session, name: &str, args: Value) -> Value {
    let (v, _, error) = call(s, name, args);
    assert!(!error, "{name} failed: {v}");
    v
}

#[test]
fn protocol_handshakes_and_errors() {
    let mut s = Session::default();
    let init = request(&mut s, "initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["instructions"].as_str().is_some_and(|t| t.contains("detect_cells")));
    let old = request(&mut s, "initialize", json!({ "protocolVersion": "2023-01-01" }));
    assert_eq!(old["result"]["protocolVersion"], "2025-11-25");
    let meta = json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } });
    let discover = request(&mut s, "server/discover", meta);
    assert_eq!(discover["result"]["resultType"], "complete");
    assert_eq!(discover["result"]["supportedVersions"][0], "2026-07-28");
    let bad = request(&mut s, "tools/list", json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "1999-01-01" } }));
    assert_eq!(bad["error"]["code"], -32022);
    assert_eq!(request(&mut s, "no/such", json!({}))["error"]["code"], -32601);
    assert!(mcp::handle(&mut s, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
}

#[test]
fn tools_are_listed_with_object_schemas() {
    let tools = mcp::list_tools();
    let names: std::collections::HashSet<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names.len(), tools.len(), "duplicate tool names");
    for t in &tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        assert!(t["description"].as_str().is_some_and(|d| d.len() > 40), "{} needs a description", t["name"]);
    }
    for n in ["open_image", "view_image", "detect_cells", "measurements", "export_script"] {
        assert!(names.contains(n), "missing {n}");
    }
}

#[test]
fn stdio_transport_is_one_json_line_per_reply() {
    let input =
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\nnot json\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n";
    let mut out = Vec::new();
    mcp::serve(input.as_bytes(), &mut out).expect("serve");
    let lines: Vec<Value> = String::from_utf8(out).expect("utf8").lines().map(|l| serde_json::from_str(l).expect("json")).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[1]["error"]["code"], -32700);
}

#[test]
fn errors_tell_the_agent_what_to_do() {
    let mut s = Session::default();
    let (msg, _, error) = call(&mut s, "detect_cells", json!({}));
    assert!(error);
    assert!(msg.as_str().is_some_and(|m| m.contains("open_image")), "{msg}");
    let (_, _, error) = call(&mut s, "no_such_tool", json!({}));
    assert!(error);
}

#[test]
fn project_workflow_detects_cells_and_persists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let image = write_patch(dir.path());
    let mut s = Session::default();
    let project = dir.path().join("project");
    ok(&mut s, "create_project", json!({ "dir": project }));
    let added = ok(&mut s, "add_images", json!({ "paths": [image] }));
    let id = added["added"][0]["id"].as_u64().expect("entry id");
    ok(&mut s, "open_project_image", json!({ "id": id }));
    ok(&mut s, "set_image_type", json!({ "type": "BRIGHTFIELD_H_DAB" }));
    ok(&mut s, "set_pixel_size", json!({ "pixel_width_um": 0.25 }));
    let roi = ok(&mut s, "add_annotation", json!({ "shape": { "type": "full_image" }, "name": "All", "class": "Tumor" }));
    let found = ok(&mut s, "detect_cells", json!({ "parent_ids": [roi["id"]], "positive": true }));
    let cells = found["cells"].as_u64().expect("cells");
    assert!((350..900).contains(&cells), "{cells} cells");
    let pct = found["per_annotation"][0]["positive_percent"].as_f64().expect("percent");
    assert!((22.0..40.0).contains(&pct), "{pct}% positive");

    let (view, pngs, _) = call(&mut s, "view_image", json!({ "x": 500, "y": 500, "width": 300, "height": 300 }));
    assert_eq!(pngs.len(), 1);
    assert!(pngs[0].starts_with(b"\x89PNG"));
    assert!(view["objects_drawn"].as_u64().expect("drawn") > 10, "{view}");

    let table = ok(&mut s, "measurements", json!({ "kind": "annotation", "columns": ["Num Detections", "Positive %"] }));
    assert_eq!(table["rows"][0]["Num Detections"].as_f64(), Some(cells as f64));

    // Edits were saved: reopening the project brings the cells back.
    let mut fresh = Session::default();
    ok(&mut fresh, "open_project", json!({ "path": project }));
    let info = ok(&mut fresh, "open_project_image", json!({ "name": "patch.png" }));
    assert_eq!(info["objects"].as_u64(), Some(cells + 1));
    assert_eq!(info["image_type"], "BRIGHTFIELD_H_DAB");
    let script = ok(&mut fresh, "export_script", json!({}));
    assert!(script["script"].as_str().is_some_and(|t| t.contains("PositiveCellDetection")));
    let removed = ok(&mut fresh, "delete_objects", json!({ "kind": "cell" }));
    assert_eq!(removed["removed"].as_u64(), Some(cells));

    // Exports never replace a file unless asked to.
    let out = dir.path().join("objects.geojson");
    ok(&mut fresh, "export_geojson", json!({ "path": out }));
    let (msg, _, error) = call(&mut fresh, "export_geojson", json!({ "path": out }));
    assert!(error && msg.as_str().is_some_and(|m| m.contains("overwrite")), "{msg}");
    ok(&mut fresh, "export_geojson", json!({ "path": out, "overwrite": true }));
}

#[test]
fn file_writing_tools_are_marked_destructive() {
    let tools = mcp::list_tools();
    let hint = |name: &str| tools.iter().find(|t| t["name"] == name).expect(name)["annotations"]["destructiveHint"].clone();
    for name in ["export_geojson", "export_measurements", "export_script", "delete_objects"] {
        assert_eq!(hint(name), true, "{name}");
    }
    assert_eq!(hint("view_image"), false);
}

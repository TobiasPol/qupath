//! A Model Context Protocol server over stdio (`qupath-rs mcp`), so AI coding
//! agents (Claude Code, Codex, Gemini CLI, Cursor, …) can drive qupath-rs.
//!
//! Messages are newline-delimited JSON-RPC 2.0. Both protocol eras are served:
//! the stateless 2026-07-28 revision (`server/discover`, per-request `_meta`
//! version) and the `initialize` handshake of 2025-11-25 and earlier. stdout
//! carries only protocol messages; logs go to stderr.

use std::io::{BufRead, Write};

use anyhow::Result;
use base64::Engine;
use serde_json::{Map, Value, json};

use crate::agent::Session;
use crate::agent::tools::{self, Effect, Output};

/// Protocol versions served, newest first.
pub const VERSIONS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// The newest version that still uses the `initialize` handshake.
const HANDSHAKE_VERSION: &str = "2025-11-25";
const VERSION_META: &str = "io.modelcontextprotocol/protocolVersion";
const UNSUPPORTED_VERSION: i64 = -32022;

/// What the model reads first: how to work with this server.
pub const INSTRUCTIONS: &str = "qupath-rs: whole-slide image analysis (QuPath's core in Rust). Workflow: create_project or open_project, \
add_images, open_project_image (or open_image for a single file); check image_info and set_image_type (H&E or H-DAB for brightfield \
cell detection); view_image to look at the slide; add_annotation (or threshold_regions for tissue) to define regions; detect_cells, \
create_tiles; read results with measurements; export_geojson / export_measurements / export_script to hand results to QuPath. \
Coordinates are full-resolution pixels. Objects are referred to by id (UUID). Edits to project images are saved automatically.";

/// Serves requests from `input` until it closes, writing replies to `output`.
pub fn serve(input: impl BufRead, mut output: impl Write) -> Result<()> {
    let mut session = Session::default();
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| handle(&mut session, m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => handle(&mut session, &msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("parse error: {e}"), None)),
        };
        if let Some(r) = reply {
            serde_json::to_writer(&mut output, &r)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Answers one message; notifications and responses get no reply.
pub fn handle(session: &mut Session, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id")?.clone();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
    if let Some(v) = params.pointer("/_meta").and_then(|m| m.get(VERSION_META)).and_then(Value::as_str)
        && !VERSIONS.contains(&v)
    {
        let data = json!({ "supported": VERSIONS, "requested": v });
        return Some(error(id, UNSUPPORTED_VERSION, &format!("unsupported protocol version {v}"), Some(data)));
    }
    let result = match method {
        "initialize" => initialize(&params),
        "server/discover" => cacheable(discover()),
        "ping" => json!({}),
        "tools/list" => cacheable(json!({ "tools": list_tools() })),
        "tools/call" => call_tool(session, &params),
        "resources/list" => cacheable(json!({ "resources": [] })),
        "resources/templates/list" => cacheable(json!({ "resourceTemplates": [] })),
        "prompts/list" => cacheable(json!({ "prompts": [] })),
        _ => return Some(error(id, -32601, &format!("method not found: {method}"), None)),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": complete(result) }))
}

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({ "code": code, "message": message });
    if let Some(d) = data {
        e["data"] = d;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": e })
}

/// Results of the 2026 revision carry `resultType`; older clients ignore it.
fn complete(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.insert("resultType".into(), json!("complete"));
    }
    v
}

/// The tool list is fixed and holds no user data, so anyone may cache it.
fn cacheable(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.insert("ttlMs".into(), json!(3_600_000));
        o.insert("cacheScope".into(), json!("public"));
    }
    v
}

fn server_info() -> Value {
    json!({ "name": "qupath-rs", "title": "QuPath (qupath-rs)", "version": env!("CARGO_PKG_VERSION") })
}

fn capabilities() -> Value {
    json!({ "tools": { "listChanged": false } })
}

fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(HANDSHAKE_VERSION);
    let version = if VERSIONS[1..].contains(&requested) { requested } else { HANDSHAKE_VERSION };
    json!({ "protocolVersion": version, "capabilities": capabilities(), "serverInfo": server_info(), "instructions": INSTRUCTIONS })
}

fn discover() -> Value {
    json!({ "supportedVersions": VERSIONS, "capabilities": capabilities(), "serverInfo": server_info(), "instructions": INSTRUCTIONS })
}

/// Tool definitions as MCP lists them.
pub fn list_tools() -> Vec<Value> {
    tools::all()
        .into_iter()
        .map(|t| {
            let annotations = json!({
                "title": t.title,
                "readOnlyHint": t.effect == Effect::Read,
                "destructiveHint": t.effect == Effect::Delete,
                "idempotentHint": t.effect == Effect::Read,
                "openWorldHint": false,
            });
            json!({ "name": t.name, "title": t.title, "description": t.description, "inputSchema": (t.schema)(), "annotations": annotations })
        })
        .collect()
}

/// Runs a tool. Failures become `isError` results the model can read and fix.
fn call_tool(session: &mut Session, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let run = std::panic::AssertUnwindSafe(|| tools::call(session, name, &args));
    match std::panic::catch_unwind(run) {
        Ok(Ok(out)) => content(out),
        Ok(Err(e)) => tool_error(&format!("{e:#}")),
        Err(_) => tool_error(&format!("{name} failed unexpectedly (see the qupath-rs log)")),
    }
}

fn content(out: Output) -> Value {
    let mut blocks = vec![json!({ "type": "text", "text": out.json.to_string() })];
    for png in out.images {
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        blocks.push(json!({ "type": "image", "data": data, "mimeType": "image/png" }));
    }
    let mut result = Map::new();
    result.insert("content".into(), Value::Array(blocks));
    result.insert("isError".into(), json!(false));
    Value::Object(result)
}

fn tool_error(message: &str) -> Value {
    log::warn!("tool error: {message}");
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

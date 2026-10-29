//! The tools an agent can call: name, description, JSON Schema for the arguments,
//! and the function that runs them against a `Session`.

mod analysis;
mod image;
mod objects;
mod project;
mod results;
mod shapes;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::session::Session;

/// What a tool hands back: JSON for the agent, plus any rendered images (PNG).
#[derive(Default)]
pub struct Output {
    /// Structured result.
    pub json: Value,
    /// PNG images.
    pub images: Vec<Vec<u8>>,
}

impl From<Value> for Output {
    fn from(json: Value) -> Self {
        Output { json, images: Vec::new() }
    }
}

/// How a tool affects state, for MCP's tool annotations and autosave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Only reads.
    Read,
    /// Changes the image's objects or settings (saved to the project afterwards).
    Edit,
    /// Removes objects.
    Delete,
    /// Changes what is open, or writes inside the project.
    Files,
    /// Writes a file the caller names (replacing one only with `overwrite: true`).
    Export,
}

/// One tool.
pub struct Tool {
    /// Name, e.g. `detect_cells`.
    pub name: &'static str,
    /// Short human title.
    pub title: &'static str,
    /// What it does and when to use it (shown to the model).
    pub description: &'static str,
    /// JSON Schema of the arguments object.
    pub schema: fn() -> Value,
    /// How it affects state.
    pub effect: Effect,
    /// Runs it.
    pub run: fn(&mut Session, &Value) -> Result<Output>,
}

/// Every tool, in the order they are listed.
pub fn all() -> Vec<Tool> {
    let mut v = project::tools();
    v.extend(image::tools());
    v.extend(objects::tools());
    v.extend(analysis::tools());
    v.extend(results::tools());
    v
}

/// Runs a tool by name. Edits to a project image are saved straight away, so the
/// GUI (File › Reload data) and QuPath see them.
pub fn call(session: &mut Session, name: &str, args: &Value) -> Result<Output> {
    let tools = all();
    let Some(tool) = tools.iter().find(|t| t.name == name) else {
        bail!("unknown tool `{name}`");
    };
    let empty = json!({});
    let args = if args.is_null() { &empty } else { args };
    let mut out = (tool.run)(session, args)?;
    if matches!(tool.effect, Effect::Edit | Effect::Delete)
        && let Some(dir) = session.save()?
        && let Some(o) = out.json.as_object_mut()
    {
        o.insert("saved_to".into(), json!(dir));
    }
    Ok(out)
}

/// A JSON Schema object with these properties and required names.
fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// The output path of an export tool; an existing file is replaced only with `overwrite: true`.
fn output_path(a: &Value) -> Result<std::path::PathBuf> {
    let path = std::path::PathBuf::from(super::args::text(a, "path")?);
    if path.exists() && !super::args::bool_or(a, "overwrite", false) {
        bail!("{} already exists; pass overwrite: true to replace it, or choose another path", path.display());
    }
    Ok(path)
}

/// The `path` property of export tools.
fn path_property(path: &str) -> Value {
    json!({ "type": "string", "description": path })
}

/// The `overwrite` flag of export tools.
fn overwrite_property() -> Value {
    json!({ "type": "boolean", "description": "Replace the file if it exists (default false)" })
}

/// The `ids` property most object tools take.
fn ids_property(what: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": what })
}

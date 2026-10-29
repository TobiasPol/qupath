//! Projects: create, open, add images, list them, open one, save.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use super::{Effect, Output, Tool, schema};
use crate::agent::args;
use crate::agent::session::Session;
use crate::io::project::Project;

pub(super) fn tools() -> Vec<Tool> {
    vec![CREATE_PROJECT, OPEN_PROJECT, ADD_IMAGES, LIST_PROJECT_IMAGES, OPEN_PROJECT_IMAGE, SAVE]
}

const CREATE_PROJECT: Tool = Tool {
    name: "create_project",
    title: "Create project",
    description: "Create a QuPath project (project.qpproj) in an empty or new folder and make it current. Projects keep each \
                  image's objects and settings on disk; QuPath 0.7 and the qupath-rs GUI open them. Prefer working in a project so results persist.",
    schema: || schema(json!({ "dir": { "type": "string", "description": "Empty or new folder for the project" } }), &["dir"]),
    effect: Effect::Files,
    run: create,
};

const OPEN_PROJECT: Tool = Tool {
    name: "open_project",
    title: "Open project",
    description: "Open an existing QuPath project (project.qpproj or its folder) and list its images.",
    schema: || schema(json!({ "path": { "type": "string", "description": "project.qpproj or the folder holding it" } }), &["path"]),
    effect: Effect::Files,
    run: open,
};

const ADD_IMAGES: Tool = Tool {
    name: "add_images",
    title: "Add images to project",
    description: "Add image files (SVS, OME-TIFF, TIFF, NDPI, MRXS, PNG, JPEG…) to the current project. Returns their entry ids.",
    schema: || {
        let paths = json!({ "type": "array", "items": { "type": "string" }, "description": "Image files" });
        schema(json!({ "paths": paths }), &["paths"])
    },
    effect: Effect::Files,
    run: add_images,
};

const LIST_PROJECT_IMAGES: Tool = Tool {
    name: "list_project_images",
    title: "List project images",
    description: "List the current project's images: id, name, path, and whether objects were saved for it.",
    schema: || schema(json!({}), &[]),
    effect: Effect::Read,
    run: list,
};

const OPEN_PROJECT_IMAGE: Tool = Tool {
    name: "open_project_image",
    title: "Open project image",
    description: "Open a project image by entry id or name, with its saved objects. It becomes the current image; edits to it are saved automatically.",
    schema: || {
        schema(
            json!({ "id": { "type": "integer", "description": "Entry id" }, "name": { "type": "string", "description": "Image name" } }),
            &[],
        )
    },
    effect: Effect::Files,
    run: open_image,
};

const SAVE: Tool = Tool {
    name: "save",
    title: "Save",
    description: "Write the current project image's objects and settings, and the project file. Edits are saved automatically; use this after open_image + create_project flows or to be sure.",
    schema: || schema(json!({}), &[]),
    effect: Effect::Files,
    run: save,
};

fn summary(p: &Project) -> Value {
    let images: Vec<Value> = p
        .entries
        .iter()
        .map(|e| {
            let saved = p.entry_dir(e.id).join(crate::io::image_data::OBJECTS_FILE).exists();
            json!({ "id": e.id, "name": e.name, "path": e.path, "has_data": saved })
        })
        .collect();
    json!({ "project": p.file, "name": p.name(), "images": images })
}

fn create(s: &mut Session, a: &Value) -> Result<Output> {
    let dir = PathBuf::from(args::text(a, "dir")?);
    let p = Project::create(&dir)?;
    s.project = Some(p);
    s.save_project()?;
    Ok(summary(s.project()?).into())
}

fn open(s: &mut Session, a: &Value) -> Result<Output> {
    let p = Project::open(Path::new(args::text(a, "path")?))?;
    s.image = None;
    s.project = Some(p);
    Ok(summary(s.project()?).into())
}

fn add_images(s: &mut Session, a: &Value) -> Result<Output> {
    let paths = args::opt_strings(a, "paths").context("missing `paths`")?;
    let p = s.project.as_mut().context("no project is open: call open_project or create_project first")?;
    let mut added = Vec::new();
    for raw in paths {
        let path = PathBuf::from(&raw).canonicalize().with_context(|| format!("cannot find {raw}"))?;
        let server = crate::image::open(&path)?;
        let md = server.metadata();
        let openslide = md.server_type.to_lowercase().contains("openslide");
        let id = p.add_image(&path, &md.name, openslide);
        let dir = p.entry_dir(id);
        std::fs::create_dir_all(&dir)?;
        if let Ok(thumb) = crate::image::thumbnail(server.as_ref(), 256) {
            let _ = thumb.save(dir.join("thumbnail.jpg"));
        }
        added.push(json!({ "id": id, "name": md.name, "width": md.width(), "height": md.height() }));
        // The open image joins the project with the objects made so far.
        if let Some(img) = s.image.as_mut().filter(|i| i.entry.is_none() && i.server.metadata().path == path) {
            img.entry = Some(id);
        }
    }
    s.save()?;
    s.save_project()?;
    Ok(json!({ "added": added }).into())
}

fn list(s: &mut Session, _a: &Value) -> Result<Output> {
    Ok(summary(s.project()?).into())
}

fn open_image(s: &mut Session, a: &Value) -> Result<Output> {
    let p = s.project()?;
    let id = match (a.get("id").and_then(Value::as_u64), args::opt_str(a, "name")) {
        (Some(id), _) => id,
        (None, Some(name)) => p.entries.iter().find(|e| e.name == name).map(|e| e.id).with_context(|| format!("no image named {name}"))?,
        _ => bail!("give `id` or `name` (list_project_images shows them)"),
    };
    s.open_entry(id)?;
    super::image::info(s, &json!({}))
}

fn save(s: &mut Session, _a: &Value) -> Result<Output> {
    match s.save()? {
        Some(dir) => Ok(json!({ "saved_to": dir }).into()),
        None if s.project.is_some() => {
            s.save_project()?;
            Ok(json!({ "saved_to": s.project()?.file, "note": "the current image is not in the project; add it with add_images to keep its objects" }).into())
        }
        None => bail!("nothing to save into: the image is not in a project. Use export_geojson, or create_project + add_images"),
    }
}

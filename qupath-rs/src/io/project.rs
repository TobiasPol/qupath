//! Projects in QuPath's format: `project.qpproj`, `data/<id>/`, `classifiers/classes.json`.
//!
//! A project written here opens in QuPath (images, names, metadata, tags and
//! classes). Object data is stored per entry as `objects.geojson`, which QuPath
//! can import, alongside QuPath's own `data.qpdata` when one exists.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::objects::{PathClass, Rgb};

/// QuPath version written to new projects, so QuPath accepts them without migration.
pub const QUPATH_VERSION: &str = "0.7.0";

/// One image in a project.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectEntry {
    /// `entryID`, also the data folder name.
    pub id: u64,
    /// Shown name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Key/value metadata.
    pub metadata: IndexMap<String, String>,
    /// Tags.
    pub tags: Vec<String>,
    /// The image location.
    pub path: PathBuf,
    /// QuPath's `serverBuilder`, kept as read so QuPath projects round-trip.
    pub server_builder: Value,
    randomized_name: String,
}

/// A project.
#[derive(Clone, Debug)]
pub struct Project {
    /// The `.qpproj` file.
    pub file: PathBuf,
    /// Entries, in order.
    pub entries: Vec<ProjectEntry>,
    created: i64,
    last_id: u64,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// `file:/path` with QuPath's (Java `File.toURI`) escaping.
pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file:");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:!$&'()*+,;=@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Inverse of [`path_to_uri`] (also accepts `file:///`).
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file:")?;
    let rest = rest.trim_start_matches("//");
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let s = String::from_utf8_lossy(&out).into_owned();
    // Windows: "/C:/x" → "C:/x".
    let s = if s.len() > 2 && s.as_bytes()[2] == b':' { s[1..].to_owned() } else { s };
    Some(PathBuf::from(s))
}

/// The builder QuPath uses for a file it should open itself.
fn server_builder_for(path: &Path, openslide: bool) -> Value {
    let provider = if openslide {
        "qupath.lib.images.servers.openslide.OpenslideServerBuilder"
    } else {
        "qupath.lib.images.servers.bioformats.BioFormatsServerBuilder"
    };
    let args: Vec<&str> = if openslide { vec![] } else { vec!["--series", "0"] };
    json!({ "builderType": "uri", "providerClassName": provider, "uri": path_to_uri(path), "args": args })
}

impl Project {
    /// Creates a project in an empty (or new) directory.
    pub fn create(dir: &Path) -> Result<Project> {
        if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
            bail!("a project needs an empty folder; {} is not empty", dir.display());
        }
        std::fs::create_dir_all(dir)?;
        Ok(Project { file: dir.join("project.qpproj"), entries: Vec::new(), created: now_ms(), last_id: 0 })
    }

    /// Opens `project.qpproj` (or the folder containing it).
    pub fn open(path: &Path) -> Result<Project> {
        let file = if path.is_dir() { path.join("project.qpproj") } else { path.to_path_buf() };
        let text = std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
        let v: Value = serde_json::from_str(&text).context("project file is not valid JSON")?;
        let entries: Vec<ProjectEntry> =
            v.get("images").and_then(Value::as_array).map(|a| a.iter().filter_map(entry_from_json).collect()).unwrap_or_default();
        let max_id = entries.iter().map(|e| e.id).max().unwrap_or(0);
        Ok(Project {
            file,
            created: v.get("createTimestamp").and_then(Value::as_i64).unwrap_or_else(now_ms),
            last_id: v.get("lastID").and_then(Value::as_u64).unwrap_or(0).max(max_id),
            entries,
        })
    }

    /// The project folder.
    pub fn base(&self) -> PathBuf {
        self.file.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// Project name (the folder name, as QuPath shows it).
    pub fn name(&self) -> String {
        self.base().file_name().map_or_else(|| "Project".into(), |n| n.to_string_lossy().into_owned())
    }

    /// The folder holding an entry's data.
    pub fn entry_dir(&self, id: u64) -> PathBuf {
        self.base().join("data").join(id.to_string())
    }

    /// Adds an image; returns its id. Duplicate paths are allowed, as in QuPath.
    pub fn add_image(&mut self, path: &Path, name: &str, openslide: bool) -> u64 {
        self.last_id += 1;
        let entry = ProjectEntry {
            id: self.last_id,
            name: name.to_owned(),
            description: None,
            metadata: IndexMap::new(),
            tags: Vec::new(),
            path: path.to_path_buf(),
            server_builder: server_builder_for(path, openslide),
            randomized_name: uuid::Uuid::new_v4().to_string(),
        };
        self.entries.push(entry);
        self.last_id
    }

    /// Removes an entry (its data folder is left for the caller to delete).
    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
    }

    /// An entry by id.
    pub fn entry(&self, id: u64) -> Option<&ProjectEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// An entry by id, for editing.
    pub fn entry_mut(&mut self, id: u64) -> Option<&mut ProjectEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// The `.qpproj` contents.
    pub fn to_json(&self) -> Vec<u8> {
        let images: Vec<Value> = self.entries.iter().map(entry_to_json).collect();
        let v = json!({
            "version": QUPATH_VERSION,
            "createTimestamp": self.created,
            "modifyTimestamp": now_ms(),
            "uri": path_to_uri(&self.file),
            "lastID": self.last_id,
            "images": images,
        });
        serde_json::to_vec_pretty(&v).unwrap_or_default()
    }

    /// Where the classes are kept.
    pub fn classes_file(&self) -> PathBuf {
        self.base().join("classifiers").join("classes.json")
    }

    /// Reads `classifiers/classes.json` (colors are Java packed ints).
    pub fn read_classes(&self) -> Option<Vec<PathClass>> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(self.classes_file()).ok()?).ok()?;
        let arr = v.get("pathClasses")?.as_array()?;
        Some(
            arr.iter()
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?;
                    let class = PathClass::new(name);
                    if let Some(col) = c.get("color").and_then(Value::as_i64) {
                        class.set_color(Rgb::from_packed(col as i32));
                    }
                    Some(class)
                })
                .collect(),
        )
    }

    /// `classifiers/classes.json` contents.
    pub fn classes_json(classes: &[PathClass]) -> Vec<u8> {
        let arr: Vec<Value> = classes.iter().map(|c| json!({ "name": c.name(), "color": c.color().to_packed() })).collect();
        serde_json::to_vec_pretty(&json!({ "pathClasses": arr })).unwrap_or_default()
    }
}

fn entry_from_json(v: &Value) -> Option<ProjectEntry> {
    let builder = v.get("serverBuilder")?.clone();
    let uri = builder.get("uri").and_then(Value::as_str).or_else(|| nested_uri(&builder))?;
    let path = uri_to_path(uri)?;
    Some(ProjectEntry {
        id: v.get("entryID")?.as_u64()?,
        name: v.get("imageName").and_then(Value::as_str).map_or_else(|| file_name(&path), str::to_owned),
        description: v.get("description").and_then(Value::as_str).map(str::to_owned),
        metadata: v
            .get("metadata")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned())).collect())
            .unwrap_or_default(),
        tags: v
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        path,
        server_builder: builder,
        randomized_name: v.get("randomizedName").and_then(Value::as_str).unwrap_or_default().to_owned(),
    })
}

/// Transformed servers nest the original builder: find the first URI inside.
fn nested_uri(v: &Value) -> Option<&str> {
    match v {
        Value::Object(o) => o.get("uri").and_then(Value::as_str).or_else(|| o.values().find_map(nested_uri)),
        Value::Array(a) => a.iter().find_map(nested_uri),
        _ => None,
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn entry_to_json(e: &ProjectEntry) -> Value {
    let mut builder = e.server_builder.clone();
    if let Some(o) = builder.as_object_mut()
        && o.contains_key("uri")
    {
        o.insert("uri".into(), json!(path_to_uri(&e.path)));
    }
    let mut v = json!({
        "serverBuilder": builder,
        "entryID": e.id,
        "randomizedName": e.randomized_name,
        "imageName": e.name,
        "metadata": e.metadata,
        "tags": e.tags,
    });
    if let (Some(d), Some(o)) = (&e.description, v.as_object_mut()) {
        o.insert("description".into(), json!(d));
    }
    v
}

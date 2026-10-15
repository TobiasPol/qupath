//! `PathObject`: annotations, detections, cells, tiles and TMA cores.

use indexmap::IndexMap;
use uuid::Uuid;

use super::classes::PathClass;
use super::color::Rgb;
use super::roi::Roi;

slotmap::new_key_type! {
    /// A handle to an object in a [`super::Hierarchy`].
    pub struct ObjKey;
}

/// The kinds of object QuPath distinguishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    /// The invisible root of the hierarchy.
    Root,
    /// A region drawn or created to define where to analyze.
    Annotation,
    /// A detected structure.
    Detection,
    /// A detection with a nucleus.
    Cell,
    /// A tile (a detection used as a region).
    Tile,
    /// A tissue microarray core.
    TmaCore,
}

impl ObjectKind {
    /// The `objectType` QuPath writes in GeoJSON.
    pub fn geojson_name(self) -> &'static str {
        match self {
            ObjectKind::Root => "root",
            ObjectKind::Annotation => "annotation",
            ObjectKind::Detection => "detection",
            ObjectKind::Cell => "cell",
            ObjectKind::Tile => "tile",
            ObjectKind::TmaCore => "tmaCore",
        }
    }

    /// Parses an `objectType`.
    pub fn from_geojson_name(s: &str) -> Option<Self> {
        Some(match s {
            "annotation" => ObjectKind::Annotation,
            "detection" => ObjectKind::Detection,
            "cell" => ObjectKind::Cell,
            "tile" => ObjectKind::Tile,
            "tmaCore" | "tma_core" => ObjectKind::TmaCore,
            "root" => ObjectKind::Root,
            _ => return None,
        })
    }

    /// The name shown in tables ("Annotation", "Detection", ...).
    pub fn display_name(self) -> &'static str {
        match self {
            ObjectKind::Root => "Image",
            ObjectKind::Annotation => "Annotation",
            ObjectKind::Detection => "Detection",
            ObjectKind::Cell => "Cell",
            ObjectKind::Tile => "Tile",
            ObjectKind::TmaCore => "TMA core",
        }
    }

    /// Detections, cells and tiles.
    pub fn is_detection(self) -> bool {
        matches!(self, ObjectKind::Detection | ObjectKind::Cell | ObjectKind::Tile)
    }
}

/// An object with a region, an optional classification and measurements.
#[derive(Clone, Debug)]
pub struct PathObject {
    /// Stable identifier, written to GeoJSON as `id`.
    pub id: Uuid,
    /// What kind of object this is.
    pub kind: ObjectKind,
    /// The region.
    pub roi: Roi,
    /// The nucleus of a cell.
    pub nucleus: Option<Roi>,
    /// The classification.
    pub class: Option<PathClass>,
    /// An optional name.
    pub name: Option<String>,
    /// A color that overrides the class color.
    pub color: Option<Rgb>,
    /// Locked objects cannot be moved or edited.
    pub locked: bool,
    /// Stored measurements, in insertion order.
    pub measurements: IndexMap<String, f64>,
    pub(crate) parent: Option<ObjKey>,
    pub(crate) children: Vec<ObjKey>,
}

impl PathObject {
    /// A new object of a kind.
    pub fn new(kind: ObjectKind, roi: Roi) -> Self {
        PathObject {
            id: Uuid::new_v4(),
            kind,
            roi,
            nucleus: None,
            class: None,
            name: None,
            color: None,
            locked: false,
            measurements: IndexMap::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    /// A new annotation.
    pub fn annotation(roi: Roi, class: Option<PathClass>) -> Self {
        PathObject { class, ..PathObject::new(ObjectKind::Annotation, roi) }
    }

    /// A new detection.
    pub fn detection(roi: Roi, class: Option<PathClass>) -> Self {
        PathObject { class, ..PathObject::new(ObjectKind::Detection, roi) }
    }

    /// A new cell.
    pub fn cell(cell: Roi, nucleus: Option<Roi>, class: Option<PathClass>) -> Self {
        PathObject { class, nucleus, ..PathObject::new(ObjectKind::Cell, cell) }
    }

    /// A new tile.
    pub fn tile(roi: Roi) -> Self {
        PathObject::new(ObjectKind::Tile, roi)
    }

    /// The color to draw with: own color, then class color, then QuPath's defaults.
    pub fn display_color(&self) -> Rgb {
        if let Some(c) = self.color {
            return c;
        }
        if let Some(c) = &self.class {
            return c.color();
        }
        match self.kind {
            ObjectKind::Annotation => Rgb(255, 0, 0),
            ObjectKind::TmaCore => Rgb(102, 102, 255),
            ObjectKind::Tile => Rgb(80, 80, 80),
            _ => Rgb(150, 0, 0),
        }
    }

    /// Whether this is an annotation.
    pub fn is_annotation(&self) -> bool {
        self.kind == ObjectKind::Annotation
    }

    /// Whether this is a detection, cell or tile.
    pub fn is_detection(&self) -> bool {
        self.kind.is_detection()
    }

    /// The parent, if inserted in a hierarchy.
    pub fn parent(&self) -> Option<ObjKey> {
        self.parent
    }

    /// The children, if inserted in a hierarchy.
    pub fn children(&self) -> &[ObjKey] {
        &self.children
    }

    /// The label QuPath shows in lists: name, else class and ROI type.
    pub fn label(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        let class = self.class.as_ref().map(|c| format!(" ({c})")).unwrap_or_default();
        format!("{}{class}", self.roi.type_name())
    }
}

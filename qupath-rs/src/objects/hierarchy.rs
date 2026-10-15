//! The object hierarchy: objects nested by containment under a root.

use indexmap::IndexSet;
use slotmap::SlotMap;

use super::object::{ObjKey, ObjectKind, PathObject};
use super::roi::{Bounds, Roi};

/// All objects of an image, the parent/child relations, and the selection.
#[derive(Clone)]
pub struct Hierarchy {
    objects: SlotMap<ObjKey, PathObject>,
    root: ObjKey,
    selection: IndexSet<ObjKey>,
    version: u64,
}

impl Default for Hierarchy {
    fn default() -> Self {
        Self::new()
    }
}

impl Hierarchy {
    /// An empty hierarchy.
    pub fn new() -> Self {
        let mut objects = SlotMap::with_key();
        let root = objects.insert(PathObject::new(ObjectKind::Root, Roi::Points(vec![])));
        Hierarchy { objects, root, selection: IndexSet::new(), version: 0 }
    }

    /// Bumped on every change, so caches know when to rebuild.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Marks the hierarchy as changed.
    pub fn touch(&mut self) {
        self.version += 1;
    }

    /// The root object.
    pub fn root(&self) -> ObjKey {
        self.root
    }

    /// An object.
    pub fn get(&self, k: ObjKey) -> Option<&PathObject> {
        self.objects.get(k)
    }

    /// An object, for changing it (marks the hierarchy as changed).
    pub fn get_mut(&mut self, k: ObjKey) -> Option<&mut PathObject> {
        self.version += 1;
        self.objects.get_mut(k)
    }

    /// Every object except the root.
    pub fn iter(&self) -> impl Iterator<Item = (ObjKey, &PathObject)> {
        let root = self.root;
        self.objects.iter().filter(move |(k, _)| *k != root)
    }

    /// Every annotation.
    pub fn annotations(&self) -> impl Iterator<Item = (ObjKey, &PathObject)> {
        self.iter().filter(|(_, o)| o.is_annotation())
    }

    /// Every detection, cell and tile.
    pub fn detections(&self) -> impl Iterator<Item = (ObjKey, &PathObject)> {
        self.iter().filter(|(_, o)| o.is_detection())
    }

    /// Number of objects, excluding the root.
    pub fn len(&self) -> usize {
        self.objects.len() - 1
    }

    /// No objects besides the root.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The children of an object.
    pub fn children(&self, k: ObjKey) -> &[ObjKey] {
        self.objects.get(k).map_or(&[], |o| o.children.as_slice())
    }

    /// All descendants, depth first.
    pub fn descendants(&self, k: ObjKey) -> Vec<ObjKey> {
        let mut out = Vec::new();
        let mut stack: Vec<ObjKey> = self.children(k).iter().rev().copied().collect();
        while let Some(c) = stack.pop() {
            out.push(c);
            stack.extend(self.children(c).iter().rev().copied());
        }
        out
    }

    /// Adds an object where it belongs: under the smallest annotation that
    /// contains it (whole shape for annotations, centroid for detections).
    /// Objects already under that parent that the new annotation contains move under it.
    pub fn insert(&mut self, obj: PathObject) -> ObjKey {
        let parent = self.find_parent(&obj, None);
        let is_annotation = obj.kind == ObjectKind::Annotation || obj.kind == ObjectKind::TmaCore;
        let k = self.add_child(parent, obj);
        if is_annotation {
            self.adopt_contained_siblings(parent, k);
        }
        k
    }

    /// Adds objects directly under a parent, without searching (fast for detections).
    pub fn add_children(&mut self, parent: ObjKey, objs: Vec<PathObject>) -> Vec<ObjKey> {
        let keys: Vec<ObjKey> = objs.into_iter().map(|o| self.add_child_unchecked(parent, o)).collect();
        self.version += 1;
        keys
    }

    /// Adds one object directly under a parent.
    pub fn add_child(&mut self, parent: ObjKey, obj: PathObject) -> ObjKey {
        let k = self.add_child_unchecked(parent, obj);
        self.version += 1;
        k
    }

    fn add_child_unchecked(&mut self, parent: ObjKey, mut obj: PathObject) -> ObjKey {
        let parent = if self.objects.contains_key(parent) { parent } else { self.root };
        obj.parent = Some(parent);
        obj.children.clear();
        let k = self.objects.insert(obj);
        self.objects[parent].children.push(k);
        k
    }

    /// The annotation (or root) an object belongs under.
    fn find_parent(&self, obj: &PathObject, exclude: Option<ObjKey>) -> ObjKey {
        let is_detection = obj.is_detection();
        let centroid = obj.roi.centroid();
        let bounds = obj.roi.bounds();
        let geom = if is_detection { None } else { obj.roi.to_multipolygon() };
        let mut best: Option<(ObjKey, f64)> = None;
        for (k, cand) in self.iter() {
            if Some(k) == exclude || !(cand.is_annotation() || cand.kind == ObjectKind::TmaCore) || !cand.roi.is_area() {
                continue;
            }
            let cb = cand.roi.bounds();
            let fits = if is_detection {
                cb.contains(centroid) && cand.roi.contains(centroid)
            } else {
                cb.contains_bounds(&bounds) && covers(&cand.roi, geom.as_ref(), &obj.roi)
            };
            let area = cand.roi.area(1.0, 1.0);
            if fits && best.is_none_or(|(_, a)| area < a) && !(area <= obj.roi.area(1.0, 1.0) && !is_detection) {
                best = Some((k, area));
            }
        }
        best.map_or(self.root, |(k, _)| k)
    }

    fn adopt_contained_siblings(&mut self, parent: ObjKey, k: ObjKey) {
        let roi = self.objects[k].roi.clone();
        if !roi.is_area() {
            return;
        }
        let bounds = roi.bounds();
        let siblings: Vec<ObjKey> = self.children(parent).iter().copied().filter(|c| *c != k).collect();
        for s in siblings {
            let o = &self.objects[s];
            let inside = if o.is_detection() {
                bounds.contains(o.roi.centroid()) && roi.contains(o.roi.centroid())
            } else {
                bounds.contains_bounds(&o.roi.bounds()) && covers(&roi, o.roi.to_multipolygon().as_ref(), &o.roi)
            };
            if inside && o.roi.area(1.0, 1.0) < roi.area(1.0, 1.0) {
                self.reparent(s, k);
            }
        }
    }

    fn reparent(&mut self, child: ObjKey, new_parent: ObjKey) {
        if let Some(old) = self.objects[child].parent {
            self.objects[old].children.retain(|c| *c != child);
        }
        self.objects[child].parent = Some(new_parent);
        self.objects[new_parent].children.push(child);
        self.version += 1;
    }

    /// Removes an object; its children move to its parent, or are removed too.
    pub fn remove(&mut self, k: ObjKey, keep_children: bool) {
        if k == self.root || !self.objects.contains_key(k) {
            return;
        }
        let parent = self.objects[k].parent.unwrap_or(self.root);
        self.objects[parent].children.retain(|c| *c != k);
        let children = std::mem::take(&mut self.objects[k].children);
        for c in children {
            if keep_children {
                self.objects[c].parent = Some(parent);
                self.objects[parent].children.push(c);
            } else {
                for d in std::iter::once(c).chain(self.descendants(c)) {
                    self.selection.shift_remove(&d);
                    self.objects.remove(d);
                }
            }
        }
        self.selection.shift_remove(&k);
        self.objects.remove(k);
        self.version += 1;
    }

    /// Removes several objects, keeping (and re-parenting) their children.
    pub fn remove_many(&mut self, keys: &[ObjKey], keep_children: bool) {
        for k in keys {
            self.remove(*k, keep_children);
        }
    }

    /// Removes every object matching a predicate (children of removed annotations are kept).
    pub fn remove_where(&mut self, f: impl Fn(&PathObject) -> bool) {
        let keys: Vec<ObjKey> = self.iter().filter(|(_, o)| f(o)).map(|(k, _)| k).collect();
        let only_detections = keys.iter().all(|k| self.objects[*k].is_detection());
        if only_detections {
            self.remove_detections_fast(&keys);
        } else {
            self.remove_many(&keys, true);
        }
    }

    fn remove_detections_fast(&mut self, keys: &[ObjKey]) {
        let set: std::collections::HashSet<ObjKey> = keys.iter().copied().collect();
        for (_, o) in self.objects.iter_mut() {
            o.children.retain(|c| !set.contains(c));
        }
        for k in keys {
            self.selection.shift_remove(k);
            self.objects.remove(*k);
        }
        self.version += 1;
    }

    /// Re-inserts every object, so parents reflect containment again.
    pub fn resolve(&mut self) {
        let root = self.root;
        let mut all: Vec<PathObject> = Vec::new();
        let keys: Vec<ObjKey> = self.iter().map(|(k, _)| k).collect();
        for k in keys {
            if let Some(o) = self.objects.remove(k) {
                all.push(o);
            }
        }
        self.objects[root].children.clear();
        self.selection.clear();
        all.sort_by(|a, b| b.roi.area(1.0, 1.0).total_cmp(&a.roi.area(1.0, 1.0)));
        let (dets, anns): (Vec<_>, Vec<_>) = all.into_iter().partition(PathObject::is_detection);
        for a in anns {
            self.insert(a);
        }
        for d in dets {
            let p = self.find_parent(&d, None);
            self.add_child_unchecked(p, d);
        }
        self.version += 1;
    }
}

/// Whether area ROI `outer` covers `inner` (`outer_geom` is `inner`'s geometry when available).
fn covers(outer: &Roi, inner_geom: Option<&geo::MultiPolygon<f64>>, inner: &Roi) -> bool {
    use geo::Covers;
    let Some(og) = outer.to_multipolygon() else { return false };
    match inner_geom {
        Some(g) => og.covers(g),
        None => inner.outline().iter().all(|p| outer.contains(*p)),
    }
}

/// Selection handling lives in `selection.rs`; this keeps the struct fields private.
impl Hierarchy {
    pub(crate) fn selection_set(&self) -> &IndexSet<ObjKey> {
        &self.selection
    }
    pub(crate) fn selection_set_mut(&mut self) -> &mut IndexSet<ObjKey> {
        &mut self.selection
    }
    pub(crate) fn contains_key(&self, k: ObjKey) -> bool {
        self.objects.contains_key(k)
    }
    pub(crate) fn bounds_of(&self, k: ObjKey) -> Option<Bounds> {
        self.objects.get(k).map(|o| o.roi.bounds())
    }
}

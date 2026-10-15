//! Selection in the hierarchy: a set of objects, the last one being the main selection.

use super::hierarchy::Hierarchy;
use super::object::ObjKey;

impl Hierarchy {
    /// The selected objects, oldest first.
    pub fn selected(&self) -> Vec<ObjKey> {
        self.selection_set().iter().copied().collect()
    }

    /// The main (most recently) selected object.
    pub fn primary_selected(&self) -> Option<ObjKey> {
        self.selection_set().last().copied()
    }

    /// Whether an object is selected.
    pub fn is_selected(&self, k: ObjKey) -> bool {
        self.selection_set().contains(&k)
    }

    /// Selects only this object (or nothing).
    pub fn select(&mut self, k: Option<ObjKey>) {
        let set = self.selection_set_mut();
        set.clear();
        if let Some(k) = k {
            set.insert(k);
        }
    }

    /// Adds to or removes from the selection.
    pub fn toggle_selected(&mut self, k: ObjKey) {
        let set = self.selection_set_mut();
        if !set.shift_remove(&k) {
            set.insert(k);
        }
    }

    /// Adds objects to the selection.
    pub fn add_to_selection(&mut self, keys: impl IntoIterator<Item = ObjKey>) {
        let valid: Vec<ObjKey> = keys.into_iter().filter(|k| self.contains_key(*k)).collect();
        self.selection_set_mut().extend(valid);
    }

    /// Selects nothing.
    pub fn clear_selection(&mut self) {
        self.selection_set_mut().clear();
    }

    /// Drops selected keys whose objects no longer exist.
    pub fn prune_selection(&mut self) {
        let stale: Vec<ObjKey> = self.selection_set().iter().copied().filter(|k| !self.contains_key(*k)).collect();
        for k in stale {
            self.selection_set_mut().shift_remove(&k);
        }
    }

    /// Bounds of the main selection.
    pub fn primary_bounds(&self) -> Option<super::roi::Bounds> {
        self.primary_selected().and_then(|k| self.bounds_of(k))
    }
}

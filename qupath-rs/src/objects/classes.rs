//! Classifications, following QuPath's `PathClass`.
//!
//! As in QuPath, a class is identified by its name and its color is global:
//! `PathClass::new("Tumor")` anywhere refers to the same class. Derived classes
//! join names with `": "`, e.g. `Tumor: Positive`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use super::color::Rgb;

static COLORS: LazyLock<RwLock<HashMap<String, Rgb>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// The separator QuPath uses between the parts of a derived class.
pub const SEPARATOR: &str = ": ";
/// Base name of the standard positive class.
pub const POSITIVE: &str = "Positive";
/// Base name of the standard negative class.
pub const NEGATIVE: &str = "Negative";

const COLOR_POSITIVE: Rgb = Rgb(200, 50, 50);
const COLOR_NEGATIVE: Rgb = Rgb(90, 90, 180);
const COLOR_ONE_PLUS: Rgb = Rgb(255, 215, 0);
const COLOR_TWO_PLUS: Rgb = Rgb(225, 150, 50);
const COLOR_THREE_PLUS: Rgb = Rgb(200, 50, 50);

/// A classification such as `Tumor` or `Tumor: Positive`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PathClass(Arc<str>);

impl PathClass {
    /// The class with this name, given a default color the first time it is seen.
    pub fn new(name: &str) -> Self {
        let name = name.trim();
        let class = PathClass(Arc::from(name));
        let known = COLORS.read().map(|m| m.contains_key(name)).unwrap_or(true);
        if !known {
            let color = default_color(name);
            if let Ok(mut m) = COLORS.write() {
                m.entry(name.to_owned()).or_insert(color);
            }
        }
        class
    }

    /// The class with this name, setting its color.
    pub fn with_color(name: &str, color: Rgb) -> Self {
        let class = PathClass::new(name);
        class.set_color(color);
        class
    }

    /// Built from the parts of a derived class, e.g. `["Tumor", "Positive"]`.
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        let joined: Vec<&str> = names.iter().map(AsRef::as_ref).collect();
        PathClass::new(&joined.join(SEPARATOR))
    }

    /// The full name, e.g. `Tumor: Positive`.
    pub fn name(&self) -> &str {
        &self.0
    }

    /// The parts of the name.
    pub fn names(&self) -> Vec<&str> {
        self.0.split(SEPARATOR).collect()
    }

    /// The first part of the name.
    pub fn base_name(&self) -> &str {
        self.names().first().copied().unwrap_or("")
    }

    /// The class this one was derived from, if any.
    pub fn parent(&self) -> Option<PathClass> {
        let names = self.names();
        (names.len() > 1).then(|| PathClass::from_names(&names[..names.len() - 1]))
    }

    /// `self: child`.
    pub fn derived(&self, child: &str) -> PathClass {
        PathClass::new(&format!("{}{SEPARATOR}{child}", self.0))
    }

    /// The display color.
    pub fn color(&self) -> Rgb {
        COLORS.read().ok().and_then(|m| m.get(&*self.0).copied()).unwrap_or_else(|| default_color(&self.0))
    }

    /// Changes the color for every object of this class.
    pub fn set_color(&self, color: Rgb) {
        if let Ok(mut m) = COLORS.write() {
            m.insert(self.0.to_string(), color);
        }
    }

    /// Classes ending in `*` (like `Ignore*` and `Region*`) are excluded from counts.
    pub fn is_ignored(&self) -> bool {
        self.0.ends_with('*')
    }

    /// The last part is `Positive` (or an intensity score).
    pub fn is_positive(&self) -> bool {
        matches!(self.names().last().copied(), Some(POSITIVE | "1+" | "2+" | "3+"))
    }

    /// The last part is `Negative`.
    pub fn is_negative(&self) -> bool {
        self.names().last().copied() == Some(NEGATIVE)
    }
}

impl std::fmt::Display for PathClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// QuPath's color for a class seen for the first time.
fn default_color(name: &str) -> Rgb {
    let names: Vec<&str> = name.split(SEPARATOR).collect();
    let last = names.last().copied().unwrap_or("");
    if names.len() == 1 {
        return match name {
            "Tumor" => Rgb(200, 0, 0),
            "Stroma" => Rgb(150, 200, 150),
            "Immune cells" => Rgb(160, 90, 160),
            "Ignore*" => Rgb(180, 180, 180),
            "Necrosis" => Rgb(50, 50, 50),
            "Other" => Rgb(255, 200, 0),
            "Region*" => Rgb(0, 0, 180),
            POSITIVE => COLOR_POSITIVE,
            NEGATIVE => COLOR_NEGATIVE,
            "1+" => COLOR_ONE_PLUS,
            "2+" => COLOR_TWO_PLUS,
            "3+" => COLOR_THREE_PLUS,
            _ => Rgb::from_name_hash(name),
        };
    }
    let parent = PathClass::from_names(&names[..names.len() - 1]);
    let is_tumor = names[0] == "Tumor";
    let parent_rgb = parent.color();
    match last {
        "1+" if is_tumor => COLOR_ONE_PLUS,
        "2+" if is_tumor => COLOR_TWO_PLUS,
        "3+" if is_tumor => COLOR_THREE_PLUS,
        POSITIVE if is_tumor => COLOR_POSITIVE,
        NEGATIVE if is_tumor => COLOR_NEGATIVE,
        "1+" => parent_rgb.scaled(0.9),
        "2+" => parent_rgb.scaled(0.6),
        "3+" => parent_rgb.scaled(0.4),
        POSITIVE => parent_rgb.scaled(0.75),
        NEGATIVE => parent_rgb.scaled(1.25),
        _ => Rgb::from_name_hash(name),
    }
}

/// The classes QuPath offers by default (`None` is "Unclassified").
pub fn default_classes() -> Vec<Option<PathClass>> {
    let mut v = vec![None];
    for name in ["Tumor", "Stroma", "Immune cells", "Necrosis", "Other", "Region*", "Ignore*", POSITIVE, NEGATIVE] {
        v.push(Some(PathClass::new(name)));
    }
    v
}

/// The name shown for an optional class.
pub fn display_name(class: Option<&PathClass>) -> &str {
    class.map_or("Unclassified", PathClass::name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_classes_split_and_join() {
        let c = PathClass::new("Tumor").derived(POSITIVE);
        assert_eq!(c.name(), "Tumor: Positive");
        assert_eq!(c.names(), vec!["Tumor", "Positive"]);
        assert!(c.is_positive());
        assert_eq!(c.color(), COLOR_POSITIVE);
        assert_eq!(c.parent(), Some(PathClass::new("Tumor")));
    }

    #[test]
    fn stroma_positive_is_scaled_from_parent() {
        let c = PathClass::new("Stroma: Positive");
        assert_eq!(c.color(), Rgb(150, 200, 150).scaled(0.75));
    }
}

//! Regions of interest, mirroring QuPath's ROI types.
//!
//! Coordinates are in full-resolution image pixels. Rectangles and ellipses
//! keep their shape so they can be edited with handles; anything produced by
//! boolean operations becomes an [`Roi::Area`].

use geo::{Area, BoundingRect, Centroid, Contains, Coord, Euclidean, Length, LineString, MultiPolygon, Point, Polygon};

/// A point in image pixel coordinates.
pub type Pt = Coord<f64>;

/// Axis-aligned bounds.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Bounds {
    /// Left.
    pub x0: f64,
    /// Top.
    pub y0: f64,
    /// Right.
    pub x1: f64,
    /// Bottom.
    pub y1: f64,
}

impl Bounds {
    /// Width.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }
    /// Height.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }
    /// Whether the two overlap (touching counts).
    pub fn intersects(&self, o: &Bounds) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    /// Whether `o` lies entirely inside.
    pub fn contains_bounds(&self, o: &Bounds) -> bool {
        o.x0 >= self.x0 && o.x1 <= self.x1 && o.y0 >= self.y0 && o.y1 <= self.y1
    }
    /// Whether the point lies inside.
    pub fn contains(&self, p: Pt) -> bool {
        p.x >= self.x0 && p.x <= self.x1 && p.y >= self.y0 && p.y <= self.y1
    }
    fn of_points<'a>(pts: impl IntoIterator<Item = &'a Pt>) -> Bounds {
        let mut b = Bounds { x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN };
        for p in pts {
            b.x0 = b.x0.min(p.x);
            b.y0 = b.y0.min(p.y);
            b.x1 = b.x1.max(p.x);
            b.y1 = b.y1.max(p.y);
        }
        b
    }
}

/// A region of interest.
#[derive(Clone, Debug, PartialEq)]
pub enum Roi {
    /// Axis-aligned rectangle.
    Rectangle { x: f64, y: f64, w: f64, h: f64 },
    /// Ellipse inscribed in the rectangle.
    Ellipse { x: f64, y: f64, w: f64, h: f64 },
    /// A straight line.
    Line { x1: f64, y1: f64, x2: f64, y2: f64 },
    /// A closed polygon without holes (the first point is not repeated).
    Polygon(Vec<Pt>),
    /// An open polyline.
    Polyline(Vec<Pt>),
    /// A set of points.
    Points(Vec<Pt>),
    /// Arbitrary area geometry, possibly with holes and several parts.
    Area(MultiPolygon<f64>),
}

/// Number of vertices used when an ellipse becomes a polygon.
const ELLIPSE_VERTICES: usize = 64;

impl Roi {
    /// QuPath's name for this kind of ROI.
    pub fn type_name(&self) -> &'static str {
        match self {
            Roi::Rectangle { .. } => "Rectangle",
            Roi::Ellipse { .. } => "Ellipse",
            Roi::Line { .. } => "Line",
            Roi::Polygon(_) => "Polygon",
            Roi::Polyline(_) => "Polyline",
            Roi::Points(_) => "Points",
            Roi::Area(_) => "Geometry",
        }
    }

    /// Encloses an area.
    pub fn is_area(&self) -> bool {
        matches!(self, Roi::Rectangle { .. } | Roi::Ellipse { .. } | Roi::Polygon(_) | Roi::Area(_))
    }

    /// A line or polyline.
    pub fn is_line(&self) -> bool {
        matches!(self, Roi::Line { .. } | Roi::Polyline(_))
    }

    /// A points ROI.
    pub fn is_points(&self) -> bool {
        matches!(self, Roi::Points(_))
    }

    /// A rectangle from two corners, in any order.
    pub fn rect_from_corners(a: Pt, b: Pt) -> Roi {
        Roi::Rectangle { x: a.x.min(b.x), y: a.y.min(b.y), w: (a.x - b.x).abs(), h: (a.y - b.y).abs() }
    }

    /// An ellipse inscribed in the rectangle spanned by two corners.
    pub fn ellipse_from_corners(a: Pt, b: Pt) -> Roi {
        Roi::Ellipse { x: a.x.min(b.x), y: a.y.min(b.y), w: (a.x - b.x).abs(), h: (a.y - b.y).abs() }
    }

    /// The vertices that outline the ROI (for drawing and hit tests).
    pub fn outline(&self) -> Vec<Pt> {
        match self {
            Roi::Rectangle { x, y, w, h } => {
                vec![Pt { x: *x, y: *y }, Pt { x: x + w, y: *y }, Pt { x: x + w, y: y + h }, Pt { x: *x, y: y + h }]
            }
            Roi::Ellipse { x, y, w, h } => ellipse_points(*x, *y, *w, *h, ELLIPSE_VERTICES),
            Roi::Line { x1, y1, x2, y2 } => vec![Pt { x: *x1, y: *y1 }, Pt { x: *x2, y: *y2 }],
            Roi::Polygon(p) | Roi::Polyline(p) | Roi::Points(p) => p.clone(),
            Roi::Area(mp) => mp.0.first().map(|p| p.exterior().0.clone()).unwrap_or_default(),
        }
    }

    /// Bounding box.
    pub fn bounds(&self) -> Bounds {
        match self {
            Roi::Rectangle { x, y, w, h } | Roi::Ellipse { x, y, w, h } => Bounds { x0: *x, y0: *y, x1: x + w, y1: y + h },
            Roi::Area(mp) => {
                mp.bounding_rect().map(|r| Bounds { x0: r.min().x, y0: r.min().y, x1: r.max().x, y1: r.max().y }).unwrap_or_default()
            }
            _ => Bounds::of_points(&self.outline()),
        }
    }

    /// The center of mass (of the area, the line or the points).
    pub fn centroid(&self) -> Pt {
        match self {
            Roi::Rectangle { x, y, w, h } | Roi::Ellipse { x, y, w, h } => Pt { x: x + w / 2.0, y: y + h / 2.0 },
            Roi::Points(p) | Roi::Polyline(p) => mean_point(p),
            Roi::Line { x1, y1, x2, y2 } => Pt { x: (x1 + x2) / 2.0, y: (y1 + y2) / 2.0 },
            _ => self.to_multipolygon().and_then(|mp| mp.centroid()).map_or_else(|| mean_point(&self.outline()), |p| p.0),
        }
    }

    /// Area in squared calibrated units (pass 1.0 for pixels).
    pub fn area(&self, pw: f64, ph: f64) -> f64 {
        let px = match self {
            Roi::Rectangle { w, h, .. } => w * h,
            Roi::Ellipse { w, h, .. } => std::f64::consts::PI * w * h / 4.0,
            _ => self.to_multipolygon().map_or(0.0, |mp| mp.unsigned_area()),
        };
        px * pw * ph
    }

    /// Perimeter for areas, length for lines, in calibrated units.
    pub fn length(&self, pw: f64, ph: f64) -> f64 {
        let scale = |pts: &[Pt]| -> Vec<Pt> { pts.iter().map(|p| Pt { x: p.x * pw, y: p.y * ph }).collect() };
        match self {
            Roi::Line { .. } | Roi::Polyline(_) => Euclidean.length(&LineString::new(scale(&self.outline()))),
            Roi::Points(_) => 0.0,
            Roi::Area(mp) => {
                mp.0.iter()
                    .flat_map(|p| std::iter::once(p.exterior()).chain(p.interiors()))
                    .map(|ring| Euclidean.length(&LineString::new(scale(&ring.0))))
                    .sum()
            }
            _ => {
                let mut pts = scale(&self.outline());
                if let Some(first) = pts.first().copied() {
                    pts.push(first);
                }
                Euclidean.length(&LineString::new(pts))
            }
        }
    }

    /// Number of points (for points ROIs) or vertices.
    pub fn num_points(&self) -> usize {
        self.outline().len()
    }

    /// The area as polygons, or `None` for lines and points.
    pub fn to_multipolygon(&self) -> Option<MultiPolygon<f64>> {
        match self {
            Roi::Area(mp) => Some(mp.clone()),
            r if r.is_area() => Some(MultiPolygon::new(vec![Polygon::new(LineString::new(close(r.outline())), vec![])])),
            _ => None,
        }
    }

    /// The simplest ROI for an area: a polygon when it has one part and no holes.
    pub fn from_multipolygon(mp: MultiPolygon<f64>) -> Option<Roi> {
        let mp = MultiPolygon::new(mp.0.into_iter().filter(|p| p.unsigned_area() > 0.0).collect());
        match mp.0.as_slice() {
            [] => None,
            [p] if p.interiors().is_empty() => {
                let mut pts = p.exterior().0.clone();
                if pts.len() > 1 && pts.first() == pts.last() {
                    pts.pop();
                }
                Some(Roi::Polygon(pts))
            }
            _ => Some(Roi::Area(mp)),
        }
    }

    /// Whether a point lies inside an area ROI.
    pub fn contains(&self, p: Pt) -> bool {
        match self {
            Roi::Rectangle { .. } => self.bounds().contains(p),
            Roi::Ellipse { x, y, w, h } => {
                let (rx, ry) = (w / 2.0, h / 2.0);
                let (dx, dy) = ((p.x - x - rx) / rx, (p.y - y - ry) / ry);
                rx > 0.0 && ry > 0.0 && dx * dx + dy * dy <= 1.0
            }
            _ => self.to_multipolygon().is_some_and(|mp| mp.contains(&Point(p))),
        }
    }

    /// Moved by an offset.
    pub fn translate(&self, dx: f64, dy: f64) -> Roi {
        let mv = |pts: &[Pt]| pts.iter().map(|p| Pt { x: p.x + dx, y: p.y + dy }).collect::<Vec<_>>();
        match self {
            Roi::Rectangle { x, y, w, h } => Roi::Rectangle { x: x + dx, y: y + dy, w: *w, h: *h },
            Roi::Ellipse { x, y, w, h } => Roi::Ellipse { x: x + dx, y: y + dy, w: *w, h: *h },
            Roi::Line { x1, y1, x2, y2 } => Roi::Line { x1: x1 + dx, y1: y1 + dy, x2: x2 + dx, y2: y2 + dy },
            Roi::Polygon(p) => Roi::Polygon(mv(p)),
            Roi::Polyline(p) => Roi::Polyline(mv(p)),
            Roi::Points(p) => Roi::Points(mv(p)),
            Roi::Area(mp) => Roi::Area(geo::Translate::translate(mp, dx, dy)),
        }
    }

    /// Scaled about the origin (used when importing at another resolution).
    pub fn scale(&self, s: f64) -> Roi {
        let sc = |pts: &[Pt]| pts.iter().map(|p| Pt { x: p.x * s, y: p.y * s }).collect::<Vec<_>>();
        match self {
            Roi::Rectangle { x, y, w, h } => Roi::Rectangle { x: x * s, y: y * s, w: w * s, h: h * s },
            Roi::Ellipse { x, y, w, h } => Roi::Ellipse { x: x * s, y: y * s, w: w * s, h: h * s },
            Roi::Line { x1, y1, x2, y2 } => Roi::Line { x1: x1 * s, y1: y1 * s, x2: x2 * s, y2: y2 * s },
            Roi::Polygon(p) => Roi::Polygon(sc(p)),
            Roi::Polyline(p) => Roi::Polyline(sc(p)),
            Roi::Points(p) => Roi::Points(sc(p)),
            Roi::Area(mp) => Roi::Area(geo::Scale::scale_around_point(mp, s, s, Point::new(0.0, 0.0))),
        }
    }
}

/// Vertices of an ellipse inscribed in a rectangle.
pub fn ellipse_points(x: f64, y: f64, w: f64, h: f64, n: usize) -> Vec<Pt> {
    let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            Pt { x: cx + rx * t.cos(), y: cy + ry * t.sin() }
        })
        .collect()
}

fn mean_point(pts: &[Pt]) -> Pt {
    if pts.is_empty() {
        return Pt { x: 0.0, y: 0.0 };
    }
    let n = pts.len() as f64;
    Pt { x: pts.iter().map(|p| p.x).sum::<f64>() / n, y: pts.iter().map(|p| p.y).sum::<f64>() / n }
}

/// Repeats the first point at the end, as closed rings need.
pub fn close(mut pts: Vec<Pt>) -> Vec<Pt> {
    if let (Some(f), Some(l)) = (pts.first().copied(), pts.last().copied())
        && f != l
    {
        pts.push(f);
    }
    pts
}

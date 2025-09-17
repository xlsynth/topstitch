// SPDX-License-Identifier: Apache-2.0

use crate::mod_def::tracks::{TrackDefinition, TrackOrientation};
use crate::{PipelineConfig, PortSlice};
pub(crate) struct VerilogImport {
    pub(crate) sources: Vec<String>,
    pub(crate) incdirs: Vec<String>,
    pub(crate) defines: Vec<(String, String)>,
    pub(crate) skip_unsupported: bool,
    pub(crate) ignore_unknown_modules: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Assignment {
    pub lhs: PortSlice,
    pub rhs: PortSlice,
    pub pipeline: Option<PipelineConfig>,
    pub is_non_abutted: bool,
}

#[derive(Clone)]
pub(crate) struct InstConnection {
    pub(crate) inst_port_slice: PortSlice,
    pub(crate) connected_to: PortSliceOrWire,
}

#[derive(Clone)]
pub(crate) struct Wire {
    pub(crate) name: String,
    pub(crate) width: usize,
}

#[derive(Clone)]
pub(crate) enum PortSliceOrWire {
    PortSlice(PortSlice),
    Wire(Wire),
}

// Floorplanning-related types

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coordinate {
    pub x: i64,
    pub y: i64,
}

impl From<(i64, i64)> for Coordinate {
    fn from(value: (i64, i64)) -> Self {
        Coordinate {
            x: value.0,
            y: value.1,
        }
    }
}

/// Represents an optionally bounded inclusive interval along an edge or track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl Range {
    /// Creates a range spanning the inclusive bounds of `a` and `b`,
    /// automatically ordering them if necessary.
    pub fn new(a: i64, b: i64) -> Self {
        if a <= b {
            Range {
                min: Some(a),
                max: Some(b),
            }
        } else {
            Range {
                min: Some(b),
                max: Some(a),
            }
        }
    }

    /// Creates a semi-infinite range beginning at `min` and extending upward.
    pub fn from_min(min: i64) -> Self {
        Range {
            min: Some(min),
            max: None,
        }
    }

    /// Creates a semi-infinite range ending at `max` and extending downward.
    pub fn from_max(max: i64) -> Self {
        Range {
            min: None,
            max: Some(max),
        }
    }

    /// Creates a fully unbounded range.
    pub fn any() -> Self {
        Range {
            min: None,
            max: None,
        }
    }

    /// Returns `true` if `value` lies inside the inclusive bounds of this
    /// range.
    pub fn contains(&self, value: i64) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }

    /// Returns `true` if every value in `self` is also contained within
    /// `other`.
    pub fn is_subset_of(&self, other: &Range) -> bool {
        self.min
            .is_none_or(|self_min| other.min.is_none_or(|other_min| self_min >= other_min))
            && self
                .max
                .is_none_or(|self_max| other.max.is_none_or(|other_max| self_max <= other_max))
    }

    /// Computes the overlapping portion of two ranges, if any.
    pub fn intersection(&self, other: &Range) -> Option<Range> {
        let min = match (self.min, other.min) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let max = match (self.max, other.max) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        match (min, max) {
            (Some(min_val), Some(max_val)) if min_val > max_val => None,
            _ => Some(Range { min, max }),
        }
    }
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(f, "[{min}, {max}]"),
            (Some(min), None) => write!(f, "[{min}, ...]"),
            (None, Some(max)) => write!(f, "[..., {max}]"),
            (None, None) => write!(f, "[...]"),
        }
    }
}

/// A half-open directed segment of a polygon boundary.
pub struct Edge {
    pub a: Coordinate,
    pub b: Coordinate,
}

/// Enumerates the axis-aligned orientation of an edge.
pub enum EdgeOrientation {
    North,
    South,
    East,
    West,
}

impl Edge {
    /// Returns the orientation of the edge, or `None` if it is not
    /// axis-aligned.
    pub fn orientation(&self) -> Option<EdgeOrientation> {
        if self.a.y == self.b.y {
            if self.a.x < self.b.x {
                Some(EdgeOrientation::East)
            } else {
                Some(EdgeOrientation::West)
            }
        } else if self.a.x == self.b.x {
            if self.a.y < self.b.y {
                Some(EdgeOrientation::North)
            } else {
                Some(EdgeOrientation::South)
            }
        } else {
            None
        }
    }

    /// Returns the inclusive range spanned by the edge along the X axis.
    pub fn get_x_range(&self) -> Range {
        Range::new(self.a.x, self.b.x)
    }

    /// Returns the inclusive range spanned by the edge along the Y axis.
    pub fn get_y_range(&self) -> Range {
        Range::new(self.a.y, self.b.y)
    }

    /// Returns the edge-parallel coordinate range for axis-aligned edges.
    pub fn get_coord_range(&self) -> Option<Range> {
        let edge_orientation = self.orientation()?;
        match edge_orientation {
            EdgeOrientation::North | EdgeOrientation::South => Some(self.get_y_range()),
            EdgeOrientation::East | EdgeOrientation::West => Some(self.get_x_range()),
        }
    }

    /// Returns the usable track index range for the given track definition, if
    /// the track orientation is compatible with this edge.
    pub fn get_index_range(&self, track: &TrackDefinition) -> Option<Range> {
        let edge_orientation = self.orientation()?;

        let coord_range = match (&track.orientation, edge_orientation) {
            (TrackOrientation::Horizontal, EdgeOrientation::North | EdgeOrientation::South) => {
                self.get_y_range()
            }
            (TrackOrientation::Vertical, EdgeOrientation::East | EdgeOrientation::West) => {
                self.get_x_range()
            }
            _ => return None,
        };

        let track_range = track.convert_coord_range_to_index_range(&coord_range);

        // sanity check - after converting the track indices back to coordinates, the
        // resulting range should be a subset of the original coordinate range
        let round_trip_coord_range: Range = track.convert_index_range_to_coord_range(&track_range);
        assert!(round_trip_coord_range.is_subset_of(&coord_range));

        Some(track_range)
    }

    /// Returns the absolute coordinate value (in the edge's axis) of
    /// `track_index_on_edge` measured from the edge start.
    pub fn get_position_on_edge(&self, track: &TrackDefinition, track_index_on_edge: usize) -> i64 {
        let start_stop = self.get_index_range(track).unwrap();
        let start_index = start_stop.min.unwrap();
        track.index_to_position(start_index + (track_index_on_edge as i64))
    }

    /// Returns the [`Coordinate`] where `track_index_on_edge` intersects this
    /// edge when using the supplied track definition.
    pub fn get_coordinate_on_edge(
        &self,
        track: &TrackDefinition,
        track_index_on_edge: usize,
    ) -> Coordinate {
        let position = self.get_position_on_edge(track, track_index_on_edge);
        match track.orientation {
            TrackOrientation::Horizontal => Coordinate {
                x: self.a.x,
                y: position,
            },
            TrackOrientation::Vertical => Coordinate {
                x: position,
                y: self.a.y,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    R0,
    R90,
    R180,
    R270,
    MX,
    MY,
    MX90,
    MY90,
}

impl Orientation {
    pub fn prepend_transform(&self, transform: &Orientation) -> Self {
        (&Mat3::from_orientation(*transform) * &Mat3::from_orientation(*self)).as_orientation()
    }

    pub fn append_transform(&self, transform: &Orientation) -> Self {
        (&Mat3::from_orientation(*self) * &Mat3::from_orientation(*transform)).as_orientation()
    }
}

#[derive(Debug, Clone)]
pub struct Polygon(pub Vec<Coordinate>);
pub struct BoundingBox {
    pub min_x: i64,
    pub max_x: i64,
    pub min_y: i64,
    pub max_y: i64,
}

impl BoundingBox {
    pub fn get_width(&self) -> i64 {
        self.max_x - self.min_x
    }

    pub fn get_height(&self) -> i64 {
        self.max_y - self.min_y
    }

    pub fn get_width_height(&self) -> (i64, i64) {
        (self.get_width(), self.get_height())
    }

    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min_x: self.min_x.min(other.min_x),
            max_x: self.max_x.max(other.max_x),
            min_y: self.min_y.min(other.min_y),
            max_y: self.max_y.max(other.max_y),
        }
    }

    pub fn apply_transform(&self, m: &Mat3) -> BoundingBox {
        Polygon::from_bbox(self).apply_transform(m).bbox()
    }
}

impl Polygon {
    pub fn new(points: Vec<Coordinate>) -> Self {
        Polygon(points)
    }
    /// Returns the `i`th edge, wrapping around for the closing segment.
    pub fn get_edge(&self, i: usize) -> Edge {
        assert!(i < self.0.len(), "Edge index out of bounds");
        let a = self.0[i];
        let b = self.0[(i + 1) % self.0.len()];
        Edge { a, b }
    }

    pub fn from_width_height(width: i64, height: i64) -> Self {
        Self::from_bbox(&BoundingBox {
            min_x: 0,
            min_y: 0,
            max_x: width,
            max_y: height,
        })
    }

    pub fn from_bbox(bbox: &BoundingBox) -> Self {
        Self::new(vec![
            Coordinate {
                x: bbox.min_x,
                y: bbox.min_y,
            },
            Coordinate {
                x: bbox.min_x,
                y: bbox.max_y,
            },
            Coordinate {
                x: bbox.max_x,
                y: bbox.max_y,
            },
            Coordinate {
                x: bbox.max_x,
                y: bbox.min_y,
            },
        ])
    }

    pub fn apply_transform(&self, m: &Mat3) -> Polygon {
        let pts = self
            .0
            .iter()
            .map(|p| {
                let v = nalgebra::Vector3::new(p.x, p.y, 1);
                let result = m.0 * v;
                Coordinate {
                    x: result[0],
                    y: result[1],
                }
            })
            .collect();
        Polygon(pts)
    }

    pub fn bbox(&self) -> BoundingBox {
        let mut min_x = i64::MAX;
        let mut max_x = i64::MIN;
        let mut min_y = i64::MAX;
        let mut max_y = i64::MIN;
        for p in &self.0 {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        BoundingBox {
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }

    /// Returns true if all edges are axis-aligned and non-degenerate.
    pub fn is_rectilinear(&self) -> bool {
        let points = &self.0;
        for i in 0..points.len() {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            if !(a.x == b.x || a.y == b.y) {
                return false;
            }
            if a.x == b.x && a.y == b.y {
                return false;
            }
        }
        true
    }

    /// Returns true if the polygon is defined clockwise, using the shoelace
    /// formula. ref: https://en.wikipedia.org/wiki/Shoelace_formula
    pub fn is_clockwise(&self) -> bool {
        let points = &self.0;

        assert!(points.len() >= 3, "need at least 3 vertices");

        let mut twice_area: i128 = 0; // use i128 to avoid overflow
        for (idx, point) in points.iter().enumerate() {
            let point_next = points[(idx + 1) % points.len()];
            let (x, y) = (point.x as i128, point.y as i128);
            let (x_next, y_next) = (point_next.x as i128, point_next.y as i128);
            twice_area += (x * y_next) - (x_next * y);
        }

        twice_area < 0
    }

    /// Returns true if the polygon starts with the leftmost vertical edge. In
    /// the case of a tie, make sure the lowest leftmost vertical edge is
    /// chosen.
    pub fn starts_with_leftmost_vertical_edge(&self) -> bool {
        let points = &self.0;

        assert!(points.len() >= 2, "need at least 2 vertices to determine if a polygon starts with the leftmost vertical edge");

        if points[0].x != points[1].x {
            // does not start with a vertical edge
            return false;
        }

        let first_x = points[0].x;
        let first_y = points[0].y.min(points[1].y);

        for idx in 1..points.len() {
            let point = points[idx];
            let point_next = points[(idx + 1) % points.len()];
            if point.x != point_next.x {
                // skip if not a vertical edge
                continue;
            } else if point.x > first_x {
                // skip if to the right of the first edge
                continue;
            } else if point.x < first_x {
                // return false if this edge is to the left of the first edge
                return false;
            } else if point.y.min(point_next.y) < first_y {
                // edges are both leftmost; make sure the first one is lower
                return false;
            }
        }

        true
    }

    pub fn num_vertices(&self) -> usize {
        self.0.len()
    }

    pub fn num_edges(&self) -> usize {
        self.0.len()
    }
}

impl PartialEq for Polygon {
    fn eq(&self, other: &Self) -> bool {
        let a = &self.0;
        let b = &other.0;
        if a.len() != b.len() {
            return false;
        }
        if a.is_empty() {
            return true;
        }
        // rotation-invariant equality: try aligning every index
        for start in 0..b.len() {
            let mut all_match = true;
            for (i, a_i) in a.iter().enumerate() {
                let j = (start + i) % b.len();
                if a_i != &b[j] {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                return true;
            }
        }
        false
    }
}

impl Eq for Polygon {}

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub coordinate: Coordinate,
    pub orientation: Orientation,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            coordinate: Coordinate { x: 0, y: 0 },
            orientation: Orientation::R0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct Mat3(pub nalgebra::Matrix3<i64>);

impl Mat3 {
    pub fn identity() -> Mat3 {
        Mat3(nalgebra::Matrix3::identity())
    }

    pub fn translate(dx: i64, dy: i64) -> Mat3 {
        Mat3(nalgebra::Matrix3::new(1, 0, dx, 0, 1, dy, 0, 0, 1))
    }

    pub fn from_orientation(o: Orientation) -> Mat3 {
        const ROTATE_90: Mat3 = Mat3(nalgebra::Matrix3::new(0, -1, 0, 1, 0, 0, 0, 0, 1));
        const MIRROR_X: Mat3 = Mat3(nalgebra::Matrix3::new(1, 0, 0, 0, -1, 0, 0, 0, 1));
        const MIRROR_Y: Mat3 = Mat3(nalgebra::Matrix3::new(-1, 0, 0, 0, 1, 0, 0, 0, 1));

        match o {
            Orientation::R0 => Self::identity(),
            Orientation::R90 => ROTATE_90,
            Orientation::R180 => &ROTATE_90 * &ROTATE_90,
            Orientation::R270 => &(&ROTATE_90 * &ROTATE_90) * &ROTATE_90,
            Orientation::MX => MIRROR_X,
            Orientation::MY => MIRROR_Y,
            Orientation::MX90 => &ROTATE_90 * &MIRROR_X,
            Orientation::MY90 => &ROTATE_90 * &MIRROR_Y,
        }
    }

    pub fn as_orientation(&self) -> Orientation {
        for orientation in [
            Orientation::R0,
            Orientation::R90,
            Orientation::R180,
            Orientation::R270,
            Orientation::MX,
            Orientation::MY,
            Orientation::MX90,
            Orientation::MY90,
        ] {
            let ref_mat = Self::from_orientation(orientation);
            if self.0.fixed_view::<2, 2>(0, 0) == ref_mat.0.fixed_view::<2, 2>(0, 0) {
                return orientation;
            }
        }
        panic!("Unsupported orientation: {self:?}");
    }

    pub fn as_coordinate(&self) -> Coordinate {
        Coordinate {
            x: self.0[(0, 2)],
            y: self.0[(1, 2)],
        }
    }

    pub fn from_orientation_then_translation(
        orientation: &Orientation,
        translation: &Coordinate,
    ) -> Mat3 {
        let orientation_transform = Mat3::from_orientation(*orientation);
        let translation_transform = Mat3::translate(translation.x, translation.y);

        &translation_transform * &orientation_transform
    }
}

impl std::ops::Mul<&Mat3> for &Mat3 {
    type Output = Mat3;
    fn mul(self, rhs: &Mat3) -> Mat3 {
        Mat3(self.0 * rhs.0)
    }
}

// LEF pin description from ModDef
#[derive(Debug, Clone)]
pub struct PhysicalPin {
    pub layer: String,
    pub position: Coordinate,
    pub polygon: Polygon,
}

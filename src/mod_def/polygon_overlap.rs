// SPDX-License-Identifier: Apache-2.0

use crate::Polygon;
use geo::{PreparedGeometry, Relate, coordinate_position::CoordPos, dimensions::Dimensions};
use rstar::{AABB, RTreeObject};
use std::{borrow::Borrow, cell::OnceCell};

impl Polygon {
    /// Returns whether two polygons share positive area, including when one contains the other.
    /// Touching at only an edge or vertex does not count.
    pub fn overlaps_in_area(&self, other: &Polygon) -> bool {
        OverlapPolygon::new(self).overlaps_in_area(&OverlapPolygon::new(other))
    }
}

/// A polygon with a cached R-tree envelope and a lazily prepared edge index.
pub(crate) struct OverlapPolygon<P> {
    envelope: AABB<[i64; 2]>,
    polygon: P,
    geometry: OnceCell<Box<PreparedGeometry<'static, geo::Polygon<f64>>>>,
}

impl<P: Borrow<Polygon>> OverlapPolygon<P> {
    /// Computes the bounding box without indexing any polygon edges.
    pub(crate) fn new(polygon: P) -> Self {
        let bbox = polygon.borrow().bbox();
        Self {
            envelope: AABB::from_corners([bbox.min_x, bbox.min_y], [bbox.max_x, bbox.max_y]),
            polygon,
            geometry: OnceCell::new(),
        }
    }

    /// Tests for shared interior, allowing edge and vertex contact.
    pub(crate) fn overlaps_in_area(&self, other: &Self) -> bool {
        // R-tree candidates include boxes that touch; shared area requires overlap on both axes.
        let (a, b) = (&self.envelope, &other.envelope);
        if a.upper()[0] <= b.lower()[0]
            || b.upper()[0] <= a.lower()[0]
            || a.upper()[1] <= b.lower()[1]
            || b.upper()[1] <= a.lower()[1]
        {
            return false;
        }
        if self.polygon.borrow().is_rectangular() && other.polygon.borrow().is_rectangular() {
            return true;
        }
        // TODO(sherbst) 2025-12-03: faster checking for rectilinear blocks?
        // this might also allow the geo crate dependency to be removed.
        self.geometry()
            .relate(other.geometry())
            .get(CoordPos::Inside, CoordPos::Inside)
            == Dimensions::TwoDimensional
    }

    /// Tests for any shared point, given bounding boxes that already intersect or touch.
    pub(super) fn intersects_candidate(&self, other: &Self) -> bool {
        if self.polygon.borrow().is_rectangular() && other.polygon.borrow().is_rectangular() {
            return true;
        }
        self.geometry().relate(other.geometry()).is_intersects()
    }

    /// Builds the edge index on the first comparison that needs it.
    fn geometry(&self) -> &PreparedGeometry<'static, geo::Polygon<f64>> {
        self.geometry
            .get_or_init(|| Box::new(self.polygon.borrow().to_geo_polygon_f64().into()))
    }
}

impl<P> RTreeObject for OverlapPolygon<P> {
    type Envelope = AABB<[i64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coordinate;

    #[test]
    fn shared_area_is_symmetric_and_excludes_boundary_contact() {
        let diamond = Polygon::new(vec![
            (0, 5).into(),
            (5, 10).into(),
            (10, 5).into(),
            (5, 0).into(),
        ]);
        assert!(diamond.overlaps_in_area(&diamond));

        for (x, y, overlaps) in [
            (5, 4, true),
            (6, 8, true),
            (7, 8, false),
            (8, 8, false),
            (20, 0, false),
        ] {
            let square = Polygon::from_width_height(2, 2) + Coordinate { x, y };
            assert_eq!(diamond.overlaps_in_area(&square), overlaps);
            assert_eq!(square.overlaps_in_area(&diamond), overlaps);
        }
    }
}

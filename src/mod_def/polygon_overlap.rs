// SPDX-License-Identifier: Apache-2.0

use crate::{BoundingBox, Polygon};
use geo::{PreparedGeometry, Relate, coordinate_position::CoordPos, dimensions::Dimensions};
use std::{borrow::Borrow, cell::OnceCell};

impl Polygon {
    /// Returns whether two polygons share positive area, including when one contains the other.
    /// Touching at only an edge or vertex does not count.
    pub fn overlaps_in_area(&self, other: &Polygon) -> bool {
        OverlapPolygon::new(self).overlaps_in_area(&OverlapPolygon::new(other))
    }
}

/// A polygon with cached bounds for repeated overlap queries.
pub(super) struct OverlapPolygon<P> {
    pub(super) bbox: BoundingBox,
    polygon: P,
    geometry: OnceCell<Box<PreparedGeometry<'static, geo::Polygon<f64>>>>,
}

impl<P: Borrow<Polygon>> OverlapPolygon<P> {
    /// Computes the bounding box without indexing any polygon edges.
    pub(super) fn new(polygon: P) -> Self {
        Self {
            bbox: polygon.borrow().bbox(),
            polygon,
            geometry: OnceCell::new(),
        }
    }

    /// Tests the bounds first, consulting the actual edges only for ambiguous pairs.
    pub(super) fn overlaps_in_area(&self, other: &Self) -> bool {
        if !self.bbox.intersects(&other.bbox) {
            return false;
        }
        if self.polygon.borrow().is_rectangular() && other.polygon.borrow().is_rectangular() {
            return true;
        }
        self.geometry()
            .relate(other.geometry())
            .get(CoordPos::Inside, CoordPos::Inside)
            == Dimensions::TwoDimensional
    }

    /// Builds the edge index on the first comparison that needs it.
    fn geometry(&self) -> &PreparedGeometry<'static, geo::Polygon<f64>> {
        self.geometry
            .get_or_init(|| Box::new(self.polygon.borrow().to_geo_polygon_f64().into()))
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

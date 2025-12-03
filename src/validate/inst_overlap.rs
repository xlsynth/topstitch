// SPDX-License-Identifier: Apache-2.0

use crate::Polygon;
use geo::algorithm::{area::Area, bool_ops::BooleanOps};
use rstar::{AABB, RTree, RTreeObject};

struct BBoxWrapper {
    index: usize,
    envelope: AABB<[i64; 2]>,
}

impl RTreeObject for BBoxWrapper {
    type Envelope = AABB<[i64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

pub fn check(insts: &[(String, Polygon)]) {
    let num_insts = insts.len();

    if num_insts < 2 {
        return;
    }

    // Bounding boxes for each instance
    let bboxes = insts
        .iter()
        .map(|(_, poly)| poly.bbox())
        .collect::<Vec<_>>();

    // rstar representation of each bounding box
    let aabbs = bboxes
        .iter()
        .map(|bbox| AABB::from_corners([bbox.min_x, bbox.min_y], [bbox.max_x, bbox.max_y]))
        .collect::<Vec<_>>();

    // rtree of all bounding boxes, maintaining the original index for each
    let rtree = RTree::bulk_load(
        aabbs
            .iter()
            .enumerate()
            .map(|(index, aabb)| BBoxWrapper {
                index,
                envelope: *aabb,
            })
            .collect::<Vec<_>>(),
    );

    // memoized geo polygons for each instance. only created when intersection is
    // ambiguous from bounding box calculations.
    let mut insts_geo: Vec<Option<geo::Polygon<f64>>> = vec![None; num_insts];

    // function for producing standarized panic message for overlapping instances
    let panic_message = |i: usize, j: usize| {
        panic!("Instances {} and {} overlap", insts[i].0, insts[j].0);
    };

    // loop over all instances
    for (i, aabb) in aabbs.iter().enumerate() {
        // find instances flagged as intersecting by rtree
        let candidates = rtree.locate_in_envelope_intersecting(aabb);

        // for each intersecting instance, determine if it actually overlaps
        for candidate in candidates {
            let j = candidate.index;

            if j == i {
                // skip self
                continue;
            }

            if !bboxes[i].intersects(&bboxes[j]) {
                // skip abutted bounding boxes. this test is necessary because the rtree counts
                // abutted bounding boxes as intersecting.
                continue;
            }

            if insts[i].1.is_rectangular() == insts[j].1.is_rectangular() {
                // if bounding boxes intersect and both polygons are rectangles, then they
                // certainly overlap, so no further checking is needed.
                panic_message(i, j);
            }

            // TODO(sherbst) 2025-12-03: faster checking for rectilinear blocks?
            // this might also allow the geo crate dependency to be removed.

            // look up or create geo polygons for each instance to allow for a fully general
            // polygon intersection check
            if insts_geo[i].is_none() {
                insts_geo[i] = Some(insts[i].1.to_geo_polygon());
            }
            if insts_geo[j].is_none() {
                insts_geo[j] = Some(insts[j].1.to_geo_polygon());
            }

            // compute the intersection area of the two polygons to determine if they
            // actually overlap. this is necessary because geo considers abutted polygons
            // to intersect.
            if insts_geo[i]
                .as_ref()
                .unwrap()
                .intersection(insts_geo[j].as_ref().unwrap())
                .unsigned_area()
                > 0.0
            {
                panic_message(i, j);
            }
        }
    }
}

mod tests {
    use super::*;
    use crate::BoundingBox;

    #[test]
    fn test_basic_no_overlap() {
        let insts = [
            (
                "inst1".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 10,
                    max_y: 10,
                }),
            ),
            (
                "inst2".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 20,
                    min_y: 20,
                    max_x: 30,
                    max_y: 30,
                }),
            ),
        ];
        check(&insts);
    }

    #[test]
    fn test_basic_shared_corner() {
        let insts = [
            (
                "inst1".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 10,
                    max_y: 10,
                }),
            ),
            (
                "inst2".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 10,
                    min_y: 10,
                    max_x: 20,
                    max_y: 20,
                }),
            ),
        ];
        check(&insts);
    }

    #[test]
    fn test_basic_shared_edge() {
        let insts = [
            (
                "inst1".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 10,
                    max_y: 10,
                }),
            ),
            (
                "inst2".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 10,
                    min_y: 0,
                    max_x: 20,
                    max_y: 10,
                }),
            ),
        ];
        check(&insts);
    }

    #[test]
    #[should_panic(expected = "Instances inst1 and inst2 overlap")]
    fn test_basic_partial_overlap() {
        let insts = [
            (
                "inst1".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 10,
                    max_y: 10,
                }),
            ),
            (
                "inst2".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 5,
                    min_y: 5,
                    max_x: 15,
                    max_y: 15,
                }),
            ),
        ];
        check(&insts);
    }

    #[test]
    #[should_panic(expected = "Instances inst1 and inst2 overlap")]
    fn test_basic_full_overlap() {
        let insts = [
            (
                "inst1".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 1,
                    min_y: 1,
                    max_x: 9,
                    max_y: 9,
                }),
            ),
            (
                "inst2".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 10,
                    max_y: 10,
                }),
            ),
        ];
        check(&insts);
    }

    #[test]
    fn test_performance_many_instances() {
        use std::time::Instant;

        let mut insts = Vec::new();

        for i in 0..300 {
            for j in 0..300 {
                insts.push((
                    format!("inst_{i}_{j}"),
                    Polygon::from_bbox(&BoundingBox {
                        min_x: i,
                        min_y: j,
                        max_x: i + 1,
                        max_y: j + 1,
                    }),
                ));
            }
        }

        let start = Instant::now();
        check(&insts);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_secs() < 2,
            "Performance test took too long: {elapsed:?}"
        );
    }
}

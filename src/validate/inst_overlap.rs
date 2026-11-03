// SPDX-License-Identifier: Apache-2.0

use crate::{Polygon, mod_def::OverlapPolygon};
use rstar::{RTree, primitives::GeomWithData};

/// Checks placed instances for shared area; abutting edges and corners are allowed.
pub fn check(insts: &[(String, Polygon)]) {
    if insts.len() < 2 {
        return;
    }

    let tree = RTree::bulk_load(
        insts
            .iter()
            .enumerate()
            .map(|(i, (_, polygon))| GeomWithData::new(OverlapPolygon::new(polygon), i))
            .collect(),
    );

    for (a, b) in tree.intersection_candidates_with_other_tree(&tree) {
        let (i, j) = (a.data, b.data);
        // The self-query yields each instance with itself and both orders of each pair.
        if i < j && a.geom().overlaps_in_area(b.geom()) {
            panic!("Instances {} and {} overlap", insts[i].0, insts[j].0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::{BoundingBox, Polygon};
    use rstest::rstest;

    fn rectilinear_pair(offset_x: i64, offset_y: i64) -> Vec<(String, Polygon)> {
        let a = Polygon::new(vec![
            (0, 0).into(),
            (0, 200).into(),
            (100, 200).into(),
            (100, 100).into(),
            (200, 100).into(),
            (200, 0).into(),
        ]);

        let b = Polygon::new(vec![
            (0, 100).into(),
            (0, 200).into(),
            (200, 200).into(),
            (200, 0).into(),
            (100, 0).into(),
            (100, 100).into(),
        ]);

        let b = b + (offset_x, offset_y).into();

        vec![("inst1".to_string(), a), ("inst2".to_string(), b)]
    }

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

    #[rstest]
    #[case(100, 0)]
    #[case(101, 0)]
    fn placement_rectilinear_ok(#[case] offset_x: i64, #[case] offset_y: i64) {
        check(&rectilinear_pair(offset_x, offset_y));
    }

    #[rstest]
    #[case(99, 0)]
    #[case(99, -1)]
    #[case(100, -1)]
    #[should_panic]
    fn placement_rectilinear_overlap(#[case] offset_x: i64, #[case] offset_y: i64) {
        check(&rectilinear_pair(offset_x, offset_y));
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "timed in release mode")]
    fn test_performance_many_instances() {
        use std::time::{Duration, Instant};

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
        eprintln!("Instance overlap check took {elapsed:?}");

        assert!(
            elapsed < Duration::from_millis(300),
            "Performance test took too long: {elapsed:?}"
        );
    }
}

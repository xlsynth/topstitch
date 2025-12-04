// SPDX-License-Identifier: Apache-2.0

use crate::Polygon;
use geo::relate::Relate;

pub fn check(name: &str, shape: &Polygon, pins: &[(String, Polygon)]) {
    if pins.is_empty() {
        return;
    }

    let shape_bbox = shape.bbox();
    let shape_is_rectangular = shape.is_rectangular();

    let shape = shape.to_geo_polygon_f64();

    for (pin_name, pin_poly) in pins {
        // fast test if the ModDef shape and pin shape are both rectangular
        if shape_is_rectangular && pin_poly.is_rectangular() && shape_bbox.covers(&pin_poly.bbox())
        {
            continue;
        }

        // TODO(sherbst) 2025-12-03: faster test for rectilinear shapes?

        let pin_geo = pin_poly.to_geo_polygon_f64();

        let intersection = shape.relate(&pin_geo);
        if !intersection.is_covers() {
            panic!("Pin {} is not contained within {}", pin_name, name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check;
    use crate::{BoundingBox, Polygon};

    #[test]
    fn test_basic_on_edge() {
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10,
            }),
            &[(
                "xyz[0]".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 0,
                    min_y: 0,
                    max_x: 1,
                    max_y: 1,
                }),
            )],
        );
    }

    #[test]
    fn test_basic_in_interior() {
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10,
            }),
            &[(
                "xyz[0]".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 1,
                    min_y: 1,
                    max_x: 2,
                    max_y: 2,
                }),
            )],
        );
    }

    #[test]
    #[should_panic(expected = "Pin xyz[0] is not contained within TestMod")]
    fn test_basic_outside() {
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10,
            }),
            &[(
                "xyz[0]".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 20,
                    min_y: 20,
                    max_x: 21,
                    max_y: 21,
                }),
            )],
        );
    }

    #[test]
    #[should_panic(expected = "Pin xyz[0] is not contained within TestMod")]
    fn test_basic_outside_edge() {
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10,
            }),
            &[(
                "xyz[0]".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 10,
                    min_y: 5,
                    max_x: 11,
                    max_y: 6,
                }),
            )],
        );
    }

    #[test]
    #[should_panic(expected = "Pin xyz[0] is not contained within TestMod")]
    fn test_basic_partially_outside() {
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: 10,
                max_y: 10,
            }),
            &[(
                "xyz[0]".to_string(),
                Polygon::from_bbox(&BoundingBox {
                    min_x: 9,
                    min_y: 9,
                    max_x: 11,
                    max_y: 11,
                }),
            )],
        );
    }

    #[test]
    fn test_performance_many_pins() {
        use std::time::Instant;

        let mut pins = Vec::new();

        const ROWS: i64 = 300;
        const COLS: i64 = 300;

        for i in 0..ROWS {
            for j in 0..COLS {
                pins.push((
                    format!("pin[{i}][{j}]"),
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
        check(
            "TestMod",
            &Polygon::from_bbox(&BoundingBox {
                min_x: 0,
                min_y: 0,
                max_x: ROWS,
                max_y: COLS,
            }),
            &pins,
        );
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_secs() < 1,
            "Performance test took too long: {elapsed:?}"
        );
    }
}

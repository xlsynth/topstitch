// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};
use topstitch::{LefDefOptions, ModDef, Orientation, Polygon, Usage};

#[test]
fn many_neighbors_of_one_staircase() {
    const STEPS: i64 = 2048;
    const GRID: i64 = 32;
    let top = ModDef::new("top");
    let stair = staircase(STEPS);
    top.instantiate(&stair, Some("outline"), None)
        .place((0, 0), Orientation::R0);

    let square = ModDef::new("square");
    square.set_usage(Usage::EmitStubAndStop);
    square.set_width_height(1, 1);
    // Each square is inside the bounding box, but above the stair's diagonal.
    let corner = 3 * STEPS / 4;
    for i in 0..GRID * GRID {
        top.instantiate(&square, Some(&format!("unit_{i}")), None)
            .place(
                (corner + 2 * (i % GRID), corner + 2 * (i / GRID)),
                Orientation::R0,
            );
    }

    check_within(&top, 1 + (GRID * GRID) as usize, Duration::from_secs(5));
}

#[test]
fn staircases_touch_along_a_long_boundary() {
    const STEPS: i64 = 128;
    const PAIRS: usize = 16;
    let top = ModDef::new("top");
    let stair = staircase(STEPS);
    for i in 0..PAIRS {
        let x = i as i64 * (STEPS + 4);
        top.instantiate(&stair, Some(&format!("lower_{i}")), None)
            .place((x, 0), Orientation::R0);
        // A half turn and this offset make the step edges coincide without sharing area.
        top.instantiate(&stair, Some(&format!("upper_{i}")), None)
            .place((x + STEPS, STEPS + 1), Orientation::R180);
    }

    check_within(&top, 2 * PAIRS, Duration::from_secs(1));
}

/// A rectilinear triangle with unit steps on the diagonal.
fn staircase(steps: i64) -> ModDef {
    let mut vertices = vec![(0, 0).into(), (0, steps).into()];
    for x in 1..=steps {
        vertices.push((x, steps + 1 - x).into());
        vertices.push((x, steps - x).into());
    }
    let stair = ModDef::new("stair");
    stair.set_usage(Usage::EmitStubAndStop);
    stair.set_shape(Polygon::new(vertices));
    stair
}

/// Time a full placement check on a collision-free layout, excluding fixture construction.
fn check_within(top: &ModDef, instances: usize, budget: Duration) {
    let opts = LefDefOptions {
        check_for_instance_overlaps: true,
        ..Default::default()
    };
    let start = Instant::now();
    let (placements, _) = top.collect_placements_and_mod_defs(&opts);
    let elapsed = start.elapsed();
    assert_eq!(placements.len(), instances);
    eprintln!("Checked {instances} instances in {elapsed:?}");
    assert!(
        elapsed < budget,
        "Placement check took {elapsed:?} (budget {budget:?})"
    );
}

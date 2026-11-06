// SPDX-License-Identifier: Apache-2.0

//! Run with `cargo test --release --test def_export_performance -- --nocapture`.

use std::time::{Duration, Instant};
use topstitch::{LefDefOptions, ModDef, ModInst, Orientation, Usage};

#[test]
fn export_hierarchical_grid() {
    // Keep debug CI small; use the full 65,536-cell grid for release measurements.
    const SIDE: i64 = if cfg!(debug_assertions) { 32 } else { 256 };
    let opts = LefDefOptions {
        // Measure DEF export separately from the overlap checker.
        check_for_instance_overlaps: false,
        ..Default::default()
    };

    let emit = |top: ModDef, label: &str| {
        let start = Instant::now();
        let def = top.emit_def(&opts);
        let elapsed = start.elapsed();
        eprintln!("{label}: {SIDE}x{SIDE} DEF export took {elapsed:?}");
        if !cfg!(debug_assertions) {
            const BUDGET: Duration = Duration::from_millis(175);
            assert!(
                elapsed <= BUDGET,
                "{label}: DEF export took {elapsed:?}, exceeding {BUDGET:?}"
            );
        }
        def
    };

    let shared = emit(grid(SIDE, true), "shared child placements");
    let top = grid(SIDE, false);
    place_rows_from_root(
        top.get_instance("bottom"),
        top.get_instance("top"),
        SIDE / 2,
    );
    let hierarchical = emit(top, "per-occurrence placements from the root");

    assert_eq!(
        shared.matches(" cell + PLACED ").count(),
        (SIDE * SIDE) as usize
    );
    let last = SIDE - 1;
    assert!(shared.contains(&format!(" cell + PLACED ( {last} {last} ) N ;")));
    assert!(
        shared == hierarchical,
        "The two placement styles should emit identical DEFs"
    );
}

/// Build each square from two rows, and each row from two copies of the smaller square.
fn grid(side: i64, place_children: bool) -> ModDef {
    let mut tile = ModDef::new("cell");
    tile.set_width_height(1, 1);
    tile.set_usage(Usage::EmitStubAndStop);

    let mut span = 1;
    while span < side {
        for (kind, first, second, offset) in [
            ("row", "left", "right", (span, 0)),
            ("square", "bottom", "top", (0, span)),
        ] {
            let parent = ModDef::new(format!("{kind}_{span}"));
            for (name, xy) in [(first, (0, 0)), (second, offset)] {
                let child = parent.instantiate(&tile, Some(name), None);
                if place_children {
                    child.place(xy, Orientation::R0);
                }
            }
            tile = parent;
        }
        span *= 2;
    }
    tile.set_width_height(side, side);
    tile
}

/// Place both rows and their descendants through full paths, using offsets in each immediate parent.
fn place_rows_from_root(bottom: ModInst, top: ModInst, half_side: i64) {
    for (row, y) in [(bottom, 0), (top, half_side)] {
        row.place((0, y), Orientation::R0);
        for (name, x) in [("left", 0), ("right", half_side)] {
            let tile = row.get_instance(name);
            tile.place((x, 0), Orientation::R0);
            if half_side > 1 {
                place_rows_from_root(
                    tile.get_instance("bottom"),
                    tile.get_instance("top"),
                    half_side / 2,
                );
            }
        }
    }
}

// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    BoundingBox, IO, LefDefOptions, ModDef, Polygon, Range, SpreadPinsOptions, TrackDefinition,
    TrackDefinitions, TrackOrientation,
};

const NUM_BITS: usize = 5;
const MODULE_HEIGHT: i64 = 200;
const MODULE_WIDTH: i64 = 200;
const TRACK_PITCH: i64 = 20;
const PIN_WIDTH: i64 = TRACK_PITCH / 2;
const PIN_DEPTH: i64 = 20;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    let m = ModDef::new("SpreadPins");

    // Track definitions
    let pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: -PIN_WIDTH / 2,
        min_y: 0,
        max_x: PIN_WIDTH / 2,
        max_y: PIN_DEPTH,
    });

    let mut track_definitions = TrackDefinitions::new();
    let tracks = [
        ("M1", TRACK_PITCH / 2, TrackOrientation::Horizontal),
        ("M2", TRACK_PITCH / 2, TrackOrientation::Vertical),
        ("M3", 3 * TRACK_PITCH / 4, TrackOrientation::Horizontal),
        ("M4", 3 * TRACK_PITCH / 4, TrackOrientation::Vertical),
    ];

    for (name, offset, orientation) in tracks {
        track_definitions.add_track(TrackDefinition::new(
            name,
            offset,
            TRACK_PITCH,
            orientation,
            Some(pin_shape.clone()),
            None,
        ));
    }

    // Define module geometry and apply track definitions
    m.set_shape(Polygon::from_width_height(MODULE_WIDTH, MODULE_HEIGHT));
    m.set_track_definitions(track_definitions);

    // Define ports
    let in_left = m.add_port("in_left", IO::Input(NUM_BITS));
    let out_right = m.add_port("out_right", IO::Output(NUM_BITS));
    let in_top = m.add_port("in_top", IO::Input(NUM_BITS));
    let out_bottom = m.add_port("out_bottom", IO::Output(NUM_BITS));

    let layers = m.get_layers();

    // Variety of different mechanisms shown for spreading pins on edges
    in_left.spread_pins_on_left_edge(
        &layers,
        SpreadPinsOptions {
            range: Range::from_min(MODULE_HEIGHT / 2),
            ..Default::default()
        },
    )?;
    m.spread_pins_on_right_edge(
        &out_right.to_bits(),
        &layers,
        SpreadPinsOptions {
            ..Default::default()
        },
    )?;
    in_top.spread_pins_on_top_edge(
        &layers,
        SpreadPinsOptions {
            range: Range::new(MODULE_WIDTH / 4, 3 * MODULE_WIDTH / 4),
            ..Default::default()
        },
    )?;
    out_bottom.spread_pins_on_bottom_edge(
        &layers,
        SpreadPinsOptions {
            range: Range::new(MODULE_WIDTH / 4, 3 * MODULE_WIDTH / 4),
            ..Default::default()
        },
    )?;

    // Emit LEF for viewing
    let lef_path = out_dir.join("pin_range_example.lef");
    m.emit_lef_to_file(&lef_path, &LefDefOptions::default())
        .expect("emit LEF");

    Ok(())
}

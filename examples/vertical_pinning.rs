// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    BoundingBox, IO, LefDefOptions, ModDef, Polygon, SpreadPinsOptions, TrackDefinition,
    TrackDefinitions, TrackOrientation, Usage,
};

const NUM_PINS: usize = 8;
const PIN_WIDTH: i64 = 10;
const PIN_DEPTH: i64 = 20;
const TRACK_OFFSET: i64 = PIN_WIDTH;
const TRACK_PERIOD: i64 = 2 * PIN_WIDTH;

// width needed to use all tracks
const WIDTH: i64 = TRACK_PERIOD * (NUM_PINS as i64);

// width needed to skip one track
// const WIDTH: i64 = TRACK_PERIOD + (2 * TRACK_PERIOD * (NUM_PINS as i64 - 1));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    // Track definitions
    let mut track_definitions = TrackDefinitions::new();

    // No keepout shape when using all tracks
    let keepout_shape = None;

    // Keepout shape to skip every other track
    // let keepout_shape = Some(Polygon::from_bbox(&BoundingBox {
    //     min_x: -3 * TRACK_PERIOD / 2,
    //     min_y: 0,
    //     max_x: 3 * TRACK_PERIOD / 2,
    //     max_y: PIN_DEPTH,
    // }));

    track_definitions.add_track(TrackDefinition::new(
        "M1",
        TRACK_OFFSET,
        TRACK_PERIOD,
        TrackOrientation::Vertical,
        Some(Polygon::from_bbox(&BoundingBox {
            min_x: -PIN_WIDTH / 2,
            min_y: 0,
            max_x: PIN_WIDTH / 2,
            max_y: PIN_DEPTH,
        })),
        keepout_shape,
    ));

    // Define the module
    let m = ModDef::new("A");
    m.set_shape(Polygon::from_width_height(
        WIDTH,
        (NUM_PINS as i64) * PIN_WIDTH,
    ));
    m.set_track_definitions(track_definitions.clone());

    // Mark as a leaf for LEF/DEF emission
    m.set_usage(Usage::EmitStubAndStop);

    // Input pins
    let m_in = m.add_port("in", IO::Input(NUM_PINS));
    m_in.spread_pins_on_bottom_edge(
        m.get_layers(),
        SpreadPinsOptions {
            ..Default::default()
        },
    )?;

    // Output pins
    let m_out = m.add_port("out", IO::Output(NUM_PINS));
    m_out.spread_pins_on_top_edge(
        m.get_layers(),
        SpreadPinsOptions {
            ..Default::default()
        },
    )?;

    let top = m.wrap(None, None);

    // Emit LEF/DEF for viewing
    let lef_path = out_dir.join("vertical_pinning.lef");
    let def_path = out_dir.join("vertical_pinning.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &LefDefOptions::default())
        .expect("emit LEF/DEF");

    Ok(())
}

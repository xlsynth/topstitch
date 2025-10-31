// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    BoundingBox, Coordinate, LefDefOptions, ModDef, Orientation, Polygon, Range, SpreadPinsOptions,
    TrackDefinition, TrackDefinitions, TrackOrientation, Usage,
};

const WIDTH: i64 = 100;
const HEIGHT: i64 = 4 * PIN_WIDTH * (NUM_PINS as i64);
const NUM_PINS: usize = 2;
const PIN_WIDTH: i64 = 10;
const PIN_DEPTH: i64 = 20;

fn build_leaf(
    name: &str,
    track_definitions: &TrackDefinitions,
    shape: Polygon,
) -> Result<ModDef, Box<dyn std::error::Error>> {
    let m = ModDef::new(name);
    m.set_shape(shape);
    m.set_track_definitions(track_definitions.clone());

    m.feedthrough("in", "out", NUM_PINS);

    // Mark as a leaf for LEF/DEF emission
    m.set_usage(Usage::EmitStubAndStop);
    Ok(m)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    // Track definitions
    let mut track_definitions = TrackDefinitions::new();
    let pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: 0,
        min_y: -PIN_WIDTH / 2,
        max_x: PIN_DEPTH,
        max_y: PIN_WIDTH / 2,
    });
    track_definitions.add_track(TrackDefinition::new(
        "M1",
        PIN_WIDTH,
        2 * PIN_WIDTH,
        TrackOrientation::Horizontal,
        Some(pin_shape.clone()),
        None,
    ));

    // Build A and B
    let a = build_leaf(
        "A",
        &track_definitions,
        Polygon::from_width_height(WIDTH, HEIGHT),
    )?;
    let b = build_leaf(
        "B",
        &track_definitions,
        Polygon::new(vec![
            Coordinate { x: 0, y: 0 },
            Coordinate {
                x: 0,
                y: 5 * HEIGHT / 4,
            },
            Coordinate {
                x: WIDTH / 2,
                y: 5 * HEIGHT / 4,
            },
            Coordinate {
                x: WIDTH / 2,
                y: HEIGHT,
            },
            Coordinate {
                x: WIDTH,
                y: HEIGHT,
            },
            Coordinate { x: WIDTH, y: 0 },
        ]),
    )?;

    let top = ModDef::new("Top");
    let a_instance = top.instantiate(&a, Some("a_inst"), None);
    let b_instance = top.instantiate(&b, Some("b_inst"), None);

    // Coordinates chosen to cover positive and negative x and y values
    a_instance.place((-40, -40), Orientation::R0);
    b_instance.place(((2 * WIDTH) - 20, -20), Orientation::MY);

    // Pin inputs and outputs
    a.get_port("in").spread_pins_on_left_edge(
        a.get_layers(),
        SpreadPinsOptions {
            range: Range::new(HEIGHT / 4, 3 * HEIGHT / 4),
            ..Default::default()
        },
    )?;

    let a_in = a_instance.get_port("in");
    let a_out = a_instance.get_port("out");
    let b_in = b_instance.get_port("in");
    let b_out = b_instance.get_port("out");

    b_in.connect(&a_out);

    a_out.place_across_from(a_in);
    b_in.place_abutted();
    b_out.place_across_from(b_in);

    // Emit LEF/DEF for viewing
    let lef_path = out_dir.join("derived_pins.lef");
    let def_path = out_dir.join("derived_pins.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &LefDefOptions::default())
        .expect("emit LEF/DEF");

    Ok(())
}

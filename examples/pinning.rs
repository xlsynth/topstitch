// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    BoundingBox, LefDefOptions, ModDef, Orientation, Polygon, SpreadPinsOptions, TrackDefinition,
    TrackDefinitions, TrackOrientation, Usage, IO,
};

const A_WIDTH: i64 = 50;
const A_NUM_PINS: usize = 4;
const B_NUM_PINS: usize = 8;
const C_NUM_PINS: usize = 2;
const TOTAL_PINS: usize = B_NUM_PINS;
const B_WIDTH: i64 = 150;
const C_WIDTH: i64 = 100;
const PIN_WIDTH: i64 = 10;
const PIN_DEPTH: i64 = 20;

// Build a simple leaf module with pins on the left and right edges.
fn build_leaf(
    name: &str,
    width: i64,
    num_pins: usize,
    track_definitions: &TrackDefinitions,
) -> Result<ModDef, Box<dyn std::error::Error>> {
    // Define the module
    let m = ModDef::new(name);
    m.set_shape(Polygon::from_width_height(
        width,
        (num_pins as i64) * PIN_WIDTH,
    ));
    m.set_track_definitions(track_definitions.clone());
    m.set_layer(format!("OUTLINE_{}", name.to_uppercase()));

    // Input and output ports
    m.add_port("in", IO::Input(num_pins));
    m.add_port("out", IO::Output(num_pins));

    // Pin inputs and outputs
    let layers = m.get_layers();
    m.get_port("in").spread_pins_on_left_edge(
        &layers,
        SpreadPinsOptions {
            ..Default::default()
        },
    )?;
    m.get_port("out").spread_pins_on_right_edge(
        &layers,
        SpreadPinsOptions {
            ..Default::default()
        },
    )?;

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
    for layer in 0..2 {
        track_definitions.add_track(TrackDefinition::new(
            format!("M{}", layer + 1),
            PIN_WIDTH,
            2 * PIN_WIDTH,
            TrackOrientation::Horizontal,
            Some(pin_shape.clone()),
            None,
        ));
    }

    // Build A, B, and C modules
    let a = build_leaf("A", A_WIDTH, A_NUM_PINS, &track_definitions)?;
    let b = build_leaf("B", B_WIDTH, B_NUM_PINS, &track_definitions)?;
    let c = build_leaf("C", C_WIDTH, C_NUM_PINS, &track_definitions)?;

    let a_height = a.bbox().unwrap().get_height();
    let c_height = c.bbox().unwrap().get_height();

    // Top-level that places instances of A, B, and C
    let top = ModDef::new("Top");
    let a_instances = top.instantiate_array(&a, &[TOTAL_PINS / A_NUM_PINS], None, None);
    let b_instance = top.instantiate(&b, None, None);
    let c_instances = top.instantiate_array(&c, &[TOTAL_PINS / C_NUM_PINS], None, None);

    let b_inputs = b_instance.get_port("in").subdivide(a_instances.len());
    for (i, a_inst) in a_instances.iter().enumerate() {
        a_inst.place((0, (i as i64) * a_height), Orientation::R0);
        a_inst.get_port("out").connect(&b_inputs[i]);
    }

    b_instance.place((A_WIDTH, 0), Orientation::R0);

    let b_outputs = b_instance.get_port("out").subdivide(c_instances.len());
    for (i, c_inst) in c_instances.iter().enumerate() {
        c_inst.place((A_WIDTH + B_WIDTH, (i as i64) * c_height), Orientation::R0);
        c_inst.get_port("in").connect(&b_outputs[i]);
    }

    // Emit LEF/DEF for viewing
    let lef_path = out_dir.join("pinning.lef");
    let def_path = out_dir.join("pinning.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &LefDefOptions::default())
        .expect("emit LEF/DEF");

    Ok(())
}

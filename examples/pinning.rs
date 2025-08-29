// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{BoundingBox, LefDefOptions, Mat3, ModDef, Orientation, Polygon, Usage, IO};

const A_WIDTH: i64 = 100;
const A_NUM_PINS: usize = 4;
const B_NUM_PINS: usize = 8;
const C_NUM_PINS: usize = 2;
const TOTAL_PINS: usize = B_NUM_PINS;
const B_WIDTH: i64 = 300;
const C_WIDTH: i64 = 200;
const PIN_LAYER: &str = "M1";
const PIN_WIDTH: i64 = 10;
const PIN_DEPTH: i64 = 20;

// Build a simple leaf module with pins on the left and right edges.
fn build_leaf(name: &str, width: i64, num_pins: usize) -> ModDef {
    // Define the module
    let m = ModDef::new(name);
    m.set_shape(Polygon::from_width_height(
        width,
        2 * (num_pins as i64) * PIN_WIDTH,
    ));
    m.set_layer(format!("OUTLINE_{}", name.to_uppercase()));

    // Input and output ports
    m.add_port("in", IO::Input(num_pins));
    m.add_port("out", IO::Output(num_pins));

    // Define pin shape
    let pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: 0,
        min_y: -PIN_WIDTH / 2,
        max_x: PIN_DEPTH,
        max_y: PIN_WIDTH / 2,
    });

    // Define input pins on left edge (layer M1)
    let flipped = pin_shape.apply_transform(&Mat3::from_orientation(Orientation::MY));
    for i in 0..num_pins {
        let y = PIN_WIDTH + (i as i64) * (2 * PIN_WIDTH);
        m.get_port("in")
            .bit(i)
            .define_physical_pin(PIN_LAYER, (0, y).into(), pin_shape.clone());
        m.get_port("out")
            .bit(i)
            .define_physical_pin(PIN_LAYER, (width, y).into(), flipped.clone());
    }

    // Mark as a leaf for LEF/DEF emission
    m.set_usage(Usage::EmitStubAndStop);
    m
}

fn main() {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    // Build A, B, and C modules
    let a = build_leaf("A", A_WIDTH, A_NUM_PINS);
    let b = build_leaf("B", B_WIDTH, B_NUM_PINS);
    let c = build_leaf("C", C_WIDTH, C_NUM_PINS);

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
}

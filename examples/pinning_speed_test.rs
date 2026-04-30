// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    BoundingBox, IO, LefDefOptions, ModDef, Orientation, Polygon, Range, SpreadPinsOptions,
    TrackDefinition, TrackDefinitions, TrackOrientation, Usage,
};

const BIT_COUNT: usize = 8000;
const TOTAL_PIN_COUNT: usize = BIT_COUNT;
const HIERARCHY_LEVELS: usize = 10;
const METAL_LAYER_COUNT: usize = 1;
const PIN_SIZE: i64 = 20;
const PIN_GAP: i64 = 10;
const TRACK_OFFSET: i64 = 15;
const PIN_PITCH: i64 = PIN_SIZE + PIN_GAP;
const MODULE_WIDTH: i64 = 120;
const MODULE_HEIGHT: i64 = (TOTAL_PIN_COUNT.div_ceil(METAL_LAYER_COUNT) as i64) * PIN_PITCH;
const BLOCK_GAP: i64 = 40;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    let pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: -PIN_SIZE / 2,
        min_y: 0,
        max_x: PIN_SIZE / 2,
        max_y: PIN_SIZE,
    });

    let mut track_definitions = TrackDefinitions::new();
    for layer in 0..METAL_LAYER_COUNT {
        track_definitions.add_track(TrackDefinition::new(
            format!("M{}", layer + 1),
            TRACK_OFFSET,
            PIN_PITCH,
            TrackOrientation::Horizontal,
            Some(pin_shape.clone()),
            None,
        ));
    }

    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);
    a.set_shape(Polygon::from_width_height(MODULE_WIDTH, MODULE_HEIGHT));
    a.set_track_definitions(track_definitions.clone());
    a.add_port("out", IO::Output(BIT_COUNT));
    a.get_port("out").spread_pins_on_right_edge(
        a.get_layers(),
        SpreadPinsOptions {
            range: Range::new(PIN_GAP, MODULE_HEIGHT - PIN_GAP),
            ..Default::default()
        },
    )?;

    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);
    b.set_shape(Polygon::from_width_height(MODULE_WIDTH, MODULE_HEIGHT));
    b.set_track_definitions(track_definitions);
    b.add_port("in", IO::Input(BIT_COUNT));

    let base_name = if HIERARCHY_LEVELS == 0 {
        "Top".to_string()
    } else {
        "Top_level_0".to_string()
    };
    let base = ModDef::new(&base_name);
    let a_inst = base.instantiate(&a, Some("a_inst"), None);
    let b_inst = base.instantiate(&b, Some("b_inst"), None);

    a_inst.place((0, 0), Orientation::R0);
    b_inst.place((MODULE_WIDTH + BLOCK_GAP, 0), Orientation::R0);

    a_inst.get_port("out").connect(&b_inst.get_port("in"));
    b_inst.get_port("in").place_abutted();

    let mut top = base;
    for level in 0..HIERARCHY_LEVELS {
        let wrapper_name = if level + 1 == HIERARCHY_LEVELS {
            "Top".to_string()
        } else {
            format!("Top_level_{}", level + 1)
        };
        let wrapper = ModDef::new(&wrapper_name);
        let wrapper_inst = wrapper.instantiate(&top, Some(&format!("wrap_{level}_inst")), None);
        wrapper_inst.place((0, 0), Orientation::R0);
        top = wrapper;
    }

    let opts = LefDefOptions::default();
    let lef_path = out_dir.join("pinning_speed_test.lef");
    let def_path = out_dir.join("pinning_speed_test.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &opts)?;

    eprintln!("Emitted LEF: {}", lef_path.display());
    eprintln!("Emitted DEF: {}", def_path.display());

    Ok(())
}

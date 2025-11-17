// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{BoundingBox, LefDefOptions, ModDef, Orientation, PhysicalPin, Polygon, Usage, IO};

const PIN_ROWS: usize = 2;
const PIN_COLUMNS: usize = 2;
const SMALL_PIN_WIDTH: i64 = 20;
const SMALL_PIN_HEIGHT: i64 = 20;
const LARGE_PIN_WIDTH: i64 = 40;
const LARGE_PIN_HEIGHT: i64 = 40;
const OVERLAP_WIDTH: i64 = ((2 * (PIN_COLUMNS as i64)) + 1) * LARGE_PIN_WIDTH;
const OVERLAP_HEIGHT: i64 = ((2 * (PIN_ROWS as i64)) + 1) * LARGE_PIN_HEIGHT;
const BLOCK_A_WIDTH: i64 = 120 + OVERLAP_WIDTH;
const BLOCK_A_HEIGHT: i64 = 120 + OVERLAP_HEIGHT;
const BLOCK_B_WIDTH: i64 = 250 + OVERLAP_WIDTH;
const BLOCK_B_HEIGHT: i64 = 250 + OVERLAP_HEIGHT;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Paths for output files under examples/output
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let out_dir = examples.join("output");
    std::fs::create_dir_all(&out_dir).expect("create examples/output/");

    // Build A
    let a = ModDef::new("A");
    a.set_usage(Usage::EmitStubAndStop);
    a.set_shape(Polygon::from_width_height(BLOCK_A_WIDTH, BLOCK_A_HEIGHT));
    let x = a.add_port("x", IO::Output(PIN_ROWS * PIN_COLUMNS));
    let small_pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: -SMALL_PIN_WIDTH / 2,
        min_y: -SMALL_PIN_HEIGHT / 2,
        max_x: SMALL_PIN_WIDTH / 2,
        max_y: SMALL_PIN_HEIGHT / 2,
    });
    let origin_x =
        (BLOCK_A_WIDTH - OVERLAP_WIDTH) + ((3 * LARGE_PIN_WIDTH) / 2) - (SMALL_PIN_WIDTH / 2);
    let origin_y = ((3 * LARGE_PIN_HEIGHT) / 2) - (SMALL_PIN_HEIGHT / 2);
    let small_pin = PhysicalPin::new("M1", small_pin_shape);
    for row in 0..PIN_ROWS {
        for col in 0..PIN_COLUMNS {
            let position = (
                origin_x + ((col as i64) * 2 * LARGE_PIN_WIDTH) + SMALL_PIN_WIDTH / 2,
                origin_y + ((row as i64) * 2 * LARGE_PIN_HEIGHT) + SMALL_PIN_HEIGHT / 2,
            );
            x.bit(row * PIN_COLUMNS + col)
                .place(small_pin.with_translation(position.into()));
        }
    }

    // Build B
    let b = ModDef::new("B");
    b.set_usage(Usage::EmitStubAndStop);
    b.set_shape(Polygon::from_width_height(BLOCK_B_WIDTH, BLOCK_B_HEIGHT));
    b.add_port("y", IO::Input(PIN_ROWS * PIN_COLUMNS));

    // Build Top
    let top = ModDef::new("Top");
    let a_inst = top.instantiate(&a, Some("a_inst"), None);
    let b_inst = top.instantiate(&b, Some("b_inst"), None);
    a_inst.get_port("x").connect(&b_inst.get_port("y"));
    a_inst.place((0, 0), Orientation::R0);
    b_inst.place(
        (
            BLOCK_A_WIDTH - OVERLAP_WIDTH + BLOCK_B_WIDTH,
            OVERLAP_HEIGHT - BLOCK_B_HEIGHT,
        ),
        Orientation::MY,
    );

    let large_pin_shape = Polygon::from_bbox(&BoundingBox {
        min_x: -LARGE_PIN_WIDTH / 2,
        min_y: -LARGE_PIN_HEIGHT / 2,
        max_x: LARGE_PIN_WIDTH / 2,
        max_y: LARGE_PIN_HEIGHT / 2,
    });
    let large_pin = PhysicalPin::new("M2", large_pin_shape);
    b_inst.get_port("y").place_overlapped(&large_pin);

    // Emit LEF/DEF for viewing
    let lef_path = out_dir.join("overlapped_pins.lef");
    let def_path = out_dir.join("overlapped_pins.def");
    top.emit_lef_def_to_files(&lef_path, &def_path, &LefDefOptions::default())
        .expect("emit LEF/DEF");

    Ok(())
}

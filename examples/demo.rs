// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{EmitOptions, ModDef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Path to the "examples" folder

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

    // Import the adder module definition from a Verilog file

    let adder_8_bit = ModDef::from_verilog_file("adder", examples.join("input/adder.sv"))?;
    let adder_9_bit = adder_8_bit.parameterize(&[("W", 9)]);

    // Create a top-level module definition

    let top = ModDef::new("top");

    // Instantiate adders in a tree

    let i00 = top.instantiate(&adder_8_bit, Some("i00"), None);
    let i01 = top.instantiate(&adder_8_bit, Some("i01"), None);
    let i11 = top.instantiate(&adder_9_bit, Some("i11"), None);

    // Expose the inputs and output on the top-level module

    i00.get_port("a").export_as("in0");
    i00.get_port("b").export_as("in1");
    i01.get_port("a").export_as("in2");
    i11.get_port("sum").export();

    // Wire together adders in a tree

    i01.get_port("b").tieoff(42); // required because unconnected inputs are not allowed

    i00.get_port("sum").connect(&i11.get_port("a"));
    i01.get_port("sum").connect(&i11.get_port("b"));

    // Emit the final Verilog code

    let output_dir = examples.join("output");
    std::fs::create_dir_all(&output_dir).expect("should be possible to create output dir");
    let output_file = output_dir.join("top.sv");
    top.emit_to_file(&output_file, EmitOptions::default());
    eprintln!("Emitted to output file: {}", output_file.display());
    Ok(())
}

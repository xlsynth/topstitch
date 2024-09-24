// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    ModDef, Usage,
    IO::{Input, Output},
};

fn main() {
    // Path to the "examples" folder

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

    // Import the adder module definition from a Verilog file

    let adder = ModDef::from_verilog_file(
        "adder",
        &examples.join("input").join("adder.sv"),
        true,
        Usage::EmitDefinitionAndStop,
    );

    // Create a top-level module definition

    let top = ModDef::new("top", Default::default());

    // Add ports to the top-level module

    let in0 = top.add_port("in0", Input(8));
    let in1 = top.add_port("in1", Input(8));
    let in2 = top.add_port("in2", Input(8));
    let sum = top.add_port("sum", Output(8));

    // Instantiate adders

    let adder1 = top.instantiate(&adder, "adder1", None);
    let adder2 = top.instantiate(&adder, "adder2", None);
    let adder3 = top.instantiate(&adder, "adder3", None);

    // Wire together adders in a tree

    in0.connect(&adder1.get_port("a"));
    adder1.get_port("b").connect(&in1); // order doesn't matter

    in2.connect(&adder2.get_port("a"));
    adder2.get_port("b").tieoff(42); // required because unconnected inputs are not allowed

    adder1.get_port("sum").connect(&adder3.get_port("a"));
    adder2.get_port("sum").connect(&adder3.get_port("b"));

    // Connect the final adder output the top-level output

    sum.connect(&adder3.get_port("sum"));

    // Emit the final Verilog code

    top.emit_to_file(&examples.join("output").join("top.sv"));
}

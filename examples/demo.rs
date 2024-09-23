// SPDX-License-Identifier: Apache-2.0

use num_bigint::ToBigInt;
use std::{fs, path::PathBuf};
use topstitch::{EmitConfig, ModDef, IO};

fn main() {
    // Path to Verilog files

    let adder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("input")
        .join("adder.sv");
    let adder = std::fs::read_to_string(adder).unwrap();
    let adder = ModDef::from_verilog("adder", &adder, true, EmitConfig::Leaf);

    // Create a top-level module definition

    let top = ModDef::new("top");

    // Add ports to the top-level module

    let in0 = top.add_port("in0", IO::Input(8));
    let in1 = top.add_port("in1", IO::Input(8));
    let in2 = top.add_port("in2", IO::Input(8));
    let sum = top.add_port("sum", IO::Output(8));

    // Instantiate adders

    let adder1 = top.instantiate(&adder, "adder1", None);
    let adder2 = top.instantiate(&adder, "adder2", None);
    let adder3 = top.instantiate(&adder, "adder3", None);

    // Wire together adders in a tree

    in0.connect(&adder1.get_port("a"), 0);
    adder1.get_port("b").connect(&in1, 0); // order doesn't matter

    in2.connect(&adder2.get_port("a"), 0);
    adder2.get_port("b").tieoff(42.to_bigint().unwrap());

    adder1.get_port("sum").connect(&adder3.get_port("a"), 0);
    adder2.get_port("sum").connect(&adder3.get_port("b"), 0);

    // Connect the final adder output the top-level output

    sum.connect(&adder3.get_port("sum"), 0);

    // Emit the final Verilog code
    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join("output")
            .join("top.sv"),
        top.emit(),
    )
    .unwrap();
}

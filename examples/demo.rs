// SPDX-License-Identifier: Apache-2.0

use num_bigint::ToBigInt;
use std::path::PathBuf;
use topstitch::{EmitConfig, ModDef, IO};

fn main() {
    // Path to Verilog files
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("verilog");

    // Read in the Verilog modules from files
    let adder_verilog = std::fs::read_to_string(path.join("adder.sv")).unwrap();
    let mult_verilog = std::fs::read_to_string(path.join("multiplier.sv")).unwrap();

    // Create module definitions from the Verilog code
    let adder = ModDef::from_verilog("adder", &adder_verilog, true, EmitConfig::Nothing);
    let multiplier = ModDef::from_verilog("multiplier", &mult_verilog, true, EmitConfig::Nothing);

    // Create a top-level module definition
    let top = ModDef::new("top_module");

    // Add ports to the top-level module
    let a = top.add_port("a", IO::Input(16));
    let b = top.add_port("b", IO::Input(16));
    let c = top.add_port("c", IO::Input(16));
    let d = top.add_port("d", IO::Output(32));

    // Instantiate adder and multiplier modules in the top module
    let adder_i = top.instantiate(&adder, "adder_i", None);
    let mult_i = top.instantiate(&multiplier, "mult_i", None);

    // Connect top-level inputs to the adder and multiplier inputs
    a.connect(&mult_i.get_port("a"), 0);
    b.connect(&mult_i.get_port("b"), 0);
    adder_i.get_port("a").connect(&mult_i.get_port("prod"), 0);
    adder_i.get_port("b").slice(15, 0).connect(&c, 0);

    // Tie off unused adder operand inputs to zero
    adder_i
        .get_port("b")
        .slice(31, 16)
        .tieoff(0.to_bigint().unwrap());

    // Connect top-level outputs to the adder and multiplier outputs
    d.connect(&adder_i.get_port("sum"), 0);

    // Emit the final Verilog code
    println!("{}", top.emit());
}

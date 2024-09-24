// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::ModDef;

fn main() {
    // Path to the "examples" folder

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

    // Import the adder module definition from a Verilog file

    let adder_8_bit =
        ModDef::from_verilog_file("adder", &examples.join("input").join("adder.sv"), true);
    let adder_9_bit = adder_8_bit.parameterize(&[("W", 9)], None, None);

    // Create a top-level module definition

    let top = ModDef::new("top");

    // Instantiate adders in a tree

    let i00 = top.instantiate(&adder_8_bit, "i00", None);
    let i01 = top.instantiate(&adder_8_bit, "i01", None);
    let i11 = top.instantiate(&adder_9_bit, "i11", None);

    let a = top.add_port("in0", i00.get_port("a").io());
    let b = top.add_port("in1", i00.get_port("b").io());
    let c = top.add_port("in2", i01.get_port("a").io());
    let sum = top.add_port("sum", i11.get_port("sum").io());

    // Wire together adders in a tree

    a.connect(&i00.get_port("a"));
    i00.get_port("b").connect(&b); // order doesn't matter

    c.connect(&i01.get_port("a"));
    i01.get_port("b").tieoff(42); // required because unconnected inputs are not allowed

    i00.get_port("sum").connect(&i11.get_port("a"));
    i01.get_port("sum").connect(&i11.get_port("b"));

    // Connect the final adder output the top-level output

    sum.connect(&i11.get_port("sum"));

    // Emit the final Verilog code

    top.emit_to_file(&examples.join("output").join("top.sv"));
}

// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use topstitch::{
    IO::{Input, Output},
    ModDef,
};

fn main() {
    // Path to the "examples" folder

    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");

    let block = ModDef::new("Block");

    ///////////////////////
    // Basic connections //
    ///////////////////////

    let a = block.add_port("a", Input(8));
    let b = block.add_port("b", Input(64));
    let c = block.add_port("c", Output(16));
    block.add_port("d", Output(32)); // will show how this can be retrieved by name
    let e = block.add_port("e", Input(1));
    let f = block.add_port("f", Input(16));
    let g = block.add_port("g", Output(8));
    let h = block.add_port("h", Output(8));

    // this would be an error due to width mismatch
    // a.connect(&stub.get_port("c"));

    a.connect(&c.slice(7, 0));
    c.slice(15, 8).tieoff(0); // without this, will get an error, "Stub.c is not fully driven."

    let d = block.get_port("d");
    b.slice(31, 0).connect(&d);
    b.slice(63, 32).unused(); // without this, will get an error, "Stub.b is not fully used."

    e.unused(); // without this, will get an error, "Stub.e is not fully used."

    let f_array = f.subdivide(2);
    f_array[0].connect(&g);
    f_array[1].connect(&h);

    //////////////////
    // Feedthroughs //
    //////////////////

    block.feedthrough("ft_in", "ft_out", 128);

    ////////////////
    // Interfaces //
    ////////////////

    // connect by matching function name

    block.add_port("a_intf_data", Input(8));
    block.add_port("a_intf_valid", Output(1));
    let a_intf = block.def_intf_from_prefix("a_intf", "a_intf_");

    block.add_port("b_intf_data", Output(8));
    block.add_port("b_intf_valid", Input(1));
    block.def_intf_from_prefix("b_intf", "b_intf_"); // will show how to retrieve this

    a_intf.connect(&block.get_intf("b_intf"), false);

    // connect by "crossover" (using regex connection)

    block.add_port("c_intf_data_tx", Output(8));
    block.add_port("c_intf_data_rx", Input(8));
    block.add_port("c_intf_valid_tx", Output(1));
    block.add_port("c_intf_valid_rx", Input(1));
    let c_intf = block.def_intf_from_prefix("c_intf", "c_intf_");

    block.add_port("d_intf_data_tx", Output(8));
    block.add_port("d_intf_data_rx", Input(8));
    block.add_port("d_intf_valid_tx", Output(1));
    block.add_port("d_intf_valid_rx", Input(1));
    let d_intf = block.def_intf_from_prefix("d_intf", "d_intf_");

    c_intf.crossover(&d_intf, "^(.*)_tx$", "^(.*)_rx$");

    // interface subdivision

    block.add_port("e_intf_data_tx", Output(16));
    block.add_port("e_intf_data_rx", Input(16));
    block.add_port("e_intf_valid_tx", Output(2));
    block.add_port("e_intf_valid_rx", Input(2));
    let e_intf = block.def_intf_from_prefix("e_intf", "e_intf_");

    block.add_port("f_intf_data_tx", Output(8));
    block.add_port("f_intf_data_rx", Input(8));
    block.add_port("f_intf_valid_tx", Output(1));
    block.add_port("f_intf_valid_rx", Input(1));
    let f_intf = block.def_intf_from_prefix("f_intf", "f_intf_");

    block.add_port("g_intf_data_tx", Output(8));
    block.add_port("g_intf_data_rx", Input(8));
    block.add_port("g_intf_valid_tx", Output(1));
    block.add_port("g_intf_valid_rx", Input(1));
    let g_intf = block.def_intf_from_prefix("g_intf", "g_intf_");

    let e_intf_array = e_intf.subdivide(2);
    e_intf_array[0].crossover(&f_intf, "^(.*)_tx$", "^(.*)_rx$");
    e_intf_array[1].crossover(&g_intf, "^(.*)_tx$", "^(.*)_rx$");

    // marking interfaces as unused

    block.add_port("h_intf_data_tx", Output(8));
    block.add_port("h_intf_data_rx", Input(8));
    block.add_port("h_intf_valid_tx", Output(1));
    block.add_port("h_intf_valid_rx", Input(1));
    let h_intf = block.def_intf_from_prefix("h_intf", "h_intf_");

    h_intf.unused(); // marks all inputs as unused
    h_intf.tieoff(0); // ties off all outputs to 0

    // Emit the final Verilog code

    let output_dir = examples.join("output");
    std::fs::create_dir_all(&output_dir).expect("should be possible to create output dir");
    let output_file = output_dir.join("block.sv");
    block.emit_to_file(&output_file, true);
    eprintln!("Emitted to output file: {}", output_file.display());
}

// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_subdivide() {
    let a = ModDef::new("A");
    a.add_port("out", IO::Output(8));
    a.set_usage(Usage::EmitNothingAndStop);

    let b = ModDef::new("B");
    b.add_port("in", IO::Input(8));
    b.set_usage(Usage::EmitNothingAndStop);

    let top = ModDef::new("top");
    let a = top.instantiate(&a, None, None).get_port("out");
    let b = top.instantiate(&b, None, None).get_port("in");

    for (asub, bsub) in a.subdivide(2).iter().zip(b.subdivide(2)) {
        for (asubsub, bsubsub) in asub.subdivide(2).iter().zip(bsub.subdivide(2)) {
            asubsub.connect(&bsubsub);
        }
    }

    assert_eq!(
        top.emit(true),
        "\
module top;
  wire [7:0] A_i_out;
  wire [7:0] B_i_in;
  A A_i (
    .out(A_i_out)
  );
  B B_i (
    .in(B_i_in)
  );
  assign B_i_in[1:0] = A_i_out[1:0];
  assign B_i_in[3:2] = A_i_out[3:2];
  assign B_i_in[5:4] = A_i_out[5:4];
  assign B_i_in[7:6] = A_i_out[7:6];
endmodule
"
    );
}

#[test]
fn test_intf_subdivide_export() {
    let module_a_verilog = "
      module ModuleA (
          output [15:0] a_data_tx,
          output [1:0] a_valid_tx,
          input [15:0] a_data_rx,
          input [1:0] a_valid_rx
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let a_intf = module_a.def_intf_from_prefix("a", "a_");
    a_intf.subdivide(2);

    let wrapper = ModDef::new("Wrapper");
    let a = wrapper.instantiate(&module_a, None, None);
    a.get_intf("a_0").export_with_prefix("lower", "lower_");
    a.get_intf("a_1").export_with_prefix("upper", "upper_");

    let top_module = ModDef::new("TopModule");
    let w0 = top_module.instantiate(&wrapper, Some("w0"), None);
    let w1 = top_module.instantiate(&wrapper, Some("w1"), None);

    w0.get_intf("lower")
        .crossover(&w1.get_intf("lower"), "(.*)_rx$", "(.*)_tx$");
    w0.get_intf("upper")
        .crossover(&w1.get_intf("upper"), "(.*)_rx$", "(.*)_tx$");

    assert_eq!(
        top_module.emit(true),
        "\
module Wrapper(
  output wire [7:0] lower_data_tx,
  output wire lower_valid_tx,
  input wire [7:0] lower_data_rx,
  input wire lower_valid_rx,
  output wire [7:0] upper_data_tx,
  output wire upper_valid_tx,
  input wire [7:0] upper_data_rx,
  input wire upper_valid_rx
);
  wire [15:0] ModuleA_i_a_data_tx;
  wire [1:0] ModuleA_i_a_valid_tx;
  wire [15:0] ModuleA_i_a_data_rx;
  wire [1:0] ModuleA_i_a_valid_rx;
  ModuleA ModuleA_i (
    .a_data_tx(ModuleA_i_a_data_tx),
    .a_valid_tx(ModuleA_i_a_valid_tx),
    .a_data_rx(ModuleA_i_a_data_rx),
    .a_valid_rx(ModuleA_i_a_valid_rx)
  );
  assign lower_data_tx[7:0] = ModuleA_i_a_data_tx[7:0];
  assign lower_valid_tx = ModuleA_i_a_valid_tx[0:0];
  assign ModuleA_i_a_data_rx[7:0] = lower_data_rx[7:0];
  assign ModuleA_i_a_valid_rx[0:0] = lower_valid_rx;
  assign upper_data_tx[7:0] = ModuleA_i_a_data_tx[15:8];
  assign upper_valid_tx = ModuleA_i_a_valid_tx[1:1];
  assign ModuleA_i_a_data_rx[15:8] = upper_data_rx[7:0];
  assign ModuleA_i_a_valid_rx[1:1] = upper_valid_rx;
endmodule
module TopModule;
  wire [7:0] w0_lower_data_tx;
  wire w0_lower_valid_tx;
  wire [7:0] w0_lower_data_rx;
  wire w0_lower_valid_rx;
  wire [7:0] w0_upper_data_tx;
  wire w0_upper_valid_tx;
  wire [7:0] w0_upper_data_rx;
  wire w0_upper_valid_rx;
  wire [7:0] w1_lower_data_tx;
  wire w1_lower_valid_tx;
  wire [7:0] w1_lower_data_rx;
  wire w1_lower_valid_rx;
  wire [7:0] w1_upper_data_tx;
  wire w1_upper_valid_tx;
  wire [7:0] w1_upper_data_rx;
  wire w1_upper_valid_rx;
  Wrapper w0 (
    .lower_data_tx(w0_lower_data_tx),
    .lower_valid_tx(w0_lower_valid_tx),
    .lower_data_rx(w0_lower_data_rx),
    .lower_valid_rx(w0_lower_valid_rx),
    .upper_data_tx(w0_upper_data_tx),
    .upper_valid_tx(w0_upper_valid_tx),
    .upper_data_rx(w0_upper_data_rx),
    .upper_valid_rx(w0_upper_valid_rx)
  );
  Wrapper w1 (
    .lower_data_tx(w1_lower_data_tx),
    .lower_valid_tx(w1_lower_valid_tx),
    .lower_data_rx(w1_lower_data_rx),
    .lower_valid_rx(w1_lower_valid_rx),
    .upper_data_tx(w1_upper_data_tx),
    .upper_valid_tx(w1_upper_valid_tx),
    .upper_data_rx(w1_upper_data_rx),
    .upper_valid_rx(w1_upper_valid_rx)
  );
  assign w0_lower_data_rx[7:0] = w1_lower_data_tx[7:0];
  assign w0_lower_valid_rx = w1_lower_valid_tx;
  assign w1_lower_data_rx[7:0] = w0_lower_data_tx[7:0];
  assign w1_lower_valid_rx = w0_lower_valid_tx;
  assign w0_upper_data_rx[7:0] = w1_upper_data_tx[7:0];
  assign w0_upper_valid_rx = w1_upper_valid_tx;
  assign w1_upper_data_rx[7:0] = w0_upper_data_tx[7:0];
  assign w1_upper_valid_rx = w0_upper_valid_tx;
endmodule
"
    );
}

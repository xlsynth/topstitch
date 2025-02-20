// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_export_as_single_port() {
    // Define ModuleB with a single output port
    let module_b_verilog = "
        module ModuleB (
            input [7:0] data_in,
            output [7:0] data_out
        );
        endmodule
        ";

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    let module_a = ModDef::new("ModuleA");

    let b_inst = module_a.instantiate(&module_b, Some("inst_b"), None);
    b_inst.get_port("data_in").export_as("b_data_in");
    b_inst.get_port("data_out").export();

    assert_eq!(
        module_a.emit(true),
        "\
module ModuleA(
  input wire [7:0] b_data_in,
  output wire [7:0] data_out
);
  wire [7:0] inst_b_data_in;
  wire [7:0] inst_b_data_out;
  ModuleB inst_b (
    .data_in(inst_b_data_in),
    .data_out(inst_b_data_out)
  );
  assign inst_b_data_in[7:0] = b_data_in[7:0];
  assign data_out[7:0] = inst_b_data_out[7:0];
endmodule
"
    );
}

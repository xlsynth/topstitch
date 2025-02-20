// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_flip_and_copy() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] bus_data_out,
          output bus_valid_out,
          input bus_ready_in
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    let a_intf = module_a.def_intf_from_name_underscore("bus");

    let module_b = ModDef::new("ModuleB");
    let b_intf = a_intf.copy_to(&module_b);
    b_intf.unused_and_tieoff(0);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    a_inst.get_intf("bus").unused_and_tieoff(0);

    let b_inst = top_module.instantiate(&module_b, None, None);
    b_inst.get_intf("bus").unused_and_tieoff(0);

    let module_c = ModDef::new("ModuleC");
    let c_intf = a_inst.get_intf("bus").flip_to(&module_c);
    c_intf.unused_and_tieoff(0);
    let c_inst = top_module.instantiate(&module_c, None, None);
    c_inst.get_intf("bus").unused_and_tieoff(0);

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  output wire [7:0] bus_data_out,
  output wire bus_valid_out,
  input wire bus_ready_in
);
  assign bus_data_out[7:0] = 8'h00;
  assign bus_valid_out = 1'h0;
endmodule
module ModuleC(
  input wire [7:0] bus_data_out,
  input wire bus_valid_out,
  output wire bus_ready_in
);
  assign bus_ready_in = 1'h0;
endmodule
module TopModule;
  ModuleA ModuleA_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleB ModuleB_i (
    .bus_data_out(),
    .bus_valid_out(),
    .bus_ready_in(1'h0)
  );
  ModuleC ModuleC_i (
    .bus_data_out(8'h00),
    .bus_valid_out(1'h0),
    .bus_ready_in()
  );
endmodule
"
    );
}

#[test]
fn test_intf_copy_to_with_prefix() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a", "a_");

    let module_b = ModDef::new("ModuleB");
    let b_intf = module_a
        .get_intf("a")
        .copy_to_with_prefix(&module_b, "b", "");
    b_intf.unused_and_tieoff(0);

    assert_eq!(
        module_b.emit(true),
        "\
module ModuleB(
  output wire [31:0] data,
  output wire valid,
  input wire ready
);
  assign data[31:0] = 32'h0000_0000;
  assign valid = 1'h0;
endmodule
"
    );
}

#[test]
fn test_intf_copy_to_with_name_underscore() {
    let module_a_verilog = "
    module ModuleA (
        output [31:0] a_data,
        output a_valid,
        input a_ready
    );
    endmodule
    ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_b = ModDef::new("ModuleB");
    let b_intf = module_a
        .get_intf("a")
        .copy_to_with_name_underscore(&module_b, "b");
    b_intf.unused_and_tieoff(0);

    assert_eq!(
        module_b.emit(true),
        "\
module ModuleB(
  output wire [31:0] b_data,
  output wire b_valid,
  input wire b_ready
);
  assign b_data[31:0] = 32'h0000_0000;
  assign b_valid = 1'h0;
endmodule
"
    );
}

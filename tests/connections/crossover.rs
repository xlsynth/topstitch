// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_crossover() {
    let module_a_verilog = "
      module ModuleA (
          output a_tx,
          input a_rx
      );
      endmodule
      ";

    let module_b_verilog = "
      module ModuleB (
        output b_tx,
        input b_rx
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_prefix("a_intf", "a_");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_prefix("b_intf", "b_");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    let a_intf = a_inst.get_intf("a_intf");
    let b_intf = b_inst.get_intf("b_intf");

    a_intf.crossover(&b_intf, "tx", "rx");

    assert_eq!(
        top_module.emit(true),
        "\
module TopModule;
  wire inst_a_a_tx;
  wire inst_b_b_tx;
  ModuleA inst_a (
    .a_tx(inst_a_a_tx),
    .a_rx(inst_b_b_tx)
  );
  ModuleB inst_b (
    .b_tx(inst_b_b_tx),
    .b_rx(inst_a_a_tx)
  );
endmodule
"
    );
}

#[test]
fn test_crossover_except() {
    let module_a_verilog = "
      module ModuleA (
          output intf_a_tx,
          input intf_a_rx,
          output intf_b_tx,
          input intf_b_rx
      );
      endmodule
      ";

    let module_b_verilog = "
      module ModuleB (
        output intf_b_tx,
        input intf_b_rx,
        output intf_a_tx,
        input intf_a_rx
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("intf");

    let module_b = ModDef::from_verilog("ModuleB", module_b_verilog, true, false);
    module_b.def_intf_from_name_underscore("intf");

    let top_module = ModDef::new("TopModule");

    let a_inst = top_module.instantiate(&module_a, Some("inst_a"), None);
    let b_inst = top_module.instantiate(&module_b, Some("inst_b"), None);

    a_inst.get_intf("intf").crossover_except(
        &b_inst.get_intf("intf"),
        "^(.*)_tx$",
        "^(.*)_rx$",
        Some(&["b_tx"]),
    );
    a_inst
        .get_intf("intf")
        .get("b_tx")
        .unwrap()
        .unused_or_tieoff(0);
    b_inst
        .get_intf("intf")
        .get("b_rx")
        .unwrap()
        .unused_or_tieoff(0);
    a_inst
        .get_intf("intf")
        .get("b_rx")
        .unwrap()
        .unused_or_tieoff(0);
    b_inst
        .get_intf("intf")
        .get("b_tx")
        .unwrap()
        .unused_or_tieoff(0);

    let emitted = top_module.emit(true);
    assert_eq!(
        emitted,
        "\
module TopModule;
  wire inst_a_intf_a_tx;
  wire inst_b_intf_a_tx;
  ModuleA inst_a (
    .intf_a_tx(inst_a_intf_a_tx),
    .intf_a_rx(inst_b_intf_a_tx),
    .intf_b_tx(),
    .intf_b_rx(1'h0)
  );
  ModuleB inst_b (
    .intf_b_tx(),
    .intf_b_rx(1'h0),
    .intf_a_tx(inst_b_intf_a_tx),
    .intf_a_rx(inst_a_intf_a_tx)
  );
endmodule
"
    );
}

#[test]
fn test_intf_crossover_through() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_out
      );
      endmodule
      ";

    let module_e_verilog = "
      module ModuleE (
          input [7:0] e_data_in,
          input e_valid_in,
          output e_ready_in
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_e = ModDef::from_verilog("ModuleE", module_e_verilog, true, false);
    module_e.def_intf_from_name_underscore("e");

    let module_b = ModDef::new("ModuleB");
    let module_c = ModDef::new("ModuleC");
    let module_d = ModDef::new("ModuleD");

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);
    let d_inst = top_module.instantiate(&module_d, None, None);
    let e_inst = top_module.instantiate(&module_e, None, None);

    a_inst.get_intf("a").crossover_through(
        &e_inst.get_intf("e"),
        &[&b_inst, &c_inst, &d_inst],
        "(.*)_out",
        "(.*)_in",
        "ft_x",
        "ft_y",
    );

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out = ft_x_data_in;
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module ModuleC(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out = ft_x_data_in;
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module ModuleD(
  input wire [7:0] ft_x_data_in,
  output wire [7:0] ft_y_data_out,
  input wire ft_x_valid_in,
  output wire ft_y_valid_out,
  output wire ft_x_ready_in,
  input wire ft_y_ready_out
);
  assign ft_y_data_out = ft_x_data_in;
  assign ft_y_valid_out = ft_x_valid_in;
  assign ft_x_ready_in = ft_y_ready_out;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire ModuleB_i_ft_x_ready_in;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_out(ModuleB_i_ft_x_ready_in)
  );
  wire [7:0] ModuleB_i_ft_y_data_out;
  wire ModuleB_i_ft_y_valid_out;
  wire ModuleC_i_ft_x_ready_in;
  ModuleB ModuleB_i (
    .ft_x_data_in(ModuleA_i_a_data_out),
    .ft_y_data_out(ModuleB_i_ft_y_data_out),
    .ft_x_valid_in(ModuleA_i_a_valid_out),
    .ft_y_valid_out(ModuleB_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleB_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleC_i_ft_x_ready_in)
  );
  wire [7:0] ModuleC_i_ft_y_data_out;
  wire ModuleC_i_ft_y_valid_out;
  wire ModuleD_i_ft_x_ready_in;
  ModuleC ModuleC_i (
    .ft_x_data_in(ModuleB_i_ft_y_data_out),
    .ft_y_data_out(ModuleC_i_ft_y_data_out),
    .ft_x_valid_in(ModuleB_i_ft_y_valid_out),
    .ft_y_valid_out(ModuleC_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleC_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleD_i_ft_x_ready_in)
  );
  wire [7:0] ModuleD_i_ft_y_data_out;
  wire ModuleD_i_ft_y_valid_out;
  wire ModuleE_i_e_ready_in;
  ModuleD ModuleD_i (
    .ft_x_data_in(ModuleC_i_ft_y_data_out),
    .ft_y_data_out(ModuleD_i_ft_y_data_out),
    .ft_x_valid_in(ModuleC_i_ft_y_valid_out),
    .ft_y_valid_out(ModuleD_i_ft_y_valid_out),
    .ft_x_ready_in(ModuleD_i_ft_x_ready_in),
    .ft_y_ready_out(ModuleE_i_e_ready_in)
  );
  ModuleE ModuleE_i (
    .e_data_in(ModuleD_i_ft_y_data_out),
    .e_valid_in(ModuleD_i_ft_y_valid_out),
    .e_ready_in(ModuleE_i_e_ready_in)
  );
endmodule
"
    );
}

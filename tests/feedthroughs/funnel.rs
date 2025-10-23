// SPDX-License-Identifier: Apache-2.0

use topstitch::*;

#[test]
fn test_funnel() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_in
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in,
          output c_ready_out
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);

    let module_b = ModDef::new("ModuleB");
    module_b.feedthrough("ft_left_i", "ft_right_o", 10);
    module_b.feedthrough("ft_right_i", "ft_left_o", 10);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    let mut funnel = Funnel::new(
        (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
        (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
    );

    funnel.connect(
        &a_inst.get_port("a_data_out"),
        &c_inst.get_port("c_data_in"),
    );
    funnel.connect(
        &a_inst.get_port("a_valid_out"),
        &c_inst.get_port("c_valid_in"),
    );
    funnel.connect(
        &a_inst.get_port("a_ready_in"),
        &c_inst.get_port("c_ready_out"),
    );

    funnel.tieoff_remaining();

    assert!(funnel.a2b_yield_remaining().is_none());
    assert!(funnel.b2a_yield_remaining().is_none());

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [9:0] ft_left_i,
  output wire [9:0] ft_right_o,
  input wire [9:0] ft_right_i,
  output wire [9:0] ft_left_o
);
  assign ft_right_o = ft_left_i;
  assign ft_left_o = ft_right_i;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire [9:0] ModuleB_i_ft_left_o;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_in(ModuleB_i_ft_left_o[0])
  );
  wire [9:0] ModuleB_i_ft_right_o;
  wire ModuleC_i_c_ready_out;
  ModuleB ModuleB_i (
    .ft_left_i({1'h0, ModuleA_i_a_valid_out, ModuleA_i_a_data_out}),
    .ft_right_o({ModuleB_i_ft_right_o[9], ModuleB_i_ft_right_o[8:0]}),
    .ft_right_i({9'h000, ModuleC_i_c_ready_out}),
    .ft_left_o({ModuleB_i_ft_left_o[9:1], ModuleB_i_ft_left_o[0]})
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleB_i_ft_right_o[7:0]),
    .c_valid_in(ModuleB_i_ft_right_o[8]),
    .c_ready_out(ModuleC_i_c_ready_out)
  );
endmodule
"
    );
}

#[test]
#[should_panic(expected = "Funnel error: out of capacity")]
fn test_funnel_capacity() {
    let module_a_verilog = "
      module ModuleA (
          output a_out_0,
          output a_out_1,
          input a_in_0,
          input a_in_1
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input c_in_0,
          input c_in_1,
          output c_out_0,
          output c_out_1
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);

    let module_b = ModDef::new("ModuleB");
    module_b.feedthrough("ft_left_i", "ft_right_o", 1);
    module_b.feedthrough("ft_right_i", "ft_left_o", 1);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    let mut funnel = Funnel::new(
        (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
        (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
    );

    funnel.connect(&a_inst.get_port("a_in_0"), &c_inst.get_port("c_out_0"));
    funnel.connect(&a_inst.get_port("a_in_1"), &c_inst.get_port("c_out_1"));
}

#[test]
#[should_panic(expected = "Funnel error: a -> b channel is not full")]
fn test_funnel_not_full() {
    let module_a_verilog = "
      module ModuleA (
          output a_out_0,
          output a_out_1,
          input a_in_0,
          input a_in_1
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input c_in_0,
          input c_in_1,
          output c_out_0,
          output c_out_1
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);

    let module_b = ModDef::new("ModuleB");
    module_b.feedthrough("ft_left_i", "ft_right_o", 2);
    module_b.feedthrough("ft_right_i", "ft_left_o", 2);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    let mut funnel = Funnel::new(
        (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
        (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
    );

    funnel.connect(&a_inst.get_port("a_in_0"), &c_inst.get_port("c_out_0"));
    funnel.connect(&a_inst.get_port("a_in_1"), &c_inst.get_port("c_out_1"));

    funnel.connect(&a_inst.get_port("a_out_0"), &c_inst.get_port("c_in_0"));

    funnel.assert_full();
}

#[test]
fn test_funnel_connect_intf() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data,
          output a_valid,
          input a_ready
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data,
          input c_valid,
          output c_ready
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);
    module_c.def_intf_from_name_underscore("c");

    let module_b = ModDef::new("ModuleB");
    module_b.feedthrough("ft_left_i", "ft_right_o", 9);
    module_b.feedthrough("ft_right_i", "ft_left_o", 9);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    let mut funnel = Funnel::new(
        (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
        (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
    );

    funnel.connect_intf(&a_inst.get_intf("a"), &c_inst.get_intf("c"), false);

    funnel.assert_a2b_full();
    funnel.tieoff_remaining();
    funnel.assert_full();

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [8:0] ft_left_i,
  output wire [8:0] ft_right_o,
  input wire [8:0] ft_right_i,
  output wire [8:0] ft_left_o
);
  assign ft_right_o = ft_left_i;
  assign ft_left_o = ft_right_i;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data;
  wire ModuleA_i_a_valid;
  wire [8:0] ModuleB_i_ft_left_o;
  ModuleA ModuleA_i (
    .a_data(ModuleA_i_a_data),
    .a_valid(ModuleA_i_a_valid),
    .a_ready(ModuleB_i_ft_left_o[0])
  );
  wire [8:0] ModuleB_i_ft_right_o;
  wire ModuleC_i_c_ready;
  ModuleB ModuleB_i (
    .ft_left_i({ModuleA_i_a_valid, ModuleA_i_a_data}),
    .ft_right_o(ModuleB_i_ft_right_o),
    .ft_right_i({8'h00, ModuleC_i_c_ready}),
    .ft_left_o({ModuleB_i_ft_left_o[8:1], ModuleB_i_ft_left_o[0]})
  );
  ModuleC ModuleC_i (
    .c_data(ModuleB_i_ft_right_o[7:0]),
    .c_valid(ModuleB_i_ft_right_o[8]),
    .c_ready(ModuleC_i_c_ready)
  );
endmodule
"
    );
}

#[test]
fn test_funnel_crossover_intf() {
    let module_a_verilog = "
      module ModuleA (
          output [7:0] a_data_out,
          output a_valid_out,
          input a_ready_in
      );
      endmodule
      ";

    let module_c_verilog = "
      module ModuleC (
          input [7:0] c_data_in,
          input c_valid_in,
          output c_ready_out
      );
      endmodule
      ";

    let module_a = ModDef::from_verilog("ModuleA", module_a_verilog, true, false);
    module_a.def_intf_from_name_underscore("a");

    let module_c = ModDef::from_verilog("ModuleC", module_c_verilog, true, false);
    module_c.def_intf_from_name_underscore("c");

    let module_b = ModDef::new("ModuleB");
    module_b.feedthrough("ft_left_i", "ft_right_o", 10);
    module_b.feedthrough("ft_right_i", "ft_left_o", 1);

    let top_module = ModDef::new("TopModule");
    let a_inst = top_module.instantiate(&module_a, None, None);
    let b_inst = top_module.instantiate(&module_b, None, None);
    let c_inst = top_module.instantiate(&module_c, None, None);

    let mut funnel = Funnel::new(
        (b_inst.get_port("ft_left_i"), b_inst.get_port("ft_left_o")),
        (b_inst.get_port("ft_right_i"), b_inst.get_port("ft_right_o")),
    );

    funnel.crossover_intf(
        &a_inst.get_intf("a"),
        &c_inst.get_intf("c"),
        "(.*)_out",
        "(.*)_in",
    );

    funnel.assert_b2a_full();
    funnel.tieoff_remaining();
    funnel.assert_full();

    assert_eq!(
        top_module.emit(true),
        "\
module ModuleB(
  input wire [9:0] ft_left_i,
  output wire [9:0] ft_right_o,
  input wire ft_right_i,
  output wire ft_left_o
);
  assign ft_right_o = ft_left_i;
  assign ft_left_o = ft_right_i;
endmodule
module TopModule;
  wire [7:0] ModuleA_i_a_data_out;
  wire ModuleA_i_a_valid_out;
  wire ModuleB_i_ft_left_o;
  ModuleA ModuleA_i (
    .a_data_out(ModuleA_i_a_data_out),
    .a_valid_out(ModuleA_i_a_valid_out),
    .a_ready_in(ModuleB_i_ft_left_o)
  );
  wire [9:0] ModuleB_i_ft_right_o;
  wire ModuleC_i_c_ready_out;
  ModuleB ModuleB_i (
    .ft_left_i({1'h0, ModuleA_i_a_valid_out, ModuleA_i_a_data_out}),
    .ft_right_o({ModuleB_i_ft_right_o[9], ModuleB_i_ft_right_o[8:0]}),
    .ft_right_i(ModuleC_i_c_ready_out),
    .ft_left_o(ModuleB_i_ft_left_o)
  );
  ModuleC ModuleC_i (
    .c_data_in(ModuleB_i_ft_right_o[7:0]),
    .c_valid_in(ModuleB_i_ft_right_o[8]),
    .c_ready_out(ModuleC_i_c_ready_out)
  );
endmodule
"
    );
}
